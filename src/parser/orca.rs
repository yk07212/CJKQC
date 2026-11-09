//! ORCA output files: chained driver jobs whose final job governs the
//! record — the last `FINAL SINGLE POINT ENERGY`, the final Cartesian
//! geometry, the genuine vibrational modes of the canonical
//! `VIBRATIONAL FREQUENCIES` section (trivial modes dropped, minus sign
//! on imaginary ones), and the final population, dipole, and point-group
//! analyses. Detection is the ASCII-art banner.
//!
//! (rq-0aab8d11)

use crate::parser::extract::{
    parse_fortran_f64, read_geometry_table, vibrational_modes, LengthUnits, SymbolColumn, TableSpec,
};
use crate::parser::{
    first_line_offset, frequency_table_short, parse_vector3, rows_following, FileWindows,
    LinePattern, Parser, Program,
};
use crate::record::{Record, RecordStatus};

/// ORCA output reader: `program()` is `Program::ORCA`.
///
/// (rq-1831b7fb)
pub struct OrcaParser;

/// The ASCII-art title line: the letters `O R C A` spaced apart between
/// asterisks. The test is allocation-free and rejects a non-candidate
/// line at its first field (`rq-15b1763d`).
fn is_banner(line: &str) -> bool {
    let mut tokens = line
        .split(|character: char| character == '*' || character.is_whitespace())
        .filter(|token| !token.is_empty());
    tokens.next() == Some("O")
        && tokens.next() == Some("R")
        && tokens.next() == Some("C")
        && tokens.next() == Some("A")
        && tokens.next().is_none()
}

/// The echoed input line of the INPUT FILE section: a `|`-led line whose
/// fields include the `!` keyword marker.
fn is_input_echo(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('|') && trimmed.split_whitespace().any(|field| field == "!")
}

/// The `Multiplicity Mult …` echo line.
fn is_multiplicity_line(line: &str) -> bool {
    let mut fields = line.split_whitespace();
    fields.next() == Some("Multiplicity") && fields.next() == Some("Mult")
}

/// The thermodynamics point-group line carrying the symmetry number.
fn is_point_group_line(line: &str) -> bool {
    line.contains("Point Group:") && line.contains("Symmetry Number:")
}

/// An ORCA error termination or reported error line.
fn is_orca_abort(line: &str) -> bool {
    line.contains("ORCA TERMINATED DUE TO") || line.trim_start().starts_with("ERROR")
}

/// The SCF total line, in either printed spelling: the SCF summary box
/// of newer printings (`| TOTAL ENERGY <value> Eh |`) or the section-end
/// line of older ones (`Total Energy : <value> Eh`). The field shapes
/// distinguish them from every other energy line — the `TOTAL SCF ENERGY`
/// section's `Total energy after final integration` echo (lowercase
/// `energy`, no colon) and the box neighbors never match. Non-candidates
/// are rejected on the line's leading bytes — the first field is `|` or
/// `Total` — before any field is read, and fields are iterated, never
/// collected, so the locate walk pays leading-byte cost for the window's
/// other lines.
// rq-7331508e rq-15b1763d
fn is_scf_total_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if !(trimmed.starts_with('|') || trimmed.starts_with("Total")) {
        return false;
    }
    let mut fields = line.split_whitespace();
    match (fields.next(), fields.next(), fields.next(), fields.next(), fields.next()) {
        (Some("|"), Some("TOTAL"), Some("ENERGY"), Some(value), Some("Eh")) => {
            parse_fortran_f64(value).is_some()
        }
        (Some("Total"), Some("Energy"), Some(":"), Some(value), Some("Eh")) => {
            parse_fortran_f64(value).is_some()
        }
        _ => false,
    }
}

/// The SCF total value of a matched line: the fourth whitespace field in
/// both spellings.
fn scf_total_value(line: &str) -> Option<f64> {
    parse_fortran_f64(line.split_whitespace().nth(3)?)
}

/// The locate-pattern indices of the markers ORCA prints.
///
/// (rq-15b1763d)
const VERSION: usize = 0;
const INPUT_ECHO: usize = 1;
const TOTAL_CHARGE: usize = 2;
const MULTIPLICITY: usize = 3;
const SINGLE_POINT_ENERGY: usize = 4;
const GEOMETRY_HEADER: usize = 5;
const FREQUENCIES_HEADER: usize = 6;
const DIPOLE_HEADER: usize = 7;
const MULLIKEN_HEADER: usize = 8;
const POINT_GROUP: usize = 9;
const ABORT: usize = 10;
const SCF_TOTAL: usize = 11;

/// Every marker line ORCA's parse path needs, located in one walk of
/// the windows. The index constants above key this table by position;
/// the alignment unit test pins each constant to its entry.
const PATTERNS: [LinePattern<'static>; 12] = [
    LinePattern::Contains("Program Version"),
    LinePattern::Where(&is_input_echo),
    LinePattern::Contains("Total Charge"),
    LinePattern::Where(&is_multiplicity_line),
    LinePattern::Contains("FINAL SINGLE POINT ENERGY"),
    LinePattern::TrimmedEquals("CARTESIAN COORDINATES (ANGSTROEM)"),
    LinePattern::TrimmedEquals("VIBRATIONAL FREQUENCIES"),
    LinePattern::TrimmedEquals("DIPOLE MOMENT"),
    LinePattern::TrimmedEquals("MULLIKEN ATOMIC CHARGES"),
    LinePattern::Where(&is_point_group_line),
    LinePattern::Where(&is_orca_abort),
    LinePattern::Where(&is_scf_total_line),
];

/// The command: all text after `!` in the echoed keyword line,
/// whitespace-trimmed; an empty remainder leaves the command absent.
// rq-527a66db
fn command_from_echo(line: &str) -> Option<String> {
    let position = line.find('!')?;
    let keywords = line[position + 1..].trim();
    (!keywords.is_empty()).then(|| keywords.to_string())
}

/// A frequency value: a decimal, optionally signed, optionally written
/// with a trailing `i` to mark an imaginary mode.
// rq-9d9e798e
fn frequency_value(field: &str) -> Option<f64> {
    match field.strip_suffix('i') {
        Some(imaginary) => parse_fortran_f64(imaginary).map(|value| -value.abs()),
        None => parse_fortran_f64(field),
    }
}

/// The mode value of a frequency row: a decimal, optionally signed,
/// optionally written with a trailing `i` to mark an imaginary mode.
/// `None` when the first three fields are not `<mode>: <value> cm**-1`
/// — the row-shape test, iterated field by field without collecting.
// rq-15b1763d
fn frequency_row_value(mut fields: std::str::SplitWhitespace<'_>) -> Option<f64> {
    let mode = fields.next()?;
    let value = frequency_value(fields.next()?)?;
    if !mode.ends_with(':') || fields.next() != Some("cm**-1") {
        return None;
    }
    Some(value)
}

/// The genuine mode values of the frequency rows leading `lines` (rows
/// stop the section at its first non-row), appended to `values`. The
/// span walk from a located header: bounded by the section's own end.
fn collect_frequency_rows<'a>(lines: impl Iterator<Item = &'a str>, values: &mut Vec<f64>) {
    let mut started = false;
    for line in lines {
        match frequency_row_value(line.split_whitespace()) {
            Some(value) => {
                values.push(value);
                started = true;
            }
            None if started => break,
            None => {}
        }
    }
}

/// The whole-window gather of the seam fallback: the candidate rows of a
/// window that carries no visible section header, located by one SIMD
/// literal scan of the row marker (`cm**-1`) and verified in shape on
/// the candidates only — never a field split of every line. Consecutive
/// candidates continue the section exactly as the walk's first non-row
/// break: a candidate not beginning where the previous candidate's next
/// line begins ends the section, and so does a candidate that fails the
/// row shape.
// rq-15b1763d rq-17559f33
fn scan_frequency_rows(window: &str, values: &mut Vec<f64>) {
    let mut started = false;
    let mut next_row_start: Option<usize> = None;
    for marker in crate::parser::extract::marker_lines(window, "cm**-1") {
        if started && next_row_start != Some(marker.line_start) {
            break;
        }
        match frequency_row_value(marker.line.split_whitespace()) {
            Some(value) => {
                values.push(value);
                started = true;
                next_row_start = Some(marker.next_start);
            }
            None if started => break,
            None => {}
        }
    }
}

// rq-1831b7fb
impl Parser for OrcaParser {
    fn program(&self) -> Program {
        Program::ORCA
    }

    // rq-5b9c015a rq-5f31c98f rq-15b1763d
    fn detect(&self, head: &str) -> Option<usize> {
        memchr::memchr(b'*', head.as_bytes())?;
        first_line_offset(head, is_banner)
    }

    fn parse(&self, windows: &FileWindows) -> Record {
        // One identification walk finds every marker this format reads,
        // status markers included (rq-15b1763d).
        let matches = windows.locate(&PATTERNS);

        let program_version = matches.first(VERSION).and_then(|line| {
            let mut following_version = false;
            for field in line.split_whitespace() {
                if following_version {
                    return Some(field.to_string());
                }
                following_version = field == "Version";
            }
            None
        });

        // The echoed input line of the INPUT FILE section — `|  1> ! …`
        // — carries the command; a stray `!` elsewhere is not an echo and
        // leaves the command absent.
        // rq-527a66db
        let command = matches.first(INPUT_ECHO).and_then(command_from_echo);

        // rq-eb7784d8
        let charge = matches.last(TOTAL_CHARGE).and_then(|line| {
            parse_fortran_f64(line.split_whitespace().last()?).map(|value| value.round() as i32)
        });
        let multiplicity = matches
            .last(MULTIPLICITY)
            .and_then(|line| line.split_whitespace().last()?.parse::<u32>().ok());

        // The final job's energy: the last single-point value printed.
        // rq-eb7784d8 rq-b7387503
        let electronic_energy = matches
            .last(SINGLE_POINT_ENERGY)
            .and_then(|line| parse_fortran_f64(line.split_whitespace().last()?));

        // The converged SCF total of the final job's last SCF section —
        // the Hartree–Fock energy of a correlated job, the DFT energy of
        // a density-functional one — distinct from the final single
        // point energy, which adds the correlation treatment and any
        // additive corrections on top.
        // rq-7331508e
        let scf_energy = matches.last(SCF_TOTAL).and_then(scf_total_value);

        // The final geometry: the last Ångström coordinate table.
        let geometry_rows = rows_following(matches.after_last(GEOMETRY_HEADER), |fields| {
            fields.len() == 4
                && fields[0].bytes().all(|byte| byte.is_ascii_alphabetic())
                && fields[1..4]
                    .iter()
                    .all(|field| parse_fortran_f64(field).is_some())
        });
        let geometry = read_geometry_table(
            geometry_rows.iter().copied(),
            &TableSpec {
                symbol_column: SymbolColumn::Symbol(0),
                coordinate_columns: [1, 2, 3],
                units: LengthUnits::Angstrom,
                expected_rows: None,
            },
        );

        // The genuine vibrational modes of the canonical frequency
        // section: the printed values pass through the shared trivial-mode
        // filter (near-zero translational/rotational entries dropped,
        // imaginary modes negative). The section ends at the rule before
        // the IR table. When the last header is not visible in the tail
        // the section may straddle the elided middle, so the head
        // remainder and the whole tail are both gathered.
        // rq-eb7784d8 rq-9d9e798e rq-69c48453 rq-17559f33
        let mut parsed_frequencies: Vec<f64> = Vec::new();
        let header_in_tail = matches.last_in_tail(FREQUENCIES_HEADER).is_some();
        collect_frequency_rows(
            matches.after_last(FREQUENCIES_HEADER),
            &mut parsed_frequencies,
        );
        if !header_in_tail {
            // The whole-tail fallback gathers by literal scan
            // (`rqm/parser-framework.md`, Performance Constraints).
            // rq-17559f33 rq-15b1763d
            scan_frequency_rows(&windows.tail, &mut parsed_frequencies);
        }
        // rq-c37111b1
        let frequencies = vibrational_modes(&parsed_frequencies);

        // The dipole vector in atomic units, unchanged.
        // rq-ec125e27
        let dipole_moment = matches.after_last(DIPOLE_HEADER).find_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() == 7 && fields[0] == "Total" && fields[1] == "Dipole" {
                parse_vector3([fields[4], fields[5], fields[6]])
            } else {
                None
            }
        });

        // The Mulliken charges of the last population analysis. ORCA
        // prints the symbol in a two-character field ending at the colon,
        // so a two-character symbol reads `0 Na:    0.685633` (three
        // fields) and a one-character symbol reads `1 I :   -0.797525`
        // (four fields); either spelling is a row, and the charge is the
        // row's last field.
        // rq-80fdb0f3 rq-d3ed1e6c
        let charge_rows = rows_following(matches.after_last(MULLIKEN_HEADER), |fields| {
            (fields.len() == 3 || fields.len() == 4)
                && fields
                    .last()
                    .is_some_and(|charge| parse_fortran_f64(charge).is_some())
                && match fields.len() {
                    3 => fields[1]
                        .strip_suffix(':')
                        .is_some_and(|symbol| !symbol.is_empty()),
                    4 => fields[2] == ":",
                    _ => false,
                }
        });

        // The rotational symmetry number from the thermodynamics block.
        // rq-eb7784d8 rq-69c48453
        let rotational_symmetry_number = matches
            .last(POINT_GROUP)
            .and_then(|line| line.split_whitespace().last()?.parse::<u32>().ok());

        // Status: the run's own termination banner governs it. The
        // banner test is a whole-window SIMD byte search
        // (`rqm/parser-framework.md`, Performance Constraints).
        // rq-a5fee06d rq-0f8ebb73 rq-f6a569e1 rq-80f193f5 rq-15b1763d
        let (status, error) = if memchr::memmem::find(
            windows.tail.as_bytes(),
            b"ORCA TERMINATED NORMALLY",
        )
        .is_some()
        {
            (RecordStatus::Ok, None)
        } else if matches.last_in_tail(ABORT).is_some() {
            // Provisional error signal, pending the genuine failed
            // fixture that pins ORCA's abort wording
            // (`rqm/parsers/conformance.md`).
            (
                RecordStatus::Failed,
                Some("ORCA reported an error termination".to_string()),
            )
        } else {
            (
                RecordStatus::Truncated,
                Some("the ORCA run ends before its termination banner".to_string()),
            )
        };

        // Seam honesty: a banner-`ok` record whose frequency rows came
        // from the fallback and fall below the 3N-6 minimum was cut by
        // the elided middle, so it is demoted to `truncated`; `failed`
        // outcomes and header-delimited sections are never demoted.
        let atoms = geometry.as_ref().map(|geometry| geometry.symbols.len());
        let (status, error) =
            match (status, error, !header_in_tail && !frequencies.is_empty() && frequency_table_short(frequencies.len(), atoms)) {
                (RecordStatus::Ok, _, true) => (
                    RecordStatus::Truncated,
                    Some("the ORCA frequency table may be incomplete: fewer than 3N-6 modes are visible in the windows; rerun with a larger --head-bytes/--tail-bytes window".to_string()),
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
            scf_energy,
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
                        // Every collected row's last field is its charge,
                        // already proven parseable by the row test.
                        .filter_map(|line| parse_fortran_f64(line.split_whitespace().last()?))
                        .collect(),
                )
            },
            rotational_symmetry_number,
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
    const MARKER_LINES: [(&str, usize); 12] = [
        ("Program Version 6.0.0  -   RELEASE  -", VERSION),
        ("|  1> ! wB97x-3c marker", INPUT_ECHO),
        (
            " Total Charge           Charge          ....    0",
            TOTAL_CHARGE,
        ),
        (
            " Multiplicity           Mult            ....    1",
            MULTIPLICITY,
        ),
        (
            "FINAL SINGLE POINT ENERGY       -76.402725",
            SINGLE_POINT_ENERGY,
        ),
        ("CARTESIAN COORDINATES (ANGSTROEM)", GEOMETRY_HEADER),
        ("VIBRATIONAL FREQUENCIES", FREQUENCIES_HEADER),
        ("DIPOLE MOMENT", DIPOLE_HEADER),
        ("MULLIKEN ATOMIC CHARGES", MULLIKEN_HEADER),
        ("Point Group:  C1, Symmetry Number:   1", POINT_GROUP),
        ("ORCA TERMINATED DUE TO AN ERROR", ABORT),
        (
            "| TOTAL ENERGY              -28.322836452494 Eh   |",
            SCF_TOTAL,
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
