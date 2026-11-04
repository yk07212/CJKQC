//! Extrapolation corrections (`rqm/extrapolation.md`): the invocation
//! grammar of `--extrapolate`, the two-point complete-basis-set and
//! CPS/LAF blend formulas, and the pure pass that combines same-named
//! records from source databases into each run record's correction
//! object. The stored `electronic_energy` is never altered; the
//! correlation-like residual `electronic_energy − scf_energy` is derived
//! on demand and never stored.
//!
//! (rq-b0a418c4)

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::path::PathBuf;

use crate::record::{Correction, Record, RecordStatus};

/// One parsed `--extrapolate` invocation.
///
/// (rq-2b4ba772)
#[derive(Debug, Clone, PartialEq)]
pub struct ExtrapolationSpec {
    /// The mode and its parameters, in JKextrapolate's order.
    pub mode: ExtrapMode,
}

/// The three extrapolation forms.
///
/// (rq-e4c551ee)
#[derive(Debug, Clone, PartialEq)]
pub enum ExtrapMode {
    /// Two-point basis-set extrapolation over two databases: the lower
    /// cardinal number's single points in `low`, the next cardinal
    /// number's in `high` (https://doi.org/10.1021/ct100396y).
    TwoPoint {
        /// The lower-cardinality database.
        low: PathBuf,
        /// The higher-cardinality database.
        high: PathBuf,
        /// The lower cardinal number of the pair, at least 1.
        cardinal: u32,
        /// The SCF exponent, above zero.
        alpha: f64,
        /// The correlation exponent, above zero.
        beta: f64,
    },
    /// The CPS/LAF settings blend over two databases that differ in
    /// calculation settings at fixed basis.
    SettingsBlend {
        /// The low-settings database.
        low: PathBuf,
        /// The high-settings database.
        high: PathBuf,
        /// The blend fraction, finite.
        fraction: f64,
    },
    /// The combined form: one two-point extrapolation per settings level
    /// over comma-joined path pairs, then the blend. `low_pair` holds the
    /// low basis at (low settings, high settings), `high_pair` the high
    /// basis in the same order.
    TwoPointBlended {
        /// The low basis's pair: (low settings, high settings).
        low_pair: (PathBuf, PathBuf),
        /// The high basis's pair: (low settings, high settings).
        high_pair: (PathBuf, PathBuf),
        /// The lower cardinal number of each basis pair, at least 1.
        cardinal: u32,
        /// The SCF exponent, above zero.
        alpha: f64,
        /// The correlation exponent, above zero.
        beta: f64,
        /// The blend fraction, finite.
        fraction: f64,
    },
}

impl ExtrapolationSpec {
    /// Whether the mode extrapolates over bases and so demands a present
    /// `scf_energy` from every source candidate (`rqm/extrapolation.md`
    /// Candidates and Matching). CLI support alongside `skip_reason`.
    pub fn needs_scf(&self) -> bool {
        matches!(
            self.mode,
            ExtrapMode::TwoPoint { .. } | ExtrapMode::TwoPointBlended { .. }
        )
    }

    /// The mode's `kind` token (`rqm/extrapolation.md` The Attached
    /// Correction).
    fn kind(&self) -> &'static str {
        match self.mode {
            ExtrapMode::TwoPoint { .. } => "cbs",
            ExtrapMode::SettingsBlend { .. } => "cps",
            ExtrapMode::TwoPointBlended { .. } => "cbs-cps",
        }
    }

    /// How many source databases the mode combines.
    fn source_count(&self) -> usize {
        match &self.mode {
            ExtrapMode::TwoPoint { .. } | ExtrapMode::SettingsBlend { .. } => 2,
            ExtrapMode::TwoPointBlended { .. } => 4,
        }
    }
}

/// What an extrapolation pass did: the enriched records plus the counts,
/// mirroring `Attachment` (`rqm/correction.md`).
///
/// (rq-60ca0795)
#[derive(Debug, Clone, PartialEq)]
pub struct Extrapolation {
    /// Enriched copies of the input, in input order; the input slice is
    /// never mutated.
    pub records: Vec<Record>,
    /// Records that received an extrapolated correction.
    pub attached: usize,
    /// Rejected source candidates.
    pub skipped: usize,
    /// Run records whose name is not fully covered.
    pub uncovered: usize,
    /// Source candidates no run record names.
    pub unmatched: usize,
}

/// Why an extrapolation pass fails.
///
/// (rq-ae61b293)
#[derive(Debug, Clone, PartialEq)]
pub enum ExtrapolationError {
    /// The source slice list does not hold the number of databases the
    /// mode combines.
    SourceCount {
        /// The number of databases the mode combines.
        expected: usize,
        /// The number of source slices supplied.
        actual: usize,
    },
    /// Two candidates share the name in the source at index `source`.
    DuplicateName {
        /// The source's index in the invocation's contributor order.
        source: usize,
        /// The name two candidates share.
        name: String,
    },
    /// Two contributors of one name carry different present programs.
    ProgramMismatch {
        /// The name whose contributors disagree.
        name: String,
        /// The first contributor's source index.
        source_a: usize,
        /// The second contributor's source index.
        source_b: usize,
        /// The first contributor's program token.
        program_a: String,
        /// The second contributor's program token.
        program_b: String,
    },
    /// Two contributors of one name carry different present program
    /// versions.
    VersionMismatch {
        /// The name whose contributors disagree.
        name: String,
        /// The first contributor's source index.
        source_a: usize,
        /// The second contributor's source index.
        source_b: usize,
        /// The first contributor's version string.
        version_a: String,
        /// The second contributor's version string.
        version_b: String,
    },
    /// Two contributors of one name carry exactly equal electronic
    /// energies — the same calculation entered the sources twice.
    DuplicateEnergy {
        /// The name whose contributors repeat a calculation.
        name: String,
        /// The first contributor's source index.
        source_a: usize,
        /// The second contributor's source index.
        source_b: usize,
    },
}

impl fmt::Display for ExtrapolationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExtrapolationError::SourceCount { expected, actual } => write!(
                f,
                "extrapolation needs {expected} source databases; got {actual}"
            ),
            ExtrapolationError::DuplicateName { source, name } => write!(
                f,
                "extrapolation source {source} holds multiple records named \"{name}\""
            ),
            ExtrapolationError::ProgramMismatch {
                name,
                source_a,
                source_b,
                program_a,
                program_b,
            } => write!(
                f,
                "extrapolation contributors for \"{name}\" mix programs: \
                 {program_a} (source {source_a}) and {program_b} (source {source_b})"
            ),
            ExtrapolationError::VersionMismatch {
                name,
                source_a,
                source_b,
                version_a,
                version_b,
            } => write!(
                f,
                "extrapolation contributors for \"{name}\" mix program versions: \
                 {version_a} (source {source_a}) and {version_b} (source {source_b})"
            ),
            ExtrapolationError::DuplicateEnergy {
                name,
                source_a,
                source_b,
            } => write!(
                f,
                "extrapolation contributors for \"{name}\" carry exactly equal \
                 electronic energies (sources {source_a} and {source_b})"
            ),
        }
    }
}

impl std::error::Error for ExtrapolationError {}

/// The invocation grammar and parameter validation (`rqm/extrapolation.md`
/// The Invocation): the mode token leads, the parameters follow in
/// JKextrapolate's order, and every shape or bound violation returns the
/// usage message the CLI prints and exits 2 on.
///
/// (rq-52f7d9a4)
pub fn parse_extrapolation(values: &[String]) -> Result<ExtrapolationSpec, String> {
    let Some((mode, rest)) = values.split_first() else {
        return Err(
            "extrapolation needs a mode token: 2p LOW HIGH N ALPHA BETA, \
             2s LOW HIGH F, or 2p LOWPAIR HIGHPAIR N ALPHA BETA F"
                .to_string(),
        );
    };
    match mode.as_str() {
        "2p" => match rest.len() {
            5 => {
                single_path(mode, &rest[0])?;
                single_path(mode, &rest[1])?;
                let (cardinal, alpha, beta) = parameters(mode, &rest[2..])?;
                Ok(ExtrapolationSpec {
                    mode: ExtrapMode::TwoPoint {
                        low: PathBuf::from(&rest[0]),
                        high: PathBuf::from(&rest[1]),
                        cardinal,
                        alpha,
                        beta,
                    },
                })
            }
            6 => {
                let low_pair = pair(mode, &rest[0])?;
                let high_pair = pair(mode, &rest[1])?;
                let (cardinal, alpha, beta) = parameters(mode, &rest[2..5])?;
                let fraction = fraction(mode, &rest[5])?;
                Ok(ExtrapolationSpec {
                    mode: ExtrapMode::TwoPointBlended {
                        low_pair,
                        high_pair,
                        cardinal,
                        alpha,
                        beta,
                        fraction,
                    },
                })
            }
            _ => Err(format!(
                "extrapolation mode \"2p\" expects 5 values \
                 (2p LOW HIGH N ALPHA BETA) or 6 values \
                 (2p LOWPAIR HIGHPAIR N ALPHA BETA F); got {}",
                rest.len()
            )),
        },
        "2s" => {
            if rest.len() != 3 {
                return Err(format!(
                    "extrapolation mode \"2s\" expects 3 values (2s LOW HIGH F); got {}",
                    rest.len()
                ));
            }
            single_path(mode, &rest[0])?;
            single_path(mode, &rest[1])?;
            let fraction = fraction(mode, &rest[2])?;
            Ok(ExtrapolationSpec {
                mode: ExtrapMode::SettingsBlend {
                    low: PathBuf::from(&rest[0]),
                    high: PathBuf::from(&rest[1]),
                    fraction,
                },
            })
        }
        other => Err(format!(
            "unknown extrapolation mode \"{other}\"; expected \
             \"2p LOW HIGH N ALPHA BETA\", \"2s LOW HIGH F\", or \
             \"2p LOWPAIR HIGHPAIR N ALPHA BETA F\""
        )),
    }
}

/// A single path: no comma-joined group where one path is required.
fn single_path(mode: &str, value: &str) -> Result<(), String> {
    if value.contains(',') {
        Err(format!(
            "extrapolation mode \"{mode}\" takes single paths without commas; \
             comma-joined pairs belong to the combined form \
             (2p LOWPAIR HIGHPAIR N ALPHA BETA F)"
        ))
    } else {
        Ok(())
    }
}

/// A comma-joined path pair: exactly two entries.
fn pair(mode: &str, value: &str) -> Result<(PathBuf, PathBuf), String> {
    let entries: Vec<&str> = value.split(',').collect();
    if entries.len() != 2 {
        return Err(format!(
            "extrapolation mode \"{mode}\" expects comma-joined path pairs of \
             exactly two entries (2p LOWPAIR HIGHPAIR N ALPHA BETA F)"
        ));
    }
    Ok((PathBuf::from(entries[0]), PathBuf::from(entries[1])))
}

/// N, ALPHA, and BETA: an integer at least 1 and two exponents above zero.
fn parameters(mode: &str, values: &[String]) -> Result<(u32, f64, f64), String> {
    let cardinal = values[0].parse::<u32>().map_err(|_| {
        format!(
            "extrapolation mode \"{mode}\" expects N as an integer at least 1; \
                 got \"{}\"",
            values[0]
        )
    })?;
    if cardinal < 1 {
        return Err(format!(
            "extrapolation mode \"{mode}\" expects N as an integer at least 1; got {cardinal}"
        ));
    }
    let alpha = exponent(mode, "ALPHA", &values[1])?;
    let beta = exponent(mode, "BETA", &values[2])?;
    Ok((cardinal, alpha, beta))
}

/// ALPHA or BETA: a number above zero.
fn exponent(mode: &str, name: &str, value: &str) -> Result<f64, String> {
    let parsed = value.parse::<f64>().map_err(|_| {
        format!(
            "extrapolation mode \"{mode}\" expects {name} as a number above zero; \
             got \"{value}\""
        )
    })?;
    if parsed.is_nan() || parsed <= 0.0 {
        return Err(format!(
            "extrapolation mode \"{mode}\" expects {name} as a number above zero; got {parsed}"
        ));
    }
    Ok(parsed)
}

/// F: a finite number.
fn fraction(mode: &str, value: &str) -> Result<f64, String> {
    let parsed = value.parse::<f64>().map_err(|_| {
        format!("extrapolation mode \"{mode}\" expects F as a finite number; got \"{value}\"")
    })?;
    if !parsed.is_finite() {
        return Err(format!(
            "extrapolation mode \"{mode}\" expects F as a finite number; got {parsed}"
        ));
    }
    Ok(parsed)
}

/// The two-point extrapolation to the complete-basis-set limit
/// (https://doi.org/10.1021/ct100396y): the SCF term over the two SCF
/// energies plus the correlation term over the two correlation
/// residuals, with e_X = e^(−α√N) and e_Y = e^(−α√(N+1)).
///
/// (rq-bb43d915 rq-443ba191)
pub fn two_point_energy(
    scf_low: f64,
    corr_low: f64,
    scf_high: f64,
    corr_high: f64,
    cardinal: u32,
    alpha: f64,
    beta: f64,
) -> f64 {
    let n = f64::from(cardinal);
    let e_low = (-alpha * n.sqrt()).exp();
    let e_high = (-alpha * (n + 1.0).sqrt()).exp();
    let scf_term = (scf_low * e_high - scf_high * e_low) / (e_high - e_low);
    let n_low = n.powf(beta);
    let n_high = (n + 1.0).powf(beta);
    let corr_term = (n_low * corr_low - n_high * corr_high) / (n_low - n_high);
    scf_term + corr_term
}

/// The CPS/LAF blend: `E = E_low + F·(E_high − E_low)`.
///
/// (rq-bb43d915 rq-e8016516)
pub fn blend_energy(low: f64, high: f64, fraction: f64) -> f64 {
    low + fraction * (high - low)
}

/// Whether a source record can contribute: status `ok`, a present
/// `name`, a present `electronic_energy`, and — in every mode that
/// extrapolates over bases — a present `scf_energy`
/// (`rqm/extrapolation.md` Candidates and Matching).
// rq-c68c02b0
pub fn is_candidate(record: &Record, needs_scf: bool) -> bool {
    skip_reason(record, needs_scf).is_none()
}

/// Why a source record cannot contribute, if it cannot: not `ok`, or ok
/// without a `name`, an `electronic_energy`, or — in the base-extrapolating
/// modes — an `scf_energy`. The single definition behind the pass's
/// `skipped` count and the CLI's per-record warnings, so the two cannot
/// drift. CLI support: not part of the library's documented surface
/// (`src/lib.rs` does not re-export it).
// rq-c68c02b0
pub fn skip_reason(record: &Record, needs_scf: bool) -> Option<&'static str> {
    match record.status {
        RecordStatus::Failed => Some("failed"),
        RecordStatus::Truncated => Some("truncated"),
        RecordStatus::Ok => {
            if record.name.is_none() {
                Some("no name")
            } else if record.electronic_energy.is_none() {
                Some("no electronic energy")
            } else if needs_scf && record.scf_energy.is_none() {
                Some("no scf_energy")
            } else {
                None
            }
        }
    }
}

/// The correction one fully covered name computes to, from the sources
/// in the spec's contributor order: `None` when a source lacks a
/// candidate for the name or the result is not finite. The same assembly
/// the pass performs, exposed for the CLI's uncovered-record warnings.
/// CLI support: not part of the library's documented surface
/// (`src/lib.rs` does not re-export it).
// rq-9e38e14b
pub fn correction_for(
    sources: &[&[Record]],
    spec: &ExtrapolationSpec,
    name: &str,
) -> Option<Correction> {
    let needs_scf = spec.needs_scf();
    let contributors: Vec<&Record> = sources
        .iter()
        .map(|source| {
            source.iter().find(|record| {
                is_candidate(record, needs_scf) && record.name.as_deref() == Some(name)
            })
        })
        .collect::<Option<Vec<_>>>()?;
    correction_from_contributors(&contributors, spec)
}

/// The attached correction of one contributor set (`rqm/extrapolation.md`
/// The Attached Correction): the mode's result as `energy`, the mode's
/// `kind`, the contributing commands in contributor order, and the last
/// contributor's program and version. `None` when a result is not
/// finite — never an infinite or NaN energy.
// rq-9e38e14b
fn correction_from_contributors(
    contributors: &[&Record],
    spec: &ExtrapolationSpec,
) -> Option<Correction> {
    if contributors.len() < spec.source_count() {
        return None;
    }
    let correlation = |record: &Record| Some(record.electronic_energy? - record.scf_energy?);
    let energy = match &spec.mode {
        ExtrapMode::TwoPoint {
            cardinal,
            alpha,
            beta,
            ..
        } => two_point_energy(
            contributors[0].scf_energy?,
            correlation(contributors[0])?,
            contributors[1].scf_energy?,
            correlation(contributors[1])?,
            *cardinal,
            *alpha,
            *beta,
        ),
        ExtrapMode::SettingsBlend { fraction, .. } => blend_energy(
            contributors[0].electronic_energy?,
            contributors[1].electronic_energy?,
            *fraction,
        ),
        ExtrapMode::TwoPointBlended {
            cardinal,
            alpha,
            beta,
            fraction,
            ..
        } => {
            let low = two_point_energy(
                contributors[0].scf_energy?,
                correlation(contributors[0])?,
                contributors[1].scf_energy?,
                correlation(contributors[1])?,
                *cardinal,
                *alpha,
                *beta,
            );
            let high = two_point_energy(
                contributors[2].scf_energy?,
                correlation(contributors[2])?,
                contributors[3].scf_energy?,
                correlation(contributors[3])?,
                *cardinal,
                *alpha,
                *beta,
            );
            blend_energy(low, high, *fraction)
        }
    };
    if !energy.is_finite() {
        return None;
    }
    let commands: Vec<String> = contributors
        .iter()
        .filter_map(|record| record.command.clone())
        .collect();
    let last = contributors[contributors.len() - 1];
    Some(Correction {
        energy,
        program: last.program.clone(),
        program_version: last.program_version.clone(),
        commands: (!commands.is_empty()).then_some(commands),
        kind: spec.kind().to_string(),
    })
}

/// Pure: builds each source's candidate map in order (duplicate names
/// yield `Err`), attaches per the matching rules — including the
/// uniformity checks, a contributor set that mixes programs, mixes
/// program versions, or repeats a calculation yielding `Err` — and
/// returns the enriched records with the counts. `sources` supplies the
/// record slices in the spec's contributor order: `[low, high]` for
/// `TwoPoint` and `SettingsBlend`, `[lowB at low settings, highB at low
/// settings, lowB at high settings, highB at high settings]` for
/// `TwoPointBlended`. The list holds exactly the mode's number of
/// databases — two, or four for the combined form — and any other length
/// yields `Err(ExtrapolationError::SourceCount)`. The CLI loads each
/// source with `load_database` (reporting its problems) and hands the
/// valid records over.
///
/// (rq-a757e0f8)
pub fn extrapolate_corrections(
    records: &[Record],
    sources: &[&[Record]],
    spec: &ExtrapolationSpec,
) -> Result<Extrapolation, ExtrapolationError> {
    let needs_scf = spec.needs_scf();
    let needed = spec.source_count();
    if sources.len() != needed {
        return Err(ExtrapolationError::SourceCount {
            expected: needed,
            actual: sources.len(),
        });
    }

    // The candidate maps, one per source in contributor order; duplicate
    // candidate names within one source are fatal, non-candidates never
    // collide.
    // rq-c68c02b0
    let mut maps: Vec<HashMap<&str, &Record>> = Vec::with_capacity(sources.len());
    for (index, source) in sources.iter().enumerate() {
        let mut map: HashMap<&str, &Record> = HashMap::new();
        for record in *source {
            if !is_candidate(record, needs_scf) {
                continue;
            }
            let name = record.name.as_deref().expect("candidate carries a name");
            if map.insert(name, record).is_some() {
                return Err(ExtrapolationError::DuplicateName {
                    source: index,
                    name: name.to_string(),
                });
            }
        }
        maps.push(map);
    }

    // The names covered by every contributing source, in a deterministic
    // order: each name's contributors must come from one program at one
    // version and be distinct calculations, whatever the run records
    // demand.
    // rq-c68c02b0
    let mut covered: BTreeSet<&str> = maps
        .first()
        .map(|map| map.keys().copied().collect())
        .unwrap_or_default();
    covered.retain(|name| maps.iter().all(|map| map.contains_key(*name)));
    let mut corrections: HashMap<&str, Correction> = HashMap::new();
    for name in &covered {
        let contributors: Vec<&Record> = maps[..needed].iter().map(|map| map[*name]).collect();
        for (a, record_a) in contributors.iter().enumerate() {
            for (b, record_b) in contributors.iter().enumerate().skip(a + 1) {
                // Programs and versions compare as exact strings; a
                // candidate without one never violates — only
                // present-and-different values do.
                // rq-c68c02b0
                if let (Some(program_a), Some(program_b)) = (&record_a.program, &record_b.program) {
                    if program_a != program_b {
                        return Err(ExtrapolationError::ProgramMismatch {
                            name: name.to_string(),
                            source_a: a,
                            source_b: b,
                            program_a: program_a.clone(),
                            program_b: program_b.clone(),
                        });
                    }
                }
                if let (Some(version_a), Some(version_b)) =
                    (&record_a.program_version, &record_b.program_version)
                {
                    if version_a != version_b {
                        return Err(ExtrapolationError::VersionMismatch {
                            name: name.to_string(),
                            source_a: a,
                            source_b: b,
                            version_a: version_a.clone(),
                            version_b: version_b.clone(),
                        });
                    }
                }
                // Distinct calculations: byte-identical electronic
                // energies mean the same calculation entered the sources
                // twice. The SCF energies are exempt — the combined
                // form's within-basis pairs legitimately share them.
                // rq-c68c02b0
                if record_a.electronic_energy == record_b.electronic_energy {
                    return Err(ExtrapolationError::DuplicateEnergy {
                        name: name.to_string(),
                        source_a: a,
                        source_b: b,
                    });
                }
            }
        }
        // A result that is not finite leaves the name without a
        // correction — the records naming it count as uncovered, never
        // written as an infinite or NaN energy.
        // rq-a4fc7526
        if let Some(correction) = correction_from_contributors(&contributors, spec) {
            corrections.insert(name, correction);
        }
    }

    // Attachment: every run record sharing a fully covered name receives
    // the correction, whatever its status; a nameless record or a name
    // without a computed correction stays uncovered and keeps whatever
    // correction it stores.
    // rq-c68c02b0
    let run_names: HashSet<&str> = records.iter().filter_map(|r| r.name.as_deref()).collect();
    let mut attached = 0usize;
    let mut uncovered = 0usize;
    let mut enriched_records = Vec::with_capacity(records.len());
    for record in records {
        let mut enriched = record.clone();
        match record
            .name
            .as_deref()
            .and_then(|name| corrections.get(name))
        {
            Some(correction) => {
                enriched.correction = Some(correction.clone());
                attached += 1;
            }
            None => uncovered += 1,
        }
        enriched_records.push(enriched);
    }
    let skipped = sources
        .iter()
        .map(|source| {
            source
                .iter()
                .filter(|record| skip_reason(record, needs_scf).is_some())
                .count()
        })
        .sum();
    // A candidate (which carries a name) no run record names is
    // unmatched, per source.
    // rq-c68c02b0
    let unmatched = maps
        .iter()
        .map(|map| {
            map.keys()
                .filter(|name| !run_names.contains(**name))
                .count()
        })
        .sum();
    Ok(Extrapolation {
        records: enriched_records,
        attached,
        skipped,
        uncovered,
        unmatched,
    })
}

/// The source database paths in the spec's contributor order: `[low,
/// high]` for `TwoPoint` and `SettingsBlend`, `[lowB at low settings,
/// highB at low settings, lowB at high settings, highB at high settings]`
/// for `TwoPointBlended`. CLI support: not part of the library's
/// documented surface (`src/lib.rs` does not re-export it).
// rq-a757e0f8
pub fn source_paths(spec: &ExtrapolationSpec) -> Vec<PathBuf> {
    match &spec.mode {
        ExtrapMode::TwoPoint { low, high, .. } | ExtrapMode::SettingsBlend { low, high, .. } => {
            vec![low.clone(), high.clone()]
        }
        ExtrapMode::TwoPointBlended {
            low_pair,
            high_pair,
            ..
        } => vec![
            low_pair.0.clone(),
            high_pair.0.clone(),
            low_pair.1.clone(),
            high_pair.1.clone(),
        ],
    }
}
