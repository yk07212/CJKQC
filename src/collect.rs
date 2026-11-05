//! The collection pipeline behind the `CJKQC` command line (see
//! `rqm/collection.md`). Inputs are classified by content — a database
//! input's first non-blank line begins with `{`, a program output's does
//! not — and an invocation is either all databases (merge, records copied
//! verbatim) or all program outputs (build, parsed through the standard
//! registry with compositions resolved). The output database is replaced.
//!
//! (rq-c1056d55)

use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::composition::resolve_components;
use crate::db::{self, DbError, MergeProblem};
use crate::parser::{ParseError, ParserRegistry};
use crate::record::{Record, RecordStatus};

/// Which operation the inputs call for.
///
/// (rq-4293ca31)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionMode {
    /// All inputs are QC program outputs.
    Build,
    /// All inputs are record databases.
    Merge,
}

/// What a completed run did.
///
/// (rq-d99aaa73)
#[derive(Debug, Clone, PartialEq)]
pub enum CollectOutcome {
    /// Build: record counts by status; `written` is the total.
    Build {
        written: usize,
        failed: usize,
        truncated: usize,
    },
    /// Merge: records written and the corrupt lines skipped.
    Merge {
        written: usize,
        problems: Vec<MergeProblem>,
    },
}

/// Why a run wrote nothing.
///
/// (rq-989e6417)
#[derive(Debug, Clone, PartialEq)]
pub enum CollectError {
    /// Both input kinds were given — a usage error (exit 2).
    Mixed {
        databases: Vec<PathBuf>,
        outputs: Vec<PathBuf>,
    },
    /// A build input carried no recognizable program output (unrecognized
    /// format or empty file).
    Unrecognized { path: PathBuf, reason: String },
    /// An input could not be read or the output could not be written.
    Io {
        kind: std::io::ErrorKind,
        reason: String,
    },
    /// The database layer failed for a non-Io reason while writing.
    Db(DbError),
    /// The caller's confirmation hook declined the replacement of an
    /// existing output; nothing was written. The CLI reports the reason
    /// itself and never surfaces this variant's own text
    /// (`rqm/collection.md`, Overwrite Confirmation).
    ///
    /// (rq-eff8dce8)
    NotConfirmed,
}

impl CollectError {
    fn from_io(path: &Path, error: std::io::Error) -> Self {
        Self::Io {
            kind: error.kind(),
            reason: format!("{}: {error}", path.display()),
        }
    }

    fn io_from_parse(path: &Path, error: ParseError) -> Self {
        let ParseError::Io { kind, reason } = error;
        Self::Io {
            kind,
            reason: format!("{}: {reason}", path.display()),
        }
    }
}

impl fmt::Display for CollectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CollectError::Mixed { databases, outputs } => {
                write!(
                    f,
                    "cannot mix database inputs [{}] with program outputs [{}]",
                    list_paths(databases),
                    list_paths(outputs)
                )
            }
            CollectError::Unrecognized { path, reason } => {
                write!(f, "{}: {reason}", path.display())
            }
            CollectError::Io { reason, .. } => write!(f, "I/O failure: {reason}"),
            CollectError::Db(error) => write!(f, "{error}"),
            CollectError::NotConfirmed => write!(f, "the output replacement was not confirmed"),
        }
    }
}

impl std::error::Error for CollectError {}

fn list_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Classifies every input (opening each file and reading up to its first
/// non-blank line): unanimous kinds yield the mode, a mixture yields
/// `Mixed` listing the inputs of each kind, a read failure yields `Io`.
///
/// (rq-550ff715)
pub fn classify_inputs(inputs: &[&Path]) -> Result<CollectionMode, CollectError> {
    let mut databases = Vec::new();
    let mut outputs = Vec::new();
    for path in inputs {
        if is_database_input(path)? {
            databases.push(path.to_path_buf());
        } else {
            outputs.push(path.to_path_buf());
        }
    }
    match (databases.is_empty(), outputs.is_empty()) {
        (false, false) => Err(CollectError::Mixed { databases, outputs }),
        (false, true) => Ok(CollectionMode::Merge),
        _ => Ok(CollectionMode::Build),
    }
}

/// The classification rule: the first non-blank line, leading whitespace
/// trimmed, begins with `{`. A file with no non-blank line is a program
/// output.
fn is_database_input(path: &Path) -> Result<bool, CollectError> {
    let file = File::open(path).map_err(|error| CollectError::from_io(path, error))?;
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = reader
            .read_until(b'\n', &mut line)
            .map_err(|error| CollectError::from_io(path, error))?;
        if read == 0 {
            return Ok(false);
        }
        if !line.iter().all(|byte| byte.is_ascii_whitespace()) {
            let trimmed = line
                .iter()
                .position(|byte| !byte.is_ascii_whitespace())
                .unwrap_or(line.len());
            return Ok(line[trimmed..].first() == Some(&b'{'));
        }
    }
}

/// Classifies, then performs the selected mode in full. Nothing is ever
/// written when an error is returned: in build mode every input is read and
/// every record produced before the output is opened. The confirmation of
/// an existing output is the caller's concern: `collect_inputs` runs with
/// an unconditional confirmation and never asks
/// (`rqm/collection.md`, Overwrite Confirmation).
///
/// (rq-9d8725d5)
pub fn collect_inputs(inputs: &[&Path], output: &Path) -> Result<CollectOutcome, CollectError> {
    collect_inputs_with(
        inputs,
        output,
        &crate::parser::WindowBudget::default(),
        &mut || true,
    )
}

/// `collect_inputs` with an explicit window budget (the CLI's
/// `--head-bytes`/`--tail-bytes`) and the caller's confirmation hook: every
/// build-mode input is read through windows of the given sizes, so a wider
/// budget can recover data an elided seam cut. Merge mode reads record
/// databases and ignores the budget. `confirm` is invoked immediately
/// before an existing output is replaced; `false` aborts with nothing
/// written (`rqm/collection.md`, Overwrite Confirmation).
///
/// (rq-3b5861a5)
pub fn collect_inputs_with(
    inputs: &[&Path],
    output: &Path,
    budget: &crate::parser::WindowBudget,
    confirm: &mut dyn FnMut() -> bool,
) -> Result<CollectOutcome, CollectError> {
    match classify_inputs(inputs)? {
        CollectionMode::Build => build_database(inputs, output, budget, confirm),
        CollectionMode::Merge => merge_databases(inputs, output, confirm),
    }
}

/// Parse every input with the standard registry, abort on any record the
/// framework marked unrecognized or empty (status `failed` with no
/// program), and resolve compositions.
fn parse_records(
    inputs: &[&Path],
    budget: &crate::parser::WindowBudget,
) -> Result<Vec<Record>, CollectError> {
    let registry = ParserRegistry::standard();
    let mut records = Vec::with_capacity(inputs.len());
    for path in inputs {
        let mut record = registry
            .parse_file_with(path, budget)
            .map_err(|error| CollectError::io_from_parse(path, error))?;
        if record.is_unrecognized() {
            return Err(CollectError::Unrecognized {
                path: path.to_path_buf(),
                reason: record
                    .error
                    .unwrap_or_else(|| "no recognizable program output".to_string()),
            });
        }
        resolve_components(&mut record);
        records.push(record);
    }
    Ok(records)
}

/// Parse every input with the standard registry, abort on any record the
/// framework marked unrecognized or empty (status `failed` with no
/// program), resolve compositions, then write the collected records as the
/// complete new content of the output. The confirmation hook is consulted
/// only when the output already exists and is then invoked immediately
/// before replacing it; `false` declines the write with `NotConfirmed`
/// (`rqm/collection.md`, Overwrite Confirmation).
fn build_database(
    inputs: &[&Path],
    output: &Path,
    budget: &crate::parser::WindowBudget,
    confirm: &mut dyn FnMut() -> bool,
) -> Result<CollectOutcome, CollectError> {
    let records = parse_records(inputs, budget)?;

    // Every input is read, validated, and serialized before the output is
    // touched; write_database then replaces it atomically, so a failure
    // anywhere above leaves any existing database intact.
    let mut lines = Vec::with_capacity(records.len());
    for record in &records {
        record
            .validate()
            .map_err(|invalidity| CollectError::Db(DbError::InvalidRecord(invalidity)))?;
        lines.push(
            serde_json::to_string(record)
                .map_err(|error| CollectError::Db(DbError::Serialize(error.to_string())))?,
        );
    }
    if output.exists() && !confirm() {
        return Err(CollectError::NotConfirmed);
    }
    db::write_database(output, &lines).map_err(from_db)?;

    let failed = records
        .iter()
        .filter(|record| record.status == RecordStatus::Failed)
        .count();
    let truncated = records
        .iter()
        .filter(|record| record.status == RecordStatus::Truncated)
        .count();
    Ok(CollectOutcome::Build {
        written: records.len(),
        failed,
        truncated,
    })
}

/// One merge: the inputs' verbatim lines are loaded in order, and — only
/// when the output already exists — the confirmation hook is consulted
/// immediately before the write and the lines are copied into the
/// atomically replaced output; `DbError::Io` surfaces as
/// `CollectError::Io` (`rqm/collection.md`, Overwrite Confirmation).
fn merge_databases(
    inputs: &[&Path],
    output: &Path,
    confirm: &mut dyn FnMut() -> bool,
) -> Result<CollectOutcome, CollectError> {
    let (lines, problems) = db::merge_lines(inputs).map_err(from_db)?;
    if output.exists() && !confirm() {
        return Err(CollectError::NotConfirmed);
    }
    db::write_database(output, &lines).map_err(from_db)?;
    Ok(CollectOutcome::Merge {
        written: lines.len(),
        problems,
    })
}

/// `DbError::Io` surfaces as `CollectError::Io`; other database failures
/// pass through as `Db`.
fn from_db(error: DbError) -> CollectError {
    match error {
        DbError::Io { kind, reason } => CollectError::Io { kind, reason },
        other => CollectError::Db(other),
    }
}

/// Records gathered before any manipulation (see `rqm/filtering.md`):
/// the same classification and collection a plain run performs, held in
/// memory instead of written to an output.
///
/// (rq-9b4dda85)
#[derive(Debug, Clone, PartialEq)]
pub enum Gathered {
    /// Build: every parsed record, in argument order.
    Build { records: Vec<Record> },
    /// Merge: every valid record in file order, with the skipped lines.
    Merge {
        records: Vec<Record>,
        problems: Vec<MergeProblem>,
    },
}

/// Classifies the inputs and gathers their records without writing
/// anything; errors are exactly the ones `collect_inputs` reports before
/// writing.
///
/// (rq-9b4dda85)
pub fn gather_inputs(inputs: &[&Path]) -> Result<Gathered, CollectError> {
    gather_inputs_with(inputs, &crate::parser::WindowBudget::default())
}

/// `gather_inputs` with an explicit window budget (the CLI's
/// `--head-bytes`/`--tail-bytes`): build-mode inputs are read through
/// windows of the given sizes. Merge mode reads record databases and
/// ignores the budget.
///
/// (rq-024c9db0)
pub fn gather_inputs_with(
    inputs: &[&Path],
    budget: &crate::parser::WindowBudget,
) -> Result<Gathered, CollectError> {
    match classify_inputs(inputs)? {
        CollectionMode::Build => Ok(Gathered::Build {
            records: parse_records(inputs, budget)?,
        }),
        CollectionMode::Merge => {
            let mut records = Vec::new();
            let mut problems = Vec::new();
            for path in inputs {
                let outcome = db::load_database(path).map_err(from_db)?;
                records.extend(outcome.records);
                problems.extend(outcome.problems.into_iter().map(|problem| MergeProblem {
                    input: path.to_path_buf(),
                    line: problem.line,
                    reason: problem.reason,
                }));
            }
            Ok(Gathered::Merge { records, problems })
        }
    }
}
