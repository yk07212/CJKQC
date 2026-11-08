//! The collection pipeline behind the `CJKQC` command line (see
//! `rqm/collection.md`). Inputs are classified by content — a database
//! input's first non-blank line begins with `{`, a program output's does
//! not — and an invocation is either all databases (merge, records copied
//! verbatim) or all program outputs (build, parsed through the standard
//! registry with compositions resolved). The output database is replaced.
//!
//! The pipeline classifies and parses an input in one task — the handle
//! opened for the classification line is the handle the windows are read
//! from, closed when the task ends — so a run holds at most the worker
//! count of input handles at a time, whatever the input count. When the
//! run asks for worker threads (`--parallel`) the tasks are distributed
//! over them from a shared counter; the mode verdict, the reported first
//! error, and the records are decided afterwards in strict argument
//! order, so any worker count produces the outcome of a sequential run.
//!
//! (rq-c1056d55)

use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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

/// The message appended to an input error caused by the process's
/// open-file limit: the run holds a small multiple of the worker count
/// of input handles at a time, so fewer workers or a higher open-file
/// limit fits the batch.
// rq-8617d3fb
const OPEN_FILE_LIMIT_HINT: &str = " — the run holds a small multiple of the worker count of input handles at a time; fewer workers or a higher open-file limit fits the batch";

impl CollectError {
    // rq-8617d3fb rq-989e6417
    fn from_io(path: &Path, error: std::io::Error) -> Self {
        let limit_hint = match error.raw_os_error() {
            Some(24) => OPEN_FILE_LIMIT_HINT,
            _ => "",
        };
        Self::Io {
            kind: error.kind(),
            reason: format!("{}: {error}{limit_hint}", path.display()),
        }
    }

    // rq-8617d3fb
    fn io_from_parse(path: &Path, error: ParseError) -> Self {
        let ParseError::Io { kind, reason } = error;
        // The open-file-limit hint fires on the parse side too: the
        // source-path canonicalization opens the input a second time,
        // and an EMFILE surfaces there with its os error number carried
        // in the reason text.
        let limit_hint = if reason.contains("os error 24") {
            OPEN_FILE_LIMIT_HINT
        } else {
            ""
        };
        Self::Io {
            kind,
            reason: format!("{}: {reason}{limit_hint}", path.display()),
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
/// `Mixed` listing the inputs of each kind, a read failure yields the
/// argument-order-first `Io`.
///
/// (rq-550ff715)
pub fn classify_inputs(inputs: &[&Path]) -> Result<CollectionMode, CollectError> {
    let outcomes = run_pipeline(inputs, &crate::parser::WindowBudget::default(), 1, false);
    reduce_mode(&outcomes, inputs)
}

/// What one input contributed to the run: its kind, its parsed record, or
/// the failure it hit. The pipeline stores one outcome per input, in
/// argument order; the mode and the reported error are decided from the
/// whole set afterwards, in the order a sequential run would decide them.
// rq-8ad5063d rq-3e15605c
enum InputOutcome {
    /// A database input: the merge reader opens it.
    Database,
    /// A program output. The record is `None` when its parse was skipped
    /// as pointless — a classification failure or a database input
    /// elsewhere already fixes the verdict, so no observable outcome can
    /// depend on this input's record.
    Output(Option<Box<Record>>),
    /// The input's open or first-line read failed.
    Classification(CollectError),
    /// The input's windows, parser, or composition resolution failed.
    Parse(CollectError),
}

/// The classification rule applied to one input: whether its first
/// non-blank line, leading whitespace trimmed, begins with `{`, and the
/// input's still-open handle — classification reads the first line from
/// the same handle the window reader later reads from. A
/// file with no non-blank line is a program output.
// rq-550ff715 rq-8ad5063d
fn classify_one(path: &Path) -> Result<(bool, File), CollectError> {
    let file = File::open(path).map_err(|error| CollectError::from_io(path, error))?;
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = reader
            .read_until(b'\n', &mut line)
            .map_err(|error| CollectError::from_io(path, error))?;
        if read == 0 {
            return Ok((false, reader.into_inner()));
        }
        if !line.iter().all(|byte| byte.is_ascii_whitespace()) {
            let trimmed = line
                .iter()
                .position(|byte| !byte.is_ascii_whitespace())
                .unwrap_or(line.len());
            let is_database = line[trimmed..].first() == Some(&b'{');
            return Ok((is_database, reader.into_inner()));
        }
    }
}

/// Parses one program output from its classification handle: the windows
/// are read from that handle — the single-open rule, one open per input
/// per pass — through the standard registry; a record the framework
/// marked unrecognized or empty aborts with `Unrecognized`; compositions
/// are resolved. The task one worker runs per input; it touches nothing
/// shared.
// rq-3b5861a5 rq-8ad5063d
fn parse_one(
    path: &Path,
    mut handle: File,
    registry: &ParserRegistry,
    budget: &crate::parser::WindowBudget,
) -> Result<Record, CollectError> {
    let mut record = registry
        .parse_opened(&mut handle, path, budget)
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
    Ok(record)
}

/// Runs one task per input — classify it, and when `parse_outputs` asks
/// and the verdict is not already fixed, read its windows and parse it
/// from the classification handle, which the task closes at its end, so
/// at most the worker count of input handles is ever open. At `workers`
/// 1 the tasks run sequentially in argument order; above it they are
/// distributed over that many scoped threads from a shared counter, a
/// task never split across workers. Parsing is skipped once it cannot
/// change the verdict (a classification failure or a database input
/// anywhere in the batch): the outcomes, not the work, decide.
// rq-8ad5063d rq-3e15605c rq-3b5861a5
fn run_pipeline(
    inputs: &[&Path],
    budget: &crate::parser::WindowBudget,
    workers: usize,
    parse_outputs: bool,
) -> Vec<InputOutcome> {
    let registry = ParserRegistry::standard();
    let parse_pointless = AtomicBool::new(false);
    let task = |index: usize| -> InputOutcome {
        let path = inputs[index];
        let (is_database, handle) = match classify_one(path) {
            Ok(pair) => pair,
            Err(error) => {
                parse_pointless.store(true, Ordering::Relaxed);
                return InputOutcome::Classification(error);
            }
        };
        if is_database {
            // A database anywhere in the batch fixes a mixture; parsing
            // the remaining outputs cannot change the verdict.
            parse_pointless.store(true, Ordering::Relaxed);
            return InputOutcome::Database;
        }
        if !parse_outputs || parse_pointless.load(Ordering::Relaxed) {
            return InputOutcome::Output(None);
        }
        match parse_one(path, handle, &registry, budget) {
            Ok(record) => InputOutcome::Output(Some(Box::new(record))),
            Err(error) => {
                parse_pointless.store(true, Ordering::Relaxed);
                InputOutcome::Parse(error)
            }
        }
    };
    let worker_count = workers.min(inputs.len()).max(1);
    if worker_count <= 1 {
        return (0..inputs.len()).map(&task).collect();
    }
    // The workers share a task counter: each takes the next index, so
    // every input is one worker's own task and the per-input cost stays
    // the sequential one.
    let next = AtomicUsize::new(0);
    let results = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            handles.push(scope.spawn(|| {
                let mut own = Vec::new();
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= inputs.len() {
                        break;
                    }
                    own.push((index, task(index)));
                }
                own
            }));
        }
        handles
            .into_iter()
            .map(|handle| handle.join().expect("a worker thread panicked"))
            .collect::<Vec<_>>()
    });
    let mut slots: Vec<Option<InputOutcome>> = (0..inputs.len()).map(|_| None).collect();
    for (index, outcome) in results.into_iter().flatten() {
        slots[index] = Some(outcome);
    }
    slots
        .into_iter()
        .map(|slot| slot.expect("every input reports once"))
        .collect()
}

/// Decides the mode from every input's outcome, in the order a
/// sequential run decides it: the argument-order-first classification
/// failure, then the mixture, then the unanimous verdict. Parsing never
/// participates — a sequential run classifies every input before any
/// parse, so a classification failure anywhere beats a parse failure
/// anywhere, and a mixture beats both.
// rq-550ff715 rq-8ad5063d rq-3e15605c
fn reduce_mode(outcomes: &[InputOutcome], inputs: &[&Path]) -> Result<CollectionMode, CollectError> {
    if let Some(InputOutcome::Classification(error)) = outcomes
        .iter()
        .find(|outcome| matches!(outcome, InputOutcome::Classification(_)))
    {
        return Err(error.clone());
    }
    let mut databases = Vec::new();
    let mut outputs = Vec::new();
    for (path, outcome) in inputs.iter().zip(outcomes) {
        match outcome {
            InputOutcome::Database => databases.push(path.to_path_buf()),
            // A parsed output is an output whether or not its parse
            // succeeded — the mode and the `Mixed` listing read kinds,
            // and the parse failure is reported afterwards, in its own
            // sequential turn.
            InputOutcome::Output(_) | InputOutcome::Parse(_) => outputs.push(path.to_path_buf()),
            InputOutcome::Classification(_) => {
                unreachable!("the classification failure was reported before the kinds were read")
            }
        }
    }
    match (databases.is_empty(), outputs.is_empty()) {
        (false, false) => Err(CollectError::Mixed { databases, outputs }),
        (false, true) => Ok(CollectionMode::Merge),
        _ => Ok(CollectionMode::Build),
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
    collect_inputs_with(inputs, output, &crate::parser::WindowBudget::default(), 1, &mut || true)
}

/// `collect_inputs` with an explicit window budget (the CLI's
/// `--head-bytes`/`--tail-bytes`) and worker count (the CLI's
/// `--parallel`; 1 is sequential): every build-mode input is read through
/// windows of the given sizes — one open per input per pass, its
/// classification handle the window reader's handle — and parsed by one
/// of `workers` tasks, so a wider budget can recover data an elided seam
/// cut while the workers overlap the reads. The outcome — records,
/// database, errors, exit behavior — is identical for any worker count.
/// Merge mode reads record databases and ignores the budget and the
/// workers. `confirm` is invoked immediately before an existing output is
/// replaced; `false` aborts with nothing written (`rqm/collection.md`,
/// Overwrite Confirmation).
///
/// (rq-3b5861a5)
pub fn collect_inputs_with(
    inputs: &[&Path],
    output: &Path,
    budget: &crate::parser::WindowBudget,
    workers: usize,
    confirm: &mut dyn FnMut() -> bool,
) -> Result<CollectOutcome, CollectError> {
    let outcomes = run_pipeline(inputs, budget, workers, true);
    let mode = reduce_mode(&outcomes, inputs)?;
    match mode {
        CollectionMode::Build => build_database(reduce_records(outcomes)?, output, confirm),
        CollectionMode::Merge => merge_databases(inputs, output, confirm),
    }
}

/// Decides the build outcome from every input's parse, in the order a
/// sequential run decides it: the argument-order-first parse failure —
/// the same variant, the same text naming the same path — else every
/// record, in argument order, whatever order the workers finished in.
///
/// A parse panic aborts a parallel run that a sequential run would have
/// failed cleanly on an earlier input; the parser contract — a parser
/// never panics on any content (`rqm/parser-framework.md`) — covers the
/// case.
// rq-3b5861a5 rq-3e15605c
fn reduce_records(outcomes: Vec<InputOutcome>) -> Result<Vec<Record>, CollectError> {
    if let Some(InputOutcome::Parse(error)) = outcomes
        .iter()
        .find(|outcome| matches!(outcome, InputOutcome::Parse(_)))
    {
        return Err(error.clone());
    }
    Ok(outcomes
        .into_iter()
        .map(|outcome| match outcome {
            InputOutcome::Output(record) => record
                .map(|record| *record)
                .expect(
                    "a build run parsed every output — the decision reported the earlier failure",
                ),
            _ => unreachable!("the mode was decided before the records were read"),
        })
        .collect())
}

/// Write the collected records as the complete new content of the output
/// (`rqm/collection.md`, Build Mode): every record is validated and
/// serialized before the output is touched; `write_database` then
/// replaces it atomically, so a failure anywhere above leaves any
/// existing database intact. The confirmation hook is consulted only when
/// the output already exists and is then invoked immediately before
/// replacing it; `false` declines the write with `NotConfirmed`
/// (`rqm/collection.md`, Overwrite Confirmation).
fn build_database(
    records: Vec<Record>,
    output: &Path,
    confirm: &mut dyn FnMut() -> bool,
) -> Result<CollectOutcome, CollectError> {
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
    gather_inputs_with(inputs, &crate::parser::WindowBudget::default(), 1)
}

/// `gather_inputs` with an explicit window budget (the CLI's
/// `--head-bytes`/`--tail-bytes`) and worker count (the CLI's
/// `--parallel`; 1 is sequential): build-mode inputs are read through
/// windows of the given sizes and parsed by one of `workers` threads,
/// with the records, order, and errors of a sequential run. Merge mode
/// reads record databases and ignores the budget and the workers.
///
/// (rq-024c9db0)
pub fn gather_inputs_with(
    inputs: &[&Path],
    budget: &crate::parser::WindowBudget,
    workers: usize,
) -> Result<Gathered, CollectError> {
    let outcomes = run_pipeline(inputs, budget, workers, true);
    let mode = reduce_mode(&outcomes, inputs)?;
    match mode {
        CollectionMode::Build => Ok(Gathered::Build {
            records: reduce_records(outcomes)?,
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
