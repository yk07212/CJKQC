//! JSON Lines record databases — the interchange format between CJKQC
//! invocations. A database is UTF-8 text, one record per line (see the
//! `record` module). An unreadable line becomes a reported problem while
//! the valid records load; only filesystem-level failures are errors.
//!
//! (rq-2a2407ed)

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::record::{Record, RecordInvalidity};

/// The outcome of loading a database: valid records in file order, plus a
/// problem for every skipped line.
///
/// (rq-2c1dac26)
#[derive(Debug, Clone, PartialEq)]
pub struct LoadOutcome {
    /// Valid records, in file order.
    pub records: Vec<Record>,
    /// One entry per skipped line.
    pub problems: Vec<LoadProblem>,
}

/// A skipped database line.
///
/// (rq-39867e78)
#[derive(Debug, Clone, PartialEq)]
pub struct LoadProblem {
    /// 1-based line number.
    pub line: usize,
    /// Why the line was skipped.
    pub reason: String,
}

/// The outcome of merging databases: how many records were written, plus
/// every problem encountered in the inputs.
///
/// (rq-3a147f99)
#[derive(Debug, Clone, PartialEq)]
pub struct MergeOutcome {
    /// Count of records written to the output.
    pub records: usize,
    /// One entry per skipped input line.
    pub problems: Vec<MergeProblem>,
}

/// A skipped line in one merge input.
///
/// (rq-96b8c74d)
#[derive(Debug, Clone, PartialEq)]
pub struct MergeProblem {
    /// The input the skipped line belongs to.
    pub input: PathBuf,
    /// 1-based line number within the input.
    pub line: usize,
    /// Why the line was skipped.
    pub reason: String,
}

/// Error type for database operations.
///
/// (rq-125f5313)
#[derive(Debug, Clone, PartialEq)]
pub enum DbError {
    /// A filesystem operation failed: missing input file, uncreatable or
    /// unwritable output, missing parent directory, permission failure.
    Io {
        /// The underlying failure's kind (`NotFound`, `PermissionDenied`, …).
        kind: std::io::ErrorKind,
        /// The human-readable message.
        reason: String,
    },
    /// A record could not be serialized to its JSON line while writing
    /// (append or merge); a record-content failure, never an `Io`.
    Serialize(String),
    /// A record handed to `append_records` failed validation; nothing was
    /// written.
    InvalidRecord(RecordInvalidity),
}

impl DbError {
    fn from_io(path: &Path, error: std::io::Error) -> Self {
        Self::Io {
            kind: error.kind(),
            reason: format!("{}: {error}", path.display()),
        }
    }
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbError::Io { reason, .. } => write!(f, "database I/O failure: {reason}"),
            DbError::Serialize(reason) => write!(f, "record serialization failure: {reason}"),
            DbError::InvalidRecord(invalidity) => write!(f, "invalid record: {invalidity}"),
        }
    }
}

impl std::error::Error for DbError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DbError::Io { .. } | DbError::Serialize(_) => None,
            DbError::InvalidRecord(invalidity) => Some(invalidity),
        }
    }
}

/// Loads a database. The path must exist and be a readable file; otherwise
/// `Err(DbError::Io { .. })`. Blank lines are ignored silently. Every non-blank
/// line is deserialized into a `Record` and validated; a line that fails
/// either step is skipped and reported with its 1-based line number. Content
/// problems never fail the load.
///
/// (rq-2cc9076e)
pub fn load_database(path: &Path) -> Result<LoadOutcome, DbError> {
    let bytes = fs::read(path).map_err(|error| DbError::from_io(path, error))?;
    let (lines, problems) = parse_lines(&bytes);
    Ok(LoadOutcome {
        records: lines.into_iter().map(|line| line.record).collect(),
        problems: problems
            .into_iter()
            .map(|(line, reason)| LoadProblem { line, reason })
            .collect(),
    })
}

/// Appends records to a database, creating the file when missing (the
/// parent directory must already exist). Every record is validated before
/// anything is written; one invalid record means nothing is written. An
/// empty slice writes nothing and creates no file. Existing content is
/// never rewritten; when the existing file does not end with a newline, one
/// is written first so that no record shares a line with prior bytes.
///
/// (rq-aeb0d510)
pub fn append_records(path: &Path, records: &[Record]) -> Result<(), DbError> {
    for record in records {
        record.validate().map_err(DbError::InvalidRecord)?;
    }
    if records.is_empty() {
        return Ok(());
    }
    let seal = missing_final_newline(path)?;
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| DbError::from_io(path, error))?;
    let mut writer = BufWriter::new(file);
    if seal {
        writer
            .write_all(b"\n")
            .map_err(|error| DbError::from_io(path, error))?;
    }
    for record in records {
        write_line(&mut writer, record, path)?;
    }
    writer
        .flush()
        .map_err(|error| DbError::from_io(path, error))?;
    Ok(())
}

/// Writes `lines` as the complete new content of a database, atomically.
/// The content lands in a temporary file in `path`'s parent directory, is
/// flushed and synced, then renamed over `path`, replacing any existing
/// file. Any failure before the rename leaves the previous content (if
/// any) intact and removes the temporary file. Zero lines produce an empty
/// file; the parent directory must already exist. Callers hand in lines
/// they have already validated — the write neither validates nor
/// re-serializes.
///
/// (rq-96595784)
pub fn write_database(path: &Path, lines: &[String]) -> Result<(), DbError> {
    if path.file_name().is_none() {
        return Err(DbError::Io {
            kind: std::io::ErrorKind::InvalidInput,
            reason: format!("{}: not a database file path", path.display()),
        });
    }
    let mut contents = String::new();
    for line in lines {
        contents.push_str(line);
        contents.push('\n');
    }
    crate::atomic::write_atomically(path, &contents).map_err(|error| DbError::from_io(path, error))
}

/// Merges databases: loads every input in the given order with the same
/// per-line rules as `load_database` (skip and report), then copies every
/// line that loaded as a valid record **verbatim** — original line text,
/// byte for byte — to `output` in input order, so unknown keys survive.
/// A missing or unreadable input is an error and nothing is written.
/// `output` is replaced atomically via `write_database` and may be the
/// same path as one of the inputs (all inputs are read before `output` is
/// replaced). Zero inputs produce an empty output file.
///
/// (rq-13bcad95)
pub fn merge_databases(inputs: &[&Path], output: &Path) -> Result<MergeOutcome, DbError> {
    let (lines, problems) = merge_lines(inputs)?;
    let count = lines.len();
    write_database(output, &lines)?;
    Ok(MergeOutcome {
        records: count,
        problems,
    })
}

/// The loading phase of `merge_databases`: every input's verbatim valid
/// lines in input order, with the skipped lines as merge problems.
/// Crate-internal so the collection pipeline can place its confirmation
/// hook between the load and the write (`rqm/collection.md`, Overwrite
/// Confirmation); not public API.
pub(crate) fn merge_lines(inputs: &[&Path]) -> Result<(Vec<String>, Vec<MergeProblem>), DbError> {
    let mut lines: Vec<String> = Vec::new();
    let mut problems = Vec::new();
    for input in inputs {
        let bytes = fs::read(input).map_err(|error| DbError::from_io(input, error))?;
        let (loaded, errors) = parse_lines(&bytes);
        for (line, reason) in errors {
            problems.push(MergeProblem {
                input: input.to_path_buf(),
                line,
                reason,
            });
        }
        lines.extend(loaded.into_iter().map(|valid| valid.text));
    }
    Ok((lines, problems))
}

/// A database line that loaded, validated, and decoded as UTF-8, kept
/// together with its original text so merge can copy it verbatim.
struct ValidLine {
    text: String,
    record: Record,
}

// rq-2cc9076e rq-1d1a1d61
fn parse_lines(bytes: &[u8]) -> (Vec<ValidLine>, Vec<(usize, String)>) {
    let mut records = Vec::new();
    let mut problems = Vec::new();
    for (index, line) in physical_lines(bytes).into_iter().enumerate() {
        let number = index + 1;
        let text = match std::str::from_utf8(line) {
            Ok(text) => text,
            Err(_) => {
                problems.push((number, "invalid UTF-8".to_string()));
                continue;
            }
        };
        if text.trim().is_empty() {
            continue;
        }
        let record: Record = match serde_json::from_str(text) {
            Ok(record) => record,
            Err(error) => {
                problems.push((number, format!("invalid JSON: {error}")));
                continue;
            }
        };
        match record.validate() {
            Ok(()) => records.push(ValidLine {
                text: text.to_string(),
                record,
            }),
            Err(error) => problems.push((number, format!("invalid record: {error}"))),
        }
    }
    (records, problems)
}

// rq-1d1a1d61
fn physical_lines(bytes: &[u8]) -> Vec<&[u8]> {
    if bytes.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&[u8]> = bytes.split(|byte| *byte == b'\n').collect();
    if bytes.last() == Some(&b'\n') {
        lines.pop();
    }
    lines
}

// rq-aeb0d510 rq-1d1a1d61
fn missing_final_newline(path: &Path) -> Result<bool, DbError> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(DbError::from_io(path, error)),
    };
    let length = file
        .seek(SeekFrom::End(0))
        .map_err(|error| DbError::from_io(path, error))?;
    if length == 0 {
        return Ok(false);
    }
    file.seek(SeekFrom::Start(length - 1))
        .map_err(|error| DbError::from_io(path, error))?;
    let mut last = [0];
    file.read_exact(&mut last)
        .map_err(|error| DbError::from_io(path, error))?;
    Ok(last[0] != b'\n')
}

// rq-13bcad95 rq-aeb0d510 rq-125f5313
fn write_line<W: Write>(writer: &mut W, record: &Record, path: &Path) -> Result<(), DbError> {
    let mut line =
        serde_json::to_string(record).map_err(|error| DbError::Serialize(error.to_string()))?;
    line.push('\n');
    writer
        .write_all(line.as_bytes())
        .map_err(|error| DbError::from_io(path, error))?;
    Ok(())
}
