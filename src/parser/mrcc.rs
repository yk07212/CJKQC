//! MRCC energy-program outputs: the echoed input deck (version, method,
//! charge, spin), the canonical bohr-unit geometry table, and the final
//! energy summary whose last total-energy line — the program's best
//! answer, MP2 corrections included for local-correlation runs — is the
//! record's electronic energy. Detection is the opening banner.
//!
//! (rq-547a8fd6)

use crate::parser::extract::{
    parse_fortran_f64, read_geometry_table, LengthUnits, SymbolColumn, TableSpec,
};
use crate::parser::{is_integer, rows_after_last, FileWindows, Parser, Program};
use crate::record::{Record, RecordStatus};

/// MRCC output reader: `program()` is `Program::MRCC`.
///
/// (rq-d72bb4c5)
pub struct MrccParser;

/// `calc=<x>` with optional `/basis=<y>`, from the original-case input
/// echo (the lowercased `Keywords:` block comes later).
// rq-b8a853f8
fn method_from_echo(head: &str) -> Option<String> {
    let mut calc: Option<String> = None;
    let mut basis: Option<String> = None;
    for line in head.lines() {
        if line.trim() == "Keywords:" {
            break;
        }
        let trimmed = line.trim();
        if calc.is_none() && trimmed.starts_with("calc=") {
            calc = Some(trimmed["calc=".len()..].to_string());
        }
        if basis.is_none() && trimmed.starts_with("basis=") {
            basis = Some(trimmed["basis=".len()..].to_string());
        }
    }
    let calc = calc?;
    match basis.filter(|value| value != "none") {
        Some(basis) => Some(format!("{calc}/{basis}")),
        None => Some(calc),
    }
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

// rq-d72bb4c5
impl Parser for MrccParser {
    fn program(&self) -> Program {
        Program::MRCC
    }

    // rq-33ab6159 rq-8890cc1d
    fn detect(&self, head: &str) -> bool {
        head.lines()
            .any(|line| line.contains("MRCC program system"))
    }

    fn parse(&self, windows: &FileWindows) -> Record {
        let program_version = windows
            .first_line_where(|line| line.trim_start().starts_with("Version:"))
            .and_then(|line| line.split_whitespace().nth(1).map(String::from));

        // rq-de12aef8
        let method = method_from_echo(&windows.head);

        let charge = windows
            .first_line_where(|line| {
                let fields: Vec<&str> = line.split_whitespace().collect();
                fields.first() == Some(&"Charge:") && fields.len() == 2
            })
            .and_then(|line| line.split_whitespace().nth(1)?.parse::<i32>().ok());

        let multiplicity = windows
            .first_line_where(|line| line.trim_start().starts_with("Spin multiplicity:"))
            .and_then(|line| line.split_whitespace().last()?.parse::<u32>().ok());

        // The canonical geometry table, printed in bohr.
        // rq-de12aef8 rq-fd767996
        let geometry_rows = rows_after_last(
            windows,
            |line| {
                line.trim_start()
                    .starts_with("Cartesian coordinates [bohr]")
            },
            |fields| {
                fields.len() == 7
                    && is_integer(fields[0])
                    && fields[1].bytes().all(|byte| byte.is_ascii_alphabetic())
                    && fields[2..5]
                        .iter()
                        .all(|field| parse_fortran_f64(field).is_some())
            },
        );
        let expected_rows = windows
            .first_line_where(|line| line.trim_start().starts_with("Number of atoms:"))
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
        let electronic_energy = windows
            .tail
            .lines()
            .filter_map(total_energy_line)
            .next_back()
            .or_else(|| {
                windows
                    .head
                    .lines()
                    .filter_map(total_energy_line)
                    .next_back()
            });

        // Status: the run's own termination banner governs it.
        // rq-be38168f rq-91435a73
        let (status, error) = if windows.tail.contains("Normal termination of mrcc") {
            (RecordStatus::Ok, None)
        } else if windows.tail.lines().any(|line| {
            let trimmed = line.trim();
            trimmed.starts_with("Aborting") || trimmed.contains("Error termination")
        }) {
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
            method,
            charge,
            multiplicity,
            electronic_energy,
            geometry,
            ..Record::default()
        }
    }
}
