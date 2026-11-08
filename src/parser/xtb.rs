//! xtb (GFN-xTB family) stdout logs: the run's final `TOTAL ENERGY`, the
//! optimized structure, the genuine Hessian frequencies (trivial modes
//! dropped), and the population and dipole analysis of the last single
//! point. Detection is the version banner; every observable comes from
//! the last visible occurrence of its marker block. The normal-exit
//! banner governs status.
//!
//! (rq-cd5bd6c0)

use crate::parser::extract::{
    parse_fortran_f64, read_geometry_table, vibrational_modes, LengthUnits, SymbolColumn, TableSpec,
};
use crate::parser::{
    first_line_offset, frequency_table_short, is_integer, parse_vector3, rows_following,
    FileWindows, LinePattern, Parser, Program,
};
use crate::record::{Record, RecordStatus};

/// xtb log reader: `program()` is `Program::XTB`.
///
/// (rq-597bef47)
pub struct XtbParser;

/// The version token of the banner line `* xtb version <v> (<hash>) ...`.
// rq-35c8e2c9
fn version_token(line: &str) -> Option<String> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    let position = fields.iter().position(|field| *field == "version")?;
    fields.get(position + 1).map(|token| (*token).to_string())
}

/// The invocation echoed on the `program call` line: the label's value,
/// whitespace-trimmed, with the directory part of the executable path
/// stripped so the value starts at the executable name; an empty value
/// leaves the command absent.
// rq-c01ba326 rq-1fb0a352
fn command_from_call(line: &str) -> Option<String> {
    let value = match line.split_once(':') {
        Some((_, value)) => value.trim(),
        None => line
            .trim()
            .strip_prefix("program call")
            .unwrap_or(line.trim())
            .trim(),
    };
    if value.is_empty() {
        return None;
    }
    let (executable, rest) = match value.find(char::is_whitespace) {
        Some(position) => value.split_at(position),
        None => (value, ""),
    };
    let name = match executable.rfind('/') {
        Some(position) if position + 1 < executable.len() => &executable[position + 1..],
        _ => executable,
    };
    Some(format!("{name}{}", rest.trim_end()))
}

/// The energy value of the boxed final `| TOTAL ENERGY <value> Eh |` line.
// rq-20204b94
fn boxed_total_energy(fields: &[&str]) -> Option<f64> {
    let position = fields.iter().position(|field| *field == "ENERGY")?;
    fields
        .get(position + 1)
        .and_then(|value| parse_fortran_f64(value))
}

/// The value printed after a fixed two-part label line (e.g. `molecular
/// charge <value> e`): the value field's position equals the number of
/// words in the label phrase, so the caller passes the pinned label's own
/// word count rather than an arbitrary column.
/// The eigenvalue columns of the `eigval ...` lines leading `lines`
/// (the block ends at its first non-eigval line), appended to `values`.
/// The span walk from a located header: fields are iterated, never
/// collected, so the walk allocates nothing per line.
// rq-15b1763d
fn collect_eigenvalues<'a>(lines: impl Iterator<Item = &'a str>, values: &mut Vec<f64>) {
    let mut started = false;
    for line in lines {
        let mut fields = line.split_whitespace();
        if fields.next() == Some("eigval") {
            values.extend(
                fields
                    .skip(1)
                    .filter_map(parse_fortran_f64),
            );
            started = true;
        } else if started {
            break;
        }
    }
}

/// The whole-window gather of the seam fallback: the `eigval` lines of a
/// window that carries no visible section header, located by one SIMD
/// literal scan of the marker text and verified on the candidates only —
/// never a field split of every line. Consecutive candidates continue the
/// block exactly as the walk's first non-eigval break.
// rq-15b1763d
fn scan_eigenvalues(window: &str, values: &mut Vec<f64>) {
    let mut started = false;
    let mut next_row_start: Option<usize> = None;
    for marker in crate::parser::extract::marker_lines(window, "eigval") {
        if started && next_row_start != Some(marker.line_start) {
            break;
        }
        let mut fields = marker.line.split_whitespace();
        if fields.next() == Some("eigval") {
            values.extend(
                fields
                    .skip(1)
                    .filter_map(parse_fortran_f64),
            );
            started = true;
            next_row_start = Some(marker.next_start);
        } else if started {
            break;
        }
    }
}

fn numeric_field(line: &str, words_in_label: usize) -> Option<f64> {
    parse_fortran_f64(line.split_whitespace().nth(words_in_label)?)
}

/// The locate-pattern indices of the markers xtb prints, in the order of
/// the `PATTERNS` array below.
///
/// (rq-15b1763d)
const VERSION: usize = 0;
const PROGRAM_CALL: usize = 1;
const CHARGE: usize = 2;
const UNPAIRED: usize = 3;
const TOTAL_ENERGY: usize = 4;
const FINAL_STRUCTURE: usize = 5;
const PROJECTED_FREQUENCIES: usize = 6;
const DIPOLE_HEADER: usize = 7;
const POPULATION_HEADER: usize = 8;
const ERROR_LINE: usize = 9;

/// Every marker line xtb's parse path needs, located in one walk of the
/// windows.
const PATTERNS: [LinePattern<'static>; 10] = [
    LinePattern::Contains("xtb version"),
    LinePattern::TrimmedStartsWith("program call"),
    LinePattern::Contains("molecular charge"),
    LinePattern::Contains("number of unpaired electrons"),
    LinePattern::Contains("TOTAL ENERGY"),
    LinePattern::TrimmedEquals("final structure:"),
    LinePattern::TrimmedStartsWith("projected vibrational frequencies"),
    LinePattern::TrimmedEquals("Atomic dipole moments (in atomic units):"),
    LinePattern::TrimmedStartsWith("Atomic charges and shell populations"),
    LinePattern::TrimmedStartsWith("ERROR"),
];

// rq-597bef47
impl Parser for XtbParser {
    fn program(&self) -> Program {
        Program::XTB
    }

    // rq-35c8e2c9 rq-0bb4b64f rq-15b1763d
    fn detect(&self, head: &str) -> Option<usize> {
        memchr::memmem::find(head.as_bytes(), b"xtb version")?;
        first_line_offset(head, |line| line.contains("xtb version"))
    }

    fn parse(&self, windows: &FileWindows) -> Record {
        // One identification walk finds every marker this format reads,
        // status markers included (rq-15b1763d).
        let matches = windows.locate(&PATTERNS);

        // Status: the run's own end and error reporting governs it. The
        // end test is a whole-window SIMD byte search
        // (`rqm/parser-framework.md`, Performance Constraints).
        // rq-ce1b9305 rq-62e1382e rq-9c076f14 rq-a652c946 rq-15b1763d
        let (status, error) = if memchr::memmem::find(windows.tail.as_bytes(), b"finished run")
            .is_some()
        {
            (RecordStatus::Ok, None)
        } else if matches.last_in_tail(ERROR_LINE).is_some() {
            // Provisional error signal, pending the genuine failed
            // fixture that pins xtb's abort wording
            // (`rqm/parsers/conformance.md`).
            (
                RecordStatus::Failed,
                Some("xtb reported an error and did not finish the run".to_string()),
            )
        } else {
            (
                RecordStatus::Truncated,
                Some("the xtb run ends before its 'finished run' banner".to_string()),
            )
        };

        let program_version = matches.first(VERSION).and_then(version_token);

        // rq-c01ba326
        let command = matches.last(PROGRAM_CALL).and_then(command_from_call);

        // rq-68850338
        let charge = matches
            .last(CHARGE)
            .and_then(|line| numeric_field(line, 2))
            .map(|value| value.round() as i32);
        let multiplicity = matches
            .last(UNPAIRED)
            .and_then(|line| numeric_field(line, 4))
            .map(|value| value.round() as u32 + 1);

        let electronic_energy = matches
            .last(TOTAL_ENERGY)
            .and_then(|line| boxed_total_energy(&line.split_whitespace().collect::<Vec<&str>>()));

        // The optimized structure: the `final structure:` block holds an
        // xyz-style table behind its rule and count lines.
        // rq-68850338 rq-afe58819
        let structure_rows = rows_following(matches.after_last(FINAL_STRUCTURE), |fields| {
            fields.len() >= 4
                && fields[0].bytes().all(|byte| byte.is_ascii_alphabetic())
                && fields[1..4]
                    .iter()
                    .all(|field| parse_fortran_f64(field).is_some())
        });
        let geometry = read_geometry_table(
            structure_rows.iter().copied(),
            &TableSpec {
                symbol_column: SymbolColumn::Symbol(0),
                coordinate_columns: [1, 2, 3],
                units: LengthUnits::Angstrom,
                expected_rows: None,
            },
        );

        // The projected frequencies printed after the numerical Hessian:
        // the parsed values pass through the shared trivial-mode filter
        // (near-zero translational/rotational eigenvalues dropped); every
        // genuine mode keeps its printed sign (a minus marks imaginary).
        // When the last header is not visible in the tail the block may
        // straddle the elided middle, so the head remainder and the whole
        // tail are both gathered — the whole-tail gather by literal scan
        // (`rqm/parser-framework.md`, Performance Constraints).
        // rq-68850338 rq-5882d984 rq-03fd1a40 rq-15b1763d
        let mut parsed_frequencies: Vec<f64> = Vec::new();
        let header_in_tail = matches.last_in_tail(PROJECTED_FREQUENCIES).is_some();
        collect_eigenvalues(
            matches.after_last(PROJECTED_FREQUENCIES),
            &mut parsed_frequencies,
        );
        if !header_in_tail {
            scan_eigenvalues(&windows.tail, &mut parsed_frequencies);
        }
        // rq-c37111b1
        let frequencies = vibrational_modes(&parsed_frequencies);

        // The molecule's dipole from the last property printout, already
        // in atomic units.
        let dipole_moment = matches.after_last(DIPOLE_HEADER).find_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.first() == Some(&"total") && fields.len() == 4 {
                parse_vector3([fields[1], fields[2], fields[3]])
            } else {
                None
            }
        });

        // The Mulliken charge column of the last population analysis.
        let charge_rows = rows_following(matches.after_last(POPULATION_HEADER), |fields| {
            fields.len() >= 4
                && is_integer(fields[0])
                && is_integer(fields[1])
                && parse_fortran_f64(fields[3]).is_some()
        });

        // Seam honesty: a banner-`ok` record whose eigenvalues came from
        // the fallback and fall below the 3N-6 minimum was cut by the
        // elided middle, so it is demoted to `truncated`; `failed`
        // outcomes and header-delimited blocks are never demoted.
        let atoms = geometry.as_ref().map(|geometry| geometry.symbols.len());
        let (status, error) = match (
            status,
            error,
            !header_in_tail && !frequencies.is_empty() && frequency_table_short(frequencies.len(), atoms),
        ) {
            (RecordStatus::Ok, _, true) => (
                RecordStatus::Truncated,
                Some("the xtb frequency table may be incomplete: fewer than 3N-6 modes are visible in the windows; rerun with a larger --head-bytes/--tail-bytes window".to_string()),
            ),
            (status, error, _) => (status, error),
        };

        Record {
            status,
            error,
            program_version,
            command,
            charge,
            multiplicity,
            electronic_energy,
            geometry,
            vibrational_frequencies: if frequencies.is_empty() {
                None
            } else {
                Some(frequencies)
            },
            dipole_moment,
            mulliken_charges: if charge_rows.is_empty() {
                None
            } else {
                Some(
                    charge_rows
                        .iter()
                        .filter_map(|line| parse_fortran_f64(line.split_whitespace().nth(3)?))
                        .collect(),
                )
            },
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
    const MARKER_LINES: [(&str, usize); 10] = [
        ("     * xtb version 6.7.1 (26dd68d)", VERSION),
        (
            "          program call               : /opt/xtb marker",
            PROGRAM_CALL,
        ),
        ("molecular charge               0.0E+00 e", CHARGE),
        ("number of unpaired electrons   0.0E+00 e", UNPAIRED),
        ("| TOTAL ENERGY             -10.5 Eh    |", TOTAL_ENERGY),
        (" final structure:", FINAL_STRUCTURE),
        (
            "projected vibrational frequencies (cm-1)",
            PROJECTED_FREQUENCIES,
        ),
        ("Atomic dipole moments (in atomic units):", DIPOLE_HEADER),
        (
            "Atomic charges and shell populations (in atomic units):",
            POPULATION_HEADER,
        ),
        ("ERROR: something went wrong", ERROR_LINE),
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
