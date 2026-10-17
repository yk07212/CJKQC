//! Integration tests for the format-detection and parser framework, one
//! per Gherkin scenario in `rqm/parser-framework.md`. The framework is
//! exercised through small in-test parser implementations behind the
//! trait and synthetic window content; boundary scenarios use explicit
//! small window sizes.
//!
//! (rq-4da7b314)

use std::fs::{self, File};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use complied_jkqc::{
    element_symbol, parse_fortran_f64, read_geometry_table, FileWindows, LengthUnits, ParseError,
    Parser, ParserRegistry, Program, Record, RecordStatus, SymbolColumn, TableSpec,
    BOHR_TO_ANGSTROM, DEFAULT_HEAD_BYTES, DEFAULT_TAIL_BYTES,
};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "cjkqc-parser-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn write_file(dir: &Path, name: &str, content: impl AsRef<[u8]>) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, content).expect("write file");
    path
}

/// Windows of `content` as a seekable source reads them.
fn seekable_windows(
    content: impl AsRef<[u8]>,
    head_bytes: usize,
    tail_bytes: usize,
) -> FileWindows {
    let mut cursor = Cursor::new(content.as_ref());
    FileWindows::from_seekable(&mut cursor, head_bytes, tail_bytes).expect("seekable windows")
}

/// Windows of `content` as a non-seekable stream reads them.
fn streamed_windows(
    content: impl AsRef<[u8]>,
    head_bytes: usize,
    tail_bytes: usize,
) -> FileWindows {
    let mut cursor = Cursor::new(content.as_ref());
    FileWindows::from_stream(&mut cursor, head_bytes, tail_bytes).expect("streamed windows")
}

/// Content made of `lines` (each newline-terminated) plus the window
/// byte sizes that hold exactly the first `head_count` and last
/// `tail_count` lines whole.
fn line_window_sizes(
    lines: &[&str],
    head_count: usize,
    tail_count: usize,
) -> (String, usize, usize) {
    let content: String = lines.iter().map(|line| format!("{line}\n")).collect();
    let head_bytes: usize = lines[..head_count].iter().map(|line| line.len() + 1).sum();
    let tail_bytes: usize = lines[lines.len() - tail_count..]
        .iter()
        .map(|line| line.len() + 1)
        .sum();
    (content, head_bytes, tail_bytes)
}

/// An in-test parser: claims every head containing `signature` (the empty
/// signature claims any head) and returns a fixed record.
///
/// (rq-4da7b314)
struct StubParser {
    program: Program,
    signature: &'static str,
    record: Record,
}

// rq-4dc770c8
impl Parser for StubParser {
    fn program(&self) -> Program {
        self.program
    }

    fn detect(&self, head: &str) -> bool {
        head.contains(self.signature)
    }

    fn parse(&self, _windows: &FileWindows) -> Record {
        self.record.clone()
    }
}

fn register_stub(
    registry: &mut ParserRegistry,
    program: Program,
    signature: &'static str,
    record: Record,
) {
    registry.register(Box::new(StubParser {
        program,
        signature,
        record,
    }));
}

fn error_of(record: &Record) -> &str {
    record.error.as_deref().expect("error present")
}

fn source_path_of(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

// --- Detection and routing ---

#[test] // rq-a7a34dab
fn an_unregistered_format_yields_a_failed_record() {
    let dir = temp_dir();
    let path = write_file(&dir, "job.log", "arbitrary text\nnothing recognized\n");
    let registry = ParserRegistry::new();

    let record = registry.parse_file(&path).expect("exactly one record");

    assert_eq!(record.status, RecordStatus::Failed);
    assert!(
        error_of(&record).contains("unrecognized"),
        "unexpected error: {}",
        error_of(&record)
    );
    assert_eq!(record.name.as_deref(), Some("job"));
    assert_eq!(record.source_path, Some(source_path_of(&path)));
    assert_eq!(record.program, None);
}

#[test] // rq-033ff671
fn a_head_matching_a_registered_parser_is_parsed_by_it() {
    let dir = temp_dir();
    let path = write_file(&dir, "run.out", "MAGIC\nfirst line\nsecond line\n");
    let mut registry = ParserRegistry::new();
    register_stub(
        &mut registry,
        Program::XTB,
        "MAGIC",
        Record {
            electronic_energy: Some(-19.3),
            ..Record::default()
        },
    );

    let record = registry.parse_file(&path).expect("exactly one record");

    assert_eq!(record.program.as_deref(), Some("xtb"));
    assert_eq!(record.name.as_deref(), Some("run"));
    assert_eq!(record.source_path, Some(source_path_of(&path)));
    assert_eq!(record.electronic_energy, Some(-19.3));
}

#[test] // rq-761f2b95
fn detection_ignores_the_file_name_and_extension() {
    let dir = temp_dir();
    let content = "MAGIC\nsame body\n";
    let mut registry = ParserRegistry::new();
    register_stub(&mut registry, Program::XTB, "MAGIC", Record::default());

    let mut stripped = Vec::new();
    let mut source_paths = Vec::new();
    for file_name in ["a.log", "a.xyz", "a"] {
        let path = write_file(&dir, file_name, content);
        let record = registry.parse_file(&path).expect("parse file");
        assert_eq!(record.program.as_deref(), Some("xtb"));
        source_paths.push(record.source_path.clone().expect("source path"));
        stripped.push(Record {
            name: None,
            source_path: None,
            ..record
        });
    }

    // routed to the same parser: the records are identical once the
    // identity fields are removed
    assert_eq!(stripped[0], stripped[1]);
    assert_eq!(stripped[1], stripped[2]);
    assert_eq!(
        source_paths,
        ["a.log", "a.xyz", "a"]
            .iter()
            .map(|name| source_path_of(&dir.join(name)))
            .collect::<Vec<_>>()
    );
}

#[test] // rq-3562f71a
fn the_first_registered_parser_wins_a_signature_conflict() {
    let dir = temp_dir();
    let path = write_file(&dir, "conflict.out", "MAGIC\nbody\n");
    let mut registry = ParserRegistry::new();
    register_stub(&mut registry, Program::GAUSSIAN, "MAGIC", Record::default());
    register_stub(&mut registry, Program::ORCA, "MAGIC", Record::default());

    let record = registry.parse_file(&path).expect("parse file");

    assert_eq!(record.program.as_deref(), Some("gaussian"));
}

#[test] // rq-1fd7e03d
fn a_signature_beyond_the_head_window_is_not_detected() {
    let lines = [
        "filler 0", "filler 1", "filler 2", "filler 3", "MAGIC", "filler 5", "filler 6",
        "filler 7", "filler 8", "filler 9",
    ];
    let (content, head_bytes, tail_bytes) = line_window_sizes(&lines, 3, 3);
    let windows = seekable_windows(content, head_bytes, tail_bytes);
    assert!(!windows.covered());

    let mut registry = ParserRegistry::new();
    register_stub(&mut registry, Program::XTB, "MAGIC", Record::default());

    let record = registry.parse_windows(&windows);

    assert_eq!(record.status, RecordStatus::Failed);
    assert!(
        error_of(&record).contains("unrecognized"),
        "unexpected error: {}",
        error_of(&record)
    );
}

#[test] // rq-ffaf4e5b
fn a_signature_anywhere_in_a_covered_file_is_detected() {
    let dir = temp_dir();
    let lines = [
        "filler 0", "filler 1", "filler 2", "filler 3", "filler 4", "filler 5", "filler 6",
        "MAGIC", "filler 8", "filler 9",
    ];
    let content: String = lines.iter().map(|line| format!("{line}\n")).collect();
    let path = write_file(&dir, "deep.out", content);
    let mut registry = ParserRegistry::new();
    register_stub(&mut registry, Program::XTB, "MAGIC", Record::default());

    let record = registry.parse_file(&path).expect("parse file");

    assert_eq!(record.program.as_deref(), Some("xtb"));
}

// --- Window reading ---

#[test] // rq-5ab5050d
fn a_file_within_the_window_sizes_is_read_whole() {
    let content: String = (0..10).map(|index| format!("row {index}\n")).collect();
    let line_bytes = "row 0\n".len();

    let windows = seekable_windows(&content, 16 * line_bytes, 16 * line_bytes);

    assert!(windows.covered());
    assert_eq!(windows.head, content);
    assert_eq!(windows.tail, content);
}

#[test] // rq-3172eb32
fn a_large_file_yields_disjoint_line_aligned_windows() {
    let lines: Vec<String> = (0..20).map(|index| format!("line {index:02}")).collect();
    let content: String = lines.iter().map(|line| format!("{line}\n")).collect();
    let head_bytes: usize = lines[..4].iter().map(|line| line.len() + 1).sum();
    let tail_bytes: usize = lines[16..].iter().map(|line| line.len() + 1).sum();

    let windows = seekable_windows(content, head_bytes, tail_bytes);

    assert!(!windows.covered());
    assert_eq!(windows.head, "line 00\nline 01\nline 02\nline 03\n");
    assert_eq!(windows.tail, "line 16\nline 17\nline 18\nline 19\n");
    let head_lines: Vec<&str> = windows.head.lines().collect();
    let tail_lines: Vec<&str> = windows.tail.lines().collect();
    assert_eq!(head_lines.len(), 4);
    assert_eq!(tail_lines.len(), 4);
    assert!(head_lines.iter().all(|line| !tail_lines.contains(line)));
}

#[test] // rq-a37342d6
fn the_tail_keeps_the_final_unterminated_line() {
    let mut content: String = (0..10).map(|index| format!("row {index}\n")).collect();
    content.push_str("final unterminated line");
    let line_bytes = "row 0\n".len();
    let final_line = "final unterminated line";

    let windows = seekable_windows(&content, 2 * line_bytes, 2 * line_bytes + final_line.len());

    assert!(!windows.covered());
    assert!(windows.tail.ends_with(final_line));
    assert!(!windows.tail.ends_with('\n'));
    assert_eq!(windows.tail.lines().last(), Some(final_line));
}

#[test] // rq-5996dff9
fn partial_lines_at_the_window_seams_are_dropped() {
    let content = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n";

    let windows = seekable_windows(content, 3, 3);

    assert!(!windows.covered());
    assert_eq!(windows.head, "a\n");
    assert_eq!(windows.tail, "j\n");
}

#[test] // rq-e8bb37c3
fn invalid_utf8_bytes_become_replacement_characters() {
    let content = b"\xff\xfeMAGIC banner\nplain body\n";

    let windows = seekable_windows(content, 1024, 1024);

    assert!(windows.covered());
    assert!(windows.head.contains('\u{FFFD}'));
    assert!(windows.head.contains("MAGIC banner"));
}

#[test] // rq-0bd419c3
fn streamed_and_seekable_reads_produce_identical_windows() {
    let content: String = (0..10).map(|index| format!("row {index}\n")).collect();
    let line_bytes = "row 0\n".len();

    for (head_bytes, tail_bytes) in [
        (16 * line_bytes, 16 * line_bytes),
        (2 * line_bytes, 2 * line_bytes),
    ] {
        let seekable = seekable_windows(&content, head_bytes, tail_bytes);
        let streamed = streamed_windows(&content, head_bytes, tail_bytes);
        assert_eq!(seekable.head, streamed.head);
        assert_eq!(seekable.tail, streamed.tail);
        assert_eq!(seekable.covered(), streamed.covered());

        // the 16-line size covers the 10-line file, the 2-line size
        // exceeds it: both constructions agree on both
        assert_eq!(seekable.covered(), head_bytes == 16 * line_bytes);
    }
}

// --- Record assembly ---

#[test] // rq-662d12db
fn parse_windows_stamps_the_program_but_not_the_identity() {
    let windows = seekable_windows("MAGIC\nbody\n", 64, 64);
    let mut registry = ParserRegistry::new();
    register_stub(&mut registry, Program::XYZ, "MAGIC", Record::default());

    let record = registry.parse_windows(&windows);

    assert_eq!(record.program.as_deref(), Some("xyz"));
    assert_eq!(record.name, None);
    assert_eq!(record.source_path, None);
}

#[test] // rq-e4a815f2
fn parse_windows_strips_identity_furnished_by_the_parser() {
    let windows = seekable_windows("MAGIC\nbody\n", 64, 64);
    let mut registry = ParserRegistry::new();
    register_stub(
        &mut registry,
        Program::XYZ,
        "MAGIC",
        Record {
            name: Some("hijacked".to_string()),
            source_path: Some("/somewhere/hijacked.log".to_string()),
            ..Record::default()
        },
    );

    let record = registry.parse_windows(&windows);

    assert_eq!(record.program.as_deref(), Some("xyz"));
    assert_eq!(record.name, None);
    assert_eq!(record.source_path, None);
}

#[test] // rq-219164a5
fn the_parsers_status_and_error_are_preserved_verbatim() {
    let dir = temp_dir();
    let path = write_file(&dir, "scf.out", "MAGIC\nbody\n");
    let mut registry = ParserRegistry::new();
    register_stub(
        &mut registry,
        Program::XTB,
        "MAGIC",
        Record {
            status: RecordStatus::Failed,
            error: Some("SCF did not converge".to_string()),
            ..Record::default()
        },
    );

    let record = registry.parse_file(&path).expect("parse file");

    assert_eq!(record.status, RecordStatus::Failed);
    assert_eq!(record.error.as_deref(), Some("SCF did not converge"));
}

#[test] // rq-dce37e6e
fn a_parser_record_that_fails_validation_is_replaced() {
    let dir = temp_dir();
    let path = write_file(&dir, "bad.log", "MAGIC\nbody\n");
    let mut registry = ParserRegistry::new();
    register_stub(
        &mut registry,
        Program::ORCA,
        "MAGIC",
        Record {
            electronic_energy: Some(f64::NAN),
            ..Record::default()
        },
    );

    let record = registry.parse_file(&path).expect("parse file");

    assert_eq!(record.status, RecordStatus::Failed);
    let error = error_of(&record);
    assert!(
        error.contains("invalid record"),
        "unexpected error: {error}"
    );
    assert!(
        error.contains("electronic_energy"),
        "violated invariant not named: {error}"
    );
    assert_eq!(record.name.as_deref(), Some("bad"));
    assert_eq!(record.source_path, Some(source_path_of(&path)));
    assert_eq!(record.program.as_deref(), Some("orca"));
    assert_eq!(record.electronic_energy, None);
}

#[test] // rq-58b86d07
fn an_empty_file_yields_a_failed_record() {
    let dir = temp_dir();
    let path = write_file(&dir, "empty.log", b"");
    let mut registry = ParserRegistry::new();
    // the empty signature claims any head
    register_stub(&mut registry, Program::XTB, "", Record::default());

    let record = registry.parse_file(&path).expect("parse file");

    assert_eq!(record.status, RecordStatus::Failed);
    assert!(
        error_of(&record).contains("empty"),
        "unexpected error: {}",
        error_of(&record)
    );
    assert_eq!(record.name.as_deref(), Some("empty"));
    assert_eq!(record.source_path, Some(source_path_of(&path)));
    assert_eq!(record.program, None);
}

#[test] // rq-447f650f
fn a_missing_file_is_an_io_error() {
    let registry = ParserRegistry::new();

    let result = registry.parse_file(Path::new("no/such.log"));

    assert!(matches!(
        result,
        Err(ParseError::Io {
            kind: std::io::ErrorKind::NotFound,
            ..
        })
    ));
}

#[test] // rq-86740b39
fn a_parser_from_outside_the_shipped_set_carries_its_own_token() {
    let dir = temp_dir();
    let path = write_file(&dir, "run.out", "MAGIC\nfirst line\n");
    let mut registry = ParserRegistry::new();
    register_stub(
        &mut registry,
        Program::new("turbomole"),
        "MAGIC",
        Record::default(),
    );

    let record = registry.parse_file(&path).expect("exactly one record");

    assert_eq!(record.program.as_deref(), Some("turbomole"));
}

#[test] // rq-f87aad94
fn a_directory_is_an_io_error() {
    let dir = temp_dir();
    let somedir = dir.join("somedir");
    fs::create_dir(&somedir).expect("create directory");
    let registry = ParserRegistry::new();

    let result = registry.parse_file(&somedir);

    assert!(matches!(result, Err(ParseError::Io { .. })));
}

// --- Line search across windows ---

#[test] // rq-f4a74fc9
fn last_line_where_prefers_the_match_in_the_tail() {
    let lines = [
        "MARK in head",
        "plain 01",
        "plain 02",
        "plain 03",
        "plain 04",
        "plain 05",
        "plain 06",
        "plain 07",
        "MARK in tail",
        "plain 09",
    ];
    let (content, head_bytes, tail_bytes) = line_window_sizes(&lines, 2, 2);
    let windows = seekable_windows(content, head_bytes, tail_bytes);
    assert!(!windows.covered());

    let line = windows
        .last_line_where(|line| line.contains("MARK"))
        .expect("match");

    assert_eq!(line, "MARK in tail");
}

#[test] // rq-4aa069d5
fn last_line_where_falls_back_to_the_head() {
    let lines = [
        "MARK in head",
        "plain 01",
        "plain 02",
        "plain 03",
        "plain 04",
        "plain 05",
        "plain 06",
        "plain 07",
        "plain 08",
        "plain 09",
    ];
    let (content, head_bytes, tail_bytes) = line_window_sizes(&lines, 2, 2);
    let windows = seekable_windows(content, head_bytes, tail_bytes);
    assert!(!windows.covered());

    let line = windows
        .last_line_where(|line| line.contains("MARK"))
        .expect("match");

    assert_eq!(line, "MARK in head");
}

#[test] // rq-eb75bf8e
fn first_line_where_prefers_the_match_in_the_head() {
    let lines = [
        "MARK in head",
        "plain 01",
        "plain 02",
        "plain 03",
        "plain 04",
        "plain 05",
        "plain 06",
        "plain 07",
        "MARK in tail",
        "plain 09",
    ];
    let (content, head_bytes, tail_bytes) = line_window_sizes(&lines, 2, 2);
    let windows = seekable_windows(content, head_bytes, tail_bytes);
    assert!(!windows.covered());

    let line = windows
        .first_line_where(|line| line.contains("MARK"))
        .expect("match");

    assert_eq!(line, "MARK in head");
}

#[test] // rq-fb368603
fn first_line_where_falls_back_to_the_tail() {
    let lines = [
        "plain 00",
        "plain 01",
        "plain 02",
        "plain 03",
        "plain 04",
        "plain 05",
        "plain 06",
        "plain 07",
        "MARK in tail",
        "plain 09",
    ];
    let (content, head_bytes, tail_bytes) = line_window_sizes(&lines, 2, 2);
    let windows = seekable_windows(content, head_bytes, tail_bytes);
    assert!(!windows.covered());

    let line = windows
        .first_line_where(|line| line.contains("MARK"))
        .expect("match");

    assert_eq!(line, "MARK in tail");
}

#[test] // rq-bdb74057
fn lines_in_the_elided_middle_are_invisible() {
    let lines = [
        "plain 00",
        "plain 01",
        "plain 02",
        "plain 03",
        "MARK middle",
        "plain 05",
        "plain 06",
        "plain 07",
        "plain 08",
        "plain 09",
    ];
    let (content, head_bytes, tail_bytes) = line_window_sizes(&lines, 2, 2);
    let windows = seekable_windows(content, head_bytes, tail_bytes);
    assert!(!windows.covered());

    assert_eq!(windows.first_line_where(|line| line.contains("MARK")), None);
    assert_eq!(windows.last_line_where(|line| line.contains("MARK")), None);
}

#[test] // rq-be817932
fn after_last_line_where_stops_at_the_window_boundary() {
    let lines = [
        "MARK anchor",
        "follows one",
        "follows two",
        "plain 03",
        "plain 04",
        "plain 05",
        "plain 06",
        "plain 07",
        "tail eight",
        "tail nine",
    ];
    let (content, head_bytes, tail_bytes) = line_window_sizes(&lines, 3, 2);
    let windows = seekable_windows(content, head_bytes, tail_bytes);
    assert!(!windows.covered());

    let following: Vec<&str> = windows
        .after_last_line_where(|line| line.contains("MARK"))
        .collect();

    assert_eq!(following, ["follows one", "follows two"]);

    // the same anchor seen as the first match yields the same lines
    let first_following: Vec<&str> = windows
        .after_first_line_where(|line| line.contains("MARK"))
        .collect();
    assert_eq!(first_following, ["follows one", "follows two"]);
}

// --- Geometry table extraction ---

fn table_spec(
    symbol_column: SymbolColumn,
    units: LengthUnits,
    expected_rows: Option<usize>,
) -> TableSpec {
    TableSpec {
        symbol_column,
        coordinate_columns: [1, 2, 3],
        units,
        expected_rows,
    }
}

#[test] // rq-aeb8b3a8
fn a_symbol_table_in_angstrom_parses_to_a_geometry() {
    let lines = ["h  0.0  0.0  0.9", "cl 1.5  -2.0  3.25"];
    let spec = table_spec(SymbolColumn::Symbol(0), LengthUnits::Angstrom, Some(2));

    let geometry = read_geometry_table(lines, &spec).expect("geometry");

    assert_eq!(geometry.symbols, ["H", "Cl"]);
    assert_eq!(geometry.coordinates, [0.0, 0.0, 0.9, 1.5, -2.0, 3.25]);
}

#[test] // rq-383eea52
fn an_atomic_number_column_maps_to_element_symbols() {
    let lines = ["1  0.0 0.0 0.9", "17 1.0 2.0 3.0", "35 4.0 5.0 6.0"];
    let spec = table_spec(
        SymbolColumn::AtomicNumber(0),
        LengthUnits::Angstrom,
        Some(3),
    );

    let geometry = read_geometry_table(lines, &spec).expect("geometry");

    assert_eq!(geometry.symbols, ["H", "Cl", "Br"]);
    assert_eq!(
        geometry.coordinates,
        [0.0, 0.0, 0.9, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0]
    );
}

#[test] // rq-18bc1945
fn bohr_coordinates_are_converted_to_angstrom() {
    let lines = ["C 0.0 0.0 2.0", "O 1.0 0.0 0.0"];
    let spec = table_spec(SymbolColumn::Symbol(0), LengthUnits::Bohr, None);

    let geometry = read_geometry_table(lines, &spec).expect("geometry");

    assert_eq!(
        geometry.coordinates,
        [
            0.0,
            0.0,
            2.0 * BOHR_TO_ANGSTROM,
            1.0 * BOHR_TO_ANGSTROM,
            0.0,
            0.0
        ]
    );
}

#[test] // rq-d737472c
fn a_table_ends_at_its_first_non_row_line() {
    let lines = [
        "H  0.0 0.0 0.0",
        "O  1.0 1.0 1.0",
        "",
        "Total energy = -76.010",
        "unrelated footer text",
    ];
    let spec = table_spec(SymbolColumn::Symbol(0), LengthUnits::Angstrom, Some(2));

    let geometry = read_geometry_table(lines, &spec).expect("geometry");

    assert_eq!(geometry.symbols, ["H", "O"]);
    assert_eq!(geometry.coordinates, [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
}

#[test] // rq-582aea7a
fn a_row_count_below_expected_rows_yields_none() {
    let spec = table_spec(SymbolColumn::Symbol(0), LengthUnits::Angstrom, Some(3));

    // simply short
    assert!(read_geometry_table(["H 0.0 0.0 0.0", "O 1.0 1.0 1.0"], &spec).is_none());
    // truncated by the file's end: zero rows
    assert!(read_geometry_table(std::iter::empty::<&str>(), &spec).is_none());
    // cut by a window boundary: the rows past the cut are invisible
    let table = ["H 0.0 0.0 0.0", "O 1.0 1.0 1.0", "N 2.0 2.0 2.0"];
    assert!(read_geometry_table(table[..2].iter().copied(), &spec).is_none());
    // the complete table parses
    assert!(read_geometry_table(table, &spec).is_some());
}

// --- Number and element helpers ---

#[test] // rq-ed0686aa
// the requirements scenario spells the value as `3.14` verbatim
#[allow(clippy::approx_constant)]
fn fortran_float_parsing_accepts_e_and_d_exponents_in_either_case() {
    assert_eq!(parse_fortran_f64("1.2345678D-05"), Some(1.2345678e-05));
    assert_eq!(parse_fortran_f64("1.2345678d-05"), Some(1.2345678e-05));
    assert_eq!(parse_fortran_f64("1.2345678E-05"), Some(1.2345678e-05));
    assert_eq!(parse_fortran_f64("-1.0D+03"), Some(-1000.0));
    assert_eq!(parse_fortran_f64("3.14"), Some(3.14));
}

#[test] // rq-017a3a9c
fn fortran_float_parsing_rejects_non_numeric_and_non_finite_fields() {
    assert_eq!(parse_fortran_f64("abc"), None);
    assert_eq!(parse_fortran_f64(""), None);
    assert_eq!(parse_fortran_f64("NaN"), None);
    assert_eq!(parse_fortran_f64("Inf"), None);
}

#[test] // rq-b9209565
fn atomic_numbers_map_to_title_case_element_symbols() {
    assert_eq!(element_symbol(1), Some("H".to_string()));
    assert_eq!(element_symbol(17), Some("Cl".to_string()));
    assert_eq!(element_symbol(118), Some("Og".to_string()));
}

#[test] // rq-39966431
fn atomic_numbers_outside_1_to_118_map_to_nothing() {
    assert_eq!(element_symbol(0), None);
    assert_eq!(element_symbol(119), None);
}

// --- Window honesty over the fixture corpus ---

fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read fixture dir") {
        let path = entry.expect("fixture entry").path();
        if path.is_dir() {
            collect_files(&path, files);
        } else {
            files.push(path);
        }
    }
}

#[test] // rq-f61c0532
fn default_windows_cover_every_file_of_the_fixture_corpus() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir");
    let fixtures = Path::new(&manifest_dir).join("tests").join("fixtures");
    let mut files = Vec::new();
    collect_files(&fixtures, &mut files);
    assert!(!files.is_empty(), "fixture corpus not found");

    for file in &files {
        let mut handle = File::open(file).expect("open fixture");
        let windows =
            FileWindows::from_seekable(&mut handle, DEFAULT_HEAD_BYTES, DEFAULT_TAIL_BYTES)
                .expect("read fixture windows");
        assert!(windows.covered(), "fixture not covered: {}", file.display());
    }
}
