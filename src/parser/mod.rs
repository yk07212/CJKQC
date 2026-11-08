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
use std::io::{Read, Seek};
use std::path::Path;

use crate::record::{Record, RecordStatus};

pub use gaussian::GaussianParser;
pub use mrcc::MrccParser;
pub use orca::OrcaParser;
pub use windows::{
    FileWindows, LineMatches, LinePattern, TailLine, WindowBudget, DEFAULT_HEAD_BYTES,
    DEFAULT_TAIL_BYTES,
};
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
    /// `source_path` with the file's canonical absolute path (symlinks and
    /// `.`/`..` segments resolved, the Windows `\\?\` verbatim prefix
    /// stripped); a path that cannot be canonicalized keeps `source_path`
    /// verbatim as given — stamping never fails a parse. Filesystem-level
    /// failures — a missing file, an unreadable file, a directory —
    /// return `Err`; every content-level outcome is `Ok` with one
    /// record.
    ///
    /// (rq-b28b6e69 rq-e4a815f2)
    pub fn parse_file(&self, path: &Path) -> Result<Record, ParseError> {
        self.parse_file_with(path, &WindowBudget::default())
    }

    /// `parse_file` with an explicit window budget: the caller widens
    /// (or narrows) the head and tail past the framework defaults, the
    /// knob behind the CLI's `--head-bytes` and `--tail-bytes` flags.
    ///
    /// (rq-362c1c7f)
    pub fn parse_file_with(
        &self,
        path: &Path,
        budget: &WindowBudget,
    ) -> Result<Record, ParseError> {
        let mut file = File::open(path).map_err(ParseError::from_io)?;
        // Some filesystems report a directory's length as zero, where a
        // directory read would otherwise slip through the window reader
        // as an empty file; the metadata check makes a directory an `Err`
        // everywhere.
        if file.metadata().map_err(ParseError::from_io)?.is_dir() {
            return Err(ParseError::Io {
                kind: std::io::ErrorKind::IsADirectory,
                reason: "is a directory".to_string(),
            });
        }
        self.parse_opened(&mut file, path, budget)
    }

    /// The parse half of `parse_file_with` over a handle the caller has
    /// already opened — the single-open collection pipeline reads each
    /// input's classification line and its windows from one handle
    /// (`rqm/collection.md`, Input Classification). The handle has been
    /// read from, so a directory never reaches this entry; a caller
    /// that opens its own handle keeps the directory check of
    /// `parse_file_with`.
    ///
    /// (rq-362c1c7f rq-8ad5063d)
    pub(crate) fn parse_opened<S: Read + Seek>(
        &self,
        seekable: &mut S,
        path: &Path,
        budget: &WindowBudget,
    ) -> Result<Record, ParseError> {
        let windows =
            FileWindows::from_seekable(seekable, budget.head_bytes, budget.tail_bytes)
                .map_err(ParseError::from_io)?;
        let mut record = self.assemble(&windows);
        record.name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned());
        record.source_path = Some(source_path_of(path));
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

/// The value `parse_file` stamps into `Record::source_path`: the file's
/// canonical absolute path — symlinks and `.`/`..` segments resolved, the
/// Windows `\\?\` verbatim prefix stripped — so the record locates its
/// source independently of the working directory and argument spelling of
/// the invocation. A path that cannot be canonicalized keeps the path as
/// given, verbatim: stamping never fails a parse.
///
/// (rq-e4a815f2)
fn source_path_of(path: &Path) -> String {
    match path.canonicalize() {
        Ok(resolved) => strip_verbatim_prefix(&resolved)
            .to_string_lossy()
            .into_owned(),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

/// Strips the `\\?\` verbatim prefix (and folds the `\\?\UNC\` form back
/// to `\\server\share`) that `Path::canonicalize` yields on Windows, so
/// stored paths stay human-inspectable (`rqm/ARCHITECTURE.md`).
///
/// (rq-e4a815f2)
#[cfg(windows)]
fn strip_verbatim_prefix(path: &Path) -> std::path::PathBuf {
    let text = path.as_os_str().to_string_lossy();
    let stripped = match text.strip_prefix(r"\\?\") {
        Some(rest) => rest,
        None => &text,
    };
    match stripped.strip_prefix(r"UNC\") {
        Some(unc) => std::path::PathBuf::from(format!(r"\\{unc}")),
        None => std::path::PathBuf::from(stripped.to_string()),
    }
}

#[cfg(not(windows))]
fn strip_verbatim_prefix(path: &Path) -> &Path {
    path
}

/// The lines of a located span that form a table: every line from the
/// first row-shaped one (`row` on its whitespace fields) through the last
/// consecutive row-shaped one. Leading non-row lines (rules, column
/// headings) are skipped; the first non-row line after the table starts
/// ends it. Shared by the per-format block extractors, fed by the located
/// spans of one `locate` walk (`rq-15b1763d`).
pub(crate) fn rows_following<'a>(
    lines: impl Iterator<Item = &'a str>,
    row: impl Fn(&[&str]) -> bool,
) -> Vec<&'a str> {
    let mut rows: Vec<&'a str> = Vec::new();
    let mut started = false;
    for line in lines {
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

/// The seam-honesty count of the frequency fallback: the stored mode
/// count falls below 3N-6, the fewest normal modes a genuine frequency
/// table prints (a linear molecule prints 3N-5, never fewer), so the
/// table was cut by the window seam. Best-effort: a job with frozen
/// coordinates legitimately prints fewer modes and can be flagged.
///
/// Shared by the Gaussian, ORCA and xtb fallback rules
/// (`rqm/parsers/gaussian.md`, `rqm/parsers/orca.md`, `rqm/parsers/xtb.md`).
pub(crate) fn frequency_table_short(found: usize, atoms: Option<usize>) -> bool {
    atoms.is_some_and(|atoms| found < (3 * atoms).saturating_sub(6))
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

#[cfg(test)]
mod tests {
    use super::source_path_of;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// Restores the process working directory when dropped.
    struct CwdGuard(PathBuf);

    impl Drop for CwdGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.0);
        }
    }

    #[test] // rq-f7dd8d6f
    #[cfg(unix)]
    fn an_uncanonicalizable_path_falls_back_to_the_path_as_given() {
        // Deleting the process's working directory orphans its inode:
        // the directory keeps resolving as the root of a relative path
        // while the filesystem can no longer name it, so canonicalizing
        // any relative path fails and stamping falls back to the path as
        // given. (An end-to-end `parse_file` run cannot reach this state:
        // Linux refuses to create a file inside an unlinked directory,
        // and canonicalization of an openable absolute path cannot fail.)
        let base = std::env::temp_dir().join(format!(
            "cjkqc-source-path-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let inside = base.join("inside");
        fs::create_dir_all(&inside).expect("create temp directories");
        let _cwd = CwdGuard(std::env::current_dir().expect("current directory"));
        std::env::set_current_dir(&inside).expect("enter the temp directory");
        fs::remove_dir_all(&base).expect("delete the working directory");
        let stamped = source_path_of(Path::new("job.log"));
        drop(_cwd);
        assert_eq!(stamped, "job.log");
    }
}
