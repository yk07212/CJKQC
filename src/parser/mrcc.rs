//! MRCC energy-program outputs: the echoed input deck (version, command,
//! charge, spin), the canonical bohr-unit geometry table, and the final
//! energy summary whose last total-energy line — the program's best
//! answer, MP2 corrections included for local-correlation runs — is the
//! record's electronic energy. Detection is the opening banner.
//!
//! (rq-547a8fd6)

use crate::parser::extract::{
    parse_fortran_f64, read_geometry_table, LengthUnits, SymbolColumn, TableSpec,
};
use crate::parser::{
    first_line_offset, is_integer, rows_following, FileWindows, LinePattern, Parser, Program,
};
use crate::record::{Record, RecordStatus};

/// MRCC output reader: `program()` is `Program::MRCC`.
///
/// (rq-d72bb4c5)
pub struct MrccParser;

/// The command: the echoed `Input file:` deck's keyword lines (original
/// case), joined with single spaces, ending before the first `mem=`,
/// `charge=`, `mult=`, or `geom=` line; charge and multiplicity live in
/// their own record fields. An empty block or a missing echo leaves the
/// command absent.
// rq-b2af2117 rq-b8a853f8
fn command_from_echo(head: &str) -> Option<String> {
    const STOP_KEYS: [&str; 4] = ["mem=", "charge=", "mult=", "geom="];
    let (_, deck) = head.split_once("Input file:")?;
    let mut keywords: Vec<&str> = Vec::new();
    for line in deck.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if STOP_KEYS.iter().any(|key| trimmed.starts_with(key)) {
            break;
        }
        match trimmed.split_once('=') {
            Some((key, value))
                if !key.is_empty() && !key.contains(char::is_whitespace) && !value.is_empty() =>
            {
                keywords.push(trimmed);
            }
            _ => break,
        }
    }
    (!keywords.is_empty()).then(|| keywords.join(" "))
}

/// A final-total-energy line of the energy summary: a `Total … energy …
/// [au]:` line that is not a correlation-energy line.
// rq-e719ddf4 rq-a60f442f
fn total_energy_line(line: &str) -> Option<f64> {
    let trimmed = line.trim();
    if trimmed.starts_with("Total ")
        && trimmed.contains("[au]:")
        && !trimmed.contains("correlation energy")
    {
        parse_fortran_f64(trimmed.split_whitespace().last()?)
    } else {
        None
    }
}

/// The charge echo line: a two-field `Charge: <n>` line.
fn is_charge_line(line: &str) -> bool {
    let mut fields = line.split_whitespace();
    fields.next() == Some("Charge:") && fields.next().is_some() && fields.next().is_none()
}

/// The final Hartree-Fock energy line's value: the converged HF total
/// the correlation method builds on, from the
/// `***FINAL HARTREE-FOCK ENERGY: <value> [AU]` line.
// rq-ef61dab4
fn hf_energy_value(line: &str) -> Option<f64> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    match fields.as_slice() {
        ["***FINAL", "HARTREE-FOCK", "ENERGY:", value, ..] => parse_fortran_f64(value),
        _ => None,
    }
}

/// Whether the line is the final Hartree-Fock energy line.
fn is_hf_energy_line(line: &str) -> bool {
    hf_energy_value(line).is_some()
}

/// An MRCC abort or error-termination line.
fn is_mrcc_abort(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("Aborting") || trimmed.contains("Error termination")
}

/// Whether the line is a final total-energy line.
fn is_total_energy_line(line: &str) -> bool {
    total_energy_line(line).is_some()
}

/// The locate-pattern indices of the markers MRCC prints.
///
/// (rq-15b1763d)
const VERSION: usize = 0;
const CHARGE: usize = 1;
const SPIN_MULTIPLICITY: usize = 2;
const GEOMETRY_HEADER: usize = 3;
const NUMBER_OF_ATOMS: usize = 4;
const TOTAL_ENERGY: usize = 5;
const ABORT: usize = 6;
const HF_ENERGY: usize = 7;

/// Every marker line MRCC's parse path needs, located in one walk of
/// the windows. The index constants above key this table by position;
/// the alignment unit test pins each constant to its entry.
const PATTERNS: [LinePattern<'static>; 8] = [
    LinePattern::TrimmedStartsWith("Version:"),
    LinePattern::Where(&is_charge_line),
    LinePattern::TrimmedStartsWith("Spin multiplicity:"),
    LinePattern::TrimmedStartsWith("Cartesian coordinates [bohr]"),
    LinePattern::TrimmedStartsWith("Number of atoms:"),
    LinePattern::Where(&is_total_energy_line),
    LinePattern::Where(&is_mrcc_abort),
    LinePattern::Where(&is_hf_energy_line),
];

// rq-d72bb4c5
impl Parser for MrccParser {
    fn program(&self) -> Program {
        Program::MRCC
    }

    // rq-33ab6159 rq-8890cc1d rq-15b1763d
    fn detect(&self, head: &str) -> Option<usize> {
        memchr::memmem::find(head.as_bytes(), b"MRCC program system")?;
        first_line_offset(head, |line| line.contains("MRCC program system"))
    }

    fn parse(&self, windows: &FileWindows) -> Record {
        // One identification walk finds every marker this format reads,
        // status markers included (rq-15b1763d).
        let matches = windows.locate(&PATTERNS);

        let program_version = matches
            .first(VERSION)
            .and_then(|line| line.split_whitespace().nth(1).map(String::from));

        // rq-de12aef8
        let command = command_from_echo(&windows.head);

        let charge = matches
            .first(CHARGE)
            .and_then(|line| line.split_whitespace().nth(1)?.parse::<i32>().ok());

        let multiplicity = matches
            .first(SPIN_MULTIPLICITY)
            .and_then(|line| line.split_whitespace().last()?.parse::<u32>().ok());

        // The canonical geometry table, printed in bohr.
        // rq-de12aef8 rq-fd767996
        let geometry_rows = rows_following(matches.after_last(GEOMETRY_HEADER), |fields| {
            fields.len() == 7
                && is_integer(fields[0])
                && fields[1].bytes().all(|byte| byte.is_ascii_alphabetic())
                && fields[2..5]
                    .iter()
                    .all(|field| parse_fortran_f64(field).is_some())
        });
        let expected_rows = matches
            .first(NUMBER_OF_ATOMS)
            .and_then(|line| line.split_whitespace().last()?.parse::<usize>().ok());
        let geometry = if geometry_rows.is_empty() {
            None
        } else {
            read_geometry_table(
                geometry_rows.iter().copied(),
                &TableSpec {
                    symbol_column: SymbolColumn::Symbol(1),
                    coordinate_columns: [2, 3, 4],
                    units: LengthUnits::Bohr,
                    expected_rows,
                },
            )
        };

        // The run's best answer: the last non-correlation total.
        // rq-e719ddf4 rq-a60f442f
        let electronic_energy = matches.last(TOTAL_ENERGY).and_then(total_energy_line);

        // The converged HF total the correlation method builds on.
        // rq-ef61dab4
        let scf_energy = matches.last(HF_ENERGY).and_then(hf_energy_value);

        // Status: the run's own termination banner governs it.
        // rq-be38168f rq-91435a73 rq-97790ec7 rq-5825f5e3
        let (status, error) = if windows.tail.contains("Normal termination of mrcc") {
            (RecordStatus::Ok, None)
        } else if matches.last_in_tail(ABORT).is_some() {
            // Provisional error signal, pending the genuine failed
            // fixture that pins MRCC's abort wording
            // (`rqm/parsers/conformance.md`).
            (
                RecordStatus::Failed,
                Some("MRCC reported an error and aborted".to_string()),
            )
        } else {
            (
                RecordStatus::Truncated,
                Some("the MRCC run ends before its termination banner".to_string()),
            )
        };

        Record {
            status,
            error,
            program_version,
            command,
            charge,
            multiplicity,
            electronic_energy,
            scf_energy,
            geometry,
            ..Record::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// One line per `PATTERNS` entry, each matching exactly one pattern
    /// of this format: the located answer for a constant must be the
    /// line written for its entry, so a constant and a table entry that
    /// drift apart fail here rather than silently misreading markers.
    const MARKER_LINES: [(&str, usize); 8] = [
        ("Version:   2021.1 marker", VERSION),
        ("Charge:  0", CHARGE),
        ("Spin multiplicity: 1", SPIN_MULTIPLICITY),
        ("Cartesian coordinates [bohr]:", GEOMETRY_HEADER),
        ("Number of atoms: 3", NUMBER_OF_ATOMS),
        ("Total energy [au]: -1.0", TOTAL_ENERGY),
        ("Aborting marker", ABORT),
        (
            "***FINAL HARTREE-FOCK ENERGY:       -134.2943930780812707 [AU]",
            HF_ENERGY,
        ),
    ];

    #[test]
    fn marker_constants_align_with_the_pattern_table() {
        let content: String = MARKER_LINES
            .iter()
            .map(|(line, _)| format!("{line}\n"))
            .collect();
        let mut cursor = Cursor::new(content.as_bytes());
        let windows =
            FileWindows::from_seekable(&mut cursor, 1 << 20, 1 << 20).expect("covered windows");
        let matches = windows.locate(&PATTERNS);
        for (line, index) in MARKER_LINES {
            assert_eq!(matches.first(index), Some(line), "pattern index {index}");
            assert_eq!(matches.last(index), Some(line), "pattern index {index}");
        }
    }
}
