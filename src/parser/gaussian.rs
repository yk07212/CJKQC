//! Gaussian log files: chained jobs whose final job governs the record —
//! the last `SCF Done` energy, the final job's geometry and population
//! analysis, and the frequency table. Detection is the version banner.
//! Gaussian's dipole table is in Debye and is converted to the record's
//! atomic units at extraction time.
//!
//! (rq-33106344)

use crate::parser::extract::{
    parse_fortran_f64, read_geometry_table, vibrational_modes, LengthUnits, SymbolColumn, TableSpec,
};
use crate::parser::{
    first_line_offset, frequency_table_short, is_integer, parse_vector3, rows_following,
    FileWindows, LinePattern, Parser, Program,
};
use crate::record::{Record, RecordStatus};
use crate::thermo::DEBYE_PER_E_A0;

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
/// bare version line. The test rejects a non-candidate line at its first
/// field and allocates nothing before it is a candidate (`rq-15b1763d`).
// rq-d8a0968b
fn version_from_banner(line: &str) -> Option<String> {
    let mut fields = line.split_whitespace();
    if fields.next() != Some("Gaussian") {
        return None;
    }
    let release = fields.next()?.trim_matches([',', ':']);
    if !GAUSSIAN_VERSIONS.iter().any(|name| line.contains(name)) {
        return None;
    }
    if line.contains("Revision") {
        let mut following_revision = false;
        for field in line.split_whitespace() {
            if following_revision {
                return Some(format!(
                    "Gaussian {release} Rev. {}",
                    field.trim_matches(',')
                ));
            }
            following_revision = field == "Revision";
        }
        return None;
    }
    Some(format!("Gaussian {release}"))
}

/// Whether the line is the version banner.
fn is_version_banner(line: &str) -> bool {
    version_from_banner(line).is_some()
}

/// Whether the line echoes charge and multiplicity.
fn is_charge_mult_line(line: &str) -> bool {
    line.trim_start().starts_with("Charge =") && line.contains("Multiplicity")
}

/// The route section: the located `#` line and its continuations, `#`
/// stripped and whitespace collapsed — stored verbatim as reported. The
/// echo surrounds the route with all-dash rules, so a rule (or a blank
/// line) ends the section.
// rq-d8a0968b
fn route_section<'a>(start: &str, following: impl Iterator<Item = &'a str>) -> Option<String> {
    let start = start.trim();
    let mut collected: Vec<&str> = vec![start.trim_start_matches('#')];
    for line in following {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.bytes().all(|byte| byte == b'-') {
            break;
        }
        collected.push(trimmed);
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
/// The frequency values on `Frequencies -- <v1> ...` lines of `lines`,
/// appended to `values`. The span walk from a located header: fields are
/// iterated, never collected, so the walk allocates nothing per line.
// rq-363a8a65 rq-15b1763d
fn collect_frequencies<'a>(lines: impl Iterator<Item = &'a str>, values: &mut Vec<f64>) {
    for line in lines {
        let mut fields = line.split_whitespace();
        if fields.next() == Some("Frequencies") && fields.next() == Some("--") {
            values.extend(fields.filter_map(parse_fortran_f64));
        }
    }
}

/// The whole-window gather of the seam fallback: every `Frequencies --`
/// line of a window that carries no visible geometry header, located by
/// one SIMD literal scan of the marker text and verified on the
/// candidates only — never a field split of every line.
// rq-15b1763d rq-363a8a65
fn scan_frequencies(window: &str, values: &mut Vec<f64>) {
    for marker in crate::parser::extract::marker_lines(window, "Frequencies --") {
        let mut fields = marker.line.split_whitespace();
        if fields.next() == Some("Frequencies") && fields.next() == Some("--") {
            values.extend(fields.filter_map(parse_fortran_f64));
        }
    }
}

fn charge_and_multiplicity(line: &str) -> Option<(i32, u32)> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    let charge_at = fields.iter().position(|field| *field == "Charge")? + 2;
    let mult_at = fields.iter().position(|field| *field == "Multiplicity")? + 2;
    let charge = fields.get(charge_at)?.parse::<i32>().ok()?;
    let multiplicity = fields.get(mult_at)?.parse::<u32>().ok()?;
    Some((charge, multiplicity))
}

/// The locate-pattern indices of the markers Gaussian prints.
///
/// (rq-15b1763d)
const VERSION: usize = 0;
const ROUTE: usize = 1;
const CHARGE_MULT: usize = 2;
const SCF_DONE: usize = 3;
const GEOMETRY_HEADER: usize = 4;
const NATOMS: usize = 5;
const DIPOLE_HEADER: usize = 6;
const MULLIKEN_HEADER: usize = 7;
const SYMMETRY_NUMBER: usize = 8;
const NORMAL_TERMINATION: usize = 9;
const ERROR_TERMINATION: usize = 10;

/// Every marker line Gaussian's parse path needs, located in one walk
/// of the windows. The index constants above key this table by position;
/// the alignment unit test pins each constant to its entry.
const PATTERNS: [LinePattern<'static>; 11] = [
    LinePattern::Where(&is_version_banner),
    LinePattern::TrimmedStartsWith("#"),
    LinePattern::Where(&is_charge_mult_line),
    LinePattern::Contains("SCF Done"),
    LinePattern::TrimmedStartsWith("Standard orientation:"),
    LinePattern::TrimmedStartsWith("NAtoms="),
    LinePattern::TrimmedStartsWith("Dipole moment (field-independent basis, Debye):"),
    LinePattern::TrimmedStartsWith("Mulliken charges:"),
    LinePattern::TrimmedStartsWith("Rotational symmetry number"),
    LinePattern::Contains("Normal termination of Gaussian"),
    LinePattern::Contains("Error termination"),
];

// rq-9ea94102
impl Parser for GaussianParser {
    fn program(&self) -> Program {
        Program::GAUSSIAN
    }

    // rq-799f6155 rq-9f5aa509 rq-15b1763d
    fn detect(&self, head: &str) -> Option<usize> {
        let bytes = head.as_bytes();
        if !GAUSSIAN_VERSIONS
            .iter()
            .any(|name| memchr::memmem::find(bytes, name.as_bytes()).is_some())
        {
            return None;
        }
        first_line_offset(head, |line| {
            GAUSSIAN_VERSIONS.iter().any(|name| line.contains(name))
        })
    }

    fn parse(&self, windows: &FileWindows) -> Record {
        // One identification walk finds every marker this format reads,
        // status markers included (rq-15b1763d).
        let matches = windows.locate(&PATTERNS);

        let program_version = matches.first(VERSION).and_then(version_from_banner);

        // The route echo always lives at the top of the job, inside the
        // head window: the route is read only from head matches, so a
        // `#` line in an uncovered file's tail can never fabricate a
        // command from the elided middle's tail.
        // rq-6aa61c29
        let command = matches
            .first_in_head(ROUTE)
            .and_then(|route| route_section(route, matches.after_first(ROUTE)));

        // rq-d8a0968b
        let (charge, multiplicity) = matches
            .last(CHARGE_MULT)
            .and_then(charge_and_multiplicity)
            .map(|(charge, multiplicity)| (Some(charge), Some(multiplicity)))
            .unwrap_or((None, None));

        // The final job's energy: the last converged SCF value. The SCF
        // energy is the value of the same line: for Hartree–Fock and
        // density-functional jobs the two coincide, and a post-HF job's
        // total is a different marker this parser does not read.
        // rq-d8a0968b rq-bf318b99 rq-8caf5900
        let electronic_energy = matches.last(SCF_DONE).and_then(|line| {
            let mut following_equals = false;
            for field in line.split_whitespace() {
                if following_equals {
                    return parse_fortran_f64(field);
                }
                following_equals = field == "=";
            }
            None
        });
        let scf_energy = electronic_energy;

        // The final job's geometry: the Standard orientation table reads
        // its element from the atomic-number column.
        let geometry_rows = rows_following(matches.after_last(GEOMETRY_HEADER), |fields| {
            fields.len() == 6
                && is_integer(fields[0])
                && is_integer(fields[1])
                && fields[3..6]
                    .iter()
                    .all(|field| parse_fortran_f64(field).is_some())
        });
        let expected_rows = matches
            .last(NATOMS)
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

        // Every printed frequency of the final frequency job, passed
        // through the shared trivial-mode filter like every other program
        // so the record's vibrational-mode convention holds
        // program-independently; Gaussian prints no trivial modes, so the
        // filter is inert on its tables. A leading minus marks an imaginary
        // mode. The final job is delimited by the last geometry table, so
        // a chained run with more than one frequency job contributes only
        // its last one. When that header is not visible in the tail the
        // table may straddle the elided middle, so the head remainder and
        // the whole tail are both gathered rather than trusting one
        // window.
        // rq-d8a0968b rq-c7886a80 rq-2d633569 rq-363a8a65 rq-c37111b1
        let mut parsed_frequencies: Vec<f64> = Vec::new();
        let header_in_tail = matches.last_in_tail(GEOMETRY_HEADER).is_some();
        collect_frequencies(matches.after_last(GEOMETRY_HEADER), &mut parsed_frequencies);
        if !header_in_tail {
            // The whole-tail fallback gathers by literal scan
            // (`rqm/parser-framework.md`, Performance Constraints).
            // rq-363a8a65 rq-15b1763d
            scan_frequencies(&windows.tail, &mut parsed_frequencies);
        }
        // rq-c37111b1
        let frequencies = vibrational_modes(&parsed_frequencies);

        // The ground-state dipole, converted from Debye to e·a₀.
        // rq-d8a0968b rq-58995f44
        let dipole_moment = matches.after_last(DIPOLE_HEADER).find_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() >= 6 && fields[0] == "X=" && fields[2] == "Y=" && fields[4] == "Z=" {
                let debye = parse_vector3([fields[1], fields[3], fields[5]])?;
                Some(debye.map(|value| value / DEBYE_PER_E_A0))
            } else {
                None
            }
        });

        // The Mulliken charges of the final population analysis.
        let charge_rows = rows_following(matches.after_last(MULLIKEN_HEADER), |fields| {
            fields.len() == 3
                && is_integer(fields[0])
                && fields[1].bytes().all(|byte| byte.is_ascii_alphabetic())
                && parse_fortran_f64(fields[2]).is_some()
        });

        let rotational_symmetry_number = matches.last(SYMMETRY_NUMBER).and_then(|line| {
            line.split_whitespace()
                .last()?
                .trim_end_matches('.')
                .parse::<u32>()
                .ok()
        });

        // Status: the final job's own termination line governs it. Each
        // chained job prints its own termination line, so the marker that
        // governs is the one the final job printed — the latest one in the
        // tail, which must come after the last converged SCF.
        // rq-e6338052 rq-b721514a rq-b47001c1
        let normal = matches.last_in_tail(NORMAL_TERMINATION);
        let error = matches.last_in_tail(ERROR_TERMINATION);
        let scf = matches.last_in_tail(SCF_DONE);
        let (status, error_text) = match (normal, error, scf) {
            (Some(normal), _, Some(scf)) if normal.index > scf.index => (RecordStatus::Ok, None),
            (Some(_), None, None) => (RecordStatus::Ok, None),
            (_, Some(failure), scf)
                if normal.is_none_or(|normal| failure.index > normal.index)
                    && scf.is_none_or(|scf| failure.index > scf.index) =>
            {
                (
                    RecordStatus::Failed,
                    Some(format!("Gaussian reported: {}", failure.line.trim())),
                )
            }
            _ => (
                RecordStatus::Truncated,
                Some("the final Gaussian job ends before its termination line".to_string()),
            ),
        };

        // Seam honesty: a banner-`ok` record whose frequencies came from
        // the fallback and fall below the 3N-6 minimum was cut by the
        // elided middle, so it is demoted to `truncated`. The check is
        // best-effort (frozen-coordinate jobs print fewer modes and can
        // be flagged) and never demotes `failed` outcomes or
        // header-delimited tables.
        let atoms = geometry.as_ref().map(|g| g.symbols.len()).or(expected_rows);
        let (status, error_text) =
            match (status, error_text, !header_in_tail && !frequencies.is_empty() && frequency_table_short(frequencies.len(), atoms)) {
                (RecordStatus::Ok, _, true) => (
                    RecordStatus::Truncated,
                    Some("the final Gaussian frequency table may be incomplete: fewer than 3N-6 modes are visible in the windows; rerun with a larger --head-bytes/--tail-bytes window".to_string()),
                ),
                (status, error, _) => (status, error),
            };

        Record {
            status,
            error: error_text,
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
                        .filter_map(|line| parse_fortran_f64(line.split_whitespace().nth(2)?))
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
    const MARKER_LINES: [(&str, usize); 11] = [
        (" Gaussian 16, Revision B.01,", VERSION),
        (" # b3lyp route marker", ROUTE),
        (" Charge =  0 Multiplicity = 1", CHARGE_MULT),
        (" SCF Done:  E(RB97) =  -1.0 marker", SCF_DONE),
        (
            "                         Standard orientation:",
            GEOMETRY_HEADER,
        ),
        (" NAtoms=    3 NActive=    3", NATOMS),
        (
            " Dipole moment (field-independent basis, Debye):",
            DIPOLE_HEADER,
        ),
        (" Mulliken charges:", MULLIKEN_HEADER),
        (" Rotational symmetry number  1.", SYMMETRY_NUMBER),
        (
            " Normal termination of Gaussian 16 at marker",
            NORMAL_TERMINATION,
        ),
        (" Error termination via LKV3", ERROR_TERMINATION),
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
