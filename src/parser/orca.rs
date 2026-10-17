//! ORCA output files: chained driver jobs whose final job governs the
//! record — the last `FINAL SINGLE POINT ENERGY`, the final Cartesian
//! geometry, the canonical `VIBRATIONAL FREQUENCIES` section (all 3N
//! modes, minus sign on imaginary ones), and the final population, dipole,
//! and point-group analyses. Detection is the ASCII-art banner.
//!
//! (rq-0aab8d11)

use crate::parser::extract::{
    parse_fortran_f64, read_geometry_table, LengthUnits, SymbolColumn, TableSpec,
};
use crate::parser::{parse_vector3, rows_after_last, FileWindows, Parser, Program};
use crate::record::{Record, RecordStatus};

/// ORCA output reader: `program()` is `Program::Orca`.
///
/// (rq-1831b7fb)
pub struct OrcaParser;

/// The ASCII-art title line: the letters `O R C A` spaced apart between
/// asterisks.
fn is_banner(line: &str) -> bool {
    line.replace('*', " ")
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ")
        == "O R C A"
}

/// The method keyword: the first token after `!` in the echoed input line.
// rq-eb7784d8
fn method_from_echo(line: &str) -> Option<String> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    let position = fields.iter().position(|field| *field == "!")?;
    fields.get(position + 1).map(|token| (*token).to_string())
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

fn is_frequency_row(fields: &[&str]) -> bool {
    fields.len() >= 3
        && fields[0].ends_with(':')
        && fields[2] == "cm**-1"
        && frequency_value(fields[1]).is_some()
}

// rq-1831b7fb
impl Parser for OrcaParser {
    fn program(&self) -> Program {
        Program::Orca
    }

    // rq-5b9c015a rq-5f31c98f
    fn detect(&self, head: &str) -> bool {
        head.lines().any(is_banner)
    }

    fn parse(&self, windows: &FileWindows) -> Record {
        let program_version = windows
            .first_line_where(|line| line.contains("Program Version"))
            .and_then(|line| {
                let fields: Vec<&str> = line.split_whitespace().collect();
                let position = fields.iter().position(|field| *field == "Version")?;
                Some((*fields.get(position + 1)?).to_string())
            });

        // The echoed input line of the INPUT FILE section — `|  1> ! …`
        // — carries the method; a stray `!` elsewhere is not an echo and
        // leaves the method absent.
        // rq-527a66db
        let method = windows
            .first_line_where(|line| {
                let trimmed = line.trim_start();
                trimmed.starts_with('|') && trimmed.split_whitespace().any(|field| field == "!")
            })
            .and_then(method_from_echo);

        // rq-eb7784d8
        let charge = windows
            .last_line_where(|line| line.contains("Total Charge"))
            .and_then(|line| {
                parse_fortran_f64(line.split_whitespace().last()?).map(|value| value.round() as i32)
            });
        let multiplicity = windows
            .last_line_where(|line| {
                let fields: Vec<&str> = line.split_whitespace().collect();
                fields.first() == Some(&"Multiplicity") && fields.get(1) == Some(&"Mult")
            })
            .and_then(|line| line.split_whitespace().last()?.parse::<u32>().ok());

        // The final job's energy: the last single-point value printed.
        // rq-eb7784d8 rq-b7387503
        let electronic_energy = windows
            .last_line_where(|line| line.contains("FINAL SINGLE POINT ENERGY"))
            .and_then(|line| parse_fortran_f64(line.split_whitespace().last()?));

        // The final geometry: the last Ångström coordinate table.
        let geometry_rows = rows_after_last(
            windows,
            |line| line.trim() == "CARTESIAN COORDINATES (ANGSTROEM)",
            |fields| {
                fields.len() == 4
                    && fields[0].bytes().all(|byte| byte.is_ascii_alphabetic())
                    && fields[1..4]
                        .iter()
                        .all(|field| parse_fortran_f64(field).is_some())
            },
        );
        let geometry = read_geometry_table(
            geometry_rows.iter().copied(),
            &TableSpec {
                symbol_column: SymbolColumn::Symbol(0),
                coordinate_columns: [1, 2, 3],
                units: LengthUnits::Angstrom,
                expected_rows: None,
            },
        );

        // All 3N modes of the canonical frequency section, as printed —
        // the near-zero translational modes included, imaginary modes
        // negative. The section ends at the rule before the IR table.
        // rq-eb7784d8 rq-9d9e798e rq-69c48453
        let mut frequencies: Vec<f64> = Vec::new();
        let mut started = false;
        for line in windows.after_last_line_where(|line| line.trim() == "VIBRATIONAL FREQUENCIES") {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if is_frequency_row(&fields) {
                frequencies.push(frequency_value(fields[1]).unwrap_or_default());
                started = true;
            } else if started {
                break;
            }
        }

        // The dipole vector in atomic units, unchanged.
        // rq-ec125e27
        let dipole_moment = windows
            .after_last_line_where(|line| line.trim() == "DIPOLE MOMENT")
            .find_map(|line| {
                let fields: Vec<&str> = line.split_whitespace().collect();
                if fields.len() == 7 && fields[0] == "Total" && fields[1] == "Dipole" {
                    parse_vector3([fields[4], fields[5], fields[6]])
                } else {
                    None
                }
            });

        // The Mulliken charges of the last population analysis.
        let charge_rows = rows_after_last(
            windows,
            |line| line.trim() == "MULLIKEN ATOMIC CHARGES",
            |fields| {
                fields.len() == 4 && fields[2] == ":" && parse_fortran_f64(fields[3]).is_some()
            },
        );

        // The rotational symmetry number from the thermodynamics block.
        // rq-eb7784d8 rq-69c48453
        let rotational_symmetry_number = windows
            .last_line_where(|line| {
                line.contains("Point Group:") && line.contains("Symmetry Number:")
            })
            .and_then(|line| line.split_whitespace().last()?.parse::<u32>().ok());

        // Status: the run's own termination banner governs it.
        // rq-a5fee06d rq-0f8ebb73
        let (status, error) = if windows.tail.contains("ORCA TERMINATED NORMALLY") {
            (RecordStatus::Ok, None)
        } else if windows.tail.lines().any(|line| {
            line.contains("ORCA TERMINATED DUE TO") || line.trim_start().starts_with("ERROR")
        }) {
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

        Record {
            status,
            error,
            program_version,
            method,
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
            rotational_symmetry_number,
            ..Record::default()
        }
    }
}
