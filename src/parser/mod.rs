//! Format detection and parser framework: every quantum-chemistry output
//! file is turned into exactly one record by content-based format
//! detection, a parser trait plus an ordered registry, a shared head and
//! tail file reader (the `windows` module), and shared extraction helpers
//! (the `extract` module). Detection inspects the file's leading lines
//! only — never its name or extension — and routes the file to the parser
//! whose signature appears earliest in the head window, breaking ties by
//! registration order. `ParserRegistry::new()`
//! is empty; `ParserRegistry::standard()` assembles the shipped set of
//! five format parsers (`rqm/parsers/`).
//!
//! (rq-42e59efe rq-b75cecef rq-1db98ae8)

pub mod extract;
mod gaussian;
mod mrcc;
mod orca;
mod windows;
mod xtb;
mod xyz;

use std::fmt;
use std::fs::File;
use std::path::Path;

use crate::record::{Record, RecordStatus};

pub use gaussian::GaussianParser;
pub use mrcc::MrccParser;
pub use orca::OrcaParser;
pub use windows::{FileWindows, DEFAULT_HEAD_BYTES, DEFAULT_TAIL_BYTES};
pub use xtb::XtbParser;
pub use xyz::XyzParser;

/// A program token: a wrapper over a `&'static str` lowercase token — the
/// value stamped into `Record::program`. Two `Program` values are equal
/// exactly when their tokens are equal. The five shipped formats are
/// published as associated constants; `Program::new` carries any other
/// token, so a parser defined elsewhere names its own format in its own
/// module.
///
/// (rq-0cc3c80f)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Program(&'static str);

impl Program {
    /// Gaussian log files: the token `"gaussian"`.
    pub const GAUSSIAN: Self = Self("gaussian");
    /// ORCA output files: the token `"orca"`.
    pub const ORCA: Self = Self("orca");
    /// xtb output files: the token `"xtb"`.
    pub const XTB: Self = Self("xtb");
    /// MRCC output files: the token `"mrcc"`.
    pub const MRCC: Self = Self("mrcc");
    /// Plain xyz geometry files: the token `"xyz"`.
    pub const XYZ: Self = Self("xyz");

    /// A token outside the shipped constants, named by its parser's own
    /// module.
    ///
    /// (rq-0cc3c80f)
    pub const fn new(token: &'static str) -> Self {
        Self(token)
    }

    /// The lowercase program token carried by records.
    ///
    /// (rq-0cc3c80f)
    pub fn as_str(&self) -> &str {
        self.0
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

    /// The byte offset, into `head`, of the earliest line that carries this
    /// format's signature, or `None` when the head carries none. A parser
    /// claims the file exactly when it returns `Some`
    /// (`rqm/parser-framework.md`).
    fn detect(&self, head: &str) -> Option<usize>;

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
    Io {
        /// The underlying failure's kind (`NotFound`, `PermissionDenied`, …).
        kind: std::io::ErrorKind,
        /// The human-readable message.
        reason: String,
    },
}

impl ParseError {
    fn from_io(error: std::io::Error) -> Self {
        Self::Io {
            kind: error.kind(),
            reason: error.to_string(),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Io { reason, .. } => write!(f, "parser I/O failure: {reason}"),
        }
    }
}

impl std::error::Error for ParseError {}

/// The ordered set of parsers: the earliest signature offset in the head
/// fixes detection, with registration order breaking equal-offset ties.
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

    /// Appends a parser; registration order breaks ties when two parsers
    /// share a signature offset.
    ///
    /// (rq-b28b6e69)
    pub fn register(&mut self, parser: Box<dyn Parser>) {
        self.parsers.push(parser);
    }

    /// The shipped parser set: one parser per supported format,
    /// registered `xtb`, `orca`, `gaussian`, `mrcc`, `xyz` — banner
    /// signatures first, the structurally-detected plain-xyz parser last.
    /// Routing is by earliest signature offset (`rqm/parser-framework.md`);
    /// registration order only breaks equal-offset ties, which a structural
    /// xyz head and a banner can never produce.
    ///
    /// (rq-212a38b5 rq-daf59b28)
    pub fn standard() -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(XtbParser));
        registry.register(Box::new(OrcaParser));
        registry.register(Box::new(GaussianParser));
        registry.register(Box::new(MrccParser));
        registry.register(Box::new(XyzParser));
        registry
    }

    /// The registered parsers' program tokens, in registration order.
    ///
    /// (rq-441c138f)
    pub fn programs(&self) -> Vec<Program> {
        self.parsers.iter().map(|parser| parser.program()).collect()
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

        // Detection sees only the head window; the parser whose signature
        // appears earliest in the head wins, and `min_by_key` keeps the
        // first parser on equal offsets — so registration order breaks a
        // same-offset tie (rqm/parser-framework.md).
        let Some(parser) = self
            .parsers
            .iter()
            .filter_map(|parser| parser.detect(&windows.head).map(|offset| (offset, parser)))
            .min_by_key(|(offset, _)| *offset)
            .map(|(_, parser)| parser)
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

/// The lines following the last visible line satisfying `header`: every
/// line from the first row-shaped one (`row` on its whitespace fields)
/// through the last consecutive row-shaped one. Leading non-row lines
/// (rules, column headings) are skipped; the first non-row line after the
/// table starts ends it. Shared by the per-format block extractors.
pub(crate) fn rows_after_last(
    windows: &FileWindows,
    header: impl Fn(&str) -> bool,
    row: impl Fn(&[&str]) -> bool,
) -> Vec<&str> {
    let mut rows: Vec<&str> = Vec::new();
    let mut started = false;
    for line in windows.after_last_line_where(&header) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if row(&fields) {
            rows.push(line);
            started = true;
        } else if started {
            break;
        }
    }
    rows
}

/// Whether a field reads as an integer.
pub(crate) fn is_integer(field: &str) -> bool {
    field.parse::<i64>().is_ok()
}

/// The byte offset, into `head`, of the earliest line satisfying
/// `predicate` (each line is a `'\n'`-delimited span with its trailing
/// `'\n'` withheld from the predicate); `None` when no visible line matches.
/// This is the public home of the detection offset: the registry compares
/// these offsets across parsers when more than one claims a head, and
/// banner-signature parsers — including external ones built through
/// `Program::new` — call it so every parser measures its signature offset the
/// same way.
///
/// (rq-166e917c)
pub fn first_line_offset(head: &str, predicate: impl Fn(&str) -> bool) -> Option<usize> {
    let mut offset = 0usize;
    for chunk in head.split_inclusive('\n') {
        if predicate(chunk.strip_suffix('\n').unwrap_or(chunk)) {
            return Some(offset);
        }
        offset += chunk.len();
    }
    None
}

/// Three whitespace-separated fields as a 3-vector of finite floats.
pub(crate) fn parse_vector3(fields: [&str; 3]) -> Option<[f64; 3]> {
    let mut vector = [0.0; 3];
    for (slot, field) in vector.iter_mut().zip(fields) {
        *slot = crate::parser::extract::parse_fortran_f64(field)?;
    }
    Some(vector)
}
