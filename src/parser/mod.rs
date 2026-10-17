//! Format detection and parser framework: every quantum-chemistry output
//! file is turned into exactly one record by content-based format
//! detection, a parser trait plus an ordered registry, a shared head and
//! tail file reader (the `windows` module), and shared extraction helpers
//! (the `extract` module). Detection inspects the file's leading lines
//! only — never its name or extension — and routes the file to the first
//! registered parser whose signature matches. The registry ships empty:
//! no output format is recognized until the per-format parsers register
//! (milestone 3), and every file parsed in the interim yields an
//! unrecognized-format record.
//!
//! (rq-42e59efe rq-b75cecef)

pub mod extract;
mod windows;

use std::fmt;
use std::fs::File;
use std::path::Path;

use crate::record::{Record, RecordStatus};

pub use windows::{FileWindows, DEFAULT_HEAD_BYTES, DEFAULT_TAIL_BYTES};

/// The closed vocabulary of output formats, displayed as the lowercase
/// tokens `"gaussian"`, `"orca"`, `"xtb"`, `"mrcc"`, `"xyz"` — the only
/// values that ever appear in `Record::program`. The framework owns this
/// vocabulary; every parser selects one of the five.
///
/// (rq-0cc3c80f)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Program {
    /// Gaussian log files.
    Gaussian,
    /// ORCA output files.
    Orca,
    /// xtb output files.
    Xtb,
    /// MRCC output files.
    Mrcc,
    /// Plain xyz geometry files.
    Xyz,
}

impl Program {
    /// The lowercase program token carried by records.
    ///
    /// (rq-0cc3c80f)
    pub fn as_str(&self) -> &'static str {
        match self {
            Program::Gaussian => "gaussian",
            Program::Orca => "orca",
            Program::Xtb => "xtb",
            Program::Mrcc => "mrcc",
            Program::Xyz => "xyz",
        }
    }
}

impl fmt::Display for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The trait every output-format reader implements. The trait is
/// `Send + Sync` so data-parallel collection can be added later without
/// a redesign (no concurrency commitment in v1).
///
/// (rq-4dc770c8)
pub trait Parser: Send + Sync {
    /// The format the parser handles.
    fn program(&self) -> Program;

    /// Claims files whose head window carries this format's signature.
    fn detect(&self, head: &str) -> bool;

    /// Turns the windows into exactly one record. A parser never panics
    /// on any content; content-level failures are returned as records
    /// with status `failed` or `truncated` and a human-readable `error`,
    /// never as `Err`.
    fn parse(&self, windows: &FileWindows) -> Record;
}

/// The error type of `ParserRegistry::parse_file`: a filesystem
/// operation failed. Every content-level outcome is `Ok` with one
/// record.
///
/// (rq-814d8aa2)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// A filesystem operation failed — a missing file, an unreadable
    /// file, a directory — mirroring `DbError::Io`.
    Io(String),
}

impl ParseError {
    fn from_io(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Io(reason) => write!(f, "parser I/O failure: {reason}"),
        }
    }
}

impl std::error::Error for ParseError {}

/// The ordered set of parsers: registration order fixes detection order.
/// `new()` yields the shipped state — an empty registry in which every
/// file is unrecognized.
///
/// (rq-b28b6e69)
#[derive(Default)]
pub struct ParserRegistry {
    parsers: Vec<Box<dyn Parser>>,
}

impl ParserRegistry {
    /// An empty registry (the shipped state).
    ///
    /// (rq-b28b6e69)
    pub fn new() -> Self {
        Self {
            parsers: Vec::new(),
        }
    }

    /// Appends a parser; registration order fixes detection order.
    ///
    /// (rq-b28b6e69)
    pub fn register(&mut self, parser: Box<dyn Parser>) {
        self.parsers.push(parser);
    }

    /// The core entry: detection on the head window, then `parse` on the
    /// matching parser, then the record-assembly rules. Callers reading
    /// from streams combine `FileWindows::from_stream` with this entry.
    /// Leaves `name` and `source_path` absent.
    ///
    /// (rq-b28b6e69 rq-e4a815f2)
    pub fn parse_windows(&self, windows: &FileWindows) -> Record {
        self.assemble(windows)
    }

    /// Reads the file with the default window sizes (seekable), then
    /// behaves as `parse_windows`, stamping `name` with the file stem and
    /// `source_path` with the path verbatim as given. Filesystem-level
    /// failures — a missing file, an unreadable file, a directory —
    /// return `Err`; every content-level outcome is `Ok` with one
    /// record.
    ///
    /// (rq-b28b6e69 rq-e4a815f2)
    pub fn parse_file(&self, path: &Path) -> Result<Record, ParseError> {
        let mut file = File::open(path).map_err(ParseError::from_io)?;
        let windows = FileWindows::from_seekable(&mut file, DEFAULT_HEAD_BYTES, DEFAULT_TAIL_BYTES)
            .map_err(ParseError::from_io)?;
        let mut record = self.assemble(&windows);
        record.name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned());
        record.source_path = Some(path.to_string_lossy().into_owned());
        Ok(record)
    }

    /// The record-assembly rules shared by `parse_windows` and
    /// `parse_file`: an empty file fails before detection; a non-empty
    /// file whose head matches no registered parser fails as
    /// unrecognized; the matched parser's record has the parser's program
    /// token stamped authoritatively and its status and error carried
    /// through verbatim, unless it fails `Record::validate` — then it is
    /// replaced by a failed record that preserves `name`, `source_path`,
    /// and `program`.
    ///
    /// (rq-e4a815f2)
    fn assemble(&self, windows: &FileWindows) -> Record {
        if windows.total_len == 0 {
            return failed_record("the file is empty");
        }

        // Detection sees only the head window; the first parser in
        // registration order whose signature matches wins.
        let Some(parser) = self
            .parsers
            .iter()
            .find(|parser| parser.detect(&windows.head))
        else {
            return failed_record("unrecognized output format");
        };

        let mut record = parser.parse(windows);
        record.program = Some(parser.program().to_string());
        // Identity is the framework's to own: `parse_file` stamps it from
        // the path; with no path, both fields leave `parse_windows` absent,
        // whatever the parser furnished.
        record.name = None;
        record.source_path = None;

        if let Err(invalidity) = record.validate() {
            return Record {
                status: RecordStatus::Failed,
                error: Some(format!("invalid record: {invalidity}")),
                name: record.name,
                source_path: record.source_path,
                program: record.program,
                ..Record::default()
            };
        }
        record
    }
}

/// A framework-generated failed record with the given error and no other
/// fields.
///
/// (rq-e4a815f2)
fn failed_record(message: &str) -> Record {
    Record {
        status: RecordStatus::Failed,
        error: Some(message.to_string()),
        ..Record::default()
    }
}
