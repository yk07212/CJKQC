//! xtb (GFN-xTB family) stdout logs: the run's final `TOTAL ENERGY`, the
//! optimized structure, the Hessian frequencies, and the population and
//! dipole analysis of the last single point. Detection is the version
//! banner; every observable comes from the last visible occurrence of its
//! marker block. The normal-exit banner governs status.
//!
//! (rq-cd5bd6c0)

use crate::parser::extract::{
    parse_fortran_f64, read_geometry_table, LengthUnits, SymbolColumn, TableSpec,
};
use crate::parser::{
    first_line_offset, is_integer, parse_vector3, rows_after_last, FileWindows, Parser, Program,
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

/// The method named by the program-call line's flags; no method flag is
/// the xtb 6 default.
// rq-1fb0a352
fn method_from_call(line: &str) -> String {
    for flag in [
        ("--gxtb", "g-xTB"),
        ("--gfno", "GFN0-xTB"),
        ("--gfn1", "GFN1-xTB"),
        ("--gfnff", "GFN-FF"),
        ("--gfnn", "GFN2-xTB"),
        ("--gfn2", "GFN2-xTB"),
    ] {
        if line.split_whitespace().any(|token| token == flag.0) {
            return flag.1.to_string();
        }
    }
    "GFN2-xTB".to_string()
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
fn last_numeric_line(windows: &FileWindows, marker: &str, words_in_label: usize) -> Option<f64> {
    windows
        .last_line_where(|line| line.contains(marker))
        .and_then(|line| parse_fortran_f64(line.split_whitespace().nth(words_in_label)?))
}

// rq-597bef47
impl Parser for XtbParser {
    fn program(&self) -> Program {
        Program::XTB
    }

    // rq-35c8e2c9 rq-0bb4b64f
    fn detect(&self, head: &str) -> Option<usize> {
        first_line_offset(head, |line| line.contains("xtb version"))
    }

    fn parse(&self, windows: &FileWindows) -> Record {
        // Status: the run's own end and error reporting governs it.
        // rq-ce1b9305 rq-62e1382e
        let (status, error) = if windows.tail.contains("finished run") {
            (RecordStatus::Ok, None)
        } else if windows
            .tail
            .lines()
            .any(|line| line.trim_start().starts_with("ERROR"))
        {
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

        let program_version = windows
            .first_line_where(|line| line.contains("xtb version"))
            .and_then(version_token);

        let method = windows
            .last_line_where(|line| line.trim_start().starts_with("program call"))
            .map(method_from_call);

        // rq-68850338
        let charge =
            last_numeric_line(windows, "molecular charge", 2).map(|value| value.round() as i32);
        let multiplicity = last_numeric_line(windows, "number of unpaired electrons", 4)
            .map(|value| value.round() as u32 + 1);

        let electronic_energy = windows
            .last_line_where(|line| line.contains("TOTAL ENERGY"))
            .and_then(|line| boxed_total_energy(&line.split_whitespace().collect::<Vec<&str>>()));

        // The optimized structure: the `final structure:` block holds an
        // xyz-style table behind its rule and count lines.
        // rq-68850338 rq-afe58819
        let structure_rows = rows_after_last(
            windows,
            |line| line.trim() == "final structure:",
            |fields| {
                fields.len() >= 4
                    && fields[0].bytes().all(|byte| byte.is_ascii_alphabetic())
                    && fields[1..4]
                        .iter()
                        .all(|field| parse_fortran_f64(field).is_some())
            },
        );
        let geometry = read_geometry_table(
            structure_rows.iter().copied(),
            &TableSpec {
                symbol_column: SymbolColumn::Symbol(0),
                coordinate_columns: [1, 2, 3],
                units: LengthUnits::Angstrom,
                expected_rows: None,
            },
        );

        // The projected frequencies printed after the numerical Hessian;
        // values keep their printed sign (a minus marks imaginary).
        // rq-68850338 rq-5882d984
        let mut frequencies: Vec<f64> = Vec::new();
        let mut started = false;
        for line in windows.after_last_line_where(|line| {
            line.trim_start()
                .starts_with("projected vibrational frequencies")
        }) {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.first() == Some(&"eigval") {
                frequencies.extend(
                    fields
                        .iter()
                        .skip(2)
                        .filter_map(|value| parse_fortran_f64(value)),
                );
                started = true;
            } else if started {
                break;
            }
        }

        // The molecule's dipole from the last property printout, already
        // in atomic units.
        let dipole_moment = windows
            .after_last_line_where(|line| line.trim() == "Atomic dipole moments (in atomic units):")
            .find_map(|line| {
                let fields: Vec<&str> = line.split_whitespace().collect();
                if fields.first() == Some(&"total") && fields.len() == 4 {
                    parse_vector3([fields[1], fields[2], fields[3]])
                } else {
                    None
                }
            });

        // The Mulliken charge column of the last population analysis.
        let charge_rows = rows_after_last(
            windows,
            |line| {
                line.trim_start()
                    .starts_with("Atomic charges and shell populations")
            },
            |fields| {
                fields.len() >= 4
                    && is_integer(fields[0])
                    && is_integer(fields[1])
                    && parse_fortran_f64(fields[3]).is_some()
            },
        );

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
            ..Record::default()
        }
    }
}
