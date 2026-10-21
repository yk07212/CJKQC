//! Single-point energy corrections (see `rqm/correction.md`): a correction
//! database built by an ordinary collection run carries the single-point
//! jobs' energies and provenance; [`attach_corrections`] matches its
//! records to a run's records by exact `name` and copies energy and
//! provenance into the records' `correction` objects. The stored
//! optimization energy is never altered; corrected values surface through
//! the registry keys `elcorr`, `hcorr`, and `gcorr`.
//!
//! (rq-f8ab9d1b)

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::record::{Correction, Record, RecordStatus};
use crate::thermo::{thermochemistry_with_energy, Thermo, ThermoConditions};

/// Why an attach pass fails.
///
/// (rq-874abd6a)
#[derive(Debug, Clone, PartialEq)]
pub enum CorrectionError {
    /// The correction database could not be read (mirrors `DbError::Io`).
    Io {
        /// The underlying failure's kind.
        kind: std::io::ErrorKind,
        /// A human-readable message naming the path the failure concerned.
        reason: String,
    },
    /// Two candidates share the name.
    DuplicateName {
        /// The name two candidates share.
        name: String,
    },
}

impl fmt::Display for CorrectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CorrectionError::Io { reason, .. } => write!(f, "I/O failure: {reason}"),
            CorrectionError::DuplicateName { name } => write!(
                f,
                "correction database holds multiple records named \"{name}\""
            ),
        }
    }
}

impl std::error::Error for CorrectionError {}

/// What an attach pass did: the enriched records plus the counts.
///
/// (rq-952b74c9)
#[derive(Debug, Clone, PartialEq)]
pub struct Attachment {
    /// Enriched copies of the input, in input order; the input slice is
    /// never mutated.
    pub records: Vec<Record>,
    /// Records that received a correction.
    pub attached: usize,
    /// Candidates rejected for status or missing energy.
    pub skipped: usize,
    /// Candidates no input record names.
    pub unmatched: usize,
}

/// Whether a correction record can attach: status `ok`, a present
/// `electronic_energy`, and a present `name` (`rqm/correction.md`).
// rq-233e0670
fn is_candidate(record: &Record) -> bool {
    record.status == RecordStatus::Ok && record.electronic_energy.is_some() && record.name.is_some()
}

/// The attached correction of one candidate: its energy, identity, and
/// command, with the `sp` kind (`rqm/correction.md`).
// rq-587473fd
fn correction_from(candidate: &Record) -> Correction {
    Correction {
        energy: candidate
            .electronic_energy
            .expect("candidate carries an energy"),
        program: candidate.program.clone(),
        program_version: candidate.program_version.clone(),
        commands: candidate.command.clone().map(|command| vec![command]),
        kind: "sp".to_string(),
    }
}

/// Why a correction record is skipped, if it is: not ok, or ok without an
/// `electronic_energy` (`rqm/correction.md`). The single definition behind
/// the attach pass's `skipped` count and the CLI's per-record warnings, so
/// the two cannot drift. CLI support: not part of the library's documented
/// surface (`src/lib.rs` does not re-export it).
// rq-e59918d8 rq-a83c76ac
pub fn skip_reason(record: &Record) -> Option<&'static str> {
    if record.status != RecordStatus::Ok {
        Some(match record.status {
            RecordStatus::Failed => "failed",
            RecordStatus::Truncated => "truncated",
            RecordStatus::Ok => "ok",
        })
    } else if record.electronic_energy.is_none() {
        Some("no electronic energy")
    } else {
        None
    }
}

/// Pure: builds the candidate map from `corrections` (two candidates with
/// equal names yield `Err(DuplicateName)`), returns the `Attachment` whose
/// `records` are enriched copies of the input — the input slice is never
/// mutated — alongside the counts. Matching is exact, case-sensitive
/// equality of `name`: every input record sharing a candidate's name
/// receives the correction (status-independently), an input record that
/// already carries a correction has it replaced, and a record the
/// correction database does not name keeps whatever correction it stores.
///
/// (rq-a9dae115)
pub fn attach_corrections(
    records: &[Record],
    corrections: &[Record],
) -> Result<Attachment, CorrectionError> {
    let mut candidates: HashMap<&str, &Record> = HashMap::new();
    for record in corrections {
        if !is_candidate(record) {
            continue;
        }
        let name = record.name.as_deref().expect("candidate carries a name");
        if candidates.insert(name, record).is_some() {
            return Err(CorrectionError::DuplicateName {
                name: name.to_string(),
            });
        }
    }
    let input_names: HashSet<&str> = records.iter().filter_map(|r| r.name.as_deref()).collect();
    let mut attached = 0usize;
    let enriched: Vec<Record> = records
        .iter()
        .map(
            |record| match record.name.as_deref().and_then(|name| candidates.get(name)) {
                Some(candidate) => {
                    attached += 1;
                    let mut enriched = record.clone();
                    enriched.correction = Some(correction_from(candidate));
                    enriched
                }
                None => record.clone(),
            },
        )
        .collect();
    // A non-ok correction record, or an ok one without an electronic
    // energy, is skipped and warned individually by the CLI; an
    // attachable record (ok, with an energy) that no input record names —
    // nameless ones included — is unmatched (rqm/correction.md).
    // rq-e59918d8 rq-a83c76ac rq-ea21b840
    let skipped = corrections
        .iter()
        .filter(|record| skip_reason(record).is_some())
        .count();
    let unmatched = corrections
        .iter()
        .filter(|record| {
            record.status == RecordStatus::Ok
                && record.electronic_energy.is_some()
                && record
                    .name
                    .as_deref()
                    .is_none_or(|name| !input_names.contains(name))
        })
        .count();
    Ok(Attachment {
        records: enriched,
        attached,
        skipped,
        unmatched,
    })
}

/// The attached correction's energy in hartree; `None` without a
/// correction.
///
/// (rq-fc9218b9)
pub fn corrected_energy(record: &Record) -> Option<f64> {
    record
        .correction
        .as_ref()
        .map(|correction| correction.energy)
}

/// The corrected derivation (`rqm/correction.md`): the standard
/// thermochemistry with the attached correction's energy in place of the
/// stored one. `None` without a correction, and under the same definedness
/// rules as any derivation (geometry, frequencies, and so on).
///
/// (rq-fd59200d)
pub(crate) fn corrected_thermo(record: &Record, conditions: &ThermoConditions) -> Option<Thermo> {
    thermochemistry_with_energy(record, corrected_energy(record)?, conditions)
}
