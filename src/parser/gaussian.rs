//! Gaussian log files: chained jobs whose final job governs the record —
//! the last `SCF Done` energy, the final job's geometry and population
//! analysis, and the frequency table. Detection is the version banner.
//! Gaussian's dipole table is in Debye and is converted to the record's
//! atomic units at extraction time.
//!
//! (rq-33106344)

use crate::parser::extract::{
    parse_fortran_f64, read_geometry_table, LengthUnits, SymbolColumn, TableSpec,
};
use crate::parser::{is_integer, parse_vector3, rows_after_last, FileWindows, Parser, Program};
use crate::record::{Record, RecordStatus};

/// Debye per elementary-charge bohr (CODATA), the conversion Gaussian's
/// dipole table needs to reach the record's atomic units.
const DEBYE_PER_E_A0: f64 = 2.541746473;

/// Gaussian log reader: `program()` is `Program::GAUSSIAN`.
///
/// (rq-9ea94102)
pub struct GaussianParser;

/// The version names a detectable Gaussian release line may carry.
const GAUSSIAN_VERSIONS: [&str; 5] = [
    "Gaussian 03",
    "Gaussian 09",
    "Gaussian 11",
    "Gaussian 13",
    "Gaussian 16",
];

/// `Gaussian <n> Rev. <rev>` from the banner line, `Gaussian <n>` from a
/// bare version line.
// rq-d8a0968b
fn version_from_banner(line: &str) -> Option<String> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() >= 2
        && fields[0] == "Gaussian"
        && GAUSSIAN_VERSIONS.iter().any(|name| line.contains(name))
    {
        let release = fields[1].trim_matches([',', ':']);
        if line.contains("Revision") {
            let position = fields.iter().position(|field| *field == "Revision")?;
            let revision = fields.get(position + 1)?.trim_matches(',');
            return Some(format!("Gaussian {release} Rev. {revision}"));
        }
        return Some(format!("Gaussian {release}"));
    }
    None
}

/// The route section: the `#` line and its continuations, `#` stripped
/// and whitespace collapsed — stored verbatim as reported. The echo
/// surrounds the route with all-dash rules, so a rule (or a blank line)
/// ends the section.
// rq-d8a0968b
fn route_section(head: &str) -> Option<String> {
    let mut collected: Vec<&str> = Vec::new();
    let mut in_route = false;
    for line in head.lines() {
        let trimmed = line.trim();
        if !in_route {
            if trimmed.starts_with('#') {
                in_route = true;
                collected.push(trimmed.trim_start_matches('#'));
            }
        } else if trimmed.is_empty() || trimmed.bytes().all(|byte| byte == b'-') {
            break;
        } else {
            collected.push(trimmed);
        }
    }
    if collected.is_empty() {
        return None;
    }
    Some(
        collected
            .iter()
            .flat_map(|part| part.split_whitespace())
            .collect::<Vec<&str>>()
            .join(" "),
    )
}

/// The charge and multiplicity from a `Charge = <c> Multiplicity = <m>`
/// echo line.
// rq-d8a0968b
fn charge_and_multiplicity(line: &str) -> Option<(i32, u32)> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    let charge_at = fields.iter().position(|field| *field == "Charge")? + 2;
    let mult_at = fields.iter().position(|field| *field == "Multiplicity")? + 2;
    let charge = fields.get(charge_at)?.parse::<i32>().ok()?;
    let multiplicity = fields.get(mult_at)?.parse::<u32>().ok()?;
    Some((charge, multiplicity))
}

// rq-9ea94102
impl Parser for GaussianParser {
    fn program(&self) -> Program {
        Program::GAUSSIAN
    }

    // rq-799f6155 rq-9f5aa509
    fn detect(&self, head: &str) -> bool {
        head.lines()
            .any(|line| GAUSSIAN_VERSIONS.iter().any(|name| line.contains(name)))
    }

    fn parse(&self, windows: &FileWindows) -> Record {
        let program_version = windows
            .first_line_where(|line| version_from_banner(line).is_some())
            .and_then(version_from_banner);

        let method = route_section(&windows.head);

        // rq-d8a0968b
        let (charge, multiplicity) = windows
            .last_line_where(|line| {
                line.trim_start().starts_with("Charge =") && line.contains("Multiplicity")
            })
            .and_then(charge_and_multiplicity)
            .map(|(charge, multiplicity)| (Some(charge), Some(multiplicity)))
            .unwrap_or((None, None));

        // The final job's energy: the last converged SCF value.
        // rq-d8a0968b rq-bf318b99
        let electronic_energy = windows
            .last_line_where(|line| line.contains("SCF Done"))
            .and_then(|line| {
                let fields: Vec<&str> = line.split_whitespace().collect();
                let position = fields.iter().position(|field| *field == "=")?;
                parse_fortran_f64(fields.get(position + 1)?)
            });

        // The final job's geometry: the Standard orientation table reads
        // its element from the atomic-number column.
        let geometry_rows = rows_after_last(
            windows,
            |line| line.trim_start().starts_with("Standard orientation:"),
            |fields| {
                fields.len() == 6
                    && is_integer(fields[0])
                    && is_integer(fields[1])
                    && fields[3..6]
                        .iter()
                        .all(|field| parse_fortran_f64(field).is_some())
            },
        );
        let expected_rows = windows
            .last_line_where(|line| line.trim_start().starts_with("NAtoms="))
            .and_then(|line| line.split_whitespace().nth(1)?.parse::<usize>().ok());
        let geometry = if geometry_rows.is_empty() {
            None
        } else {
            read_geometry_table(
                geometry_rows.iter().copied(),
                &TableSpec {
                    symbol_column: SymbolColumn::AtomicNumber(1),
                    coordinate_columns: [3, 4, 5],
                    units: LengthUnits::Angstrom,
                    expected_rows,
                },
            )
        };

        // Every printed frequency of the final frequency job; a leading
        // minus marks an imaginary mode. The final job is delimited by
        // the last geometry table in the tail, so a chained run with more
        // than one frequency job contributes only its last one.
        // rq-d8a0968b rq-c7886a80 rq-2d633569 rq-363a8a65
        let tail_lines: Vec<&str> = windows.tail.lines().collect();
        let job_start = tail_lines
            .iter()
            .rposition(|line| line.trim_start().starts_with("Standard orientation:"))
            .map_or(0, |position| position + 1);
        let mut frequencies: Vec<f64> = Vec::new();
        for line in &tail_lines[job_start..] {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() >= 3 && fields[0] == "Frequencies" && fields[1] == "--" {
                frequencies.extend(
                    fields[2..]
                        .iter()
                        .filter_map(|value| parse_fortran_f64(value)),
                );
            }
        }

        // The ground-state dipole, converted from Debye to e·a₀.
        // rq-d8a0968b rq-58995f44
        let dipole_moment = windows
            .after_last_line_where(|line| {
                line.trim_start()
                    .starts_with("Dipole moment (field-independent basis, Debye):")
            })
            .find_map(|line| {
                let fields: Vec<&str> = line.split_whitespace().collect();
                if fields.len() >= 6 && fields[0] == "X=" && fields[2] == "Y=" && fields[4] == "Z="
                {
                    let debye = parse_vector3([fields[1], fields[3], fields[5]])?;
                    Some(debye.map(|value| value / DEBYE_PER_E_A0))
                } else {
                    None
                }
            });

        // The Mulliken charges of the final population analysis.
        let charge_rows = rows_after_last(
            windows,
            |line| line.trim_start().starts_with("Mulliken charges:"),
            |fields| {
                fields.len() == 3
                    && is_integer(fields[0])
                    && fields[1].bytes().all(|byte| byte.is_ascii_alphabetic())
                    && parse_fortran_f64(fields[2]).is_some()
            },
        );

        let rotational_symmetry_number = windows
            .last_line_where(|line| line.trim_start().starts_with("Rotational symmetry number"))
            .and_then(|line| {
                line.split_whitespace()
                    .last()?
                    .trim_end_matches('.')
                    .parse::<u32>()
                    .ok()
            });

        // Status: the final job's own termination line governs it.
        // rq-e6338052 rq-b721514a
        // Status: each chained job prints its own termination line, so
        // the marker that governs is the one the final job printed — the
        // latest one, which must come after the last converged SCF.
        // rq-e6338052 rq-b721514a
        let tail: Vec<&str> = windows.tail.lines().collect();
        let last_index = |needle: &str| tail.iter().rposition(|line| line.contains(needle));
        let normal = last_index("Normal termination of Gaussian");
        let error = last_index("Error termination");
        let scf = last_index("SCF Done");
        let (status, error_text) = match (normal, error, scf) {
            (Some(normal), _, Some(scf)) if normal > scf => (RecordStatus::Ok, None),
            (Some(_), None, None) => (RecordStatus::Ok, None),
            (_, Some(failure), scf)
                if normal.is_none_or(|normal| failure > normal)
                    && scf.is_none_or(|scf| failure > scf) =>
            {
                (
                    RecordStatus::Failed,
                    Some(format!("Gaussian reported: {}", tail[failure].trim())),
                )
            }
            _ => (
                RecordStatus::Truncated,
                Some("the final Gaussian job ends before its termination line".to_string()),
            ),
        };

        Record {
            status,
            error: error_text,
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
                        .filter_map(|line| parse_fortran_f64(line.split_whitespace().nth(2)?))
                        .collect(),
                )
            },
            rotational_symmetry_number,
            ..Record::default()
        }
    }
}
