//! Integration tests for the CJKQC command line: the scaffold behavior
//! from `rqm/scaffolding.md` and the collection scenarios from
//! `rqm/collection.md`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use compiled_jkqc::db::load_database;
use compiled_jkqc::{Components, Correction, Record, RecordStatus};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

// rq-e61c2633
fn cjkqc(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_CJKQC"))
        .args(args)
        .output()
        .expect("failed to start CJKQC")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// The whitespace-separated cells of a table line: aligned tables pad
/// their cells, so cell comparisons split on runs of whitespace.
fn cells(line: &str) -> Vec<&str> {
    line.split_whitespace().collect()
}

/// A table's lines as whitespace-split cells, the `cells` view of every
/// line: aligned tables pad their cells, so cell comparisons split on
/// runs of whitespace.
fn rows(text: &str) -> Vec<Vec<&str>> {
    text.trim().lines().map(cells).collect()
}

/// The line index of the first line whose cells equal `header`, so a
/// section header is found however wide its aligned columns print.
fn find_line(text: &str, header: &[&str]) -> Option<usize> {
    text.lines().position(|line| cells(line) == header)
}

/// The substring of `text` from the first line whose cells equal
/// `header` onward, so a section's rows are sliced whatever widths the
/// aligned table printed.
fn section_from<'a>(text: &'a str, header: &[&str]) -> &'a str {
    let mut offset = 0usize;
    for line in text.lines() {
        if cells(line) == header {
            return &text[offset..];
        }
        offset += line.len() + 1;
    }
    panic!("no {header:?} section in:\n{text}");
}

/// The (start, end) byte spans of a line's whitespace-separated cells,
/// so alignment (where a cell sits, not just what it says) is checked.
fn cell_spans(line: &str) -> Vec<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut spans = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i > start {
            spans.push((start, i));
        }
    }
    spans
}

#[test] // rq-f9a3759e
fn help_prints_usage() {
    let out = cjkqc(&["--help"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("Usage: CJKQC"));
}

#[test] // rq-e1c51ff7
fn short_help_flag_behaves_like_help() {
    let out = cjkqc(&["-h"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("Usage: CJKQC"));
}

#[test] // rq-1bbcf87c
fn version_prints_the_crate_version() {
    let out = cjkqc(&["--version"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("CJKQC"));
    assert!(text.contains(env!("CARGO_PKG_VERSION")));
}

#[test] // rq-843d152e
fn short_version_flag_behaves_like_version() {
    let out = cjkqc(&["-V"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains(env!("CARGO_PKG_VERSION")));
}

#[test] // rq-72e0ded9
fn help_takes_precedence_over_extra_arguments() {
    let out = cjkqc(&["--help", "extra"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("Usage: CJKQC"));
}

#[test] // rq-953a8521
fn version_takes_precedence_over_extra_arguments() {
    let out = cjkqc(&["--version", "extra"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains(env!("CARGO_PKG_VERSION")));
}

#[test] // rq-6b872f89
fn no_arguments_is_a_usage_error() {
    let out = cjkqc(&[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("Usage: CJKQC"));
}

#[test] // rq-be35a7b3
fn positional_arguments_without_out_are_a_usage_error() {
    let out = cjkqc(&["output.log", "output2.out"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("--out"));
}

#[test] // rq-e003541d
fn unknown_jkqc_flags_are_usage_errors() {
    let out = cjkqc(&["output.log", "-el"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("unexpected argument"),
        "{}",
        stderr(&out)
    );
}

#[test] // rq-bd59ba92
fn help_advertises_the_collection_surface() {
    let out = cjkqc(&["--help"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("--help"));
    assert!(text.contains("--version"));
    assert!(text.contains("--out"), "help must list --out: {text}");
    assert!(
        text.contains("INPUTS"),
        "help must list the positional input argument: {text}"
    );
}

#[test] // rq-2486bcc8
fn build_produces_the_exact_executable_name() {
    let path = Path::new(env!("CARGO_BIN_EXE_CJKQC"));
    let expected = format!("CJKQC{}", std::env::consts::EXE_SUFFIX);
    assert_eq!(
        path.file_name().and_then(|name| name.to_str()),
        Some(expected.as_str())
    );
    assert!(path.is_file());
}

#[test] // rq-22fd07a1
fn no_placeholder_canvas_remains() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let self_path = manifest.join(file!());
    let mut files = vec![manifest.join("Cargo.toml")];
    for dir in ["src", "tests"] {
        for entry in fs::read_dir(manifest.join(dir))
            .unwrap_or_else(|error| panic!("failed to list {dir}/: {error}"))
        {
            let path = entry
                .unwrap_or_else(|error| panic!("failed to read a {dir}/ entry: {error}"))
                .path();
            if path.extension().is_some_and(|ext| ext == "rs") && path != self_path {
                files.push(path);
            }
        }
    }
    for file in files {
        let content = fs::read_to_string(&file)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", file.display()));
        assert!(
            !content.contains("canvas"),
            "{} still references the template placeholder",
            file.display()
        );
    }
}

// --- collection scenarios (rqm/collection.md) ---

fn temp_dir() -> TempDir {
    let dir = std::env::temp_dir().join(format!(
        "cjkqc-cli-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    TempDir(dir)
}

/// A temporary directory deleted when the owning test ends.
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl AsRef<Path> for TempDir {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl std::ops::Deref for TempDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

fn fixture(relative: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(relative)
        .to_string_lossy()
        .into_owned()
}

fn write_jsonl(path: &Path, lines: &[&str]) {
    let content: String = lines.iter().map(|line| format!("{line}\n")).collect();
    fs::write(path, content).expect("write database");
}

fn sample_line(name: &str, energy: f64) -> String {
    let record = Record {
        name: Some(name.to_string()),
        electronic_energy: Some(energy),
        ..Record::default()
    };
    serde_json::to_string(&record).expect("serialization")
}

fn loaded(path: &Path) -> Vec<Record> {
    load_database(path).expect("load database").records
}

#[test] // rq-3b9e8002
fn collect_one_xtb_output_into_a_new_database() {
    let dir = temp_dir();
    let out = dir.join("db.jsonl");
    let out_text = out.to_string_lossy().into_owned();
    let run = cjkqc(&[&fixture("xtb/1dma.log"), "--out", &out_text]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(stdout(&run).contains(&format!(
        "collected 1 records into {out_text} (0 failed, 0 truncated)"
    )));
    let records = loaded(&out);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].program.as_deref(), Some("xtb"));
    assert_eq!(records[0].name.as_deref(), Some("1dma"));
    assert_eq!(
        records[0].components.as_ref().and_then(|c| c.get("dma")),
        Some(&1)
    );
}

#[test] // rq-31d988e1
fn multiple_inputs_are_written_in_argument_order() {
    let dir = temp_dir();
    let out = dir.join("db.jsonl");
    let out_text = out.to_string_lossy().into_owned();
    let run = cjkqc(&[
        &fixture("xtb/1dma.log"),
        &fixture("orca/1pinic1sa-110_1_9-str9.out"),
        "--out",
        &out_text,
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let records = loaded(&out);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].program.as_deref(), Some("xtb"));
    assert_eq!(records[1].program.as_deref(), Some("orca"));
    let second = &records[1].components;
    assert_eq!(
        second.as_ref().and_then(|c| c.get("pinic")),
        Some(&1),
        "second record components: {second:?}"
    );
    assert_eq!(
        second.as_ref().and_then(|c| c.get("sa")),
        Some(&1),
        "second record components: {second:?}"
    );
}

#[test] // rq-e2843432
fn a_non_interactive_run_refuses_to_replace_an_existing_database() {
    let dir = temp_dir();
    let out = dir.join("db.jsonl");
    let out_text = out.to_string_lossy().into_owned();
    let first = cjkqc(&[&fixture("xtb/1dma.log"), "--out", &out_text]);
    assert_eq!(first.status.code(), Some(0));
    // The CLI test harness runs with stdin piped, not a terminal: the
    // question is never asked and the replacement is refused.
    let second = cjkqc(&[&fixture("xyz/h2o.xyz"), "--out", &out_text]);
    assert_eq!(second.status.code(), Some(1), "{}", stderr(&second));
    let records = loaded(&out);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].program.as_deref(), Some("xtb"));
    assert!(stderr(&second).contains(&format!(
        "error: {out_text} exists; not interactive; use --fout to force"
    )));
}

#[test] // rq-503eebe7
fn fout_forces_the_replacement_without_asking() {
    let dir = temp_dir();
    let out = dir.join("db.jsonl");
    let out_text = out.to_string_lossy().into_owned();
    cjkqc(&[&fixture("xtb/1dma.log"), "--out", &out_text]);
    let second = cjkqc(&[&fixture("xyz/h2o.xyz"), "--out", &out_text, "--fout"]);
    assert_eq!(second.status.code(), Some(0), "{}", stderr(&second));
    assert_eq!(loaded(&out).len(), 1);
    let errput = stderr(&second);
    assert!(!errput.contains("overwrite?"), "{errput}");
    assert!(!errput.contains("warning"), "{errput}");
}

#[test] // rq-02fe3cee
fn a_new_target_is_written_without_any_question() {
    let dir = temp_dir();
    let out = dir.join("db.jsonl");
    let out_text = out.to_string_lossy().into_owned();
    let run = cjkqc(&[&fixture("xtb/1dma.log"), "--out", &out_text]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(!stderr(&run).contains("overwrite?"), "{}", stderr(&run));
}

#[test] // rq-e4f87293
fn a_truncated_output_is_stored_as_a_truncated_record() {
    let dir = temp_dir();
    let out = dir.join("db.jsonl");
    let out_text = out.to_string_lossy().into_owned();
    let run = cjkqc(&[&fixture("xtb/1dma-truncated.log"), "--out", &out_text]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let records = loaded(&out);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].status, RecordStatus::Truncated);
    assert!(stdout(&run).contains("(0 failed, 1 truncated)"));
}

#[test] // rq-180cebd1
fn an_unrecognized_output_aborts_the_run_and_writes_nothing() {
    let dir = temp_dir();
    let junk = dir.join("junk.txt");
    fs::write(&junk, "not a QC output at all\n").expect("write junk");
    let out = dir.join("db.jsonl");
    let run = cjkqc(&[&junk.to_string_lossy(), "--out", &out.to_string_lossy()]);
    assert_eq!(run.status.code(), Some(1));
    assert!(stderr(&run).contains("junk.txt"), "{}", stderr(&run));
    assert!(!out.exists());
}

#[test] // rq-e50ec596
fn an_empty_input_file_aborts_the_run_and_writes_nothing() {
    let dir = temp_dir();
    let empty = dir.join("empty.log");
    fs::write(&empty, "").expect("write empty file");
    let out = dir.join("db.jsonl");
    let run = cjkqc(&[&empty.to_string_lossy(), "--out", &out.to_string_lossy()]);
    assert_eq!(run.status.code(), Some(1));
    assert!(stderr(&run).contains("empty.log"), "{}", stderr(&run));
    assert!(!out.exists());
}

#[test] // rq-276adb9e
fn a_missing_input_is_fatal_and_writes_nothing() {
    let dir = temp_dir();
    let out = dir.join("db.jsonl");
    let missing = dir.join("nope.log");
    let run = cjkqc(&[
        &missing.to_string_lossy(),
        &fixture("xtb/1dma.log"),
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(1));
    assert!(stderr(&run).contains("nope.log"), "{}", stderr(&run));
    assert!(!out.exists());
}

#[test] // rq-bedf480a
fn a_directory_input_is_fatal() {
    let dir = temp_dir();
    let run = cjkqc(&[
        &fixture("xtb"),
        "--out",
        &dir.join("db.jsonl").to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
}

#[test] // rq-0a2ed6f8
fn an_unwritable_output_directory_is_fatal() {
    let dir = temp_dir();
    let run = cjkqc(&[
        &fixture("xtb/1dma.log"),
        "--out",
        &dir.join("no/such/dir/db.jsonl").to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
}

#[test] // rq-12d931b9
fn classification_ignores_the_file_name() {
    let dir = temp_dir();
    let weird = dir.join("weird.dat");
    fs::copy(fixture("xtb/1dma.log"), &weird).expect("copy fixture");
    let out = dir.join("db.jsonl");
    let run = cjkqc(&[&weird.to_string_lossy(), "--out", &out.to_string_lossy()]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let records = loaded(&out);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].program.as_deref(), Some("xtb"));
}

#[test] // rq-53463f17
fn databases_merge_in_argument_order() {
    let dir = temp_dir();
    let a = dir.join("a.jsonl");
    let b = dir.join("b.jsonl");
    let merged = dir.join("merged.jsonl");
    write_jsonl(
        &a,
        &[
            sample_line("A1", -1.0).as_str(),
            sample_line("A2", -2.0).as_str(),
        ],
    );
    write_jsonl(&b, &[sample_line("B1", -3.0).as_str()]);
    let run = cjkqc(&[
        &a.to_string_lossy(),
        &b.to_string_lossy(),
        "--out",
        &merged.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let names: Vec<String> = loaded(&merged)
        .iter()
        .map(|record| record.name.clone().unwrap_or_default())
        .collect();
    assert_eq!(names, ["A1", "A2", "B1"]);
    assert!(stdout(&run).contains("merged 3 records into"));
}

#[test] // rq-34231bdc
fn merge_truncates_an_existing_output() {
    let dir = temp_dir();
    let a = dir.join("a.jsonl");
    let old = dir.join("old.jsonl");
    let old_lines: Vec<String> = (0..5).map(|i| sample_line("OLD", -f64::from(i))).collect();
    write_jsonl(&a, &[sample_line("A1", -1.0).as_str()]);
    let old_refs: Vec<&str> = old_lines.iter().map(String::as_str).collect();
    write_jsonl(&old, &old_refs);
    let run = cjkqc(&[
        &a.to_string_lossy(),
        "--out",
        &old.to_string_lossy(),
        "--fout",
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(loaded(&old).len(), 1);
}

#[test] // rq-8f9a701f
fn merge_can_write_onto_one_of_its_inputs() {
    let dir = temp_dir();
    let a = dir.join("a.jsonl");
    let b = dir.join("b.jsonl");
    write_jsonl(&a, &[sample_line("A1", -1.0).as_str()]);
    write_jsonl(&b, &[sample_line("B1", -2.0).as_str()]);
    let run = cjkqc(&[
        &a.to_string_lossy(),
        &b.to_string_lossy(),
        "--out",
        &a.to_string_lossy(),
        "--fout",
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let names: Vec<String> = loaded(&a)
        .iter()
        .map(|record| record.name.clone().unwrap_or_default())
        .collect();
    assert_eq!(names, ["A1", "B1"]);
}

#[test] // rq-d431309b
fn a_corrupt_line_in_a_merged_input_is_skipped_and_reported() {
    let dir = temp_dir();
    let a = dir.join("a.jsonl");
    let merged = dir.join("merged.jsonl");
    write_jsonl(&a, &[sample_line("A1", -1.0).as_str(), "not valid json"]);
    let run = cjkqc(&[&a.to_string_lossy(), "--out", &merged.to_string_lossy()]);
    assert_eq!(run.status.code(), Some(1));
    let warnings = stderr(&run);
    assert!(warnings.contains("a.jsonl"), "{warnings}");
    assert!(warnings.contains("line 2"), "{warnings}");
    assert_eq!(loaded(&merged).len(), 1);
}

#[test] // rq-b51f8bc8
fn merge_copies_records_verbatim() {
    let dir = temp_dir();
    let a = dir.join("a.jsonl");
    let merged = dir.join("merged.jsonl");
    write_jsonl(
        &a,
        &[r#"{"status":"ok","name":"2am3sa","electronic_energy":-1.0}"#],
    );
    let run = cjkqc(&[&a.to_string_lossy(), "--out", &merged.to_string_lossy()]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let records = loaded(&merged);
    assert_eq!(records[0].name.as_deref(), Some("2am3sa"));
    assert_eq!(records[0].components, None);
}

#[test] // rq-45238f22
fn a_database_is_recognized_by_content_not_extension() {
    let dir = temp_dir();
    let sneaky = dir.join("sneaky.dat");
    write_jsonl(&sneaky, &[sample_line("A1", -1.0).as_str()]);
    let merged = dir.join("merged.jsonl");
    let run = cjkqc(&[
        &sneaky.to_string_lossy(),
        "--out",
        &merged.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(
        stdout(&run).contains("merged 1 records into"),
        "{}",
        stdout(&run)
    );
}

#[test] // rq-cdf88b68
fn mixed_inputs_are_refused() {
    let dir = temp_dir();
    let a = dir.join("a.jsonl");
    let out = dir.join("out.jsonl");
    write_jsonl(&a, &[sample_line("A1", -1.0).as_str()]);
    let log = fixture("xtb/1dma.log");
    let run = cjkqc(&[&a.to_string_lossy(), &log, "--out", &out.to_string_lossy()]);
    assert_eq!(run.status.code(), Some(2));
    let error = stderr(&run);
    assert!(error.contains("a.jsonl"), "{error}");
    assert!(error.contains("1dma.log"), "{error}");
    assert!(!out.exists());
}

#[test] // rq-f0785bb1
fn a_missing_out_is_a_usage_error() {
    let run = cjkqc(&[&fixture("xtb/1dma.log")]);
    assert_eq!(run.status.code(), Some(2));
    assert!(stderr(&run).contains("--out"), "{}", stderr(&run));
}

#[test] // rq-6cc1264d
fn adopted_jkqc_era_unknown_flags_remain_usage_errors() {
    let dir = temp_dir();
    let run = cjkqc(&[
        &fixture("xtb/1dma.log"),
        "--out",
        &dir.join("db.jsonl").to_string_lossy(),
        "-el",
    ]);
    assert_eq!(run.status.code(), Some(2));
    assert!(
        stderr(&run).contains("unexpected argument"),
        "{}",
        stderr(&run)
    );
}

#[test] // rq-54536800
fn a_dash_argument_is_an_ordinary_path_not_stdin() {
    let dir = temp_dir();
    let run = cjkqc(&["-", "--out", &dir.join("db.jsonl").to_string_lossy()]);
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
    assert!(stderr(&run).contains('-'), "{}", stderr(&run));
    assert!(!dir.join("db.jsonl").exists());
}

#[test] // rq-720a810c
fn a_genuine_failed_record_from_a_matched_parser_is_collected() {
    let dir = temp_dir();
    let out = dir.join("db.jsonl");
    let out_text = out.to_string_lossy().into_owned();
    let run = cjkqc(&[&fixture("xyz/1am1x-bad.xyz"), "--out", &out_text]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(stdout(&run).contains("(1 failed, 0 truncated)"));
    let records = loaded(&out);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].status, RecordStatus::Failed);
    assert_eq!(records[0].program.as_deref(), Some("xyz"));
    let components = records[0].components.clone();
    assert_eq!(
        components.as_ref().and_then(|c| c.get("am")),
        Some(&1),
        "components: {components:?}"
    );
    assert_eq!(
        components.as_ref().and_then(|c| c.get("x")),
        Some(&1),
        "components: {components:?}"
    );
}

// ---------------------------------------------------------------------------
// Milestone 5: filtering & manipulation scenarios from `rqm/filtering.md`
// that are CLI-shaped.
// ---------------------------------------------------------------------------

fn comp_line(
    name: &str,
    energy: Option<f64>,
    comps: &[(&str, u32)],
    status: RecordStatus,
) -> String {
    let mut record = Record {
        name: Some(name.to_string()),
        electronic_energy: energy,
        status,
        ..Record::default()
    };
    if !comps.is_empty() {
        record.components = Some(
            comps
                .iter()
                .map(|(code, count)| (code.to_string(), *count))
                .collect(),
        );
    }
    if status == RecordStatus::Failed {
        record.error = Some("boom".to_string());
    }
    serde_json::to_string(&record).expect("serialization")
}

fn database_in(dir: &TempDir, lines: &[String]) -> String {
    let path = dir.join("db.jsonl");
    let owned: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_jsonl(&path, &owned);
    path.to_string_lossy().into_owned()
}

#[test] // rq-b73d0021
fn extract_selects_the_exact_composition() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            comp_line("wide", None, &[("am", 2), ("sa", 3)], RecordStatus::Ok),
            comp_line("small", None, &[("am", 1), ("sa", 1)], RecordStatus::Ok),
        ],
    );
    let run = cjkqc(&[&db, "--extract", "2am3sa", "--filename"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "filename\nwide");
}

#[test] // rq-443f2709
fn nextract_inverts_the_selection() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            comp_line("diammonia", None, &[("am", 2)], RecordStatus::Ok),
            comp_line("sasa", None, &[("sa", 3)], RecordStatus::Ok),
        ],
    );
    let run = cjkqc(&[&db, "--nextract", "2am", "--filename"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "filename\nsasa");
}

#[test] // rq-f43b5dbf
fn an_unparsable_extract_query_is_a_usage_error() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[comp_line("a", None, &[("am", 1)], RecordStatus::Ok)],
    );
    let run = cjkqc(&[&db, "--extract", "2am$", "--filename"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test] // rq-b6a5add7
fn noname_extract_compares_names_verbatim() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            comp_line(
                "2am3sa-1conf",
                None,
                &[("am", 2), ("sa", 3)],
                RecordStatus::Ok,
            ),
            comp_line(
                "2am3sa-2conf",
                None,
                &[("am", 2), ("sa", 3)],
                RecordStatus::Ok,
            ),
        ],
    );
    let run = cjkqc(&[&db, "--noname", "--extract", "2am3sa-1conf", "--filename"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "filename\n2am3sa-1conf");
}

#[test] // rq-cd33a3de
fn non_ok_records_are_excluded_and_counted() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            comp_line("a", Some(-2.0), &[("am", 1)], RecordStatus::Ok),
            comp_line("b", Some(-1.0), &[("am", 1)], RecordStatus::Ok),
            comp_line("c", None, &[], RecordStatus::Failed),
        ],
    );
    let run = cjkqc(&[&db, "--sort", "el", "--filename"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "filename\na\nb");
    assert!(
        stderr(&run).contains("excluded 1 records with status failed or truncated"),
        "{}",
        stderr(&run)
    );
}

#[test] // rq-6d5b5f10
fn print_flags_print_the_table_while_out_writes() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[comp_line(
            "1dma",
            Some(-38.51),
            &[("am", 1)],
            RecordStatus::Ok,
        )],
    );
    let out = dir.join("kept.jsonl");
    let out_text = out.to_string_lossy().into_owned();
    let run = cjkqc(&[
        &db,
        "--sort",
        "el",
        "--filename",
        "--el",
        "--out",
        &out_text,
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(loaded(&out).len(), 1);
    let text = stdout(&run);
    assert_eq!(cells(text.lines().next().unwrap()), ["filename", "el"]);
    assert_eq!(
        cells(text.lines().nth(1).unwrap()),
        ["1dma", "-38.5100000000"]
    );
}

#[test] // rq-acf714ae
fn print_columns_follow_canonical_order_regardless_of_flag_order() {
    let dir = temp_dir();
    let mut record = Record {
        name: Some("unknown".to_string()),
        ..Record::default()
    };
    record.geometry = Some(compiled_jkqc::Geometry {
        symbols: vec!["Xx".to_string()],
        coordinates: vec![0.0, 0.0, 0.0],
    });
    let db = database_in(&dir, &[serde_json::to_string(&record).expect("line")]);
    let run = cjkqc(&[&db, "--mass", "--rg", "--filename"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["filename", "rg", "mass"]);
    assert_eq!(lines[1], ["unknown", "-", "-"]);
}

#[test] // rq-be8df504
fn the_table_aligns_its_columns_like_column_t() {
    let dir = temp_dir();
    let nameless = Record {
        name: Some("1".to_string()),
        ..Record::default()
    };
    let db = database_in(
        &dir,
        &[
            // Two H atoms 4.69 Å apart: a radius of gyration of 2.345 Å.
            geom_line("1dma", &[], &["H", "H"], &[0.0, 0.0, 0.0, 4.69, 0.0, 0.0]),
            serde_json::to_string(&nameless).expect("serialization"),
        ],
    );
    let run = cjkqc(&[&db, "--filename", "--rg"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    // filename is a text column (left-aligned), rg a numeric column
    // (right-aligned, header included): the rg column is as wide as its
    // widest cell, so the header pads and the dash right-aligns.
    assert_eq!(
        stdout(&run).trim(),
        "filename    rg\n1dma     2.345\n1            -",
        "{}",
        stdout(&run)
    );
}

#[test] // rq-207aec4d
fn lf_column_prints_the_lowest_raw_frequency() {
    let dir = temp_dir();
    let record = Record {
        vibrational_frequencies: Some(vec![1000.0, -300.0, 150.0]),
        ..Record::default()
    };
    let db = database_in(&dir, &[serde_json::to_string(&record).expect("line")]);
    let run = cjkqc(&[&db, "--lf"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["lf"]);
    assert_eq!(lines[1], ["-300.0"]);
}

#[test] // rq-4633531a
fn lf_column_is_undefined_without_stored_frequencies() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[serde_json::to_string(&Record::default()).expect("line")],
    );
    let run = cjkqc(&[&db, "--lf"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["lf"]);
    assert_eq!(lines[1], ["-"]);
}

#[test] // rq-c08f088e
fn rsn_column_prints_the_rotational_symmetry_number() {
    let dir = temp_dir();
    let record = Record {
        rotational_symmetry_number: Some(12),
        ..Record::default()
    };
    let db = database_in(&dir, &[serde_json::to_string(&record).expect("line")]);
    let run = cjkqc(&[&db, "--rsn"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["rsn"]);
    assert_eq!(lines[1], ["12"]);
}

#[test] // rq-1014452e
fn rsn_column_is_undefined_without_a_rotational_symmetry_number() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[serde_json::to_string(&Record::default()).expect("line")],
    );
    let run = cjkqc(&[&db, "--rsn"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["rsn"]);
    assert_eq!(lines[1], ["-"]);
}

#[test] // rq-c14b6917
fn dips_column_prints_the_dipole_vector_components() {
    let dir = temp_dir();
    let record = Record {
        dipole_moment: Some([0.1234, -0.4567, 0.7891]),
        ..Record::default()
    };
    let db = database_in(&dir, &[serde_json::to_string(&record).expect("line")]);
    let run = cjkqc(&[&db, "--dips"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["dips"]);
    assert_eq!(lines[1], ["0.123,-0.457,0.789"]);
}

#[test] // rq-93f9d560
fn dips_column_is_undefined_without_a_dipole_moment() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[serde_json::to_string(&Record::default()).expect("line")],
    );
    let run = cjkqc(&[&db, "--dips"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["dips"]);
    assert_eq!(lines[1], ["-"]);
}

#[test] // rq-1b7b3af3
fn pol_column_prints_the_isotropic_polarizability() {
    let dir = temp_dir();
    let record = Record {
        polarizability: Some([3.0, 0.0, 0.0, 0.0, 6.0, 0.0, 0.0, 0.0, 9.0]),
        ..Record::default()
    };
    let db = database_in(&dir, &[serde_json::to_string(&record).expect("line")]);
    let run = cjkqc(&[&db, "--pol"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["pol"]);
    assert_eq!(lines[1], ["6.000"]);
}

#[test] // rq-b6bcec13
fn pol_column_is_undefined_without_a_polarizability_tensor() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[serde_json::to_string(&Record::default()).expect("line")],
    );
    let run = cjkqc(&[&db, "--pol"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["pol"]);
    assert_eq!(lines[1], ["-"]);
}

#[test] // rq-77dd2f68
fn pols_column_prints_the_polarizability_tensor_row_major() {
    let dir = temp_dir();
    let record = Record {
        polarizability: Some([3.0, 0.1, 0.2, 0.1, 6.0, 0.3, 0.2, 0.3, 9.0]),
        ..Record::default()
    };
    let db = database_in(&dir, &[serde_json::to_string(&record).expect("line")]);
    let run = cjkqc(&[&db, "--pols"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["pols"]);
    assert_eq!(
        lines[1],
        ["3.000,0.100,0.200,0.100,6.000,0.300,0.200,0.300,9.000"]
    );
}

#[test] // rq-31d1c63f
fn pols_column_is_undefined_without_a_polarizability_tensor() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[serde_json::to_string(&Record::default()).expect("line")],
    );
    let run = cjkqc(&[&db, "--pols"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["pols"]);
    assert_eq!(lines[1], ["-"]);
}

#[test] // rq-ee2f64a3
fn unit_converts_the_dipole_columns_to_debye() {
    let dir = temp_dir();
    let record = Record {
        dipole_moment: Some([0.1234, -0.4567, 0.7891]),
        ..Record::default()
    };
    let db = database_in(&dir, &[serde_json::to_string(&record).expect("line")]);
    let run = cjkqc(&[&db, "--dip", "--dips", "--unit"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["dip", "dips"]);
    assert_eq!(lines[1], ["2.339", "0.314,-1.161,2.006"]);
}

#[test] // rq-ac4624f4
fn unit_converts_the_polarizability_columns_to_cubic_angstroms() {
    let dir = temp_dir();
    let record = Record {
        polarizability: Some([3.0, 0.1, 0.2, 0.1, 6.0, 0.3, 0.2, 0.3, 9.0]),
        ..Record::default()
    };
    let db = database_in(&dir, &[serde_json::to_string(&record).expect("line")]);
    let run = cjkqc(&[&db, "--pol", "--pols", "--unit"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["pol", "pols"]);
    assert_eq!(
        lines[1],
        [
            "0.889",
            "0.445,0.015,0.030,0.015,0.889,0.044,0.030,0.044,1.334"
        ]
    );
}

#[test] // rq-79642963
fn without_unit_the_dipole_and_polarizability_columns_remain_atomic_units() {
    let dir = temp_dir();
    let record = Record {
        dipole_moment: Some([0.1234, -0.4567, 0.7891]),
        polarizability: Some([3.0, 0.0, 0.0, 0.0, 6.0, 0.0, 0.0, 0.0, 9.0]),
        ..Record::default()
    };
    let db = database_in(&dir, &[serde_json::to_string(&record).expect("line")]);
    let run = cjkqc(&[&db, "--dip", "--pol"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["dip", "pol"]);
    assert_eq!(lines[1], ["0.920", "6.000"]);
}

#[test] // rq-c6b69701
fn unit_leaves_the_dip_cutoff_in_atomic_units() {
    let dir = temp_dir();
    let record = Record {
        name: Some("polar".to_string()),
        dipole_moment: Some([0.1234, -0.4567, 0.7891]),
        ..Record::default()
    };
    let db = database_in(&dir, &[serde_json::to_string(&record).expect("line")]);
    let run = cjkqc(&[&db, "--unit", "--cut", "dip", "1.0", "--filename"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["filename"]);
    assert_eq!(lines[1], ["polar"]);
}

#[test] // rq-003218f8
fn freqs_column_prints_every_stored_frequency_in_stored_order() {
    let dir = temp_dir();
    let record = Record {
        vibrational_frequencies: Some(vec![1000.0, -300.0, 150.0]),
        ..Record::default()
    };
    let db = database_in(&dir, &[serde_json::to_string(&record).expect("line")]);
    let run = cjkqc(&[&db, "--freqs"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["freqs"]);
    assert_eq!(lines[1], ["1000.0,-300.0,150.0"]);
}

#[test] // rq-27634d1a
fn freqs_column_is_undefined_without_stored_frequencies() {
    let dir = temp_dir();
    let record = Record {
        vibrational_frequencies: Some(Vec::new()),
        ..Record::default()
    };
    let db = database_in(&dir, &[serde_json::to_string(&record).expect("line")]);
    let run = cjkqc(&[&db, "--freqs"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["freqs"]);
    assert_eq!(lines[1], ["-"]);
}

#[test] // rq-f321c97b
fn rots_column_prints_the_descending_rotational_constants() {
    let dir = temp_dir();
    let db = dir.join("db.jsonl");
    let db_text = db.to_string_lossy().into_owned();
    let collected = cjkqc(&[
        &fixture("orca/1pinic1sa-110_1_9-str9.out"),
        "--out",
        &db_text,
    ]);
    assert_eq!(collected.status.code(), Some(0), "{}", stderr(&collected));
    let run = cjkqc(&[&db_text, "--rots"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["rots"]);
    assert_eq!(lines[1], ["0.024157,0.008371,0.007528"]);
}

#[test] // rq-b04ab918
fn rots_column_prints_one_constant_for_a_linear_record() {
    let dir = temp_dir();
    let record = Record {
        geometry: Some(compiled_jkqc::Geometry {
            symbols: vec!["H".to_string(), "O".to_string(), "H".to_string()],
            coordinates: vec![-1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        }),
        ..Record::default()
    };
    let expected = compiled_jkqc::rotational_constants(&record).expect("linear constants");
    assert_eq!(expected.len(), 1);
    let db = database_in(&dir, &[serde_json::to_string(&record).expect("line")]);
    let run = cjkqc(&[&db, "--rots"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["rots"]);
    assert_eq!(lines[1], [format!("{:.6}", expected[0])]);
}

#[test] // rq-3d385e42
fn rots_column_is_undefined_when_the_rotational_constants_are_undefined() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[serde_json::to_string(&Record::default()).expect("line")],
    );
    let run = cjkqc(&[&db, "--rots"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["rots"]);
    assert_eq!(lines[1], ["-"]);
}

#[test] // rq-96b5a37a
fn rots_column_is_undefined_for_a_degenerate_geometry() {
    let dir = temp_dir();
    let record = Record {
        geometry: Some(compiled_jkqc::Geometry {
            symbols: vec!["O".to_string(), "O".to_string()],
            coordinates: vec![0.0; 6],
        }),
        ..Record::default()
    };
    let db = database_in(&dir, &[serde_json::to_string(&record).expect("line")]);
    let run = cjkqc(&[&db, "--rots"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["rots"]);
    assert_eq!(lines[1], ["-"]);
}

#[test] // rq-2550fa8c
fn rots_column_is_undefined_for_a_monatomic_record() {
    let dir = temp_dir();
    let record = Record {
        geometry: Some(compiled_jkqc::Geometry {
            symbols: vec!["H".to_string()],
            coordinates: vec![0.0, 0.0, 0.0],
        }),
        ..Record::default()
    };
    let db = database_in(&dir, &[serde_json::to_string(&record).expect("line")]);
    let run = cjkqc(&[&db, "--rots"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["rots"]);
    assert_eq!(lines[1], ["-"]);
}

#[test] // rq-af106fa7
fn new_print_columns_follow_canonical_order() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[serde_json::to_string(&Record::default()).expect("line")],
    );
    let run = cjkqc(&[
        &db,
        "--rots",
        "--rsn",
        "--freqs",
        "--dips",
        "--dip",
        "--lf",
        "--filename",
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(
        lines[0],
        ["filename", "dip", "dips", "rsn", "lf", "freqs", "rots"]
    );
}

#[test] // rq-1f63ac38
fn name_column_prints_components_without_separators() {
    let dir = temp_dir();
    let record = Record {
        components: Some(
            [("am", 2u32), ("sa", 3u32)]
                .into_iter()
                .map(|(code, count)| (code.to_string(), count))
                .collect(),
        ),
        ..Record::default()
    };
    let db = database_in(&dir, &[serde_json::to_string(&record).expect("line")]);
    let run = cjkqc(&[&db, "--name"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "name\n2am3sa");
}

#[test] // rq-7b0d8978
fn name_column_is_undefined_without_components() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[serde_json::to_string(&Record::default()).expect("line")],
    );
    let run = cjkqc(&[&db, "--name"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "name\n-");
}

#[test] // rq-c0c700b2
fn filename_column_prints_the_records_name() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[comp_line("2am3sa-1conf", None, &[], RecordStatus::Ok)],
    );
    let run = cjkqc(&[&db, "--filename"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "filename\n2am3sa-1conf");
}

#[test] // rq-9bea312b
fn filename_column_is_undefined_without_a_name() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[serde_json::to_string(&Record::default()).expect("line")],
    );
    let run = cjkqc(&[&db, "--filename"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "filename\n-");
}

#[test] // rq-5ad1c077
fn path_column_prints_the_records_source_path() {
    let dir = temp_dir();
    let record = Record {
        source_path: Some("/scratch/runs/xtb/1dma.log".to_string()),
        ..Record::default()
    };
    let db = database_in(&dir, &[serde_json::to_string(&record).expect("line")]);
    let run = cjkqc(&[&db, "--path"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "path\n/scratch/runs/xtb/1dma.log");
}

#[test] // rq-3f2648cf
fn path_column_is_undefined_without_a_source_path() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[serde_json::to_string(&Record::default()).expect("line")],
    );
    let run = cjkqc(&[&db, "--path"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "path\n-");
}

#[test] // rq-6fb14f23
fn identity_columns_lead_the_canonical_order() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[comp_line("a", None, &[("am", 1)], RecordStatus::Ok)],
    );
    let run = cjkqc(&[&db, "--status", "--path", "--name", "--filename"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(
        cells(stdout(&run).lines().next().unwrap()),
        ["filename", "name", "path", "status"]
    );
}

#[test] // rq-2371c178
fn cli_sort_filename_sorts_by_the_records_name() {
    let dir = temp_dir();
    let nameless = Record {
        electronic_energy: Some(-1.0),
        components: Some(
            [("am", 1u32)]
                .into_iter()
                .map(|(code, count)| (code.to_string(), count))
                .collect(),
        ),
        ..Record::default()
    };
    let db = database_in(
        &dir,
        &[
            comp_line("b", None, &[("am", 1)], RecordStatus::Ok),
            comp_line("a", None, &[("am", 1)], RecordStatus::Ok),
            serde_json::to_string(&nameless).expect("nameless serializes"),
        ],
    );
    let run = cjkqc(&[&db, "--sort", "filename", "--filename"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "filename\n-\na\nb");
    let reversed = cjkqc(&[&db, "--sort", "filename", "--reverse", "--filename"]);
    assert_eq!(reversed.status.code(), Some(0), "{}", stderr(&reversed));
    assert_eq!(stdout(&reversed).trim(), "filename\nb\na\n-");
}

#[test] // rq-f9ccc35c
fn program_column_prints_the_program_and_its_version() {
    let dir = temp_dir();
    let record = Record {
        program: Some("xtb".to_string()),
        program_version: Some("6.7.1".to_string()),
        ..Record::default()
    };
    let db = database_in(&dir, &[serde_json::to_string(&record).expect("line")]);
    let run = cjkqc(&[&db, "--program"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "program\nxtb 6.7.1");
}

#[test] // rq-6decd555
fn program_column_prints_only_the_name_without_a_version() {
    let dir = temp_dir();
    let record = Record {
        program: Some("xyz".to_string()),
        ..Record::default()
    };
    let db = database_in(&dir, &[serde_json::to_string(&record).expect("line")]);
    let run = cjkqc(&[&db, "--program"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "program\nxyz");
}

#[test] // rq-777c8582
fn command_column_quotes_its_value() {
    let dir = temp_dir();
    let record = Record {
        command: Some("xtb 1dma.xyz --gxtb --acc 0.01".to_string()),
        ..Record::default()
    };
    let db = database_in(&dir, &[serde_json::to_string(&record).expect("line")]);
    let run = cjkqc(&[&db, "--command"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(
        stdout(&run).trim(),
        "command\n\"xtb 1dma.xyz --gxtb --acc 0.01\""
    );
}

#[test] // rq-804a53e2
fn command_column_is_undefined_without_a_command() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[serde_json::to_string(&Record::default()).expect("line")],
    );
    let run = cjkqc(&[&db, "--command"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "command\n-");
}

#[test] // rq-b81cce20
fn a_manipulation_without_out_or_print_flags_is_a_usage_error() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[comp_line("a", Some(-1.0), &[("am", 1)], RecordStatus::Ok)],
    );
    let run = cjkqc(&[&db, "--sort", "el"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test] // rq-6e881115
fn a_print_only_run_prints_every_record_without_writing() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            comp_line("a", Some(-2.0), &[("am", 1)], RecordStatus::Ok),
            comp_line("b", Some(-1.0), &[("am", 1)], RecordStatus::Ok),
            comp_line("c", None, &[], RecordStatus::Failed),
        ],
    );
    let run = cjkqc(&[&db, "--status", "--filename"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["filename", "status"]);
    assert_eq!(lines[1], ["a", "ok"]);
    assert_eq!(lines[2], ["b", "ok"]);
    assert_eq!(lines[3], ["c", "failed"]);
    assert_eq!(
        fs::read_to_string(&db).expect("db").lines().count(),
        3,
        "the input database is untouched and nothing else is written"
    );
}

#[test] // rq-2b862de1
fn out_writes_the_filtered_database_and_prints_a_summary() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            comp_line("a", Some(-38.0), &[("am", 1)], RecordStatus::Ok),
            comp_line("b", Some(-38.005), &[("am", 1)], RecordStatus::Ok),
            comp_line("c", Some(-38.02), &[("am", 1)], RecordStatus::Ok),
        ],
    );
    let out = dir.join("kept.jsonl");
    let out_text = out.to_string_lossy().into_owned();
    let run = cjkqc(&[&db, "--cutr", "el", "1", "--out", &out_text]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let survivors = loaded(&out);
    assert!(!survivors.is_empty() && survivors.len() <= 3);
    assert_eq!(survivors.len(), 1, "only the group minimum survives");
    assert_eq!(
        stdout(&run).trim(),
        format!("filtered 1 records into {out_text}")
    );
}

#[test] // rq-9b28d28a
fn missing_out_is_still_required_without_manipulation_flags() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[comp_line("a", Some(-1.0), &[("am", 1)], RecordStatus::Ok)],
    );
    let run = cjkqc(&[&db]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
    assert!(stderr(&run).contains("--out"), "{}", stderr(&run));
}

#[test] // rq-3bbefb05
fn mwarbalign_remains_an_unknown_usage_error() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[comp_line("a", Some(-1.0), &[("am", 1)], RecordStatus::Ok)],
    );
    let run = cjkqc(&[&db, "--MWarbalign", "0.38"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
    assert!(stderr(&run).contains("MWarbalign"), "{}", stderr(&run));
}

// ---------------------------------------------------------------------------
// PR-review follow-ups: new extract-scenario coverage and CLI-level pinning
// of the flag wiring in `src/main.rs` (`rqm/filtering.md`).
// ---------------------------------------------------------------------------

fn geom_line(name: &str, comps: &[(&str, u32)], symbols: &[&str], coords: &[f64]) -> String {
    let mut record = Record {
        name: Some(name.to_string()),
        ..Record::default()
    };
    if !comps.is_empty() {
        record.components = Some(
            comps
                .iter()
                .map(|(code, count)| (code.to_string(), *count))
                .collect(),
        );
    }
    record.geometry = Some(compiled_jkqc::Geometry {
        symbols: symbols.iter().map(|symbol| symbol.to_string()).collect(),
        coordinates: coords.to_vec(),
    });
    serde_json::to_string(&record).expect("serialization")
}

#[test] // rq-071ab7e7
fn an_underscore_in_an_extract_pattern_is_a_usage_error() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[comp_line(
            "a",
            None,
            &[("am", 2), ("sa", 3)],
            RecordStatus::Ok,
        )],
    );
    let run = cjkqc(&[&db, "--extract", "2am3sa_1conf", "--filename"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test] // rq-32c242d5
fn noname_extract_accepts_names_outside_the_composition_grammar() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            comp_line("conf-A", None, &[("am", 2)], RecordStatus::Ok),
            comp_line("conf-B", None, &[("am", 2)], RecordStatus::Ok),
        ],
    );
    let run = cjkqc(&[&db, "--noname", "--extract", "conf-A", "--filename"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "filename\nconf-A");
}

#[test]
fn cli_uniq_dup_and_keys_pick_records() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            comp_line("x", Some(-2.0), &[("am", 1)], RecordStatus::Ok),
            comp_line("x", Some(-1.0), &[("am", 1)], RecordStatus::Ok),
            comp_line("y", Some(-3.0), &[("am", 1)], RecordStatus::Ok),
        ],
    );
    let dup = cjkqc(&[&db, "--uniq", "dup", "--filename"]);
    assert_eq!(dup.status.code(), Some(0), "{}", stderr(&dup));
    assert_eq!(stdout(&dup).trim(), "filename\nx\ny");
    let keys = cjkqc(&[&db, "--uniq", "el", "--filename"]);
    assert_eq!(keys.status.code(), Some(0), "{}", stderr(&keys));
    // No two energies agree at 3 decimals, so uniq keeps all of them in
    // implicit el-ascending order.
    assert_eq!(stdout(&keys).trim(), "filename\ny\nx\nx");
}

#[test]
fn cli_reacted_and_reacted2_use_the_fragment_consensus() {
    let dir = temp_dir();
    let bonded = || {
        geom_line(
            "d",
            &[("n", 2)],
            &["N", "N"],
            &[0.0, 0.0, 0.0, 1.4, 0.0, 0.0],
        )
    };
    let db = database_in(
        &dir,
        &[
            geom_line(
                "a",
                &[("n", 2)],
                &["N", "N"],
                &[0.0, 0.0, 0.0, 3.0, 0.0, 0.0],
            ),
            geom_line(
                "b",
                &[("n", 2)],
                &["N", "N"],
                &[0.0, 0.0, 0.0, 3.0, 0.0, 0.0],
            ),
            bonded(),
        ],
    );
    let consensus = cjkqc(&[&db, "--reacted", "--filename"]);
    assert_eq!(consensus.status.code(), Some(0), "{}", stderr(&consensus));
    assert_eq!(stdout(&consensus).trim(), "filename\na\nb");
    let deviants = cjkqc(&[&db, "--reacted2", "--filename"]);
    assert_eq!(deviants.status.code(), Some(0), "{}", stderr(&deviants));
    assert_eq!(stdout(&deviants).trim(), "filename\nd");
}

#[test]
fn cli_shuffle_is_reproducible_per_seed() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &(0..5)
            .map(|index| {
                comp_line(
                    &format!("n{index}"),
                    Some(index as f64),
                    &[("am", 1)],
                    RecordStatus::Ok,
                )
            })
            .collect::<Vec<_>>(),
    );
    let first = cjkqc(&[&db, "--shuffle", "--seed", "7", "--filename"]);
    let second = cjkqc(&[&db, "--shuffle", "--seed", "7", "--filename"]);
    assert_eq!(stdout(&first), stdout(&second));
    assert_ne!(stdout(&first).trim(), "filename\nn0\nn1\nn2\nn3\nn4");
}

#[test]
fn cli_select_and_reverse_sort_apply() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            comp_line("a", Some(-3.0), &[("am", 1)], RecordStatus::Ok),
            comp_line("b", Some(-2.0), &[("am", 1)], RecordStatus::Ok),
            comp_line("s", Some(-9.0), &[("sa", 1)], RecordStatus::Ok),
        ],
    );
    let selected = cjkqc(&[&db, "--select", "1", "--filename"]);
    assert_eq!(selected.status.code(), Some(0), "{}", stderr(&selected));
    // Implicit el-ascending sort puts s (-9.0) before a (-3.0); each group
    // contributes its first record.
    assert_eq!(stdout(&selected).trim(), "filename\ns\na");
    let reversed = cjkqc(&[&db, "--sort", "el", "--reverse", "--filename"]);
    assert_eq!(reversed.status.code(), Some(0), "{}", stderr(&reversed));
    assert_eq!(stdout(&reversed).trim(), "filename\nb\na\ns");
}

#[test]
fn cli_cutoff_flags_wire_their_operators() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            // 3 atoms, no imaginary frequency; 2 atoms; charge variants.
            geom_line(
                "big",
                &[("am", 1)],
                &["N", "N", "N"],
                &[0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 6.0, 0.0, 0.0],
            ),
            geom_line(
                "small",
                &[("am", 1)],
                &["N", "N"],
                &[0.0, 0.0, 0.0, 3.0, 0.0, 0.0],
            ),
        ],
    );
    let pass = cjkqc(&[&db, "--pass", "natoms", "2", "--filename"]);
    assert_eq!(pass.status.code(), Some(0), "{}", stderr(&pass));
    assert_eq!(stdout(&pass).trim(), "filename\nbig");

    let mut imag = Record {
        name: Some("imag".to_string()),
        components: Some([("am".to_string(), 1)].into_iter().collect()),
        ..Record::default()
    };
    imag.vibrational_frequencies = Some(vec![-50.0]);
    let mut real = Record {
        name: Some("real".to_string()),
        components: Some([("am".to_string(), 1)].into_iter().collect()),
        ..Record::default()
    };
    real.vibrational_frequencies = Some(vec![50.0]);
    let freq_db = database_in(
        &dir,
        &[
            serde_json::to_string(&imag).expect("line"),
            serde_json::to_string(&real).expect("line"),
        ],
    );
    let is_not = cjkqc(&[&freq_db, "--isnot", "nimag", "0", "--filename"]);
    assert_eq!(is_not.status.code(), Some(0), "{}", stderr(&is_not));
    assert_eq!(stdout(&is_not).trim(), "filename\nimag");
    let is_zero = cjkqc(&[&freq_db, "--is", "nimag", "0", "--filename"]);
    assert_eq!(is_zero.status.code(), Some(0), "{}", stderr(&is_zero));
    assert_eq!(stdout(&is_zero).trim(), "filename\nreal");
}

#[test]
fn cli_sort_keys_cover_rg_mass_and_name() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            geom_line(
                "wide",
                &[("n", 2)],
                &["N", "N"],
                &[0.0, 0.0, 0.0, 6.0, 0.0, 0.0],
            ),
            geom_line(
                "tight",
                &[("n", 2)],
                &["N", "N"],
                &[0.0, 0.0, 0.0, 1.4, 0.0, 0.0],
            ),
        ],
    );
    let by_rg = cjkqc(&[&db, "--sort", "rg", "--filename"]);
    assert_eq!(by_rg.status.code(), Some(0), "{}", stderr(&by_rg));
    assert_eq!(stdout(&by_rg).trim(), "filename\ntight\nwide");
    let by_name = cjkqc(&[&db, "--sort", "filename", "--filename"]);
    assert_eq!(by_name.status.code(), Some(0), "{}", stderr(&by_name));
    assert_eq!(stdout(&by_name).trim(), "filename\ntight\nwide");

    let mass_db = database_in(
        &dir,
        &[
            geom_line("carbon", &[("c", 1)], &["C"], &[0.0, 0.0, 0.0]),
            geom_line("hydrogen", &[("h", 1)], &["H"], &[0.0, 0.0, 0.0]),
        ],
    );
    let by_mass = cjkqc(&[&mass_db, "--sort", "mass", "--filename"]);
    assert_eq!(by_mass.status.code(), Some(0), "{}", stderr(&by_mass));
    assert_eq!(stdout(&by_mass).trim(), "filename\nhydrogen\ncarbon");
}

#[test] // rq-17ea1903
fn cli_sort_name_puts_nameless_records_first() {
    let dir = temp_dir();
    let nameless = Record {
        electronic_energy: Some(-1.0),
        components: Some(
            [("am", 1u32)]
                .into_iter()
                .map(|(code, count)| (code.to_string(), count))
                .collect(),
        ),
        ..Record::default()
    };
    let db = database_in(
        &dir,
        &[
            serde_json::to_string(&nameless).expect("nameless serializes"),
            geom_line(
                "named",
                &[("am", 1)],
                &["N", "N"],
                &[0.0, 0.0, 0.0, 1.4, 0.0, 0.0],
            ),
        ],
    );
    let run = cjkqc(&[&db, "--sort", "filename", "--filename"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "filename\n-\nnamed");
}

#[test]
fn mixed_inputs_with_print_flags_is_a_usage_error_once() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[comp_line("a", None, &[("am", 1)], RecordStatus::Ok)],
    );
    let program = dir.join("prog.out");
    fs::write(&program, "not json at all\n").expect("write program output");
    let run = cjkqc(&[&db, &program.to_string_lossy(), "--status", "--filename"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
    let errors = stderr(&run);
    assert_eq!(
        errors.matches("cannot mix").count(),
        1,
        "the mixed-input error must print exactly once"
    );
}

#[test] // rq-297b5bf7
fn every_cutoff_flag_wires_its_operator() {
    let dir = temp_dir();
    // Absolute: low < at == threshold > high, so every operator has a
    // distinct survivor set.
    let abs_db = database_in(
        &dir,
        &[
            comp_line("low", Some(-1.0), &[("am", 1)], RecordStatus::Ok),
            comp_line("at", Some(-0.5), &[("am", 1)], RecordStatus::Ok),
            comp_line("high", Some(0.0), &[("am", 1)], RecordStatus::Ok),
        ],
    );
    // Relative: one group with minimum "deep"; "zero" sits exactly
    // 0.6275094740631 kcal/mol above it (0.001 hartree), on the boundary —
    // the threshold is the exact conversion so the boundary is bit-exact.
    let rel_path = dir.join("rel.jsonl");
    write_jsonl(
        &rel_path,
        &[
            &comp_line("zero", Some(0.0), &[("am", 1)], RecordStatus::Ok),
            &comp_line("deep", Some(-0.001), &[("am", 1)], RecordStatus::Ok),
        ],
    );
    let rel_db = rel_path.to_string_lossy().into_owned();
    let boundary = format!("{}", 0.001 * compiled_jkqc::KCAL_MOL_PER_HARTREE);
    // (flag, database, threshold, expected names in file order)
    let cases: &[(&str, &str, &str, &str)] = &[
        ("--cut", "abs", "-0.5", "low\nat"),
        ("--filter-lt", "abs", "-0.5", "low"),
        ("--pass", "abs", "-0.5", "high"),
        ("--filter-ge", "abs", "-0.5", "at\nhigh"),
        ("--is", "abs", "-0.5", "at"),
        ("--isnot", "abs", "-0.5", "low\nhigh"),
        ("--cutr", "rel", &boundary, "zero\ndeep"),
        ("--rel-filter-lt", "rel", &boundary, "deep"),
        ("--passr", "rel", &boundary, ""),
        ("--rel-filter-ge", "rel", &boundary, "zero"),
    ];
    for (flag, db_kind, threshold, expected) in cases {
        let db = if *db_kind == "abs" { &abs_db } else { &rel_db };
        let run = cjkqc(&[db, flag, "el", threshold, "--filename"]);
        assert_eq!(run.status.code(), Some(0), "{flag}: {}", stderr(&run));
        let want = if expected.is_empty() {
            "filename".to_string()
        } else {
            format!("filename\n{expected}")
        };
        assert_eq!(stdout(&run).trim(), want, "{flag} {threshold}");
    }
}

// ---------------------------------------------------------------------------
// Milestone 6: thermochemistry and formation scenarios from
// `rqm/thermo.md` and `rqm/formation.md` that are CLI-shaped.
// ---------------------------------------------------------------------------

use compiled_jkqc::filter::SortKey;
use compiled_jkqc::formation::{collect_monomers, formation_values, ConcConditions};
use compiled_jkqc::thermo::{thermochemistry, ThermoConditions, ThermoMemo, E_HARTREE, K_B};
use compiled_jkqc::Geometry;

/// A serialized water-like record with geometry, optional frequencies and
/// components, and an explicit electronic energy.
fn thermo_record_json(
    name: &str,
    electronic_energy: Option<f64>,
    comps: &[(&str, u32)],
    frequencies: Option<&[f64]>,
) -> String {
    let mut record = Record {
        name: Some(name.to_string()),
        electronic_energy,
        geometry: Some(Geometry {
            symbols: ["O", "H", "H"]
                .iter()
                .map(|symbol| symbol.to_string())
                .collect(),
            coordinates: vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.9572, 0.9259, 0.0, -0.2423],
        }),
        vibrational_frequencies: frequencies.map(|values| values.to_vec()),
        ..Record::default()
    };
    if !comps.is_empty() {
        record.components = Some(
            comps
                .iter()
                .map(|(code, count)| (code.to_string(), *count))
                .collect(),
        );
    }
    serde_json::to_string(&record).expect("serialization")
}

/// A record whose Gibbs free energy hits `target` exactly at the default
/// conditions (the electronic energy absorbs the correction).
fn gibbs_record(name: &str, target: f64, comps: &[(&str, u32)]) -> Record {
    let mut record: Record = serde_json::from_str(&thermo_record_json(
        name,
        Some(0.0),
        comps,
        Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
    ))
    .expect("template deserializes");
    let correction = thermochemistry(&record, &ThermoConditions::default())
        .expect("template defined")
        .gibbs;
    record.electronic_energy = Some(target - correction);
    record
}

fn gibbs_json(name: &str, target: f64, comps: &[(&str, u32)]) -> String {
    serde_json::to_string(&gibbs_record(name, target, comps)).expect("serialization")
}

fn db_with(dir: &TempDir, filename: &str, lines: &[String]) -> String {
    let path = dir.join(filename);
    let owned: Vec<&str> = lines.iter().map(String::as_str).collect();
    write_jsonl(&path, &owned);
    path.to_string_lossy().into_owned()
}

fn formation_row(stdout_text: &str, token: &str) -> Vec<String> {
    for line in stdout_text.lines().skip(1) {
        let cells: Vec<&str> = line.split_whitespace().collect();
        if cells.first() == Some(&token) {
            return cells.iter().map(|cell| cell.to_string()).collect();
        }
    }
    panic!("no formation row for {token} in:\n{stdout_text}");
}

#[test] // rq-bf8fdacc
fn a_saddle_point_s_thermochemistry_columns_print_dashes() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[thermo_record_json(
            "saddle",
            Some(-76.0),
            &[("am", 2)],
            Some(&[1000.0, -300.0]),
        )],
    );
    let run = cjkqc(&[&db, "--filename", "--zpe", "--h", "--s", "--g"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["filename", "zpe", "h", "s", "g"]);
    assert_eq!(lines[1], ["saddle", "-", "-", "-", "-"]);
}

#[test] // rq-d04cc134
fn a_degenerate_geometry_prints_dashes_not_infinities() {
    let dir = temp_dir();
    let collapsed = Record {
        name: Some("collapsed".to_string()),
        electronic_energy: Some(-75.99),
        geometry: Some(compiled_jkqc::Geometry {
            symbols: vec!["O".to_string(), "O".to_string()],
            coordinates: vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        }),
        vibrational_frequencies: Some(vec![1000.0, 2000.0]),
        ..Record::default()
    };
    let db = database_in(
        &dir,
        &[serde_json::to_string(&collapsed).expect("collapses serializes")],
    );
    let run = cjkqc(&[&db, "--filename", "--zpe", "--h", "--s", "--g"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["filename", "zpe", "h", "s", "g"]);
    assert_eq!(lines[1], ["collapsed", "-", "-", "-", "-"]);
}

#[test] // rq-64567efa
fn absurd_temperature_records_drop_out_of_glob() {
    // A participating mode just above the cutoff diverges the vibrational
    // sums at 1e17 K, so every record's Gibbs is undefined (not a
    // comparable number): --glob treats the group as contributing nothing
    // and the table is only its header (rqm/thermo.md Definedness,
    // rqm/filtering.md).
    let dir = temp_dir();
    let line = |name: &str, energy: f64| {
        let record = Record {
            name: Some(name.to_string()),
            electronic_energy: Some(energy),
            geometry: Some(compiled_jkqc::Geometry {
                symbols: vec!["O".to_string(), "H".to_string(), "H".to_string()],
                coordinates: vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.9572, 0.9259, 0.0, -0.2423],
            }),
            vibrational_frequencies: Some(vec![1.001, 1600.0, 3700.0]),
            components: Some(
                [("am", 2u32)]
                    .into_iter()
                    .map(|(code, count)| (code.to_string(), count))
                    .collect(),
            ),
            ..Record::default()
        };
        serde_json::to_string(&record).expect("record serializes")
    };
    let db = database_in(&dir, &[line("hot", -75.99), line("cooler", -75.98)]);
    let run = cjkqc(&[&db, "--temp", "1e17", "--glob", "--filename"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "filename");
    // The same records also print - for g, like any undefined value.
    let printed = cjkqc(&[&db, "--temp", "1e17", "--filename", "--g"]);
    assert_eq!(printed.status.code(), Some(0), "{}", stderr(&printed));
    let text = stdout(&printed);
    let lines = rows(&text);
    assert_eq!(lines[0], ["filename", "g"]);
    assert_eq!(lines[1], ["hot", "-"]);
    assert_eq!(lines[2], ["cooler", "-"]);
}

#[test] // rq-9a4270bc
fn makereal_prints_the_saddle_point_s_thermochemistry() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[thermo_record_json(
            "saddle",
            Some(-76.0),
            &[("am", 2)],
            Some(&[1000.0, -300.0]),
        )],
    );
    let run = cjkqc(&[&db, "--filename", "--g", "--makereal"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let row: Vec<&str> = text
        .trim()
        .lines()
        .nth(1)
        .unwrap()
        .split_whitespace()
        .collect();
    assert_eq!(row[0], "saddle");
    assert_ne!(row[1], "-", "the g cell must be defined: {}", row[1]);
    assert!(row[1].parse::<f64>().is_ok());
}

#[test] // rq-0bcbeeb1
fn makereal_leaves_the_raw_observables_alone() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[thermo_record_json(
            "saddle",
            Some(-76.0),
            &[("am", 2)],
            Some(&[1000.0, -300.0]),
        )],
    );
    let run = cjkqc(&[&db, "--filename", "--nimag", "--g", "--makereal"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let row: Vec<&str> = text
        .trim()
        .lines()
        .nth(1)
        .unwrap()
        .split_whitespace()
        .collect();
    assert_eq!(row[0], "saddle");
    assert_ne!(row[1], "-", "g derives under --makereal");
    assert!(row[1].parse::<f64>().is_ok());
    assert_eq!(row[2], "1", "nimag still counts the imaginary mode");
    // The lf cutoff property still sees the raw -300.0, so a window that
    // only admits values below -200 keeps the record.
    let kept = cjkqc(&[&db, "--filename", "--filter-lt", "lf", "-200", "--makereal"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&kept));
    assert_eq!(stdout(&kept).trim(), "filename\nsaddle");
}

#[test] // rq-28e4ef6b
fn ignoreimag_prints_the_saddle_point_s_thermochemistry() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[thermo_record_json(
            "saddle",
            Some(-76.0),
            &[("am", 2)],
            Some(&[1000.0, -300.0]),
        )],
    );
    let run = cjkqc(&[&db, "--filename", "--g", "--ignoreimag"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let row: Vec<&str> = text
        .trim()
        .lines()
        .nth(1)
        .unwrap()
        .split_whitespace()
        .collect();
    assert_eq!(row[0], "saddle");
    assert_ne!(row[1], "-", "the g cell must be defined: {}", row[1]);
    assert!(row[1].parse::<f64>().is_ok());
}

#[test] // rq-daa8d422
fn ignoreimag_leaves_the_raw_observables_alone() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[thermo_record_json(
            "saddle",
            Some(-76.0),
            &[("am", 2)],
            Some(&[1000.0, -300.0]),
        )],
    );
    let run = cjkqc(&[&db, "--filename", "--nimag", "--g", "--ignoreimag"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let row: Vec<&str> = text
        .trim()
        .lines()
        .nth(1)
        .unwrap()
        .split_whitespace()
        .collect();
    assert_eq!(row[0], "saddle");
    assert_ne!(row[1], "-", "g derives under --ignoreimag");
    assert!(row[1].parse::<f64>().is_ok());
    assert_eq!(row[2], "1", "nimag still counts the imaginary mode");
    // The lf cutoff property still sees the raw -300.0, so a window that
    // only admits values below -200 keeps the record.
    let kept = cjkqc(&[
        &db,
        "--filename",
        "--filter-lt",
        "lf",
        "-200",
        "--ignoreimag",
    ]);
    assert_eq!(kept.status.code(), Some(0), "{}", stderr(&kept));
    assert_eq!(stdout(&kept).trim(), "filename\nsaddle");
}

#[test] // rq-efd98e52
fn ignoreimag_with_makereal_is_a_usage_error() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[thermo_record_json(
            "saddle",
            Some(-76.0),
            &[("am", 2)],
            Some(&[1000.0, -300.0]),
        )],
    );
    let run = cjkqc(&[&db, "--g", "--makereal", "--ignoreimag"]);
    assert_eq!(run.status.code(), Some(2));
}

#[test] // rq-f9e49657
fn a_nonpositive_temperature_is_a_usage_error() {
    let dir = temp_dir();
    let db = db_with(&dir, "db.jsonl", &[sample_line("A1", -1.0)]);
    let run = cjkqc(&[&db, "--temp", "0", "--filename"]);
    assert_eq!(run.status.code(), Some(2));
    assert!(stderr(&run).contains("--temp"), "{}", stderr(&run));
}

#[test] // rq-87c46fa4
fn a_negative_reference_frequency_is_a_usage_error() {
    let dir = temp_dir();
    let db = db_with(&dir, "db.jsonl", &[sample_line("A1", -1.0)]);
    let run = cjkqc(&[&db, "--fc", "-100", "--filename"]);
    assert_eq!(run.status.code(), Some(2));
    assert!(stderr(&run).contains("--fc"), "{}", stderr(&run));
}

#[test] // rq-91d514c5
fn a_zero_frequency_scale_is_a_usage_error() {
    let dir = temp_dir();
    let db = db_with(&dir, "db.jsonl", &[sample_line("A1", -1.0)]);
    let run = cjkqc(&[&db, "--fscale", "0", "--filename"]);
    assert_eq!(run.status.code(), Some(2));
    assert!(stderr(&run).contains("--fscale"), "{}", stderr(&run));
}

#[test] // rq-0585b2ea
fn settings_flags_alone_do_not_make_a_visible_outcome() {
    let dir = temp_dir();
    let db = db_with(&dir, "db.jsonl", &[sample_line("A1", -1.0)]);
    let run = cjkqc(&[&db, "--temp", "300"]);
    assert_eq!(run.status.code(), Some(2));
    assert!(stderr(&run).contains("--out"), "{}", stderr(&run));
}

#[test] // rq-9dc99217
fn undefined_thermochemistry_prints_as_a_dash() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[thermo_record_json("undef", Some(-76.0), &[("am", 2)], None)],
    );
    let run = cjkqc(&[&db, "--filename", "--g"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["filename", "g"]);
    assert_eq!(lines[1], ["undef", "-"]);
}

#[test] // rq-c879d42f
fn sorting_by_gibbs_orders_undefined_values_last() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            gibbs_json("warm", -100.0, &[("am", 1)]),
            gibbs_json("cold", -100.5, &[("am", 1)]),
            thermo_record_json("empty", None, &[("am", 1)], None),
        ],
    );
    let run = cjkqc(&[&db, "--sort", "g", "--filename", "--g"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["filename", "g"]);
    assert_eq!(lines[1], ["cold", "-100.5000000000"]);
    assert_eq!(lines[2], ["warm", "-100.0000000000"]);
    assert_eq!(lines[3], ["empty", "-"]);
}

#[test] // rq-573f5674
fn glob_keeps_each_group_s_lowest_gibbs_record() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            gibbs_json("mid", 0.0, &[("am", 2)]),
            gibbs_json("low", -1.0, &[("am", 2)]),
            gibbs_json("high", 1.0, &[("am", 2)]),
            gibbs_json("alone", 5.0, &[("sa", 1)]),
        ],
    );
    let run = cjkqc(&[&db, "--glob", "--filename"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "filename\nlow\nalone");
}

#[test] // rq-58808413
fn glob_drops_a_group_whose_gibbs_is_all_undefined() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[thermo_record_json("bare", None, &[("am", 2)], None)],
    );
    let run = cjkqc(&[&db, "--glob", "--filename"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "filename");
}

#[test] // rq-4cd891c1
fn uniqueness_rounds_gibbs_like_any_metric() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            gibbs_json("agree1", -100.0013, &[("am", 1)]),
            gibbs_json("agree2", -100.0012, &[("am", 1)]),
            gibbs_json("differs", -100.005, &[("am", 1)]),
        ],
    );
    let run = cjkqc(&[&db, "--uniq", "g", "--filename"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    // The implicit el sort orders the records before the collapse; the
    // first of the agreeing pair survives, the second drops.
    assert_eq!(stdout(&run).trim(), "filename\ndiffers\nagree1");
}

#[test] // rq-7f65572f
fn the_unit_flag_converts_the_energy_columns_to_kcal_mol() {
    let dir = temp_dir();
    let line = thermo_record_json(
        "warm",
        Some(-1.5),
        &[("am", 1)],
        Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
    );
    let db = db_with(&dir, "db.jsonl", &[line]);
    let run = cjkqc(&[&db, "--el", "--zpe", "--h", "--s", "--g", "--unit"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let record: Record = serde_json::from_str(&db_line(&db, 0)).expect("record deserializes");
    let thermo = thermochemistry(&record, &ThermoConditions::default()).unwrap();
    let per_hartree = compiled_jkqc::KCAL_MOL_PER_HARTREE;
    let lines: Vec<Vec<String>> = stdout(&run)
        .trim()
        .lines()
        .map(|line| line.split_whitespace().map(str::to_string).collect())
        .collect();
    assert_eq!(
        lines[0],
        ["el", "zpe", "h", "s", "g"].map(str::to_string).to_vec()
    );
    assert_eq!(
        lines[1],
        [
            format!("{:.4}", -1.5 * per_hartree),
            format!("{:.4}", thermo.zpe * per_hartree),
            format!("{:.4}", thermo.enthalpy * per_hartree),
            format!("{:.4}", thermo.entropy_term * per_hartree),
            format!("{:.4}", thermo.gibbs * per_hartree),
        ]
        .to_vec()
    );
}

#[test] // rq-8b4b5dc5
fn without_unit_the_energies_remain_hartree() {
    let dir = temp_dir();
    let record = gibbs_record("warm", -1.0, &[("am", 1)]);
    let db = db_with(
        &dir,
        "db.jsonl",
        &[serde_json::to_string(&record).expect("serialization")],
    );
    let run = cjkqc(&[&db, "--g"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let thermo = thermochemistry(&record, &ThermoConditions::default()).unwrap();
    let lines: Vec<Vec<String>> = stdout(&run)
        .trim()
        .lines()
        .map(|line| line.split_whitespace().map(str::to_string).collect())
        .collect();
    assert_eq!(lines[0], ["g"]);
    assert_eq!(lines[1], [format!("{:.10}", thermo.gibbs)]);
}

#[test] // rq-48b6dd65
fn a_value_passed_to_unit_is_a_usage_error() {
    let dir = temp_dir();
    let db = db_with(&dir, "db.jsonl", &[sample_line("A1", -1.0)]);
    let run = cjkqc(&[&db, "--unit", "kcal", "--filename"]);
    assert_eq!(run.status.code(), Some(2));
}

// --- formation scenarios (rqm/formation.md) ---

#[test] // rq-feddee4c
fn reverse_does_not_change_the_reference() {
    let dir = temp_dir();
    let cluster = gibbs_record("cluster", -49.0, &[("am", 1)]);
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            serde_json::to_string(&cluster).expect("serialization"),
            gibbs_json("higher", -50.0, &[("am", 1)]),
            gibbs_json("lower", -50.5, &[("am", 1)]),
        ],
    );
    let run = cjkqc(&[&db, "--sort", "g", "--reverse", "--formation"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    // The reference is the minimum-gibbs monomer (-50.5) regardless of
    // --reverse.
    let expected = format!("{:.10}", -49.0 - -50.5);
    let row = formation_row(&stdout(&run), "1am");
    assert_eq!(row[4], expected, "{:?}", row);
}

#[test] // rq-1b6b5757
fn the_formation_section_follows_the_table_and_precedes_the_summary() {
    let dir = temp_dir();
    let out = dir.join("kept.jsonl");
    let out_text = out.to_string_lossy().into_owned();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_record_json(
                "cluster",
                Some(-141.0),
                &[("am", 2)],
                Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
            ),
            thermo_record_json(
                "am1",
                Some(-50.0),
                &[("am", 1)],
                Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
            ),
            thermo_record_json(
                "sa1",
                Some(-40.0),
                &[("sa", 1)],
                Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
            ),
        ],
    );
    let run = cjkqc(&[
        &db,
        "--sort",
        "el",
        "--filename",
        "--el",
        "--formation",
        "--out",
        &out_text,
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(cells(lines[0]), ["filename", "el"], "the table comes first");
    assert_eq!(
        cells(lines[4]),
        ["name", "el", "h", "st", "g"],
        "the section follows the table"
    );
    let _ = &out;
    assert!(
        lines.last().unwrap().contains("filtered 3 records into"),
        "the summary closes the output: {text}"
    );
}

#[test] // rq-0f78a33b
fn formation_alone_is_a_visible_outcome() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_record_json(
                "cluster",
                Some(-90.0),
                &[("am", 1), ("sa", 1)],
                Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
            ),
            thermo_record_json(
                "am1",
                Some(-50.0),
                &[("am", 1)],
                Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
            ),
        ],
    );
    let run = cjkqc(&[&db, "--formation"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(
        cells(stdout(&run).lines().next().unwrap()),
        ["name", "el", "h", "st", "g"],
        "{}",
        stdout(&run)
    );
    // No database file is written: the only file in the directory is the
    // input itself.
    let entries: Vec<_> = fs::read_dir(&dir)
        .expect("read dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(entries, ["db.jsonl"], "{entries:?}");
}

#[test] // rq-2d5b9aba
fn formation_section_identifies_rows_by_composition_token() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_record_json(
                "cluster",
                Some(-141.0),
                &[("am", 2), ("sa", 1)],
                Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
            ),
            thermo_record_json(
                "am1",
                Some(-50.0),
                &[("am", 1)],
                Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
            ),
            thermo_record_json(
                "sa1",
                Some(-40.0),
                &[("sa", 1)],
                Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
            ),
        ],
    );
    let run = cjkqc(&[&db, "--formation"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(cells(lines[0]), ["name", "el", "h", "st", "g"], "{text}");
    assert_eq!(
        cells(lines[1]).first(),
        Some(&"2am1sa"),
        "the cluster row leads with its composition token: {}",
        lines[1]
    );
}

#[test] // rq-27c4fe8e
fn the_section_aligns_its_columns_like_the_main_table() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_line("cluster", -141.0, &[("am", 2), ("sa", 1)], None),
            thermo_line("am1", -50.0, &[("am", 1)], None),
            thermo_line("sa1", -40.0, &[("sa", 1)], None),
        ],
    );
    let run = cjkqc(&[&db, "--formation"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let section = formation_section(&text);
    let lines: Vec<&str> = section.lines().collect();
    assert_eq!(cells(lines[0]), ["name", "el", "h", "st", "g"], "{section}");
    let spans: Vec<Vec<(usize, usize)>> = lines.iter().map(|line| cell_spans(line)).collect();
    // Five columns on every row.
    assert!(spans.iter().all(|row| row.len() == 5), "{section}");
    // The name column is a text column: every cell, header included,
    // starts at 0.
    assert!(spans.iter().all(|row| row[0].0 == 0), "{section}");
    // el, h, st, and g are numeric columns: every cell, header included,
    // ends at its column's right edge, and the last column ends the line.
    let ends: Vec<usize> = spans[0].iter().map(|span| span.1).collect();
    assert!(
        spans.iter().all(|row| row[1..]
            .iter()
            .map(|span| span.1)
            .eq(ends[1..].iter().copied())),
        "{section}"
    );
    assert!(
        lines
            .iter()
            .zip(&spans)
            .all(|(line, row)| row[4].1 == line.len()),
        "{section}"
    );
    // Every column is as wide as its widest cell (header included): the
    // widest cell starts right after the preceding column's separator,
    // so no column carries slack beyond its widest cell.
    let mut start = spans.iter().map(|row| row[0].1).max().unwrap() + 1;
    for column in 1..5 {
        let width = ends[column] - start;
        assert_eq!(
            spans
                .iter()
                .map(|row| row[column].1 - row[column].0)
                .max()
                .unwrap(),
            width,
            "{section}"
        );
        assert!(spans.iter().any(|row| row[column].0 == start), "{section}");
        start = ends[column] + 1;
    }
}

#[test] // rq-ed9e179c
fn record_without_components_has_dash_composition_token() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[serde_json::to_string(&Record::default()).expect("line")],
    );
    let run = cjkqc(&[&db, "--formation"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["name", "el", "h", "st", "g"]);
    assert_eq!(lines[1], ["-", "-", "-", "-", "-"]);
}

#[test] // rq-043c29fe
fn undefined_formation_levels_print_as_dashes() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_record_json(
                "cluster",
                Some(-90.0),
                &[("am", 1), ("na", 1)],
                Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
            ),
            thermo_record_json(
                "am1",
                Some(-50.0),
                &[("am", 1)],
                Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
            ),
        ],
    );
    let run = cjkqc(&[&db, "--formation"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let row = formation_row(&stdout(&run), "1am1na");
    assert_eq!(row[1], "-", "{:?}: the na reference is missing", row);
    assert_eq!(row[2], "-", "{:?}", row);
    assert_eq!(row[3], "-", "{:?}", row);
    assert_eq!(row[4], "-", "{:?}", row);
}

#[test] // rq-31a55ccc
fn filtered_away_monomers_serve_as_no_references() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_record_json(
                "cluster",
                Some(-90.0),
                &[("am", 1), ("sa", 1)],
                Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
            ),
            thermo_record_json(
                "am1",
                Some(-50.0),
                &[("am", 1)],
                Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
            ),
            thermo_record_json(
                "sa1",
                Some(-40.0),
                &[("sa", 1)],
                Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
            ),
        ],
    );
    let run = cjkqc(&[&db, "--extract", "1am1sa", "--formation"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let text = text.trim();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "only the cluster row prints: {text}");
    assert_eq!(cells(lines[1]), ["1am1sa", "-", "-", "-", "-"], "{text}");
}

#[test] // rq-b46a6e05
fn a_print_only_run_sees_every_loaded_monomer() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_record_json(
                "cluster",
                Some(-90.0),
                &[("am", 1), ("sa", 1)],
                Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
            ),
            thermo_record_json(
                "am1",
                Some(-50.0),
                &[("am", 1)],
                Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
            ),
            thermo_record_json(
                "sa1",
                Some(-40.0),
                &[("sa", 1)],
                Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
            ),
        ],
    );
    let run = cjkqc(&[&db, "--formation"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let row = formation_row(&stdout(&run), "1am1sa");
    assert_ne!(row[1], "-", "{:?}: both references are loaded", row);
    assert_ne!(row[2], "-", "{:?}", row);
    assert_ne!(row[3], "-", "{:?}", row);
    assert_ne!(row[4], "-", "{:?}", row);
    // And the values are the stoichiometric ones.
    let record: Record = serde_json::from_str(&db_line(&db, 0)).expect("record deserializes");
    let references = collect_monomers(&loaded_records(&db), &ThermoMemo::default(), SortKey::El);
    let formation = formation_values(
        &record,
        &references,
        &ThermoMemo::default(),
        &ConcConditions::default(),
    );
    assert_eq!(row[1], format!("{:.10}", formation.electronic.unwrap()));
    assert_eq!(row[2], format!("{:.10}", formation.enthalpy.unwrap()));
    assert_eq!(row[3], format!("{:.10}", formation.entropy_term.unwrap()));
    assert_eq!(row[4], format!("{:.10}", formation.gibbs.unwrap()));
}

#[test] // rq-9c5b3a40
fn the_section_obeys_the_unit_setting() {
    let dir = temp_dir();
    // A 1am1sa cluster with defined formation values: the el, h, and st
    // columns print them in kcal/mol with 4 decimals. The electronic
    // level is exactly -0.01 Eh; the thermal levels are checked against
    // the documented stoichiometric arithmetic over the pure
    // thermochemistry derivation of each record, an oracle independent
    // of the formation code under test.
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_record_json(
                "cluster",
                Some(-90.01),
                &[("am", 1), ("sa", 1)],
                Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
            ),
            thermo_record_json(
                "am1",
                Some(-50.0),
                &[("am", 1)],
                Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
            ),
            thermo_record_json(
                "sa1",
                Some(-40.0),
                &[("sa", 1)],
                Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
            ),
        ],
    );
    let run = cjkqc(&[&db, "--formation", "--unit"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let row = formation_row(&stdout(&run), "1am1sa");
    let conditions = ThermoConditions::default();
    let records = loaded_records(&db);
    let cluster = thermochemistry(&records[0], &conditions).expect("cluster thermochemistry");
    let am = thermochemistry(&records[1], &conditions).expect("am thermochemistry");
    let sa = thermochemistry(&records[2], &conditions).expect("sa thermochemistry");
    let h = cluster.enthalpy - am.enthalpy - sa.enthalpy;
    let st = cluster.entropy_term - am.entropy_term - sa.entropy_term;
    let g = cluster.gibbs - am.gibbs - sa.gibbs;
    let kcal = |value: f64| format!("{:.4}", value * compiled_jkqc::KCAL_MOL_PER_HARTREE);
    assert_eq!(row[1], kcal(-0.01), "{:?}", row);
    assert_eq!(row[2], kcal(h), "{:?}", row);
    assert_eq!(row[3], kcal(st), "{:?}", row);
    assert_eq!(row[4], kcal(g), "{:?}", row);
}

#[test] // rq-39ca9fe7
fn the_printed_levels_satisfy_g_equals_h_minus_st() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_record_json(
                "cluster",
                Some(-90.0),
                &[("am", 1), ("sa", 1)],
                Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
            ),
            thermo_record_json(
                "am1",
                Some(-50.0),
                &[("am", 1)],
                Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
            ),
            thermo_record_json(
                "sa1",
                Some(-40.0),
                &[("sa", 1)],
                Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
            ),
        ],
    );
    let run = cjkqc(&[&db, "--formation"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let row = formation_row(&stdout(&run), "1am1sa");
    let h: f64 = row[2].parse().expect("the h cell");
    let st: f64 = row[3].parse().expect("the st cell");
    let g: f64 = row[4].parse().expect("the g cell");
    // The printed cells carry 10 decimals, so the identity holds within
    // the printed precision.
    assert!((g - (h - st)).abs() < 1e-9, "{:?}", row);
}

#[test] // rq-e7f0ef7b
fn the_thermal_levels_follow_the_run_s_temperature() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_record_json(
                "cluster",
                Some(-90.0),
                &[("am", 1), ("sa", 1)],
                Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
            ),
            thermo_record_json(
                "am1",
                Some(-50.0),
                &[("am", 1)],
                Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
            ),
            thermo_record_json(
                "sa1",
                Some(-40.0),
                &[("sa", 1)],
                Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
            ),
        ],
    );
    let warm = cjkqc(&[&db, "--formation", "--temp", "298.15"]);
    let hot = cjkqc(&[&db, "--formation", "--temp", "350"]);
    assert_eq!(warm.status.code(), Some(0), "{}", stderr(&warm));
    assert_eq!(hot.status.code(), Some(0), "{}", stderr(&hot));
    let warm_row = formation_row(&stdout(&warm), "1am1sa");
    let hot_row = formation_row(&stdout(&hot), "1am1sa");
    assert_eq!(
        warm_row[1], hot_row[1],
        "the el level ignores the temperature"
    );
    // The h, st, and g levels follow it.
    assert_ne!(warm_row[2], hot_row[2], "the h level follows it");
    assert_ne!(warm_row[3], hot_row[3], "the st level follows it");
    assert_ne!(warm_row[4], hot_row[4], "the g level follows it");
}

/// The record JSON line at `index` of a database file.
fn db_line(path: &str, index: usize) -> String {
    let records = fs::read_to_string(path).expect("read database");
    records.lines().nth(index).expect("line exists").to_string()
}

/// Every record of a database file, in file order.
fn loaded_records(path: &str) -> Vec<Record> {
    load_database(std::path::Path::new(path))
        .expect("load database")
        .records
}

#[test] // rq-bb4d61fb
fn a_zero_window_budget_is_a_usage_error() {
    let dir = temp_dir();
    let run = cjkqc(&[
        &fixture("xtb/1dma.log"),
        "--out",
        &dir.join("db.jsonl").to_string_lossy(),
        "--head-bytes",
        "0",
    ]);
    assert_eq!(run.status.code(), Some(2));
    assert!(stderr(&run).contains("at least 1"), "{}", stderr(&run));
    assert!(!dir.join("db.jsonl").exists());
}

// ---------------------------------------------------------------------------
// Single-point corrections (`rqm/correction.md`, `rqm/formation.md`).
// ---------------------------------------------------------------------------

/// A correction-database candidate line with orca provenance.
fn sp_line(name: &str, energy: f64, command: Option<&str>) -> String {
    let mut record = Record {
        status: RecordStatus::Ok,
        name: Some(name.to_string()),
        electronic_energy: Some(energy),
        program: Some("orca".to_string()),
        program_version: Some("6.0.0".to_string()),
        ..Record::default()
    };
    record.command = command.map(str::to_string);
    serde_json::to_string(&record).expect("serialization")
}

/// An arbitrary simple record line: optional name, optional energy, status.
fn simple_line(name: Option<&str>, energy: Option<f64>, status: RecordStatus) -> String {
    let mut record = Record {
        status,
        name: name.map(str::to_string),
        electronic_energy: energy,
        ..Record::default()
    };
    if status == RecordStatus::Failed {
        record.error = Some("boom".to_string());
    }
    serde_json::to_string(&record).expect("serialization")
}

/// A record line carrying a stored correction.
fn corrected_db_line(name: &str, energy: f64, correction: f64, commands: bool) -> String {
    let mut record = Record {
        status: RecordStatus::Ok,
        name: Some(name.to_string()),
        electronic_energy: Some(energy),
        ..Record::default()
    };
    record.correction = Some(Correction {
        energy: correction,
        program: Some("orca".to_string()),
        program_version: Some("6.0.0".to_string()),
        commands: if commands {
            Some(vec!["! wB97x-3c def2-TZVPP Sp".to_string()])
        } else {
            None
        },
        kind: "sp".to_string(),
    });
    serde_json::to_string(&record).expect("serialization")
}

/// A nonlinear 6-atom geometry (a planar hexagon): 3N−6 = 12 modes, so the
/// record's thermochemistry is defined.
fn hexagon() -> Geometry {
    let mut coordinates = Vec::new();
    for atom in 0..6 {
        let angle = atom as f64 * std::f64::consts::PI / 3.0;
        coordinates.extend([angle.cos(), angle.sin(), 0.0]);
    }
    Geometry {
        symbols: vec!["C".to_string(); 6],
        coordinates,
    }
}

fn twelve_modes() -> Vec<f64> {
    let mut values = vec![0.0; 6];
    values.resize(18, 1000.0);
    values
}

/// A full-thermochemistry ok record line, optionally with components and
/// an attached correction.
fn thermo_line(name: &str, energy: f64, comps: &[(&str, u32)], correction: Option<f64>) -> String {
    let mut record = Record {
        status: RecordStatus::Ok,
        name: Some(name.to_string()),
        electronic_energy: Some(energy),
        geometry: Some(hexagon()),
        vibrational_frequencies: Some(twelve_modes()),
        ..Record::default()
    };
    if !comps.is_empty() {
        record.components = Some(
            comps
                .iter()
                .map(|(code, count)| (code.to_string(), *count))
                .collect::<Components>(),
        );
    }
    if let Some(correction) = correction {
        record.correction = Some(Correction {
            energy: correction,
            program: Some("orca".to_string()),
            program_version: Some("6.0.0".to_string()),
            commands: Some(vec!["! wB97x-3c def2-TZVPP Sp".to_string()]),
            kind: "sp".to_string(),
        });
    }
    serde_json::to_string(&record).expect("serialization")
}

/// The Gibbs free energy of a full-thermochemistry record at the default
/// conditions.
fn raw_gibbs(energy: f64) -> f64 {
    let record = Record {
        status: RecordStatus::Ok,
        electronic_energy: Some(energy),
        geometry: Some(hexagon()),
        vibrational_frequencies: Some(twelve_modes()),
        ..Record::default()
    };
    thermochemistry(&record, &ThermoConditions::default())
        .expect("defined thermochemistry")
        .gibbs
}

/// The table's data rows as whitespace-split cells.
fn data_rows(stdout_text: &str) -> Vec<Vec<String>> {
    stdout_text
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.split_whitespace().map(str::to_string).collect())
        .collect()
}

#[test] // rq-973be061
fn an_attached_correction_carries_its_provenance() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[simple_line(Some("1am"), Some(-38.51), RecordStatus::Ok)],
    );
    let sp = db_with(
        &dir,
        "sp.jsonl",
        &[sp_line("1am", -76.4109, Some("! wB97x-3c def2-TZVPP Sp"))],
    );
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--add-correction",
        &sp,
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(stdout(&run).contains("(1 corrected)"), "{}", stdout(&run));
    let loaded = load_database(&out).expect("load out");
    assert_eq!(loaded.records.len(), 1);
    let correction = loaded.records[0].correction.as_ref().expect("attached");
    assert_eq!(correction.energy, -76.4109);
    assert_eq!(correction.program.as_deref(), Some("orca"));
    assert_eq!(correction.program_version.as_deref(), Some("6.0.0"));
    assert_eq!(
        correction.commands.as_deref(),
        Some(&["! wB97x-3c def2-TZVPP Sp".to_string()][..])
    );
    assert_eq!(correction.kind, "sp");
    assert_eq!(loaded.records[0].electronic_energy, Some(-38.51));
}

#[test] // rq-6f572182
fn matching_is_exact_and_case_sensitive() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[simple_line(Some("1am"), Some(-38.51), RecordStatus::Ok)],
    );
    let sp = db_with(&dir, "sp.jsonl", &[sp_line("1AM", -76.4109, None)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--add-correction",
        &sp,
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(stderr(&run).contains("1 unmatched correction record"));
    let loaded = load_database(&out).expect("load out");
    assert!(loaded.records[0].correction.is_none());
}

#[test] // rq-c411edfa
fn every_record_sharing_the_name_receives_the_correction() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            simple_line(Some("1am"), Some(-38.51), RecordStatus::Ok),
            simple_line(Some("1am"), Some(-38.52), RecordStatus::Ok),
        ],
    );
    let sp = db_with(&dir, "sp.jsonl", &[sp_line("1am", -76.4109, None)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--add-correction",
        &sp,
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let loaded = load_database(&out).expect("load out");
    assert_eq!(loaded.records.len(), 2);
    for record in &loaded.records {
        assert_eq!(
            record.correction.as_ref().expect("attached").energy,
            -76.4109
        );
    }
}

#[test] // rq-c907b696
fn a_record_without_a_name_never_matches() {
    let dir = temp_dir();
    let db = database_in(&dir, &[simple_line(None, Some(-38.51), RecordStatus::Ok)]);
    let sp = db_with(&dir, "sp.jsonl", &[sp_line("1am", -76.4109, None)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--add-correction",
        &sp,
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(stderr(&run).contains("1 unmatched correction record"));
    let loaded = load_database(&out).expect("load out");
    assert!(loaded.records[0].correction.is_none());
}

#[test] // rq-ea21b840
fn a_candidate_without_a_name_is_unmatched() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[simple_line(Some("1am"), Some(-38.51), RecordStatus::Ok)],
    );
    let sp = db_with(
        &dir,
        "sp.jsonl",
        &[simple_line(None, Some(-76.4109), RecordStatus::Ok)],
    );
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--add-correction",
        &sp,
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(stderr(&run).contains("1 unmatched correction record"));
    let loaded = load_database(&out).expect("load out");
    assert!(loaded.records[0].correction.is_none());
}

#[test] // rq-0627beb4
fn a_new_match_replaces_a_stored_correction() {
    let dir = temp_dir();
    let db = database_in(&dir, &[corrected_db_line("1am", -38.51, -76.0, true)]);
    let sp = db_with(&dir, "sp.jsonl", &[sp_line("1am", -76.4109, None)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--add-correction",
        &sp,
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let loaded = load_database(&out).expect("load out");
    assert_eq!(
        loaded.records[0]
            .correction
            .as_ref()
            .expect("attached")
            .energy,
        -76.4109
    );
}

#[test] // rq-53c8c5fc
fn an_unmatched_record_keeps_its_stored_correction() {
    let dir = temp_dir();
    let db = database_in(&dir, &[corrected_db_line("2am", -38.51, -76.0, true)]);
    let sp = db_with(&dir, "sp.jsonl", &[sp_line("1am", -76.4109, None)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--add-correction",
        &sp,
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(stderr(&run).contains("1 unmatched correction record"));
    let loaded = load_database(&out).expect("load out");
    assert_eq!(
        loaded.records[0].correction.as_ref().expect("kept").energy,
        -76.0
    );
}

#[test] // rq-e59918d8
fn a_failed_correction_record_is_skipped_with_a_warning() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[simple_line(Some("1am"), Some(-38.51), RecordStatus::Ok)],
    );
    let sp = db_with(
        &dir,
        "sp.jsonl",
        &[simple_line(Some("1am"), Some(-76.4), RecordStatus::Failed)],
    );
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--add-correction",
        &sp,
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(
        stderr(&run).contains("skipped correction record \"1am\""),
        "{}",
        stderr(&run)
    );
    let loaded = load_database(&out).expect("load out");
    assert!(loaded.records[0].correction.is_none());
}

#[test] // rq-d599d450
fn a_failed_rerun_does_not_collide_with_its_ok_replacement() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[simple_line(Some("1am"), Some(-38.51), RecordStatus::Ok)],
    );
    let sp = db_with(
        &dir,
        "sp.jsonl",
        &[
            simple_line(Some("1am"), Some(-76.0), RecordStatus::Failed),
            sp_line("1am", -76.4109, None),
        ],
    );
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--add-correction",
        &sp,
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(
        stderr(&run).contains("skipped correction record \"1am\""),
        "{}",
        stderr(&run)
    );
    let loaded = load_database(&out).expect("load out");
    assert_eq!(
        loaded.records[0]
            .correction
            .as_ref()
            .expect("attached")
            .energy,
        -76.4109
    );
}

#[test] // rq-a83c76ac
fn an_ok_correction_record_without_an_energy_is_skipped_with_a_warning() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[simple_line(Some("1am"), Some(-38.51), RecordStatus::Ok)],
    );
    let sp = db_with(
        &dir,
        "sp.jsonl",
        &[simple_line(Some("1am"), None, RecordStatus::Ok)],
    );
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--add-correction",
        &sp,
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(
        stderr(&run).contains("skipped correction record \"1am\""),
        "{}",
        stderr(&run)
    );
    let loaded = load_database(&out).expect("load out");
    assert!(loaded.records[0].correction.is_none());
}

#[test] // rq-e15a3e2d
fn a_correction_record_without_a_command_attaches_without_commands() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[simple_line(Some("1am"), Some(-38.51), RecordStatus::Ok)],
    );
    let sp = db_with(&dir, "sp.jsonl", &[sp_line("1am", -76.4109, None)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--add-correction",
        &sp,
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let loaded = load_database(&out).expect("load out");
    let correction = loaded.records[0].correction.as_ref().expect("attached");
    assert_eq!(correction.energy, -76.4109);
    assert_eq!(correction.program.as_deref(), Some("orca"));
    assert_eq!(correction.kind, "sp");
    assert!(correction.commands.is_none());
}

#[test] // rq-74350863
fn duplicate_candidate_names_are_fatal() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[simple_line(Some("1am"), Some(-38.51), RecordStatus::Ok)],
    );
    let sp = db_with(
        &dir,
        "sp.jsonl",
        &[sp_line("1am", -76.4109, None), sp_line("1am", -76.5, None)],
    );
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--add-correction",
        &sp,
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
    assert!(stderr(&run).contains("1am"), "{}", stderr(&run));
    assert!(!out.exists());
}

#[test] // rq-922a22be
fn a_corrupt_line_in_the_correction_database_is_skipped_and_fails_the_run() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[simple_line(Some("1am"), Some(-38.51), RecordStatus::Ok)],
    );
    let sp_path = dir.join("sp.jsonl");
    let content = format!("{}\nnot json at all\n", sp_line("1am", -76.4109, None));
    fs::write(&sp_path, content).expect("write sp");
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--add-correction",
        &sp_path.to_string_lossy(),
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
    let err = stderr(&run);
    assert!(err.contains("sp.jsonl"), "{err}");
    assert!(err.contains("line 2"), "{err}");
    let loaded = load_database(&out).expect("load out");
    assert_eq!(
        loaded.records[0]
            .correction
            .as_ref()
            .expect("attached")
            .energy,
        -76.4109
    );
}

#[test] // rq-9fd40c4e
fn a_correction_database_that_is_a_program_output_is_fatal() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[simple_line(Some("1am"), Some(-38.51), RecordStatus::Ok)],
    );
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--add-correction",
        &fixture("xtb/1dma.log"),
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
    assert!(!out.exists());
}

#[test] // rq-83e236b7
fn a_missing_correction_database_is_fatal() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[simple_line(Some("1am"), Some(-38.51), RecordStatus::Ok)],
    );
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--add-correction",
        &dir.join("missing.jsonl").to_string_lossy(),
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
    assert!(!out.exists());
}

#[test] // rq-d1bd7404
fn repeating_add_correction_is_a_usage_error() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[simple_line(Some("1am"), Some(-38.51), RecordStatus::Ok)],
    );
    let a = db_with(&dir, "a.jsonl", &[sp_line("1am", -76.4, None)]);
    let b = db_with(&dir, "b.jsonl", &[sp_line("2am", -76.5, None)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--add-correction",
        &a,
        "--add-correction",
        &b,
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test] // rq-3fd0b9bb
fn add_correction_alone_is_not_a_visible_outcome() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[simple_line(Some("1am"), Some(-38.51), RecordStatus::Ok)],
    );
    let sp = db_with(&dir, "sp.jsonl", &[sp_line("1am", -76.4, None)]);
    let run = cjkqc(&[&db, "--add-correction", &sp]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test] // rq-58d85e68
fn a_correcting_build_run_reports_its_corrections() {
    let dir = temp_dir();
    let sp = db_with(&dir, "sp.jsonl", &[sp_line("1dma", -76.4109, None)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &fixture("xtb/1dma.log"),
        "--add-correction",
        &sp,
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    assert!(text.contains("collected 1 records into"), "{text}");
    assert!(
        text.contains("(0 failed, 0 truncated, 1 corrected)"),
        "{text}"
    );
}

#[test] // rq-66e4cb89
fn a_correcting_merge_run_reports_its_corrections() {
    let dir = temp_dir();
    let a = database_in(
        &dir,
        &[
            simple_line(Some("1am"), Some(-38.51), RecordStatus::Ok),
            simple_line(Some("2am"), Some(-38.52), RecordStatus::Ok),
        ],
    );
    let sp = db_with(&dir, "sp.jsonl", &[sp_line("1am", -76.4109, None)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[&a, "--add-correction", &sp, "--out", &out.to_string_lossy()]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    assert!(text.contains("merged 2 records into"), "{text}");
    assert!(text.contains("(1 corrected)"), "{text}");
}

#[test] // rq-c0c3a1af
fn unknown_keys_do_not_survive_a_correcting_run() {
    let dir = temp_dir();
    let db_path = dir.join("a.jsonl");
    fs::write(
        &db_path,
        "{\"status\":\"ok\",\"name\":\"1am\",\"electronic_energy\":-38.51,\"future_field\":7}\n",
    )
    .expect("write db");
    let sp = db_with(&dir, "sp.jsonl", &[sp_line("1am", -76.4109, None)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db_path.to_string_lossy(),
        "--add-correction",
        &sp,
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = fs::read_to_string(&out).expect("read out");
    assert!(!text.contains("future_field"), "{text}");
    assert!(text.contains("\"correction\""), "{text}");
}

#[test] // rq-b5e3b553
fn el_and_g_stay_raw_while_the_corrected_columns_show_the_corrected_values() {
    let dir = temp_dir();
    let db = database_in(&dir, &[thermo_line("1am", -38.51, &[], Some(-76.4109))]);
    let run = cjkqc(&[&db, "--el", "--h", "--g", "--elcorr", "--hcorr", "--gcorr"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let rows = data_rows(&stdout(&run));
    assert_eq!(rows.len(), 1, "{}", stdout(&run));
    assert_eq!(rows[0].len(), 6);
    assert_eq!(rows[0][0], "-38.5100000000");
    let el: f64 = rows[0][0].parse().expect("el");
    let h: f64 = rows[0][1].parse().expect("h");
    let g: f64 = rows[0][2].parse().expect("g");
    let elcorr: f64 = rows[0][3].parse().expect("elcorr");
    let hcorr: f64 = rows[0][4].parse().expect("hcorr");
    let gcorr: f64 = rows[0][5].parse().expect("gcorr");
    assert_eq!(elcorr, -76.4109);
    assert!(
        (hcorr - (-76.4109 + (h - el))).abs() < 1e-9,
        "{}",
        stdout(&run)
    );
    assert!(
        (gcorr - (-76.4109 + (g - el))).abs() < 1e-9,
        "{}",
        stdout(&run)
    );
}

#[test] // rq-bca9f7b9
fn the_corrected_columns_print_dash_without_a_correction() {
    let dir = temp_dir();
    let db = database_in(&dir, &[thermo_line("1am", -38.51, &[], None)]);
    let run = cjkqc(&[&db, "--elcorr", "--hcorr", "--gcorr"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let rows = data_rows(&stdout(&run));
    assert_eq!(rows[0], vec!["-", "-", "-"], "{}", stdout(&run));
}

#[test] // rq-5aca0e6b
fn hcorr_and_gcorr_are_undefined_without_defined_thermochemistry() {
    let dir = temp_dir();
    // A non-monatomic record with a correction and no frequencies.
    let mut record = Record {
        status: RecordStatus::Ok,
        name: Some("1am".to_string()),
        electronic_energy: Some(-38.51),
        geometry: Some(hexagon()),
        ..Record::default()
    };
    record.correction = Some(Correction {
        energy: -76.4109,
        program: None,
        program_version: None,
        commands: None,
        kind: "sp".to_string(),
    });
    let db = database_in(
        &dir,
        &[serde_json::to_string(&record).expect("serialization")],
    );
    let run = cjkqc(&[&db, "--hcorr", "--gcorr"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let rows = data_rows(&stdout(&run));
    assert_eq!(rows[0], vec!["-", "-"], "{}", stdout(&run));
}

#[test] // rq-21414dd3
fn the_correction_column_prints_kind_program_version_and_quoted_commands() {
    let dir = temp_dir();
    let db = database_in(&dir, &[corrected_db_line("1am", -38.51, -76.4109, true)]);
    let run = cjkqc(&[&db, "--correction"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let row = text.lines().nth(1).expect("data row");
    assert_eq!(
        row,
        "sp orca 6.0.0 \"! wB97x-3c def2-TZVPP Sp\"",
        "{}",
        stdout(&run)
    );
}

#[test] // rq-be59d52a
fn the_correction_column_drops_absent_pieces() {
    let dir = temp_dir();
    // A correction with a program but no version and no commands.
    let mut record = Record {
        status: RecordStatus::Ok,
        name: Some("1am".to_string()),
        electronic_energy: Some(-38.51),
        ..Record::default()
    };
    record.correction = Some(Correction {
        energy: -76.4109,
        program: Some("orca".to_string()),
        program_version: None,
        commands: None,
        kind: "sp".to_string(),
    });
    let db = database_in(
        &dir,
        &[serde_json::to_string(&record).expect("serialization")],
    );
    let run = cjkqc(&[&db, "--correction"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let row = text.lines().nth(1).expect("data row");
    assert_eq!(row, "sp orca", "{text}");
}

#[test] // rq-f3233093
fn the_correction_column_prints_dash_without_a_correction() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[simple_line(Some("1am"), Some(-38.51), RecordStatus::Ok)],
    );
    let run = cjkqc(&[&db, "--correction"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let rows = data_rows(&stdout(&run));
    assert_eq!(rows[0][0], "-", "{}", stdout(&run));
}

#[test] // rq-e861c941 rq-a8f483c7
fn the_corrected_columns_follow_the_canonical_order() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[simple_line(Some("1am"), Some(-38.51), RecordStatus::Ok)],
    );
    let run = cjkqc(&[
        &db,
        "--gcorr",
        "--elcorr",
        "--correction",
        "--command",
        "--hcorr",
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let header = stdout(&run).lines().next().expect("header").to_string();
    assert_eq!(
        header,
        "command correction elcorr hcorr gcorr",
        "{}",
        stdout(&run)
    );
}

#[test] // rq-c51906fe
fn unit_converts_the_corrected_energy_columns() {
    let dir = temp_dir();
    let db = database_in(&dir, &[thermo_line("1am", -38.51, &[], Some(-76.4109))]);
    let run = cjkqc(&[
        &db,
        "--elcorr",
        "--hcorr",
        "--gcorr",
        "--correction",
        "--unit",
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    // The correction cell holds spaces (quoted command), so parse the
    // row from the end: the last three cells are the corrected energies.
    let text = stdout(&run);
    let row = text.lines().nth(1).expect("data row");
    assert!(
        row.starts_with("sp orca 6.0.0 \"! wB97x-3c def2-TZVPP Sp\" "),
        "{}",
        stdout(&run)
    );
    let cells: Vec<&str> = row.split_whitespace().collect();
    let numbers = &cells[cells.len() - 3..];
    for cell in numbers {
        let value: f64 = cell.parse().expect("number");
        let decimals = cell.split('.').nth(1).expect("decimals").len();
        assert_eq!(decimals, 4, "{cell}");
        assert!(value < -7000.0, "{cell} is not in kcal/mol");
    }
}

#[test] // rq-234d7e28 rq-41bf3f48
fn a_relative_corrected_energy_window_is_per_composition_group() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            thermo_line("a0", 0.0, &[("am", 1)], Some(0.0)),
            thermo_line("a2", 0.0, &[("am", 1)], Some(0.02)),
            thermo_line("bneg", 0.0, &[("sa", 1)], Some(-5.0)),
        ],
    );
    let run = cjkqc(&[&db, "--cutr", "elcorr", "5", "--filename"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let rows = data_rows(&stdout(&run));
    let names: Vec<&String> = rows.iter().map(|row| &row[0]).collect();
    assert!(names.contains(&&"a0".to_string()), "{}", stdout(&run));
    assert!(names.contains(&&"bneg".to_string()), "{}", stdout(&run));
    assert!(!names.contains(&&"a2".to_string()), "{}", stdout(&run));
}

#[test] // rq-aa358806
fn a_record_without_a_correction_fails_corrected_cutoffs_except_inequality() {
    let dir = temp_dir();
    // The corrected record's energy is positive, so `pass elcorr 0`
    // (elcorr > 0) separates it from the uncorrected record.
    let db = database_in(
        &dir,
        &[
            thermo_line("corr", -38.51, &[("am", 1)], Some(0.5)),
            thermo_line("bare", -38.51, &[("am", 1)], None),
        ],
    );
    let run = cjkqc(&[&db, "--pass", "elcorr", "0", "--filename"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let rows = data_rows(&stdout(&run));
    let names: Vec<&str> = rows.iter().map(|row| row[0].as_str()).collect();
    assert_eq!(names, vec!["corr"], "{}", stdout(&run));

    let run = cjkqc(&[&db, "--isnot", "elcorr", "0", "--filename"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let rows = data_rows(&stdout(&run));
    assert_eq!(rows.len(), 2, "{}", stdout(&run));
}

#[test] // rq-bdac8cdb
fn sort_elcorr_orders_ascending_with_undefined_last() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            simple_line(Some("high"), Some(-38.51), RecordStatus::Ok),
            corrected_db_line("low", -38.51, -76.5, false),
            corrected_db_line("mid", -38.51, -76.4, false),
        ],
    );
    let run = cjkqc(&[&db, "--sort", "elcorr", "--filename"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let rows = data_rows(&stdout(&run));
    let names: Vec<&str> = rows.iter().map(|row| row[0].as_str()).collect();
    assert_eq!(names, vec!["low", "mid", "high"], "{}", stdout(&run));

    let run = cjkqc(&[&db, "--sort", "elcorr", "--filename", "--reverse"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let rows = data_rows(&stdout(&run));
    let names: Vec<&str> = rows.iter().map(|row| row[0].as_str()).collect();
    assert_eq!(names, vec!["high", "mid", "low"], "{}", stdout(&run));
}

#[test] // rq-d84cf2be rq-16bb6b61
fn sort_gcorr_orders_ascending_with_undefined_last() {
    let dir = temp_dir();
    // Tune the correction energies so the corrected Gibbs values hit
    // -100.5 and -100.0 exactly (gcorr = elcorr + (g - el)).
    let g_minus_el = raw_gibbs(-38.51) - -38.51;
    let g1 = -100.5 - g_minus_el;
    let g2 = -100.0 - g_minus_el;
    let db = database_in(
        &dir,
        &[
            thermo_line("one", -38.51, &[("am", 1)], Some(g1)),
            thermo_line("two", -38.51, &[("am", 1)], Some(g2)),
            simple_line(Some("bare"), Some(-38.51), RecordStatus::Ok),
        ],
    );
    let run = cjkqc(&[&db, "--sort", "gcorr", "--filename"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let rows = data_rows(&stdout(&run));
    let names: Vec<&str> = rows.iter().map(|row| row[0].as_str()).collect();
    assert_eq!(names, vec!["one", "two", "bare"], "{}", stdout(&run));

    let run = cjkqc(&[&db, "--sort", "gcorr", "--filename", "--reverse"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let rows = data_rows(&stdout(&run));
    let names: Vec<&str> = rows.iter().map(|row| row[0].as_str()).collect();
    assert_eq!(names, vec!["bare", "two", "one"], "{}", stdout(&run));
}

#[test] // rq-86219849
fn stored_corrections_work_without_add_correction() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            thermo_line("low", 0.0, &[("am", 1)], Some(0.0)),
            thermo_line("high", 0.0, &[("am", 1)], Some(0.02)),
        ],
    );
    let run = cjkqc(&[&db, "--cutr", "elcorr", "5", "--filename"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let rows = data_rows(&stdout(&run));
    let names: Vec<&str> = rows.iter().map(|row| row[0].as_str()).collect();
    assert_eq!(names, vec!["low"], "{}", stdout(&run));
}

#[test] // rq-34d743c7
fn the_corrected_formation_section_follows_the_raw_one() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            thermo_line("am1", -50.0, &[("am", 1)], Some(-76.5)),
            thermo_line("cluster", -141.0, &[("am", 2)], Some(-167.5)),
        ],
    );
    let run = cjkqc(&[&db, "--formation", "--formation-corr"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let raw = find_line(&text, &["name", "el", "h", "st", "g"]).expect("raw header");
    let corrected = find_line(&text, &["name", "elcorr", "hcorr", "stcorr", "gcorr"])
        .expect("corrected header");
    assert!(raw < corrected, "{text}");
}

#[test] // rq-09e99e7b
fn formation_corr_alone_is_a_visible_outcome() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            thermo_line("am1", -50.0, &[("am", 1)], Some(-76.5)),
            thermo_line("cluster", -141.0, &[("am", 2)], Some(-167.5)),
        ],
    );
    let run = cjkqc(&[&db, "--formation-corr"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(
        lines[0],
        ["name", "elcorr", "hcorr", "stcorr", "gcorr"],
        "{}",
        stdout(&run)
    );
}

// --- correction hardening (PR review findings) ---

#[test]
fn a_correcting_run_can_print_the_corrected_formation_section() {
    // The primary real-world flow: attach corrections, then immediately
    // view the corrected formation section over the enriched survivors.
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            thermo_line("am1", -50.0, &[("am", 1)], None),
            thermo_line("cluster", -141.0, &[("am", 2)], None),
        ],
    );
    let sp = db_with(
        &dir,
        "sp.jsonl",
        &[
            sp_line("am1", -76.5, None),
            sp_line("cluster", -167.5, None),
        ],
    );
    let run = cjkqc(&[&db, "--add-correction", &sp, "--formation-corr"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(
        lines[0],
        ["name", "elcorr", "hcorr", "stcorr", "gcorr"],
        "{text}"
    );
    // The cluster's corrected electronic formation: -167.5 - 2*(-76.5).
    let mut found = false;
    for line in text.lines().skip(1) {
        let cells: Vec<&str> = line.split_whitespace().collect();
        if cells.first() == Some(&"2am") {
            found = true;
            assert_eq!(cells[1], "-14.5000000000", "{text}");
        }
    }
    assert!(found, "no 2am row: {text}");
}

#[test]
fn a_correcting_run_with_a_print_flag_writes_ok_only_survivors() {
    // The carve-out's other side: with a print flag the run takes the
    // pipeline path — ok-only write and the filtered summary.
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            simple_line(Some("1am"), Some(-38.51), RecordStatus::Ok),
            simple_line(Some("bad"), None, RecordStatus::Failed),
        ],
    );
    let sp = db_with(&dir, "sp.jsonl", &[sp_line("1am", -76.4109, None)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--add-correction",
        &sp,
        "--filename",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    assert!(text.contains("filtered 1 records into"), "{text}");
    assert!(text.contains("(1 corrected)"), "{text}");
    let loaded = load_database(&out).expect("load out");
    assert_eq!(loaded.records.len(), 1, "{}", stderr(&run));
    assert!(loaded.records[0].correction.is_some());
}

#[test]
fn an_attachment_only_run_writes_every_record() {
    // The carve-out itself: without print flags or other manipulations the
    // run keeps collection semantics — the failed record is written too,
    // uncorrected, and the summary is the collection one.
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            simple_line(Some("1am"), Some(-38.51), RecordStatus::Ok),
            simple_line(Some("bad"), None, RecordStatus::Failed),
        ],
    );
    let sp = db_with(&dir, "sp.jsonl", &[sp_line("1am", -76.4109, None)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--add-correction",
        &sp,
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    assert!(text.contains("merged 2 records into"), "{text}");
    assert!(text.contains("(1 corrected)"), "{text}");
    let loaded = load_database(&out).expect("load out");
    assert_eq!(loaded.records.len(), 2);
    let corrected: Vec<_> = loaded
        .records
        .iter()
        .filter(|record| record.correction.is_some())
        .collect();
    assert_eq!(corrected.len(), 1);
    assert_eq!(corrected[0].name.as_deref(), Some("1am"));
}

// ---------------------------------------------------------------------------
// Formation at given concentrations (`rqm/formation.md`).
// ---------------------------------------------------------------------------

/// The per-molecule R T in Eh at the given temperature: k_B T / E_h.
fn kbt_eh(temperature: f64) -> f64 {
    K_B * temperature / E_HARTREE
}

/// The stdout from the formation section's header onward, so
/// `formation_row` never matches a main-table row of the same token.
fn formation_section(stdout_text: &str) -> &str {
    section_from(stdout_text, &["name", "el", "h", "st", "g"])
}

/// The stdout from the corrected formation section's header onward.
fn corrected_section(stdout_text: &str) -> &str {
    section_from(stdout_text, &["name", "elcorr", "hcorr", "stcorr", "gcorr"])
}

/// The amount added to a formation free energy per molecule of a
/// component specified at `magnitude` ppt: −k_B T ln(magnitude·10⁻¹²)/E_h.
fn ppt_shift(magnitude: f64) -> f64 {
    -K_B * 298.15 * (magnitude * 1.0e-12).ln() / E_HARTREE
}

#[test] // rq-9423306b
fn a_value_outside_the_grammar_is_a_usage_error() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[thermo_line("am1", -50.0, &[("am", 1)], None)],
    );
    let run = cjkqc(&[&db, "--formation", "--conc", "sa", "100ppm"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test] // rq-890737af
fn a_non_positive_or_non_finite_value_is_a_usage_error() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[thermo_line("am1", -50.0, &[("am", 1)], None)],
    );
    for value in ["0", "-1e-7", "nan", "inf"] {
        let run = cjkqc(&[&db, "--formation", "--conc", "sa", value]);
        assert_eq!(run.status.code(), Some(2), "{value}: {}", stderr(&run));
    }
}

#[test] // rq-ae14b02d
fn the_caret_power_syntax_is_not_part_of_the_grammar() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[thermo_line("am1", -50.0, &[("am", 1)], None)],
    );
    let run = cjkqc(&[&db, "--formation", "--conc", "sa", "10^3ppt"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test] // rq-3384f0c4
fn a_non_letter_code_is_a_usage_error() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[thermo_line("am1", -50.0, &[("am", 1)], None)],
    );
    let run = cjkqc(&[&db, "--formation", "--conc", "2sa", "100ppt"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test] // rq-fecf8cb9
fn naming_one_code_twice_is_a_usage_error() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[thermo_line("am1", -50.0, &[("am", 1)], None)],
    );
    let run = cjkqc(&[
        &db,
        "--formation",
        "--conc",
        "sa",
        "100ppt",
        "--conc",
        "sa",
        "1ppb",
    ]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test] // rq-98440673
fn the_named_component_shifts_the_g_level_by_its_chemical_potential() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_line("am1", -50.0, &[("am", 1)], None),
            thermo_line("sa1", -40.0, &[("sa", 1)], None),
            thermo_line("cluster", -91.0, &[("am", 1), ("sa", 1)], None),
        ],
    );
    let base = cjkqc(&[&db, "--formation"]);
    assert_eq!(base.status.code(), Some(0), "{}", stderr(&base));
    let run = cjkqc(&[&db, "--formation", "--conc", "sa", "100ppt"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let base_row = formation_row(&stdout(&base), "1am1sa");
    let row = formation_row(&stdout(&run), "1am1sa");
    // The sa term: g_form − k_B T ln(100·10⁻¹²) — about +13.6 kcal/mol.
    let g_base: f64 = base_row[4].parse().expect("base gibbs");
    let g_conc: f64 = row[4].parse().expect("conc gibbs");
    let expected = -kbt_eh(298.15) * (100.0_f64 * 1.0e-12).ln();
    assert!(
        (g_conc - g_base - expected).abs() < 1e-9,
        "{}",
        stdout(&run)
    );
    // The st level moves by the same magnitude the other way: the
    // concentration term is an entropy shift, so the dilute vapor's
    // higher entropy lowers the formation entropy term.
    let st_base: f64 = base_row[3].parse().expect("base st");
    let st_conc: f64 = row[3].parse().expect("conc st");
    assert!(
        (st_conc - st_base + expected).abs() < 1e-9,
        "{}",
        stdout(&run)
    );
    // The el and h levels stay at the standard state.
    assert_eq!(row[1], base_row[1], "{}", stdout(&run));
    assert_eq!(row[2], base_row[2], "{}", stdout(&run));
    // And the g value still equals its h value minus its st value.
    let h_conc: f64 = row[2].parse().expect("conc h");
    assert!(
        (g_conc - (h_conc - st_conc)).abs() < 1e-9,
        "{}",
        stdout(&run)
    );
}

#[test] // rq-77502cf6
fn the_concentration_term_moves_g_and_st_by_equal_and_opposite_amounts() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_line("am1", -50.0, &[("am", 1)], None),
            thermo_line("sa1", -40.0, &[("sa", 1)], None),
            thermo_line("cluster", -91.0, &[("am", 1), ("sa", 1)], None),
        ],
    );
    let base = cjkqc(&[&db, "--formation"]);
    assert_eq!(base.status.code(), Some(0), "{}", stderr(&base));
    let run = cjkqc(&[&db, "--formation", "--conc", "sa", "100ppt"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let base_row = formation_row(&stdout(&base), "1am1sa");
    let row = formation_row(&stdout(&run), "1am1sa");
    // The dilute vapor makes the cluster easier to beat: g rises by
    // exactly the amount st falls.
    let g_shift =
        row[4].parse::<f64>().expect("conc g") - base_row[4].parse::<f64>().expect("base g");
    let st_shift =
        row[3].parse::<f64>().expect("conc st") - base_row[3].parse::<f64>().expect("base st");
    assert!((g_shift + st_shift).abs() < 1e-9, "{g_shift} vs {st_shift}");
    assert!(g_shift > 0.0 && st_shift < 0.0, "{g_shift} vs {st_shift}");
    // And both runs' h cells are equal.
    assert_eq!(row[2], base_row[2]);
}

#[test] // rq-85150fe0
fn several_conc_flags_each_cover_their_own_component() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_line("am1", -50.0, &[("am", 1)], None),
            thermo_line("sa1", -40.0, &[("sa", 1)], None),
            thermo_line("cluster", -131.0, &[("am", 2), ("sa", 1)], None),
        ],
    );
    let base = cjkqc(&[&db, "--formation"]);
    assert_eq!(base.status.code(), Some(0), "{}", stderr(&base));
    let run = cjkqc(&[
        &db,
        "--formation",
        "--conc",
        "am",
        "100ppt",
        "--conc",
        "sa",
        "10ppb",
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let g_base: f64 = formation_row(&stdout(&base), "2am1sa")[4]
        .parse()
        .expect("base gibbs");
    let g_conc: f64 = formation_row(&stdout(&run), "2am1sa")[4]
        .parse()
        .expect("conc gibbs");
    // am at 100 ppt contributes twice, sa at 10 ppb once.
    let expected = -(2.0 * kbt_eh(298.15) * (100.0_f64 * 1.0e-12).ln()
        + kbt_eh(298.15) * (10.0_f64 * 1.0e-9).ln());
    assert!(
        (g_conc - g_base - expected).abs() < 1e-9,
        "{}",
        stdout(&run)
    );
}

#[test] // rq-040dd0d3
fn cnt_keeps_a_monomers_formation_values_at_zero() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[thermo_line("sa1", -40.0, &[("sa", 1)], None)],
    );
    let run = cjkqc(&[&db, "--formation", "--cnt", "--conc", "sa", "100ppt"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let row = formation_row(&stdout(&run), "1sa");
    assert_eq!(row[4], "0.0000000000", "{}", stdout(&run));
    assert_eq!(row[3], "0.0000000000", "{}", stdout(&run));
    // Without --cnt the monomer carries the full chemical-potential term:
    // g up by R T ln(10^10) and st down by the same amount.
    let plain = cjkqc(&[&db, "--formation", "--conc", "sa", "100ppt"]);
    assert_eq!(plain.status.code(), Some(0), "{}", stderr(&plain));
    let row = formation_row(&stdout(&plain), "1sa");
    let g: f64 = row[4].parse().expect("monomer gibbs");
    assert!((g - ppt_shift(100.0)).abs() < 1e-9, "{}", stdout(&plain));
    let st: f64 = row[3].parse().expect("monomer st");
    assert!((st + ppt_shift(100.0)).abs() < 1e-9, "{}", stdout(&plain));
}

#[test] // rq-fd428ce5
fn a_concentration_above_the_standard_state_lowers_the_formation_free_energy() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_line("sa1", -40.0, &[("sa", 1)], None),
            thermo_line("cluster", -81.0, &[("sa", 2)], None),
        ],
    );
    let base = cjkqc(&[&db, "--formation"]);
    assert_eq!(base.status.code(), Some(0), "{}", stderr(&base));
    let run = cjkqc(&[&db, "--formation", "--conc", "sa", "202650"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let g_base: f64 = formation_row(&stdout(&base), "2sa")[4]
        .parse()
        .expect("base gibbs");
    let g_conc: f64 = formation_row(&stdout(&run), "2sa")[4]
        .parse()
        .expect("conc gibbs");
    // 202650 Pa is 2 p°: the correction flips sign and lowers g.
    let expected = -2.0 * kbt_eh(298.15) * (2.0_f64).ln();
    assert!(
        (g_conc - g_base - expected).abs() < 1e-9,
        "{}",
        stdout(&run)
    );
    assert!(g_conc < g_base);
}

#[test] // rq-b5d0f3d1
fn a_spec_at_the_standard_pressure_reproduces_the_uncorrected_values() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_line("am1", -50.0, &[("am", 1)], None),
            thermo_line("sa1", -40.0, &[("sa", 1)], None),
            thermo_line("cluster", -91.0, &[("am", 1), ("sa", 1)], None),
        ],
    );
    let plain = cjkqc(&[&db, "--formation"]);
    let corrected = cjkqc(&[
        &db,
        "--formation",
        "--conc",
        "sa",
        // p° itself: the logarithm is exactly zero.
        "101325",
        "--conc",
        "am",
        "101325",
    ]);
    assert_eq!(plain.status.code(), Some(0), "{}", stderr(&plain));
    assert_eq!(corrected.status.code(), Some(0), "{}", stderr(&corrected));
    assert_eq!(stdout(&plain), stdout(&corrected));
}

#[test] // rq-9b67220d
fn the_correction_never_creates_a_defined_value() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_line("am1", -50.0, &[("am", 1)], None),
            thermo_line("sa1", -40.0, &[("sa", 1)], None),
            thermo_line("cluster", -91.0, &[("am", 1), ("sa", 1)], None),
        ],
    );
    let run = cjkqc(&[
        &db,
        "--extract",
        "1am1sa",
        "--formation",
        "--conc",
        "sa",
        "100ppt",
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let row = formation_row(&stdout(&run), "1am1sa");
    assert_eq!(row[1], "-");
    assert_eq!(row[2], "-");
    assert_eq!(row[3], "-");
    assert_eq!(row[4], "-");
}

#[test] // rq-c984bd29
fn undefined_thermochemistry_stays_undefined_under_conc() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_line("sa1", -40.0, &[("sa", 1)], None),
            // The cluster carries no frequencies: its own Gibbs is
            // undefined. The higher el keeps the defined monomer as the
            // sa reference, so the dash below is the cluster's own
            // undefined level, not an undefined reference.
            thermo_record_json("cluster", Some(-30.0), &[("sa", 1)], None),
        ],
    );
    let run = cjkqc(&[&db, "--formation", "--conc", "sa", "100ppt"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    // Both records print the 1sa token; the defined monomer comes first.
    let rows: Vec<Vec<String>> = stdout(&run)
        .lines()
        .skip(1)
        .filter(|line| line.starts_with("1sa "))
        .map(|line| line.split_whitespace().map(str::to_string).collect())
        .collect();
    assert_eq!(rows.len(), 2, "{}", stdout(&run));
    let reference: f64 = rows[0][4].parse().expect("reference monomer gibbs");
    assert!((reference - ppt_shift(100.0)).abs() < 1e-9);
    // The cluster: its own thermochemistry is undefined under --conc.
    assert_eq!(rows[1][4], "-", "{}", stdout(&run));
}

#[test] // rq-e8ea3b7f
fn the_corrected_section_shifts_gcorr_and_stcorr_the_same_way() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_line("am1", -50.0, &[("am", 1)], Some(-76.5)),
            thermo_line("sa1", -40.0, &[("sa", 1)], Some(-69.5)),
            thermo_line("cluster", -91.0, &[("am", 1), ("sa", 1)], Some(-146.5)),
        ],
    );
    let base = cjkqc(&[&db, "--formation-corr"]);
    assert_eq!(base.status.code(), Some(0), "{}", stderr(&base));
    let run = cjkqc(&[&db, "--formation-corr", "--conc", "sa", "100ppt"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let base_row = formation_row(corrected_section(&stdout(&base)), "1am1sa");
    let row = formation_row(corrected_section(&stdout(&run)), "1am1sa");
    let g_base: f64 = base_row[4].parse().expect("base gcorr");
    let g_conc: f64 = row[4].parse().expect("conc gcorr");
    let expected = -kbt_eh(298.15) * (100.0_f64 * 1.0e-12).ln();
    assert!(
        (g_conc - g_base - expected).abs() < 1e-9,
        "{}",
        stdout(&run)
    );
    // The stcorr level moves the other way by the same magnitude, and
    // the elcorr and hcorr levels stay at the standard state.
    let st_base: f64 = base_row[3].parse().expect("base stcorr");
    let st_conc: f64 = row[3].parse().expect("conc stcorr");
    assert!(
        (st_conc - st_base + expected).abs() < 1e-9,
        "{}",
        stdout(&run)
    );
    assert_eq!(row[1], base_row[1]);
    assert_eq!(row[2], base_row[2]);
}

#[test] // rq-4ca420a5
fn the_corrected_entropy_term_equals_the_raw_entropy_term() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_line("am1", -50.0, &[("am", 1)], Some(-76.5)),
            thermo_line("sa1", -40.0, &[("sa", 1)], Some(-69.5)),
            thermo_line("cluster", -91.0, &[("am", 1), ("sa", 1)], Some(-146.5)),
            // A cluster without a correction: every corrected level,
            // stcorr included, prints a dash.
            thermo_line("bare", -81.0, &[("sa", 2)], None),
        ],
    );
    let run = cjkqc(&[&db, "--formation", "--formation-corr"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let raw = formation_row(formation_section(&stdout(&run)), "1am1sa");
    let corrected = formation_row(corrected_section(&stdout(&run)), "1am1sa");
    // The correction does not alter any thermal term: the cluster's
    // stcorr cell equals its st cell.
    assert_eq!(corrected[3], raw[3], "{}", stdout(&run));
    // A cluster without a correction prints dashes in the stcorr column.
    let bare = formation_row(corrected_section(&stdout(&run)), "2sa");
    assert_eq!(bare[3], "-", "{}", stdout(&run));
}

#[test] // rq-985cc1a5
fn a_spec_for_a_code_in_no_final_record_is_a_usage_error() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_line("am1", -50.0, &[("am", 1)], None),
            thermo_line("sa1", -40.0, &[("sa", 1)], None),
            thermo_line("cluster", -91.0, &[("am", 1), ("sa", 1)], None),
        ],
    );
    let run = cjkqc(&[&db, "--formation", "--conc", "w", "100ppt"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test] // rq-38b48ce6
fn a_spec_whose_monomers_were_filtered_away_is_not_a_usage_error() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_line("am1", -50.0, &[("am", 1)], None),
            thermo_line("sa1", -40.0, &[("sa", 1)], None),
            thermo_line("cluster", -91.0, &[("am", 1), ("sa", 1)], None),
        ],
    );
    let run = cjkqc(&[
        &db,
        "--extract",
        "1am1sa",
        "--formation",
        "--conc",
        "am",
        "100ppt",
        "--conc",
        "sa",
        "100ppt",
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let row = formation_row(&stdout(&run), "1am1sa");
    assert_eq!(row[1], "-");
    assert_eq!(row[2], "-");
    assert_eq!(row[3], "-");
    assert_eq!(row[4], "-");
}

#[test] // rq-75ac7572
fn conc_without_a_formation_section_is_an_accepted_no_op() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[thermo_line("am1", -50.0, &[("am", 1)], None)],
    );
    let plain = cjkqc(&[&db, "--filename", "--el"]);
    let with_conc = cjkqc(&[&db, "--filename", "--el", "--conc", "sa", "100ppt"]);
    assert_eq!(plain.status.code(), Some(0), "{}", stderr(&plain));
    assert_eq!(with_conc.status.code(), Some(0), "{}", stderr(&with_conc));
    assert_eq!(stdout(&plain), stdout(&with_conc));
}

#[test] // rq-d6fc375d
fn conc_alone_is_not_a_visible_outcome() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[thermo_line("am1", -50.0, &[("am", 1)], None)],
    );
    let run = cjkqc(&[&db, "--conc", "sa", "100ppt"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test] // rq-6f3e4732
fn the_main_table_and_glob_ignore_the_concentrations() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_line("am1", -50.0, &[("am", 1)], None),
            thermo_line("sa1", -40.0, &[("sa", 1)], None),
            thermo_line("cluster", -131.0, &[("am", 2), ("sa", 1)], None),
        ],
    );
    let base = cjkqc(&[&db, "--name", "--g", "--glob", "--formation"]);
    let run = cjkqc(&[
        &db,
        "--name",
        "--g",
        "--glob",
        "--formation",
        "--conc",
        "sa",
        "100ppt",
    ]);
    assert_eq!(base.status.code(), Some(0), "{}", stderr(&base));
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    // The main tables are identical; only the formation section moves.
    let table_end =
        |text: &str| find_line(text, &["name", "el", "h", "st", "g"]).expect("formation header");
    assert_eq!(
        &stdout(&base)[..table_end(&stdout(&base))],
        &stdout(&run)[..table_end(&stdout(&run))]
    );
    let g_base: f64 = formation_row(formation_section(&stdout(&base)), "2am1sa")[4]
        .parse()
        .expect("base gibbs");
    let g_conc: f64 = formation_row(formation_section(&stdout(&run)), "2am1sa")[4]
        .parse()
        .expect("conc gibbs");
    assert!((g_conc - g_base - ppt_shift(100.0)).abs() < 1e-9);
}

#[test] // rq-ef6c9159
fn the_ppt_correction_scales_linearly_with_temperature() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_line("am1", -50.0, &[("am", 1)], None),
            thermo_line("sa1", -40.0, &[("sa", 1)], None),
            thermo_line("cluster", -91.0, &[("am", 1), ("sa", 1)], None),
        ],
    );
    let base_warm = cjkqc(&[&db, "--formation", "--temp", "298.15"]);
    let conc_warm = cjkqc(&[
        &db,
        "--formation",
        "--temp",
        "298.15",
        "--conc",
        "sa",
        "100ppt",
    ]);
    let base_hot = cjkqc(&[&db, "--formation", "--temp", "350"]);
    let conc_hot = cjkqc(&[
        &db,
        "--formation",
        "--temp",
        "350",
        "--conc",
        "sa",
        "100ppt",
    ]);
    for run in [&base_warm, &conc_warm, &base_hot, &conc_hot] {
        assert_eq!(run.status.code(), Some(0), "{}", stderr(run));
    }
    let g = |out: &Output| -> f64 {
        formation_row(&stdout(out), "1am1sa")[4]
            .parse()
            .expect("gibbs")
    };
    let shift_warm = g(&conc_warm) - g(&base_warm);
    let shift_hot = g(&conc_hot) - g(&base_hot);
    // The ppt ratio is temperature-independent, so the shift scales with
    // R T alone.
    assert!((shift_hot - shift_warm * 350.0 / 298.15).abs() < 1e-9);
    // Sanity: the shift is nonzero and positive.
    assert!(shift_warm > 0.0);
}

#[test] // rq-f623ed28
fn an_absurd_magnitude_leaves_the_level_undefined_rather_than_infinite() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_line("am1", -50.0, &[("am", 1)], None),
            thermo_line("sa1", -40.0, &[("sa", 1)], None),
            thermo_line("cluster", -131.0, &[("am", 2), ("sa", 1)], None),
        ],
    );
    // A Pa pressure that underflows below the smallest positive double
    // when divided by p°, and a cmmc magnitude whose resolved pressure
    // underflows the same way.
    for value in ["1e-320", "1e-320cmmc"] {
        let run = cjkqc(&[&db, "--formation", "--conc", "sa", value]);
        assert_eq!(run.status.code(), Some(0), "{value}: {}", stderr(&run));
        assert_eq!(
            formation_row(&stdout(&run), "1sa")[4],
            "-",
            "{value}: {}",
            stdout(&run)
        );
        assert_eq!(
            formation_row(&stdout(&run), "2am1sa")[4],
            "-",
            "{value}: {}",
            stdout(&run)
        );
    }
}

// ---------------------------------------------------------------------------
// Population scenarios from `rqm/population.md` that are CLI-shaped.
// ---------------------------------------------------------------------------

/// k_B T in Eh at 298.15 K (`rqm/thermo.md` Constants).
fn pop_kbt() -> f64 {
    compiled_jkqc::K_B * 298.15 / compiled_jkqc::E_HARTREE
}

/// A conformer line with defined thermochemistry (hexagon geometry,
/// twelve 1000 cm^-1 modes), optional composition, and optionally a
/// stored correction.
fn conformer_line(
    name: &str,
    energy: Option<f64>,
    comps: &[(&str, u32)],
    correction_energy: Option<f64>,
) -> String {
    let mut coordinates = Vec::new();
    for atom in 0..6 {
        let angle = atom as f64 * std::f64::consts::PI / 3.0;
        coordinates.extend([angle.cos(), angle.sin(), 0.0]);
    }
    let mut frequencies = vec![0.0; 6];
    frequencies.resize(18, 1000.0);
    let mut record = Record {
        name: Some(name.to_string()),
        electronic_energy: energy,
        geometry: Some(compiled_jkqc::Geometry {
            symbols: vec!["C".to_string(); 6],
            coordinates,
        }),
        vibrational_frequencies: Some(frequencies),
        ..Record::default()
    };
    if !comps.is_empty() {
        record.components = Some(
            comps
                .iter()
                .map(|(code, count)| (code.to_string(), *count))
                .collect(),
        );
    }
    if let Some(energy) = correction_energy {
        record.correction = Some(Correction {
            energy,
            program: None,
            program_version: None,
            commands: None,
            kind: "sp".to_string(),
        });
    }
    serde_json::to_string(&record).expect("serialization")
}

/// A single-point line: an electronic energy, no geometry, no frequencies.
fn single_point_line(name: &str, energy: f64) -> String {
    serde_json::to_string(&Record {
        name: Some(name.to_string()),
        electronic_energy: Some(energy),
        ..Record::default()
    })
    .expect("serialization")
}

/// The population column of a printed table as parsed f64 values.
fn pop_column(text: &str, column: usize) -> Vec<Option<f64>> {
    let mut values = Vec::new();
    for line in text.lines().skip(1) {
        let cell = line.split_whitespace().nth(column).expect("cell");
        values.push(if cell == "-" {
            None
        } else {
            Some(cell.parse::<f64>().expect("population value"))
        });
    }
    values
}

#[test]
// rq-4d3860a0
fn bare_pop_prints_the_gibbs_based_column() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[conformer_line("one", Some(-50.0), &[("am", 1)], None)],
    );
    let run = cjkqc(&[&db, "--pop"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "pop\n1.0000");
}

#[test]
// rq-032b5e42
fn pop_el_prints_the_electronic_based_column() {
    let dir = temp_dir();
    let db = database_in(&dir, &[single_point_line("sp", -50.0)]);
    let run = cjkqc(&[&db, "--pop", "el"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "popel\n1.0000");
}

#[test]
// rq-4d3ba8f2
fn the_corrected_levels_print_their_own_columns() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[conformer_line(
            "one",
            Some(-50.0),
            &[("am", 1)],
            Some(-49.0),
        )],
    );
    let run = cjkqc(&[&db, "--pop", "elcorr", "--pop", "gcorr"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["popelcorr", "popgcorr"]);
    assert_eq!(lines[1], ["1.0000", "1.0000"]);
}

#[test]
// rq-9c420929
fn repeating_pop_prints_one_column_per_level_in_canonical_order() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[conformer_line("one", Some(-50.0), &[("am", 1)], None)],
    );
    let run = cjkqc(&[&db, "--pop", "el", "--pop"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["pop", "popel"]);
    assert_eq!(lines[1], ["1.0000", "1.0000"]);
}

#[test]
// rq-0c93ce7a
fn repeating_one_level_prints_its_column_once() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[conformer_line("one", Some(-50.0), &[("am", 1)], None)],
    );
    let run = cjkqc(&[&db, "--pop", "--pop", "g"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines = rows(&text);
    assert_eq!(lines[0], ["pop"]);
    assert_eq!(lines[1], ["1.0000"]);
}

#[test]
// rq-09f8b051
fn an_unknown_level_is_a_usage_error() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[conformer_line("one", Some(-50.0), &[("am", 1)], None)],
    );
    let run = cjkqc(&[&db, "--pop", "x"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test]
// rq-b6a6c98c
fn pop_alone_is_a_visible_outcome() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[conformer_line("one", Some(-50.0), &[("am", 1)], None)],
    );
    let run = cjkqc(&[&db, "--pop"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(
        cells(stdout(&run).lines().next().unwrap()),
        ["pop"],
        "{}",
        stdout(&run)
    );
    assert!(!dir.join("out.jsonl").exists());
}

#[test]
// rq-0c46871d
fn the_population_columns_sit_in_canonical_order() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[conformer_line("one", Some(-50.0), &[("am", 1)], None)],
    );
    let run = cjkqc(&[&db, "--rg", "--pop", "--pop", "el", "--el"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let header = cells(text.lines().next().unwrap());
    assert_eq!(header, ["el", "pop", "popel", "rg"]);
}

#[test]
// rq-2415978d
fn select_renormalizes_over_the_survivors() {
    let dir = temp_dir();
    let gap = pop_kbt();
    let db = database_in(
        &dir,
        &[
            conformer_line("low", Some(-50.0), &[("am", 1)], None),
            conformer_line("mid", Some(-50.0 + gap), &[("am", 1)], None),
            conformer_line("high", Some(-50.0 + 2.0 * gap), &[("am", 1)], None),
        ],
    );
    let run = cjkqc(&[&db, "--sort", "g", "--select", "2", "--pop"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let values = pop_column(&stdout(&run), 0);
    assert!((values[0].unwrap() - 0.7311).abs() < 5e-5, "{values:?}");
    assert!((values[1].unwrap() - 0.2689).abs() < 5e-5, "{values:?}");
    let text = stdout(&run);
    assert!(
        !text.contains("0.6652") && !text.contains("0.2447"),
        "{text}"
    );
}

#[test]
// rq-df30182b
fn select_1_leaves_the_survivor_at_the_full_population() {
    let dir = temp_dir();
    let gap = pop_kbt();
    let db = database_in(
        &dir,
        &[
            conformer_line("low", Some(-50.0), &[("am", 1)], None),
            conformer_line("mid", Some(-50.0 + gap), &[("am", 1)], None),
            conformer_line("high", Some(-50.0 + 2.0 * gap), &[("am", 1)], None),
        ],
    );
    let run = cjkqc(&[&db, "--sort", "g", "--select", "1", "--pop"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "pop\n1.0000");
}

#[test]
// rq-82fee485
fn glob_leaves_each_survivor_at_the_full_population() {
    let dir = temp_dir();
    let gap = pop_kbt();
    let db = database_in(
        &dir,
        &[
            conformer_line("am1", Some(-50.0), &[("am", 1)], None),
            conformer_line("am2", Some(-50.0 + gap), &[("am", 1)], None),
            conformer_line("sa1", Some(-60.0), &[("sa", 1)], None),
            conformer_line("sa2", Some(-60.0 + gap), &[("sa", 1)], None),
        ],
    );
    let run = cjkqc(&[&db, "--glob", "--pop"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    for value in pop_column(&stdout(&run), 0) {
        assert_eq!(value, Some(1.0), "{}", stdout(&run));
    }
}

#[test]
// rq-40e5e575
fn noname_pools_every_record_into_one_population() {
    let dir = temp_dir();
    let gap = pop_kbt();
    let db = database_in(
        &dir,
        &[
            conformer_line("am1", Some(-50.0), &[("am", 1)], None),
            conformer_line("am2", Some(-50.0 + gap), &[("am", 1)], None),
            conformer_line("sa1", Some(-60.0), &[("sa", 1)], None),
            conformer_line("sa2", Some(-60.0 + gap), &[("sa", 1)], None),
        ],
    );
    let run = cjkqc(&[&db, "--noname", "--pop"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let total: f64 = pop_column(&stdout(&run), 0)
        .into_iter()
        .map(|value| value.expect("defined in a global pool"))
        .sum();
    assert!((total - 1.0).abs() < 1e-9, "{total}");
}

#[test]
// rq-94d96047
fn records_without_components_form_their_own_group() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            conformer_line("first", Some(-50.0), &[], None),
            conformer_line("second", Some(-50.0 + pop_kbt() * 3.0_f64.ln()), &[], None),
            conformer_line("monomer", Some(-60.0), &[("am", 1)], None),
        ],
    );
    let run = cjkqc(&[&db, "--pop", "el"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let values = pop_column(&stdout(&run), 0);
    assert!((values[0].unwrap() - 0.75).abs() < 5e-5, "{values:?}");
    assert!((values[1].unwrap() - 0.25).abs() < 5e-5, "{values:?}");
    assert_eq!(values[2], Some(1.0));
}

#[test]
// rq-6f2e5003
fn sorting_never_changes_population_values() {
    let dir = temp_dir();
    let gap = pop_kbt();
    let db = database_in(
        &dir,
        &[
            conformer_line("low", Some(-50.0), &[("am", 1)], None),
            conformer_line("mid", Some(-50.0 + gap), &[("am", 1)], None),
            conformer_line("high", Some(-50.0 + 2.0 * gap), &[("am", 1)], None),
        ],
    );
    let ascending = cjkqc(&[&db, "--sort", "g", "--pop"]);
    let descending = cjkqc(&[&db, "--sort", "g", "--reverse", "--pop"]);
    assert_eq!(ascending.status.code(), Some(0), "{}", stderr(&ascending));
    assert_eq!(descending.status.code(), Some(0), "{}", stderr(&descending));
    let mut up = pop_column(&stdout(&ascending), 0);
    let mut down = pop_column(&stdout(&descending), 0);
    up.sort_by(|a, b| a.partial_cmp(b).unwrap());
    down.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(up, down);
}

#[test]
// rq-fd6f3491
fn a_print_only_run_sees_every_loaded_record() {
    let dir = temp_dir();
    let gap = pop_kbt();
    // The failed record keeps its full observables: it is excluded by
    // status, not by definedness — and its lower energy would dominate
    // the group's partition sum if the ok-gate regressed.
    let mut broken: Record =
        serde_json::from_str(&thermo_line("broken", -70.0, &[("am", 1)], None)).expect("template");
    broken.status = RecordStatus::Failed;
    broken.error = Some("boom".to_string());
    let db = database_in(
        &dir,
        &[
            conformer_line("low", Some(-50.0), &[("am", 1)], None),
            conformer_line("high", Some(-50.0 + gap), &[("am", 1)], None),
            serde_json::to_string(&broken).expect("serialization"),
        ],
    );
    let run = cjkqc(&[&db, "--pop"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let values = pop_column(&stdout(&run), 0);
    assert_eq!(values.len(), 3, "{}", stdout(&run));
    assert_eq!(values[2], None, "{}", stdout(&run));
    let total: f64 = values[0].unwrap() + values[1].unwrap();
    assert!((total - 1.0).abs() < 1e-9, "{total}");
}

#[test]
// rq-33c2faa3
fn a_saddle_point_pops_under_makereal() {
    let dir = temp_dir();
    let mut saddle = conformer_line("saddle", Some(-50.0), &[("am", 1)], None);
    let mut record: Record = serde_json::from_str(&saddle).expect("record");
    record.vibrational_frequencies = Some(vec![1000.0, -300.0]);
    saddle = serde_json::to_string(&record).expect("serialization");
    let minimum = conformer_line("minimum", Some(-50.5), &[("am", 1)], None);
    let db = database_in(&dir, &[saddle, minimum]);
    let without = cjkqc(&[&db, "--pop"]);
    assert_eq!(without.status.code(), Some(0), "{}", stderr(&without));
    let values = pop_column(&stdout(&without), 0);
    assert_eq!(values[0], None, "{}", stdout(&without));
    assert_eq!(values[1], Some(1.0), "{}", stdout(&without));
    let with = cjkqc(&[&db, "--pop", "--makereal"]);
    assert_eq!(with.status.code(), Some(0), "{}", stderr(&with));
    let values = pop_column(&stdout(&with), 0);
    assert!(
        values[0].is_some() && values[1].is_some(),
        "{}",
        stdout(&with)
    );
    let total = values[0].unwrap() + values[1].unwrap();
    assert!((total - 1.0).abs() < 1e-9, "{total}");
}

#[test]
// rq-b25a95ec
fn the_temperature_shifts_the_split() {
    let dir = temp_dir();
    let gap = pop_kbt() * 3.0_f64.ln();
    let db = database_in(
        &dir,
        &[
            conformer_line("low", Some(-50.0), &[("am", 1)], None),
            conformer_line("high", Some(-50.0 + gap), &[("am", 1)], None),
        ],
    );
    let room = cjkqc(&[&db, "--pop"]);
    assert_eq!(room.status.code(), Some(0), "{}", stderr(&room));
    let values = pop_column(&stdout(&room), 0);
    assert!((values[0].unwrap() - 0.75).abs() < 5e-5, "{values:?}");
    assert!((values[1].unwrap() - 0.25).abs() < 5e-5, "{values:?}");
    let hot = cjkqc(&[&db, "--pop", "--temp", "596.3"]);
    assert_eq!(hot.status.code(), Some(0), "{}", stderr(&hot));
    let values = pop_column(&stdout(&hot), 0);
    assert!((values[0].unwrap() - 0.6340).abs() < 5e-4, "{values:?}");
    assert!((values[1].unwrap() - 0.3660).abs() < 5e-4, "{values:?}");
}

#[test]
// rq-ebf60f52
fn a_very_high_temperature_flattens_the_populations() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            conformer_line("low", Some(-50.0), &[("am", 1)], None),
            conformer_line("high", Some(-50.0 + 1e-6), &[("am", 1)], None),
        ],
    );
    let run = cjkqc(&[&db, "--temp", "100000", "--pop"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    for value in pop_column(&stdout(&run), 0) {
        assert!((value.unwrap() - 0.5).abs() < 1e-5, "{}", stdout(&run));
    }
}

#[test]
// rq-9b20809b
fn a_member_far_above_the_minimum_underflows_to_zero() {
    let dir = temp_dir();
    let gap = pop_kbt();
    let db = database_in(
        &dir,
        &[
            conformer_line("low", Some(-50.0), &[("am", 1)], None),
            conformer_line("mid", Some(-50.0 + gap), &[("am", 1)], None),
            conformer_line("far", Some(-50.0 + 400.0 * gap), &[("am", 1)], None),
        ],
    );
    let run = cjkqc(&[&db, "--pop"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    assert!(text.contains("0.7311"), "{text}");
    assert!(text.contains("0.2689"), "{text}");
    assert!(text.contains("0.0000"), "{text}");
}

#[test]
// rq-8bae3ee9
fn unit_leaves_populations_alone() {
    let dir = temp_dir();
    let gap = pop_kbt();
    let db = database_in(
        &dir,
        &[
            conformer_line("low", Some(-50.0), &[("am", 1)], None),
            conformer_line("high", Some(-50.0 + gap), &[("am", 1)], None),
        ],
    );
    let plain = cjkqc(&[&db, "--pop"]);
    let kcal = cjkqc(&[&db, "--pop", "--unit"]);
    assert_eq!(plain.status.code(), Some(0), "{}", stderr(&plain));
    assert_eq!(kcal.status.code(), Some(0), "{}", stderr(&kcal));
    assert_eq!(stdout(&plain), stdout(&kcal));
}

#[test]
// rq-2c1cd4e2
fn conc_leaves_populations_alone() {
    let dir = temp_dir();
    let gap = pop_kbt();
    let db = database_in(
        &dir,
        &[
            conformer_line("low", Some(-50.0), &[("am", 1)], None),
            conformer_line("high", Some(-50.0 + gap), &[("am", 1)], None),
        ],
    );
    let plain = cjkqc(&[&db, "--pop"]);
    let shifted = cjkqc(&[&db, "--pop", "--conc", "am", "100ppt"]);
    assert_eq!(plain.status.code(), Some(0), "{}", stderr(&plain));
    assert_eq!(shifted.status.code(), Some(0), "{}", stderr(&shifted));
    assert_eq!(stdout(&plain), stdout(&shifted));
}

// ---------------------------------------------------------------------------
// Boltzmann-averaged formation sections (`rqm/formation.md`,
// Boltzmann-Averaged Sections).
// ---------------------------------------------------------------------------

/// A full-thermochemistry record line with the given spin multiplicity:
/// the electronic entropy term R*ln(m) shifts the Gibbs value by exactly
/// k_B*T*ln(m) against the multiplicity-1 record with the same energy
/// (rqm/thermo.md).
fn multiplicity_thermo_line(
    name: &str,
    energy: f64,
    comps: &[(&str, u32)],
    multiplicity: u32,
) -> String {
    let mut record: Record =
        serde_json::from_str(&thermo_line(name, energy, comps, None)).expect("template");
    record.multiplicity = Some(multiplicity);
    serde_json::to_string(&record).expect("serialization")
}

/// A single-point record line: full thermochemistry inputs minus the
/// frequencies, so no Gibbs level is defined.
fn frequencyless_line(name: &str, energy: f64, comps: &[(&str, u32)]) -> String {
    let mut record: Record =
        serde_json::from_str(&thermo_line(name, energy, comps, None)).expect("template");
    record.vibrational_frequencies = None;
    serde_json::to_string(&record).expect("serialization")
}

/// The tokens of the section's data rows, in print order — up to the
/// next section header, so a later section's rows never join in.
fn section_tokens(section: &str) -> Vec<&str> {
    section
        .lines()
        .skip(1)
        .take_while(|line| !line.starts_with("name "))
        .map(|line| line.split(' ').next().expect("token"))
        .collect()
}

#[test] // rq-85dab208
fn bavg_prints_one_row_per_composition() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_line("c1", -91.0, &[("am", 2), ("sa", 1)], None),
            thermo_line("c2", -90.9, &[("am", 2), ("sa", 1)], None),
            thermo_line("am1", -50.0, &[("am", 1)], None),
            thermo_line("sa1", -40.0, &[("sa", 1)], None),
        ],
    );
    let run = cjkqc(&[&db, "--formation", "--bavg"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(
        cells(stdout(&run).lines().next().unwrap()),
        ["name", "el", "h", "st", "g"],
        "{}",
        stdout(&run)
    );
    assert_eq!(
        section_tokens(formation_section(&stdout(&run))),
        ["2am1sa", "1am", "1sa"]
    );
}

#[test] // rq-c39939f4
fn ensemble_references_make_the_cluster_less_favorable_than_its_best_conformer() {
    let dir = temp_dir();
    // Two am monomers with equal electronic energies whose Gibbs values
    // differ by exactly kB*T*ln(3): the multiplicity-3 copy holds
    // R*ln(3) more electronic entropy, so it is the best conformer.
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_line("am_low", -50.0, &[("am", 1)], None),
            multiplicity_thermo_line("am_high", -50.0, &[("am", 1)], 3),
            thermo_line("cluster", -101.0, &[("am", 2)], None),
        ],
    );
    let plain = cjkqc(&[&db, "--formation", "--sort", "g"]);
    let bavg = cjkqc(&[&db, "--formation", "--sort", "g", "--bavg"]);
    assert_eq!(plain.status.code(), Some(0), "{}", stderr(&plain));
    assert_eq!(bavg.status.code(), Some(0), "{}", stderr(&bavg));
    let plain_row = formation_row(formation_section(&stdout(&plain)), "2am");
    let bavg_row = formation_row(formation_section(&stdout(&bavg)), "2am");
    // The cluster el formation values are equal: the monomer ensemble's
    // electronic mean of equal energies is that energy. The parsed cells
    // carry 10 decimals, so the comparisons tolerate the print rounding.
    let plain_el: f64 = plain_row[1].parse().expect("number");
    let bavg_el: f64 = bavg_row[1].parse().expect("number");
    assert!(
        (bavg_el - plain_el).abs() < 1e-9,
        "{plain_row:?} {bavg_row:?}"
    );
    // The averaged g formation value is 2*k_B*T*ln(4/3) above the
    // uncollapsed run's.
    let plain_g: f64 = plain_row[4].parse().expect("number");
    let bavg_g: f64 = bavg_row[4].parse().expect("number");
    let expected = 2.0 * kbt_eh(298.15) * (4.0f64 / 3.0).ln();
    assert!(
        (bavg_g - plain_g - expected).abs() < 1e-9,
        "{plain_row:?} {bavg_row:?}"
    );
    // The 1am row prints 0 at every level in both runs.
    for out in [&plain, &bavg] {
        let row = formation_row(formation_section(&stdout(out)), "1am");
        let values: Vec<f64> = row[1..]
            .iter()
            .map(|cell| cell.parse::<f64>().expect("number"))
            .collect();
        assert_eq!(values.len(), 4);
        assert!(values.iter().all(|value| value.abs() < 1e-9), "{row:?}");
    }
}

#[test] // rq-e1615410
fn a_single_point_only_group_prints_dashes_under_bavg() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            frequencyless_line("sp1", -91.0, &[("am", 2), ("sa", 1)]),
            frequencyless_line("sp2", -91.1, &[("am", 2), ("sa", 1)]),
            thermo_line("am1", -50.0, &[("am", 1)], None),
            thermo_line("sa1", -40.0, &[("sa", 1)], None),
        ],
    );
    let bavg = cjkqc(&[&db, "--formation", "--bavg"]);
    assert_eq!(bavg.status.code(), Some(0), "{}", stderr(&bavg));
    let row = formation_row(formation_section(&stdout(&bavg)), "2am1sa");
    assert_eq!(row[1], "-");
    assert_eq!(row[2], "-");
    assert_eq!(row[3], "-");
    assert_eq!(row[4], "-");
    // Without --bavg the same run prints the cluster's el formation
    // value (its thermal levels stay undefined without frequencies).
    let plain = cjkqc(&[&db, "--formation"]);
    let row = formation_row(formation_section(&stdout(&plain)), "2am1sa");
    assert_ne!(row[1], "-");
    assert_eq!(row[2], "-");
    assert_eq!(row[3], "-");
    assert_eq!(row[4], "-");
}

#[test] // rq-30bd57ee
fn glob_makes_the_collapse_the_identity() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_line("c1", -91.0, &[("am", 2), ("sa", 1)], None),
            thermo_line("c2", -90.9, &[("am", 2), ("sa", 1)], None),
            thermo_line("am1", -50.0, &[("am", 1)], None),
            thermo_line("sa1", -40.0, &[("sa", 1)], None),
        ],
    );
    let plain = cjkqc(&[&db, "--glob", "--formation"]);
    let bavg = cjkqc(&[&db, "--glob", "--formation", "--bavg"]);
    assert_eq!(plain.status.code(), Some(0), "{}", stderr(&plain));
    assert_eq!(bavg.status.code(), Some(0), "{}", stderr(&bavg));
    assert_eq!(stdout(&plain), stdout(&bavg));
}

#[test] // rq-f0980405
fn the_collapsed_row_carries_the_concentration_term() {
    let dir = temp_dir();
    // Two 2sa conformers whose Gibbs values differ by kB*T*ln(3): the
    // multiplicity-3 copy holds R*ln(3) more electronic entropy.
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_line("c1", -80.0, &[("sa", 2)], None),
            multiplicity_thermo_line("c2", -80.0, &[("sa", 2)], 3),
            thermo_line("sa1", -40.0, &[("sa", 1)], None),
        ],
    );
    let plain = cjkqc(&[&db, "--formation", "--bavg"]);
    let conc = cjkqc(&[&db, "--formation", "--bavg", "--conc", "sa", "100ppt"]);
    assert_eq!(plain.status.code(), Some(0), "{}", stderr(&plain));
    assert_eq!(conc.status.code(), Some(0), "{}", stderr(&conc));
    let plain_g: f64 = formation_row(formation_section(&stdout(&plain)), "2sa")[4]
        .parse()
        .expect("number");
    let conc_g: f64 = formation_row(formation_section(&stdout(&conc)), "2sa")[4]
        .parse()
        .expect("number");
    // Two molecules of sa: the conc run sits 2*R*T*ln(10^10) above. The
    // parsed cells carry 10 decimals, so the comparison tolerates the
    // print rounding.
    assert!(
        (conc_g - plain_g - 2.0 * ppt_shift(100.0)).abs() < 1e-9,
        "plain {plain_g} conc {conc_g}"
    );
    // The collapsed st moves the other way by the same amount, and the
    // collapsed h stays at the standard state.
    let plain_st: f64 = formation_row(formation_section(&stdout(&plain)), "2sa")[3]
        .parse()
        .expect("number");
    let conc_st: f64 = formation_row(formation_section(&stdout(&conc)), "2sa")[3]
        .parse()
        .expect("number");
    assert!(
        (conc_st - plain_st + 2.0 * ppt_shift(100.0)).abs() < 1e-9,
        "plain {plain_st} conc {conc_st}"
    );
    assert_eq!(
        formation_row(formation_section(&stdout(&plain)), "2sa")[2],
        formation_row(formation_section(&stdout(&conc)), "2sa")[2]
    );
}

#[test] // rq-a74b25aa
fn cnt_scales_the_collapsed_concentration_term_by_n_minus_one_over_n() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_line("c1", -80.0, &[("sa", 2)], None),
            multiplicity_thermo_line("c2", -80.0, &[("sa", 2)], 3),
            thermo_line("sa1", -40.0, &[("sa", 1)], None),
        ],
    );
    let conc = cjkqc(&[&db, "--formation", "--bavg", "--conc", "sa", "100ppt"]);
    let cnt = cjkqc(&[
        &db,
        "--formation",
        "--bavg",
        "--cnt",
        "--conc",
        "sa",
        "100ppt",
    ]);
    assert_eq!(conc.status.code(), Some(0), "{}", stderr(&conc));
    assert_eq!(cnt.status.code(), Some(0), "{}", stderr(&cnt));
    let conc_g: f64 = formation_row(formation_section(&stdout(&conc)), "2sa")[4]
        .parse()
        .expect("number");
    let cnt_g: f64 = formation_row(formation_section(&stdout(&cnt)), "2sa")[4]
        .parse()
        .expect("number");
    // The term 2*(N-1)/N = 1: the cnt run sits one R*T*ln(10^10) below
    // the conc run's.
    assert!(
        (cnt_g - conc_g + ppt_shift(100.0)).abs() < 1e-9,
        "conc {conc_g} cnt {cnt_g}"
    );
}

#[test] // rq-de842863
fn noname_does_not_pool_the_collapse() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_line("c1", -91.0, &[("am", 2)], None),
            thermo_line("s1", -40.0, &[("sa", 1)], None),
            thermo_line("am1", -50.0, &[("am", 1)], None),
            thermo_line("sa1", -40.5, &[("sa", 1)], None),
        ],
    );
    let run = cjkqc(&[&db, "--noname", "--formation", "--bavg"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(
        section_tokens(formation_section(&stdout(&run))),
        ["2am", "1sa", "1am"]
    );
}

#[test] // rq-94cec13a
fn bavg_leaves_the_main_table_per_record() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_line("c1", -91.0, &[("am", 2), ("sa", 1)], None),
            thermo_line("c2", -90.9, &[("am", 2), ("sa", 1)], None),
            thermo_line("c3", -90.8, &[("am", 2), ("sa", 1)], None),
            thermo_line("am1", -50.0, &[("am", 1)], None),
            thermo_line("sa1", -40.0, &[("sa", 1)], None),
        ],
    );
    let run = cjkqc(&[&db, "--name", "--g", "--formation", "--bavg"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let table: Vec<&str> = text.lines().take(4).collect();
    // The main table's columns follow the canonical column order, so
    // --name --g prints `name g`.
    assert_eq!(cells(table[0]), ["name", "g"], "{text}");
    assert_eq!(table.len(), 4, "the main table prints three rows: {text}");
    assert_eq!(
        section_tokens(formation_section(&text)),
        ["2am1sa", "1am", "1sa"],
        "the formation section prints one row per composition"
    );
}

#[test] // rq-9f5be6e0
fn bavg_without_a_formation_section_is_an_accepted_no_op() {
    let dir = temp_dir();
    let db = db_with(&dir, "db.jsonl", &[sample_line("A1", -1.0)]);
    let plain = cjkqc(&[&db, "--filename", "--el"]);
    let bavg = cjkqc(&[&db, "--filename", "--el", "--bavg"]);
    assert_eq!(plain.status.code(), Some(0), "{}", stderr(&plain));
    assert_eq!(bavg.status.code(), Some(0), "{}", stderr(&bavg));
    assert_eq!(stdout(&plain), stdout(&bavg));
}

#[test] // rq-b4bcf592
fn bavg_alone_is_not_a_visible_outcome() {
    let dir = temp_dir();
    let db = db_with(&dir, "db.jsonl", &[sample_line("A1", -1.0)]);
    let run = cjkqc(&[&db, "--bavg"]);
    assert_eq!(run.status.code(), Some(2));
}

#[test] // rq-603dd405
fn bavgout_is_a_usage_error() {
    let dir = temp_dir();
    let db = db_with(&dir, "db.jsonl", &[sample_line("A1", -1.0)]);
    let run = cjkqc(&[&db, "--formation", "--bavgout"]);
    assert_eq!(run.status.code(), Some(2));
}

#[test] // rq-c959877c
fn both_sections_collapse_when_both_are_printed() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_line("c1", -91.0, &[("am", 2), ("sa", 1)], Some(-5.0)),
            thermo_line("c2", -90.9, &[("am", 2), ("sa", 1)], Some(-5.0)),
            thermo_line("am1", -50.0, &[("am", 1)], Some(-5.0)),
            thermo_line("sa1", -40.0, &[("sa", 1)], Some(-5.0)),
        ],
    );
    let run = cjkqc(&[&db, "--formation", "--formation-corr", "--bavg"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    // Each section's header is the section's own.
    assert_eq!(
        cells(formation_section(&text).lines().next().unwrap()),
        ["name", "el", "h", "st", "g"]
    );
    assert_eq!(
        cells(corrected_section(&text).lines().next().unwrap()),
        ["name", "elcorr", "hcorr", "stcorr", "gcorr"]
    );
    assert_eq!(
        section_tokens(formation_section(&text)),
        ["2am1sa", "1am", "1sa"]
    );
    assert_eq!(
        section_tokens(corrected_section(&text)),
        ["2am1sa", "1am", "1sa"]
    );
}

// ---------------------------------------------------------------------------
// Milestone 8: hydration and solvation distribution scenarios from
// `rqm/hydration.md` that are CLI-shaped.
// ---------------------------------------------------------------------------

use compiled_jkqc::hydration::saturation_pressure_water;
use compiled_jkqc::P_STANDARD;

/// k_B T in Eh at the default conditions.
fn hydration_kbt() -> f64 {
    K_B * 298.15 / E_HARTREE
}

/// The vapor term per water at relative humidity `rh`, in Eh: the
/// concentration correction the implied `--conc w p` spec adds per water
/// (rqm/hydration.md Solvent Pressure).
fn hydration_vapor_term(rh: f64) -> f64 {
    K_B * 298.15 * (P_STANDARD / (rh * saturation_pressure_water(298.15))).ln() / E_HARTREE
}

/// A full-thermochemistry record line whose raw and corrected Gibbs free
/// energies hit the given targets at the default conditions; `corrected`
/// attaches a stored correction (the corrected gibbs is linear in the
/// correction energy, offset by the energy-free template gibbs).
fn hydration_line(name: &str, comps: &[(&str, u32)], raw: f64, corrected: Option<f64>) -> String {
    let mut record = gibbs_record(name, raw, comps);
    if let Some(target) = corrected {
        let template: Record = serde_json::from_str(&thermo_record_json(
            name,
            Some(0.0),
            comps,
            Some(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
        ))
        .expect("template deserializes");
        let gibbs0 = thermochemistry(&template, &ThermoConditions::default())
            .expect("defined")
            .gibbs;
        record.correction = Some(Correction {
            energy: target - gibbs0,
            program: None,
            program_version: None,
            commands: None,
            kind: "sp".to_string(),
        });
    }
    serde_json::to_string(&record).expect("serialization")
}

/// The Gibbs target for a hydrated state that should carry the
/// distribution value `value` at relative humidity `rh`, given the
/// monomers' gibbs targets: the standard-state formation value is
/// `value` minus the vapor term of its waters.
fn hydration_target(refs: &[f64], value: f64, waters: u32, rh: f64) -> f64 {
    refs.iter().sum::<f64>() + value - f64::from(waters) * hydration_vapor_term(rh)
}

/// The section blocks of stdout: the blank-line-separated groups after
/// the (optional) table and formation sections.
fn hydration_blocks(stdout_text: &str) -> Vec<String> {
    stdout_text
        .trim_end_matches('\n')
        .split("\n\n")
        .map(str::to_string)
        .collect()
}

#[test] // rq-09af191f
fn bare_hydration_distributes_over_the_hydration_states_of_water() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            hydration_line("c1", &[("sa", 1)], -60.0, None),
            hydration_line(
                "c2",
                &[("sa", 1), ("w", 1)],
                hydration_target(&[-60.0, -76.0], hydration_kbt() * 3.0f64.ln(), 1, 1.0),
                None,
            ),
            hydration_line("sa1", &[("sa", 1)], -60.0, None),
            hydration_line("w1", &[("w", 1)], -76.0, None),
        ],
    );
    let run = cjkqc(&[&db, "--hydration"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    // One group: the states 1sa (75.0) and 1sa1w (25.0), one k_B T ln(3)
    // apart at the saturated vapor.
    assert_eq!(stdout(&run), " 1sa 1sa1w\n75.0  25.0\n\n");
}

#[test] // rq-f0751abd
fn solvation_picks_the_solvent_code() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            hydration_line("c1", &[("am", 1)], -50.0, None),
            hydration_line(
                "c2",
                &[("am", 1), ("sa", 1)],
                -50.0 + -40.0 + hydration_kbt(),
                None,
            ),
            hydration_line("am1", &[("am", 1)], -50.0, None),
            hydration_line("sa1", &[("sa", 1)], -40.0, None),
        ],
    );
    let run = cjkqc(&[&db, "--solvation", "sa", "--psolvent", "101325"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    // At p° the vapor term is zero, so the one-k_B T gap splits 73.1/26.9.
    let text = stdout(&run);
    let cells: Vec<&str> = text
        .lines()
        .nth(1)
        .expect("two rows")
        .split(' ')
        .filter(|cell| !cell.is_empty())
        .collect();
    assert_eq!(cells, ["73.1", "26.9"], "{text}");
    let tokens: Vec<&str> = text
        .lines()
        .next()
        .expect("two rows")
        .split(' ')
        .filter(|cell| !cell.is_empty())
        .collect();
    assert_eq!(tokens, ["1am", "1am1sa"]);
}

#[test]
fn solvation_corr_prints_only_the_corrected_distribution() {
    // The --solvation-corr spelling routes the corrected level at the
    // CLI, the raw/corrected convention of the Flag Surface over the
    // --solvation solvent code.
    let dir = temp_dir();
    let refs = -50.0 + -40.0;
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            hydration_line("c1", &[("am", 1)], -50.0, Some(-50.0)),
            hydration_line(
                "c2",
                &[("am", 1), ("sa", 1)],
                refs + hydration_kbt(),
                Some(refs - 0.01),
            ),
            hydration_line("am1", &[("am", 1)], -50.0, Some(-50.0)),
            hydration_line("sa1", &[("sa", 1)], -40.0, Some(-40.0)),
        ],
    );
    let run = cjkqc(&[&db, "--solvation-corr", "sa", "--psolvent", "101325"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    // At p° the vapor term is zero. The corrected level favors 1am1sa;
    // the raw level (73.1 for 1am) prints nowhere.
    let text = stdout(&run);
    assert_eq!(text, "1am 1am1sa\n0.0  100.0\n\n", "{text:?}");
}

#[test] // rq-524dd0dc
fn hydration_corr_prints_only_the_corrected_distribution() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            hydration_line("c1", &[("sa", 1)], -60.0, Some(-60.0)),
            hydration_line(
                "c2",
                &[("sa", 1), ("w", 1)],
                hydration_target(&[-60.0, -76.0], 0.2, 1, 1.0),
                Some(hydration_target(&[-60.0, -76.0], -0.01, 1, 1.0)),
            ),
            hydration_line("sa1", &[("sa", 1)], -60.0, Some(-60.0)),
            hydration_line("w1", &[("w", 1)], -76.0, Some(-76.0)),
        ],
    );
    let run = cjkqc(&[&db, "--hydration-corr"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    // The corrected level favors 1sa1w; the raw level (100.0 for 1sa)
    // prints nowhere.
    let text = stdout(&run);
    assert_eq!(text, "1sa 1sa1w\n0.0 100.0\n\n", "{text:?}");
}

#[test] // rq-c3e34342
fn the_raw_and_corrected_sections_print_together_raw_first() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            hydration_line("c1", &[("sa", 1)], -60.0, Some(-60.0)),
            hydration_line(
                "c2",
                &[("sa", 1), ("w", 1)],
                hydration_target(&[-60.0, -76.0], 0.2, 1, 1.0),
                Some(hydration_target(&[-60.0, -76.0], -0.01, 1, 1.0)),
            ),
            hydration_line("sa1", &[("sa", 1)], -60.0, Some(-60.0)),
            hydration_line("w1", &[("w", 1)], -76.0, Some(-76.0)),
        ],
    );
    let run = cjkqc(&[&db, "--hydration-corr", "--hydration"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let raw_position = text.find("100.0").expect("the raw section favors 1sa");
    let corrected_position = text
        .find("\n0.0 100.0")
        .expect("the corrected section favors 1sa1w");
    assert!(
        raw_position < corrected_position,
        "the raw section prints before the corrected one: {text:?}"
    );
    assert_eq!(
        text.matches("100.0").count(),
        2,
        "each section prints exactly once: {text:?}"
    );
    // Repeated spellings of the same code are redundant, not conflicting.
    let redundant = cjkqc(&[&db, "--hydration", "--solvation", "w"]);
    assert_eq!(redundant.status.code(), Some(0), "{}", stderr(&redundant));
    assert_eq!(stdout(&redundant), "  1sa 1sa1w\n100.0   0.0\n\n");
}

#[test] // rq-534cfecc
fn hydration_and_solvation_of_another_solvent_are_a_usage_error() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            hydration_line("sa1", &[("sa", 1)], -60.0, None),
            hydration_line("w1", &[("w", 1)], -76.0, None),
        ],
    );
    let run = cjkqc(&[&db, "--hydration", "--solvation", "sa"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test] // rq-fa54bb5b
fn a_non_letter_solvent_code_is_a_usage_error() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            hydration_line("sa1", &[("sa", 1)], -60.0, None),
            hydration_line("w1", &[("w", 1)], -76.0, None),
        ],
    );
    let run = cjkqc(&[&db, "--solvation", "2sa"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test] // rq-2dbfbb9a
fn hydration_alone_is_a_visible_outcome() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            hydration_line("sa1", &[("sa", 1)], -60.0, None),
            hydration_line("w1", &[("w", 1)], -76.0, None),
        ],
    );
    let run = cjkqc(&[&db, "--hydration"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(stdout(&run).contains("1sa"), "{}", stdout(&run));
    let entries: Vec<_> = fs::read_dir(&dir)
        .expect("read dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(entries, ["db.jsonl"], "{entries:?}");
}

#[test] // rq-c0dea4f9
fn rh_without_a_hydration_section_is_an_accepted_no_op() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[hydration_line("c1", &[("sa", 1)], -60.0, None)],
    );
    let plain = cjkqc(&[&db, "--filename", "--el"]);
    let with_rh = cjkqc(&[&db, "--filename", "--el", "--rh", "0.5"]);
    assert_eq!(with_rh.status.code(), Some(0), "{}", stderr(&with_rh));
    assert_eq!(stdout(&plain), stdout(&with_rh));
}

#[test] // rq-93c3be03
fn psolvent_without_a_hydration_section_is_an_accepted_no_op() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[hydration_line("c1", &[("sa", 1)], -60.0, None)],
    );
    let plain = cjkqc(&[&db, "--filename", "--el"]);
    let with_p = cjkqc(&[&db, "--filename", "--el", "--psolvent", "100ppt"]);
    assert_eq!(with_p.status.code(), Some(0), "{}", stderr(&with_p));
    assert_eq!(stdout(&plain), stdout(&with_p));
}

#[test] // rq-eb09ca63
fn without_rh_and_psolvent_the_vapor_is_saturated() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            hydration_line(
                "c1",
                &[("sa", 1), ("w", 1)],
                hydration_target(&[-60.0, -76.0], 0.01, 1, 1.0),
                None,
            ),
            hydration_line("sa1", &[("sa", 1)], -60.0, None),
            hydration_line("w1", &[("w", 1)], -76.0, None),
        ],
    );
    let bare = cjkqc(&[&db, "--hydration"]);
    let explicit = cjkqc(&[&db, "--hydration", "--rh", "1"]);
    assert_eq!(bare.status.code(), Some(0), "{}", stderr(&bare));
    assert_eq!(explicit.status.code(), Some(0), "{}", stderr(&explicit));
    assert_eq!(stdout(&bare), stdout(&explicit));
}

#[test] // rq-56966bc2
fn rh_scales_the_saturation_vapor_pressure() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            hydration_line(
                "c1",
                &[("sa", 1), ("w", 1)],
                hydration_target(&[-60.0, -76.0], 0.01, 1, 1.0),
                None,
            ),
            hydration_line("sa1", &[("sa", 1)], -60.0, None),
            hydration_line("w1", &[("w", 1)], -76.0, None),
        ],
    );
    let plain = cjkqc(&[&db, "--formation"]);
    let run = cjkqc(&[&db, "--formation", "--hydration", "--rh", "0.5"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let plain_row = formation_row(&stdout(&plain), "1sa1w");
    let hydrate = formation_row(&stdout(&run), "1sa1w");
    let shift = hydrate[4].parse::<f64>().expect("the g cell")
        - plain_row[4].parse::<f64>().expect("the g cell");
    let expected = hydration_vapor_term(0.5);
    assert!(
        (shift - expected).abs() < 1.0e-9,
        "g = g_form + R T ln(p°/(0.5 p_sat)): {shift} vs {expected}"
    );
}

#[test] // rq-b457ce50
fn psolvent_overrides_rh() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            hydration_line(
                "c1",
                &[("sa", 1), ("w", 1)],
                hydration_target(&[-60.0, -76.0], 0.01, 1, 1.0),
                None,
            ),
            hydration_line("sa1", &[("sa", 1)], -60.0, None),
            hydration_line("w1", &[("w", 1)], -76.0, None),
        ],
    );
    let both = cjkqc(&[&db, "--hydration", "--rh", "0.5", "--psolvent", "101325"]);
    let only_p = cjkqc(&[&db, "--hydration", "--psolvent", "101325"]);
    assert_eq!(both.status.code(), Some(0), "{}", stderr(&both));
    assert_eq!(only_p.status.code(), Some(0), "{}", stderr(&only_p));
    assert_eq!(stdout(&both), stdout(&only_p));
}

#[test] // rq-83eaf0d1
fn psolvent_reuses_the_concentration_grammar() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            hydration_line(
                "c1",
                &[("sa", 1), ("w", 1)],
                hydration_target(&[-60.0, -76.0], 0.01, 1, 1.0),
                None,
            ),
            hydration_line("sa1", &[("sa", 1)], -60.0, None),
            hydration_line("w1", &[("w", 1)], -76.0, None),
        ],
    );
    let suffixed = cjkqc(&[&db, "--hydration", "--psolvent", "100ppt"]);
    let plain = cjkqc(&[&db, "--hydration", "--psolvent", "1.01325e-5"]);
    assert_eq!(suffixed.status.code(), Some(0), "{}", stderr(&suffixed));
    assert_eq!(plain.status.code(), Some(0), "{}", stderr(&plain));
    assert_eq!(stdout(&suffixed), stdout(&plain));
}

#[test] // rq-5a7bc7d5
fn a_psolvent_value_outside_the_grammar_is_a_usage_error() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            hydration_line("sa1", &[("sa", 1)], -60.0, None),
            hydration_line("w1", &[("w", 1)], -76.0, None),
        ],
    );
    let run = cjkqc(&[&db, "--hydration", "--psolvent", "10^3ppt"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test] // rq-b9f78565
fn a_non_positive_rh_is_a_usage_error() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            hydration_line("sa1", &[("sa", 1)], -60.0, None),
            hydration_line("w1", &[("w", 1)], -76.0, None),
        ],
    );
    let zero = cjkqc(&[&db, "--hydration", "--rh", "0"]);
    assert_eq!(zero.status.code(), Some(2), "{}", stderr(&zero));
    let negative = cjkqc(&[&db, "--hydration", "--rh", "-0.5"]);
    assert_eq!(negative.status.code(), Some(2), "{}", stderr(&negative));
}

#[test] // rq-49ef8ef3
fn supersaturation_above_one_is_accepted() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            hydration_line(
                "c1",
                &[("sa", 1), ("w", 1)],
                hydration_target(&[-60.0, -76.0], 0.01, 1, 1.0),
                None,
            ),
            hydration_line("sa1", &[("sa", 1)], -60.0, None),
            hydration_line("w1", &[("w", 1)], -76.0, None),
        ],
    );
    let run = cjkqc(&[&db, "--hydration", "--rh", "1.5"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
}

#[test] // rq-ee3e38de
fn a_temperature_outside_the_correlation_s_domain_is_a_usage_error() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            hydration_line("sa1", &[("sa", 1)], -60.0, None),
            hydration_line("w1", &[("w", 1)], -76.0, None),
        ],
    );
    let run = cjkqc(&[&db, "--temp", "30", "--hydration"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test] // rq-9ee7ff9c
fn solvation_of_a_non_water_solvent_without_psolvent_is_a_usage_error() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            hydration_line("c1", &[("sa", 1), ("dma", 1)], -100.0, None),
            hydration_line("sa1", &[("sa", 1)], -60.0, None),
            hydration_line("dma1", &[("dma", 1)], -66.0, None),
        ],
    );
    let run = cjkqc(&[&db, "--solvation", "dma"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test] // rq-2b028a0d
fn rh_with_a_non_water_solvent_is_a_usage_error() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            hydration_line("c1", &[("sa", 1), ("dma", 1)], -100.0, None),
            hydration_line("sa1", &[("sa", 1)], -60.0, None),
            hydration_line("dma1", &[("dma", 1)], -66.0, None),
        ],
    );
    let run = cjkqc(&[
        &db,
        "--solvation",
        "dma",
        "--psolvent",
        "100ppt",
        "--rh",
        "0.5",
    ]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test] // rq-b994d13a
fn conc_naming_the_solvent_code_is_a_usage_error() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            hydration_line("sa1", &[("sa", 1)], -60.0, None),
            hydration_line("w1", &[("w", 1)], -76.0, None),
        ],
    );
    let run = cjkqc(&[&db, "--hydration", "--conc", "w", "100ppt"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test] // rq-acc842e9
fn the_implied_spec_shifts_the_formation_section_s_solvent_term() {
    let dir = temp_dir();
    let plain_target = hydration_target(&[-60.0, -76.0], 0.01, 1, 1.0);
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            hydration_line("c1", &[("sa", 1), ("w", 1)], plain_target, None),
            hydration_line("sa1", &[("sa", 1)], -60.0, None),
            hydration_line("w1", &[("w", 1)], -76.0, None),
        ],
    );
    let plain = cjkqc(&[&db, "--formation"]);
    let hydrated = cjkqc(&[&db, "--formation", "--hydration", "--rh", "0.5"]);
    assert_eq!(plain.status.code(), Some(0), "{}", stderr(&plain));
    assert_eq!(hydrated.status.code(), Some(0), "{}", stderr(&hydrated));
    // The 1sa rows are equal; the 1sa1w g shifts by the vapor term.
    let plain_row = formation_row(&stdout(&plain), "1sa1w");
    let hydrated_row = formation_row(&stdout(&hydrated), "1sa1w");
    let plain_sa = formation_row(&stdout(&plain), "1sa");
    let hydrated_sa = formation_row(&stdout(&hydrated), "1sa");
    assert_eq!(plain_sa[4], hydrated_sa[4], "the 1sa rows are equal");
    let shift = hydrated_row[4].parse::<f64>().expect("the g cell")
        - plain_row[4].parse::<f64>().expect("the g cell");
    let expected = hydration_vapor_term(0.5);
    assert!(
        (shift - expected).abs() < 1.0e-9,
        "about 2.46 kcal/mol: {shift} vs {expected}"
    );
}

#[test] // rq-68e223b9
fn a_solvent_absent_from_the_final_record_set_is_a_usage_error() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            hydration_line("c1", &[("am", 1), ("sa", 1)], -100.0, None),
            hydration_line("am1", &[("am", 1)], -50.0, None),
            hydration_line("sa1", &[("sa", 1)], -60.0, None),
        ],
    );
    let run = cjkqc(&[&db, "--hydration"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test]
fn a_combined_formation_hydration_run_names_the_solvent_not_the_implied_spec() {
    // The implied `--conc w p` spec rides inside `concs`, but the
    // solvent-absent error must name the solvent flag the user gave,
    // not `--conc` (`rqm/hydration.md` Flag Surface, Solvent
    // Pressure).
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            thermo_line("am1", -50.0, &[("am", 1)], None),
            thermo_line("sa1", -40.0, &[("sa", 1)], None),
            thermo_line("cluster", -91.0, &[("am", 1), ("sa", 1)], None),
        ],
    );
    let run = cjkqc(&[&db, "--formation", "--hydration"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
    let text = stderr(&run);
    assert!(
        text.contains("the solvent code `w` appears in no record's composition"),
        "{text}"
    );
    assert!(!text.contains("--conc"), "{text}");
}

#[test] // rq-6375f775
fn a_filtered_away_solvent_monomer_dashes_the_hydrated_states() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            hydration_line("c1", &[("sa", 1), ("w", 1)], -140.0, None),
            hydration_line("sa1", &[("sa", 1)], -60.0, None),
            hydration_line("w1", &[("w", 1)], -76.0, None),
        ],
    );
    let extracted = cjkqc(&[&db, "--extract", "1sa1w", "--hydration"]);
    assert_eq!(extracted.status.code(), Some(0), "{}", stderr(&extracted));
    let text = stdout(&extracted);
    let cells: Vec<&str> = text
        .lines()
        .nth(1)
        .expect("two rows")
        .split(' ')
        .filter(|cell| !cell.is_empty())
        .collect();
    assert_eq!(cells, ["-"], "the 1sa1w state prints -: {text}");
    let unextracted = cjkqc(&[&db, "--hydration"]);
    assert_eq!(
        unextracted.status.code(),
        Some(0),
        "{}",
        stderr(&unextracted)
    );
    let defined = stdout(&unextracted)
        .lines()
        .nth(1)
        .expect("two rows")
        .contains('.');
    assert!(
        defined,
        "a defined percentage prints: {}",
        stdout(&unextracted)
    );
}

#[test] // rq-da98d00b
fn noname_does_not_pool_the_core_groups() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            hydration_line("c1", &[("sa", 1)], -60.0, None),
            hydration_line(
                "c2",
                &[("sa", 1), ("w", 1)],
                hydration_target(&[-60.0, -76.0], 0.01, 1, 1.0),
                None,
            ),
            hydration_line("c3", &[("am", 2)], -100.0, None),
            hydration_line(
                "c4",
                &[("am", 2), ("w", 1)],
                hydration_target(&[-50.0, -50.0, -76.0], 0.01, 1, 1.0),
                None,
            ),
            hydration_line("am1", &[("am", 1)], -50.0, None),
            hydration_line("sa1", &[("sa", 1)], -60.0, None),
            hydration_line("w1", &[("w", 1)], -76.0, None),
        ],
    );
    let run = cjkqc(&[&db, "--noname", "--hydration"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let blocks = hydration_blocks(&stdout(&run));
    assert_eq!(
        blocks.len(),
        3,
        "one group per core composition, not one global group: {:?}",
        stdout(&run)
    );
    assert!(blocks[0].contains("1sa") && blocks[0].contains("1sa1w"));
    assert!(blocks[1].contains("2am") && blocks[1].contains("2am1w"));
}

#[test] // rq-a396d259
fn the_section_prints_after_the_formation_sections_and_before_the_summary() {
    let dir = temp_dir();
    let out = dir.join("kept.jsonl");
    let out_text = out.to_string_lossy().into_owned();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            hydration_line("c1", &[("sa", 1)], -60.0, Some(-60.0)),
            hydration_line(
                "c2",
                &[("sa", 1), ("w", 1)],
                hydration_target(&[-60.0, -76.0], 0.01, 1, 1.0),
                Some(hydration_target(&[-60.0, -76.0], 0.01, 1, 1.0)),
            ),
            hydration_line("sa1", &[("sa", 1)], -60.0, Some(-60.0)),
            hydration_line("w1", &[("w", 1)], -76.0, Some(-76.0)),
        ],
    );
    let run = cjkqc(&[
        &db,
        "--sort",
        "el",
        "--formation",
        "--formation-corr",
        "--hydration",
        "--hydration-corr",
        "--out",
        &out_text,
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let formation = text.len() - section_from(&text, &["name", "el", "h", "st", "g"]).len();
    let corrected =
        text.len() - section_from(&text, &["name", "elcorr", "hcorr", "stcorr", "gcorr"]).len();
    // The hydration sections' percentage rows follow the dry-first state
    // ordering; each request prints its section once, raw before
    // corrected.
    let hydration = text
        .find("\n100.0   0.0")
        .expect("the raw hydration section");
    let hydration_corr = text
        .rfind("\n100.0   0.0")
        .expect("the corrected hydration section");
    let summary = text.find("filtered 4 records into").expect("the summary");
    assert!(formation < corrected, "{text}");
    assert!(corrected < hydration, "{text}");
    assert!(hydration < hydration_corr, "{text}");
    assert!(hydration_corr < summary, "{text}");
    assert!(
        text.contains(&out_text),
        "the summary names the database: {text}"
    );
}

#[test]
fn hydration_sections_collapse_under_bavg() {
    // The --bavg ensemble path at the CLI level, supporting
    // rq-1c4d88c8's averaged run: the 1sa1w ensemble of two conformers
    // one k_B T ln(3) apart carries k_B T ln(4/3) of conformational
    // entropy, so its distribution value moves from k_B T ln(3) down to
    // k_B T ln(9/4) and its percentage exceeds the uncollapsed run's
    // 25.0.
    let dir = temp_dir();
    let kbt = hydration_kbt();
    let conformer = hydration_target(&[-60.0, -76.0], kbt * 3.0f64.ln(), 1, 1.0);
    let db = db_with(
        &dir,
        "db.jsonl",
        &[
            hydration_line("sa1", &[("sa", 1)], -60.0, None),
            hydration_line("c1", &[("sa", 1), ("w", 1)], conformer, None),
            hydration_line(
                "c2",
                &[("sa", 1), ("w", 1)],
                conformer + kbt * 3.0f64.ln(),
                None,
            ),
            hydration_line("w1", &[("w", 1)], -76.0, None),
        ],
    );
    let plain = cjkqc(&[&db, "--hydration"]);
    assert_eq!(plain.status.code(), Some(0), "{}", stderr(&plain));
    // The uncollapsed run takes the lowest conformer: one k_B T ln(3)
    // gap, 75.0/25.0.
    assert_eq!(stdout(&plain), " 1sa 1sa1w\n75.0  25.0\n\n");
    let averaged = cjkqc(&[&db, "--hydration", "--bavg"]);
    assert_eq!(averaged.status.code(), Some(0), "{}", stderr(&averaged));
    // The ensemble's free energy sits k_B T ln(4/3) below the lowest
    // conformer, moving the gap to k_B T ln(9/4): the 1sa1w percentage
    // 100·(4/9)/(13/9) exceeds 25.0.
    let weight = 4.0f64 / 9.0;
    let hydrated = 100.0 * weight / (1.0 + weight);
    let expected = format!(" 1sa 1sa1w\n{:.1}  {:.1}\n\n", 100.0 - hydrated, hydrated);
    assert_eq!(stdout(&averaged), expected);
}

// --- extrapolation corrections (rqm/extrapolation.md) ---

/// An extrapolation source line: a named ok candidate with an electronic
/// energy, an optional SCF energy, and optional provenance.
fn source_line(
    name: &str,
    ee: f64,
    scf: Option<f64>,
    program: Option<&str>,
    version: Option<&str>,
    command: Option<&str>,
) -> String {
    let record = Record {
        status: RecordStatus::Ok,
        name: Some(name.to_string()),
        electronic_energy: Some(ee),
        scf_energy: scf,
        program: program.map(str::to_string),
        program_version: version.map(str::to_string),
        command: command.map(str::to_string),
        ..Record::default()
    };
    serde_json::to_string(&record).expect("serialization")
}

/// A source candidate with an electronic energy only.
fn blend_source(name: &str, ee: f64) -> String {
    source_line(name, ee, None, None, None, None)
}

// rq-fcd67083
#[test]
fn extrapolating_two_point_attaches_a_cbs_correction() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let low = db_with(
        &dir,
        "low.jsonl",
        &[source_line(
            "1am",
            -76.0,
            Some(-76.0),
            None,
            None,
            Some("! def2-SVP Sp"),
        )],
    );
    let high = db_with(
        &dir,
        "high.jsonl",
        &[source_line(
            "1am",
            -76.15,
            Some(-76.1),
            Some("orca"),
            Some("6.0.0"),
            Some("! def2-TZVPP Sp"),
        )],
    );
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2p",
        &low,
        &high,
        "2",
        "4.3",
        "2.51",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let loaded = loaded(&out);
    assert_eq!(loaded.len(), 1);
    let correction = loaded[0].correction.as_ref().expect("attached");
    assert!(
        (correction.energy - -76.21251721253894).abs() < 1e-9,
        "{}",
        correction.energy
    );
    assert_eq!(correction.kind, "cbs");
    assert_eq!(
        correction.commands.as_deref(),
        Some(&["! def2-SVP Sp".to_string(), "! def2-TZVPP Sp".to_string()][..])
    );
    assert_eq!(correction.program.as_deref(), Some("orca"));
    assert_eq!(correction.program_version.as_deref(), Some("6.0.0"));
    assert_eq!(loaded[0].electronic_energy, Some(-38.51));
}

// rq-8f40cd00
#[test]
fn extrapolating_two_s_attaches_a_cps_correction() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let low = db_with(&dir, "low.jsonl", &[blend_source("1am", -76.0)]);
    let high = db_with(&dir, "high.jsonl", &[blend_source("1am", -76.1)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2s",
        &low,
        &high,
        "1.5",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let correction = loaded(&out)[0].correction.clone().expect("attached");
    assert_eq!(correction.kind, "cps");
    assert!(
        (correction.energy - -76.15).abs() < 1e-12,
        "{}",
        correction.energy
    );
}

// rq-0f8f7fcd
#[test]
fn extrapolating_combined_form_attaches_a_cbs_cps_correction() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let lbl = db_with(
        &dir,
        "lbl.jsonl",
        &[source_line(
            "1am",
            -76.0,
            Some(-76.0),
            None,
            None,
            Some("c1"),
        )],
    );
    let hbl = db_with(
        &dir,
        "hbl.jsonl",
        &[source_line(
            "1am",
            -76.15,
            Some(-76.1),
            None,
            None,
            Some("c2"),
        )],
    );
    let lbh = db_with(
        &dir,
        "lbh.jsonl",
        &[source_line(
            "1am",
            -76.02,
            Some(-76.0),
            None,
            None,
            Some("c3"),
        )],
    );
    let hbh = db_with(
        &dir,
        "hbh.jsonl",
        &[source_line(
            "1am",
            -76.17,
            Some(-76.1),
            Some("orca"),
            Some("6.0.0"),
            Some("c4"),
        )],
    );
    let out = dir.join("out.jsonl");
    let pairs = format!("{lbl},{lbh} {hbl},{hbh}");
    let values: Vec<&str> = pairs.split(' ').collect();
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2p",
        values[0],
        values[1],
        "2",
        "4.3",
        "2.51",
        "1.5",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let correction = loaded(&out)[0].correction.clone().expect("attached");
    assert!(
        (correction.energy - -76.24251721253893).abs() < 1e-9,
        "{}",
        correction.energy
    );
    assert_eq!(correction.kind, "cbs-cps");
    assert_eq!(
        correction.commands.as_deref(),
        Some(
            &[
                "c1".to_string(),
                "c2".to_string(),
                "c3".to_string(),
                "c4".to_string()
            ][..]
        )
    );
    assert_eq!(correction.program.as_deref(), Some("orca"));
    assert_eq!(correction.program_version.as_deref(), Some("6.0.0"));
}

// rq-4c238015
#[test]
fn extrapolating_a_dft_zero_residual_uses_the_scf_term() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let low = db_with(
        &dir,
        "low.jsonl",
        &[source_line("1am", -76.0, Some(-76.0), None, None, None)],
    );
    let high = db_with(
        &dir,
        "high.jsonl",
        &[source_line("1am", -76.1, Some(-76.1), None, None, None)],
    );
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2p",
        &low,
        &high,
        "2",
        "4.3",
        "2.51",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let correction = loaded(&out)[0].correction.clone().expect("attached");
    assert!(
        (correction.energy - -76.13421861395366).abs() < 1e-9,
        "{}",
        correction.energy
    );
}

#[test] // rq-6c7136f0
fn an_unknown_extrapolation_mode_is_a_usage_error() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "3p",
        "low.jsonl",
        "high.jsonl",
        "2",
        "4.3",
        "2.51",
    ]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
    assert!(stderr(&run).contains("3p"), "{}", stderr(&run));
}

#[test] // rq-e18bdb98
fn a_wrong_extrapolation_value_count_is_a_usage_error() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let short = cjkqc(&[
        &db,
        "--extrapolate",
        "2p",
        "low.jsonl",
        "high.jsonl",
        "2",
        "4.3",
    ]);
    assert_eq!(short.status.code(), Some(2), "{}", stderr(&short));
    let blend_short = cjkqc(&[&db, "--extrapolate", "2s", "low.jsonl", "high.jsonl"]);
    assert_eq!(
        blend_short.status.code(),
        Some(2),
        "{}",
        stderr(&blend_short)
    );
}

#[test] // rq-3981f145
fn a_non_numeric_extrapolation_parameter_is_a_usage_error() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2p",
        "low.jsonl",
        "high.jsonl",
        "2",
        "alpha",
        "2.51",
    ]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test] // rq-08954513
fn extrapolation_parameter_bounds_are_enforced() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    for values in [
        &["2p", "low.jsonl", "high.jsonl", "0", "4.3", "2.51"][..],
        &["2p", "low.jsonl", "high.jsonl", "2.5", "4.3", "2.51"][..],
        &["2p", "low.jsonl", "high.jsonl", "2", "0", "2.51"][..],
        &["2p", "low.jsonl", "high.jsonl", "2", "4.3", "-1"][..],
        &["2s", "low.jsonl", "high.jsonl", "inf"][..],
    ] {
        let mut args = vec![db.as_str(), "--extrapolate"];
        args.extend(values.iter().copied());
        let run = cjkqc(&args);
        assert_eq!(run.status.code(), Some(2), "{values:?}: {}", stderr(&run));
    }
}

#[test] // rq-7728825b
fn a_trailing_extrapolation_f_without_comma_pairs_is_a_usage_error() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2p",
        "low.jsonl",
        "high.jsonl",
        "2",
        "4.3",
        "2.51",
        "1.5",
    ]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test] // rq-1527f9fe
fn a_half_paired_combined_form_is_a_usage_error() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2p",
        "low1.jsonl,low2.jsonl",
        "high.jsonl",
        "2",
        "4.3",
        "2.51",
        "1.5",
    ]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test] // rq-17ef3709
fn an_over_full_extrapolation_comma_group_is_a_usage_error() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2p",
        "a.jsonl,b.jsonl,c.jsonl",
        "d.jsonl,e.jsonl",
        "2",
        "4.3",
        "2.51",
        "1.5",
    ]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test] // rq-7ecf0b87
fn a_comma_pair_where_a_single_path_is_required_is_a_usage_error() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2s",
        "low1.jsonl,low2.jsonl",
        "high.jsonl",
        "1.5",
    ]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test] // rq-50c53556
fn repeating_extrapolate_is_a_usage_error() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2s",
        "low.jsonl",
        "high.jsonl",
        "1.5",
        "--extrapolate",
        "2s",
        "low.jsonl",
        "high.jsonl",
        "1.5",
    ]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
    // Even a repeated flag whose merged values would parse as one valid
    // invocation is caught by the occurrence count.
    let merged = cjkqc(&[
        &db,
        "--extrapolate",
        "2s",
        "low.jsonl",
        "high.jsonl",
        "1.5",
        "--extrapolate",
        "2",
    ]);
    assert_eq!(merged.status.code(), Some(2), "{}", stderr(&merged));
}

#[test] // rq-d576647e
fn extrapolate_with_add_correction_is_a_usage_error() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let run = cjkqc(&[
        &db,
        "--add-correction",
        "sp.jsonl",
        "--extrapolate",
        "2s",
        "low.jsonl",
        "high.jsonl",
        "1.5",
    ]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test] // rq-c492a677
fn extrapolate_alone_is_not_a_visible_outcome() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let run = cjkqc(&[&db, "--extrapolate", "2s", "low.jsonl", "high.jsonl", "1.5"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

// rq-925967be
#[test]
fn a_name_missing_from_one_extrapolation_source_is_uncovered() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let low = db_with(
        &dir,
        "low.jsonl",
        &[source_line("1am", -76.0, Some(-76.0), None, None, None)],
    );
    let high = db_with(
        &dir,
        "high.jsonl",
        &[source_line("2am", -76.1, Some(-76.1), None, None, None)],
    );
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2p",
        &low,
        &high,
        "2",
        "4.3",
        "2.51",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(loaded(&out)[0].correction.is_none());
    let errors = stderr(&run);
    assert!(errors.contains("1am"), "{errors}");
    assert!(errors.contains("high.jsonl"), "{errors}");
}

// rq-0ec3ddc5
#[test]
fn extrapolation_matching_is_exact_and_case_sensitive() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let low = db_with(&dir, "low.jsonl", &[blend_source("1AM", -76.0)]);
    let high = db_with(&dir, "high.jsonl", &[blend_source("1AM", -76.1)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2s",
        &low,
        &high,
        "1.5",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(loaded(&out)[0].correction.is_none());
    assert!(stderr(&run).contains("1am"), "{}", stderr(&run));
}

// rq-41bd180a
#[test]
fn every_run_record_sharing_a_covered_name_receives_the_extrapolated_correction() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[sample_line("1am", -38.51), sample_line("1am", -38.52)],
    );
    let low = db_with(&dir, "low.jsonl", &[blend_source("1am", -76.0)]);
    let high = db_with(&dir, "high.jsonl", &[blend_source("1am", -76.1)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2s",
        &low,
        &high,
        "1.5",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let loaded = loaded(&out);
    assert_eq!(loaded.len(), 2);
    for record in &loaded {
        assert!(record.correction.is_some());
    }
}

// rq-7786c735
#[test]
fn an_unnamed_run_record_never_matches_an_extrapolation() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[sample_line("placeholder", -38.51).replace("\"name\":\"placeholder\",", "")],
    );
    let low = db_with(&dir, "low.jsonl", &[blend_source("1am", -76.0)]);
    let high = db_with(&dir, "high.jsonl", &[blend_source("1am", -76.1)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2s",
        &low,
        &high,
        "1.5",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(loaded(&out)[0].correction.is_none());
}

// rq-73d8ed95
#[test]
fn duplicate_extrapolation_candidate_names_in_one_source_are_fatal() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let low = db_with(
        &dir,
        "low.jsonl",
        &[
            source_line("1am", -76.0, Some(-76.0), None, None, None),
            source_line("1am", -76.01, Some(-76.0), None, None, None),
        ],
    );
    let high = db_with(
        &dir,
        "high.jsonl",
        &[source_line("1am", -76.1, Some(-76.1), None, None, None)],
    );
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2p",
        &low,
        &high,
        "2",
        "4.3",
        "2.51",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
    let errors = stderr(&run);
    assert!(errors.contains("1am"), "{errors}");
    assert!(errors.contains("low.jsonl"), "{errors}");
    assert!(!out.exists());
}

// rq-ccc8ed3a
#[test]
fn mixed_extrapolation_programs_are_fatal() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let low = db_with(
        &dir,
        "low.jsonl",
        &[source_line(
            "1am",
            -76.0,
            Some(-76.0),
            Some("orca"),
            None,
            None,
        )],
    );
    let high = db_with(
        &dir,
        "high.jsonl",
        &[source_line(
            "1am",
            -76.1,
            Some(-76.1),
            Some("mrcc"),
            None,
            None,
        )],
    );
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2p",
        &low,
        &high,
        "2",
        "4.3",
        "2.51",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
    let errors = stderr(&run);
    for token in ["1am", "orca", "mrcc"] {
        assert!(errors.contains(token), "{errors}");
    }
    assert!(!out.exists());
}

// rq-c4c4e1d7
#[test]
fn the_combined_form_rejects_one_mixed_extrapolation_contributor() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let orca = |ee: f64, scf: f64, path: &str| {
        db_with(
            &dir,
            path,
            &[source_line("1am", ee, Some(scf), Some("orca"), None, None)],
        )
    };
    let lbl = orca(-76.0, -76.0, "lbl.jsonl");
    let hbl = orca(-76.15, -76.1, "hbl.jsonl");
    let lbh = orca(-76.02, -76.0, "lbh.jsonl");
    let hbh = db_with(
        &dir,
        "hbh.jsonl",
        &[source_line(
            "1am",
            -76.17,
            Some(-76.1),
            Some("mrcc"),
            None,
            None,
        )],
    );
    let out = dir.join("out.jsonl");
    let pairs = format!("{lbl},{lbh} {hbl},{hbh}");
    let values: Vec<&str> = pairs.split(' ').collect();
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2p",
        values[0],
        values[1],
        "2",
        "4.3",
        "2.51",
        "1.5",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
    let errors = stderr(&run);
    for token in ["1am", "orca", "mrcc"] {
        assert!(errors.contains(token), "{errors}");
    }
}

// rq-41d43353
#[test]
fn an_extrapolation_candidate_without_a_program_never_violates() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let low = db_with(
        &dir,
        "low.jsonl",
        &[source_line("1am", -76.0, Some(-76.0), None, None, None)],
    );
    let high = db_with(
        &dir,
        "high.jsonl",
        &[source_line(
            "1am",
            -76.1,
            Some(-76.1),
            Some("orca"),
            None,
            None,
        )],
    );
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2p",
        &low,
        &high,
        "2",
        "4.3",
        "2.51",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let correction = loaded(&out)[0].correction.clone().expect("attached");
    assert_eq!(correction.program.as_deref(), Some("orca"));
}

// rq-5f78f8a6
#[test]
fn an_extrapolation_version_mismatch_is_fatal() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let low = db_with(
        &dir,
        "low.jsonl",
        &[source_line(
            "1am",
            -76.0,
            Some(-76.0),
            Some("orca"),
            Some("6.0.0"),
            None,
        )],
    );
    let high = db_with(
        &dir,
        "high.jsonl",
        &[source_line(
            "1am",
            -76.1,
            Some(-76.1),
            Some("orca"),
            Some("6.1.0"),
            None,
        )],
    );
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2p",
        &low,
        &high,
        "2",
        "4.3",
        "2.51",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
    let errors = stderr(&run);
    for token in ["1am", "6.0.0", "6.1.0"] {
        assert!(errors.contains(token), "{errors}");
    }
    assert!(!out.exists());
}

// rq-7cb01a45
#[test]
fn matching_extrapolation_program_and_version_pass_the_uniformity_check() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let low = db_with(
        &dir,
        "low.jsonl",
        &[source_line(
            "1am",
            -76.0,
            Some(-76.0),
            Some("orca"),
            Some("6.1.0"),
            None,
        )],
    );
    let high = db_with(
        &dir,
        "high.jsonl",
        &[source_line(
            "1am",
            -76.1,
            Some(-76.1),
            Some("orca"),
            Some("6.1.0"),
            None,
        )],
    );
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2p",
        &low,
        &high,
        "2",
        "4.3",
        "2.51",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let correction = loaded(&out)[0].correction.clone().expect("attached");
    assert_eq!(correction.program_version.as_deref(), Some("6.1.0"));
}

// rq-edffc76e
#[test]
fn identical_extrapolation_electronic_energies_are_fatal() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let low = db_with(
        &dir,
        "low.jsonl",
        &[source_line("1am", -76.0, Some(-76.0), None, None, None)],
    );
    let high = db_with(
        &dir,
        "high.jsonl",
        &[source_line("1am", -76.0, Some(-76.1), None, None, None)],
    );
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2p",
        &low,
        &high,
        "2",
        "4.3",
        "2.51",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
    let errors = stderr(&run);
    for token in ["1am", "low.jsonl", "high.jsonl"] {
        assert!(errors.contains(token), "{errors}");
    }
    assert!(!out.exists());
}

// rq-28bcea43
#[test]
fn the_same_extrapolation_path_twice_is_caught_by_the_energy_rule() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let low = db_with(
        &dir,
        "low.jsonl",
        &[source_line("1am", -76.0, Some(-76.0), None, None, None)],
    );
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2p",
        &low,
        &low,
        "2",
        "4.3",
        "2.51",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
    assert!(stderr(&run).contains("1am"), "{}", stderr(&run));
}

// rq-62f744ee
#[test]
fn a_shared_scf_energy_inside_the_extrapolation_combined_form_is_not_a_violation() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let lbl = db_with(
        &dir,
        "lbl.jsonl",
        &[source_line("1am", -76.05, Some(-76.0), None, None, None)],
    );
    let lbh = db_with(
        &dir,
        "lbh.jsonl",
        &[source_line("1am", -76.07, Some(-76.0), None, None, None)],
    );
    let hbl = db_with(
        &dir,
        "hbl.jsonl",
        &[source_line("1am", -76.15, Some(-76.1), None, None, None)],
    );
    let hbh = db_with(
        &dir,
        "hbh.jsonl",
        &[source_line("1am", -76.17, Some(-76.1), None, None, None)],
    );
    let out = dir.join("out.jsonl");
    let pairs = format!("{lbl},{lbh} {hbl},{hbh}");
    let values: Vec<&str> = pairs.split(' ').collect();
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2p",
        values[0],
        values[1],
        "2",
        "4.3",
        "2.51",
        "1.5",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(loaded(&out)[0].correction.is_some());
}

// rq-17e82092
#[test]
fn a_failed_extrapolation_rerun_does_not_collide_with_its_ok_replacement() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let mut failed = Record {
        status: RecordStatus::Failed,
        name: Some("1am".to_string()),
        error: Some("did not converge".to_string()),
        ..Record::default()
    };
    failed.status = RecordStatus::Failed;
    let low = db_with(
        &dir,
        "low.jsonl",
        &[
            serde_json::to_string(&failed).expect("serialization"),
            source_line("1am", -76.0, Some(-76.0), None, None, None),
        ],
    );
    let high = db_with(
        &dir,
        "high.jsonl",
        &[source_line("1am", -76.1, Some(-76.1), None, None, None)],
    );
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2p",
        &low,
        &high,
        "2",
        "4.3",
        "2.51",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(loaded(&out)[0].correction.is_some());
    let warnings = stderr(&run);
    assert!(warnings.contains("1am"), "{warnings}");
    assert!(warnings.contains("skipped"), "{warnings}");
}

// rq-69f2f193
#[test]
fn an_extrapolation_candidate_without_an_electronic_energy_is_skipped() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let low = db_with(
        &dir,
        "low.jsonl",
        &[source_line("1am", -76.0, Some(-76.0), None, None, None)
            .replace("\"electronic_energy\":-76.0,", "")],
    );
    let high = db_with(
        &dir,
        "high.jsonl",
        &[source_line("1am", -76.1, Some(-76.1), None, None, None)],
    );
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2p",
        &low,
        &high,
        "2",
        "4.3",
        "2.51",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(loaded(&out)[0].correction.is_none());
    let warnings = stderr(&run);
    assert!(warnings.contains("1am"), "{warnings}");
    assert!(warnings.contains("skipped"), "{warnings}");
}

// rq-5c26081b
#[test]
fn an_extrapolation_candidate_without_scf_energy_is_skipped() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let qualified = source_line("1am", -76.1, Some(-76.1), None, None, None);
    // The low source lacks scf_energy.
    let low = db_with(&dir, "low.jsonl", &[blend_source("1am", -76.0)]);
    let high = db_with(&dir, "high.jsonl", std::slice::from_ref(&qualified));
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2p",
        &low,
        &high,
        "2",
        "4.3",
        "2.51",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(loaded(&out)[0].correction.is_none());
    let warnings = stderr(&run);
    assert!(warnings.contains("1am"), "{warnings}");
    assert!(warnings.contains("skipped"), "{warnings}");
    // The same holds when the high database's candidate lacks scf_energy.
    // A fresh output path: the first run's database must not stand in the
    // way of the second (the overwrite confirmation rule,
    // rqm/collection.md).
    let out = dir.join("out-no-scf.jsonl");
    let low = db_with(
        &dir,
        "low.jsonl",
        &[source_line("1am", -76.0, Some(-76.0), None, None, None)],
    );
    let high = db_with(&dir, "high.jsonl", &[blend_source("1am", -76.1)]);
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2p",
        &low,
        &high,
        "2",
        "4.3",
        "2.51",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(loaded(&out)[0].correction.is_none());
}

// rq-b92f7576
#[test]
fn extrapolating_two_s_needs_no_scf_energy() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let low = db_with(&dir, "low.jsonl", &[blend_source("1am", -76.0)]);
    let high = db_with(&dir, "high.jsonl", &[blend_source("1am", -76.1)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2s",
        &low,
        &high,
        "1.5",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(loaded(&out)[0].correction.is_some());
}

// rq-3bad508e
#[test]
fn unmatched_extrapolation_candidates_are_one_aggregate_warning() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let low = db_with(
        &dir,
        "low.jsonl",
        &[blend_source("1am", -76.0), blend_source("2am", -70.0)],
    );
    let high = db_with(
        &dir,
        "high.jsonl",
        &[blend_source("1am", -76.1), blend_source("2am", -70.1)],
    );
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2s",
        &low,
        &high,
        "1.5",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(loaded(&out)[0].correction.is_some());
    assert!(
        stderr(&run).contains("2 unmatched extrapolation records"),
        "{}",
        stderr(&run)
    );
}

// rq-61cdb639
#[test]
fn a_corrupt_line_in_an_extrapolation_source_is_skipped_and_fails_the_run() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let low_path = dir.join("low.jsonl");
    write_jsonl(
        &low_path,
        &[&blend_source("1am", -76.0), "{ this is not a record"],
    );
    let high = db_with(&dir, "high.jsonl", &[blend_source("1am", -76.1)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2s",
        &low_path.to_string_lossy(),
        &high,
        "1.5",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
    let warnings = stderr(&run);
    assert!(warnings.contains("low.jsonl"), "{warnings}");
    assert!(warnings.contains("line 2"), "{warnings}");
    assert!(loaded(&out)[0].correction.is_some());
}

// rq-ae11003d
#[test]
fn a_missing_extrapolation_source_is_fatal() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let low = db_with(&dir, "low.jsonl", &[blend_source("1am", -76.0)]);
    let out = dir.join("out.jsonl");
    let missing = dir.join("high.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2s",
        &low,
        &missing.to_string_lossy(),
        "1.5",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
    assert!(!out.exists());
}

// rq-03c8c0f9
#[test]
fn a_program_output_extrapolation_source_is_fatal() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let low = db_with(&dir, "low.jsonl", &[blend_source("1am", -76.0)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2s",
        &low,
        &fixture("xtb/1dma.log"),
        "1.5",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
    assert!(!out.exists());
}

// rq-9be77472
#[test]
fn an_empty_extrapolation_source_leaves_every_named_record_uncovered() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let low = db_with(&dir, "low.jsonl", &[]);
    let high = db_with(&dir, "high.jsonl", &[blend_source("1am", -76.1)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2s",
        &low,
        &high,
        "1.5",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(loaded(&out)[0].correction.is_none());
    let warnings = stderr(&run);
    assert!(warnings.contains("1am"), "{warnings}");
    assert!(warnings.contains("low.jsonl"), "{warnings}");
}

// rq-d516398b
#[test]
fn an_extrapolation_match_replaces_a_stored_correction() {
    let dir = temp_dir();
    let db = database_in(&dir, &[thermo_line("1am", -38.51, &[], Some(-76.0))]);
    let low = db_with(&dir, "low.jsonl", &[blend_source("1am", -76.0)]);
    let high = db_with(&dir, "high.jsonl", &[blend_source("1am", -76.1)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2s",
        &low,
        &high,
        "1.5",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let correction = loaded(&out)[0].correction.clone().expect("replaced");
    assert!(
        (correction.energy - -76.15).abs() < 1e-12,
        "{}",
        correction.energy
    );
}

// rq-54c33c26
#[test]
fn an_uncovered_extrapolation_record_keeps_its_stored_correction() {
    let dir = temp_dir();
    let db = database_in(&dir, &[thermo_line("2am", -38.51, &[], Some(-76.0))]);
    let low = db_with(&dir, "low.jsonl", &[blend_source("1am", -76.0)]);
    let high = db_with(&dir, "high.jsonl", &[blend_source("1am", -76.1)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2s",
        &low,
        &high,
        "1.5",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let correction = loaded(&out)[0].correction.clone().expect("kept");
    assert_eq!(correction.energy, -76.0);
}

// rq-00078b6e
#[test]
fn the_correction_column_shows_an_extrapolated_corrections_provenance() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let low = db_with(
        &dir,
        "low.jsonl",
        &[source_line(
            "1am",
            -76.0,
            Some(-76.0),
            Some("orca"),
            Some("6.0.0"),
            Some("! def2-SVP Sp"),
        )],
    );
    let high = db_with(
        &dir,
        "high.jsonl",
        &[source_line(
            "1am",
            -76.1,
            Some(-76.1),
            Some("orca"),
            Some("6.0.0"),
            Some("! def2-TZVPP Sp"),
        )],
    );
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2p",
        &low,
        &high,
        "2",
        "4.3",
        "2.51",
        "--correction",
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(
        stdout(&run).contains("cbs orca 6.0.0 \"! def2-SVP Sp\" \"! def2-TZVPP Sp\""),
        "{}",
        stdout(&run)
    );
}

// rq-8a869bbd
#[test]
fn extrapolate_prints_nothing_of_its_own() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let low = db_with(&dir, "low.jsonl", &[blend_source("1am", -76.0)]);
    let high = db_with(&dir, "high.jsonl", &[blend_source("1am", -76.1)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2s",
        &low,
        &high,
        "1.5",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1, "{text}");
    assert!(lines[0].starts_with("merged 1 records"), "{text}");
    assert!(lines[0].contains("(1 corrected)"), "{text}");
    assert!(loaded(&out)[0].correction.is_some());
}

// rq-7de7b92a
#[test]
fn absent_extrapolation_commands_are_omitted_from_the_column() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_line("1am", -38.51)]);
    let low = db_with(
        &dir,
        "low.jsonl",
        &[source_line(
            "1am",
            -76.0,
            Some(-76.0),
            Some("orca"),
            Some("6.0.0"),
            None,
        )],
    );
    let high = db_with(
        &dir,
        "high.jsonl",
        &[source_line(
            "1am",
            -76.1,
            Some(-76.1),
            Some("orca"),
            Some("6.0.0"),
            None,
        )],
    );
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2p",
        &low,
        &high,
        "2",
        "4.3",
        "2.51",
        "--correction",
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    assert!(text.contains("cbs orca 6.0.0"), "{text}");
    assert!(!text.contains('"'), "{text}");
}

// rq-b2e971a9
#[test]
fn an_extrapolating_build_run_reports_its_corrections() {
    let dir = temp_dir();
    let low = db_with(&dir, "low.jsonl", &[blend_source("1dma", -135.0)]);
    let high = db_with(&dir, "high.jsonl", &[blend_source("1dma", -135.1)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &fixture("xtb/1dma.log"),
        "--extrapolate",
        "2s",
        &low,
        &high,
        "1.5",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(
        stdout(&run).contains(&format!(
            "collected 1 records into {} (0 failed, 0 truncated, 1 corrected)",
            out.display()
        )),
        "{}",
        stdout(&run)
    );
}

// rq-187b51ac
#[test]
fn an_extrapolating_merge_run_reports_its_corrections() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[sample_line("1am", -38.51), sample_line("2am", -38.52)],
    );
    let low = db_with(&dir, "low.jsonl", &[blend_source("1am", -76.0)]);
    let high = db_with(&dir, "high.jsonl", &[blend_source("1am", -76.1)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2s",
        &low,
        &high,
        "1.5",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(
        stdout(&run).contains(&format!(
            "merged 2 records into {} (1 corrected)",
            out.display()
        )),
        "{}",
        stdout(&run)
    );
}

// rq-478fad20
#[test]
fn a_manipulated_extrapolating_run_reports_its_corrections() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[sample_line("1am", -38.51), sample_line("2am", -38.52)],
    );
    let low = db_with(&dir, "low.jsonl", &[blend_source("1am", -76.0)]);
    let high = db_with(&dir, "high.jsonl", &[blend_source("1am", -76.1)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2s",
        &low,
        &high,
        "1.5",
        "--filename",
        "--out",
        &out.to_string_lossy(),
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(
        stdout(&run).contains(&format!(
            "filtered 2 records into {} (1 corrected)",
            out.display()
        )),
        "{}",
        stdout(&run)
    );
}

// rq-cca456ff
#[test]
fn el_stays_raw_while_elcorr_shows_the_extrapolated_energy() {
    let dir = temp_dir();
    let db = database_in(&dir, &[thermo_line("1am", -38.51, &[], None)]);
    let low = db_with(&dir, "low.jsonl", &[blend_source("1am", -76.0)]);
    let high = db_with(&dir, "high.jsonl", &[blend_source("1am", -76.1)]);
    let run = cjkqc(&[
        &db,
        "--extrapolate",
        "2s",
        &low,
        &high,
        "1.5",
        "--el",
        "--elcorr",
        "--gcorr",
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let rows = data_rows(&stdout(&run));
    assert_eq!(rows.len(), 1, "{}", stdout(&run));
    assert_eq!(rows[0][0], "-38.5100000000");
    let el: f64 = rows[0][0].parse().expect("el");
    let elcorr: f64 = rows[0][1].parse().expect("elcorr");
    let gcorr: f64 = rows[0][2].parse().expect("gcorr");
    assert!((elcorr - -76.15).abs() < 1e-12, "{}", elcorr);
    assert!(
        (gcorr - (-76.15 + (raw_gibbs(-38.51) - el))).abs() < 1e-9,
        "{}",
        stdout(&run)
    );
}

// rq-14e9d366
#[test]
fn the_extrapolated_correction_follows_the_runs_conditions() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            thermo_line("am1", -50.0, &[("am", 1)], None),
            thermo_line("2am", -141.0, &[("am", 2)], None),
        ],
    );
    let low = db_with(
        &dir,
        "low.jsonl",
        &[blend_source("am1", -60.0), blend_source("2am", -160.0)],
    );
    let high = db_with(
        &dir,
        "high.jsonl",
        &[blend_source("am1", -60.1), blend_source("2am", -160.1)],
    );
    let warm = cjkqc(&[
        &db,
        "--extrapolate",
        "2s",
        &low,
        &high,
        "1.5",
        "--elcorr",
        "--formation-corr",
        "--temp",
        "298.15",
    ]);
    let hot = cjkqc(&[
        &db,
        "--extrapolate",
        "2s",
        &low,
        &high,
        "1.5",
        "--elcorr",
        "--formation-corr",
        "--temp",
        "350",
    ]);
    assert_eq!(warm.status.code(), Some(0), "{}", stderr(&warm));
    assert_eq!(hot.status.code(), Some(0), "{}", stderr(&hot));
    let warm_row = formation_row(&stdout(&warm), "2am");
    let hot_row = formation_row(&stdout(&hot), "2am");
    assert_eq!(
        warm_row[1], hot_row[1],
        "the elcorr level ignores the temperature"
    );
    assert_ne!(warm_row[4], hot_row[4], "the gcorr level follows it");
}

// --- movie export scenarios (rqm/movie.md) ---

/// Runs CJKQC with the given working directory, so the fixed-name
/// `movie.xyz` lands in a per-test directory instead of the workspace.
fn cjkqc_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_CJKQC"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to start CJKQC")
}

/// A database line for an ok record with a standard two-atom geometry,
/// for movie frames that carry both a name and an energy.
fn sample_geometry_line(name: &str, energy: f64) -> String {
    let record = Record {
        name: Some(name.to_string()),
        electronic_energy: Some(energy),
        geometry: Some(Geometry {
            symbols: vec!["N".to_string(), "H".to_string()],
            coordinates: vec![0.0, 0.0, 0.0, 0.0, 0.0, 1.0],
        }),
        ..Record::default()
    };
    serde_json::to_string(&record).expect("serialization")
}

/// A database line for a failed record with a geometry; a failed record
/// carries a non-empty error (`rqm/record.md`), which load validation
/// requires.
fn failed_geometry_line(name: &str) -> String {
    let record = Record {
        status: RecordStatus::Failed,
        name: Some(name.to_string()),
        error: Some("job failed".to_string()),
        geometry: Some(Geometry {
            symbols: vec!["N".to_string()],
            coordinates: vec![0.0, 0.0, 0.0],
        }),
        ..Record::default()
    };
    serde_json::to_string(&record).expect("serialization")
}

/// Parses movie.xyz into (comment, atom lines) frames; an empty file
/// yields no frames.
fn movie_frames_of(path: &Path) -> Vec<(String, Vec<String>)> {
    let text = fs::read_to_string(path).expect("read movie.xyz");
    let mut lines = text.lines();
    let mut frames = Vec::new();
    while let Some(count) = lines.next() {
        let count: usize = count.trim().parse().expect("atom count line");
        let comment = lines.next().expect("comment line").to_string();
        let atoms: Vec<String> = (0..count)
            .map(|_| lines.next().expect("atom line").to_string())
            .collect();
        frames.push((comment, atoms));
    }
    frames
}

#[test] // rq-76d13d9c
fn the_frames_are_the_pipeline_survivors_in_run_order() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            sample_geometry_line("a", -40.0),
            sample_geometry_line("b", -40.001),
            sample_geometry_line("c", -38.0),
        ],
    );
    let run = cjkqc_in(&dir, &[&db, "--cutr", "el", "5", "--movie"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let frames = movie_frames_of(&dir.join("movie.xyz"));
    let comments: Vec<&str> = frames.iter().map(|(comment, _)| comment.as_str()).collect();
    assert_eq!(comments, ["a E=-40.0000000000", "b E=-40.0010000000"]);
}

#[test] // rq-d72194bc
fn the_frame_order_follows_the_sort() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            sample_geometry_line("a", -40.0),
            sample_geometry_line("b", -40.001),
            sample_geometry_line("c", -38.0),
        ],
    );
    let run = cjkqc_in(&dir, &[&db, "--sort", "el", "--reverse", "--movie"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let frames = movie_frames_of(&dir.join("movie.xyz"));
    let comments: Vec<&str> = frames.iter().map(|(comment, _)| comment.as_str()).collect();
    assert_eq!(
        comments,
        [
            "c E=-38.0000000000",
            "a E=-40.0000000000",
            "b E=-40.0010000000"
        ]
    );
}

#[test] // rq-86a3deda
fn a_print_only_run_movies_every_loaded_ok_record() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            sample_geometry_line("a", -1.0),
            sample_geometry_line("b", -2.0),
            failed_geometry_line("f"),
        ],
    );
    let run = cjkqc_in(&dir, &[&db, "--movie"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let frames = movie_frames_of(&dir.join("movie.xyz"));
    let comments: Vec<&str> = frames.iter().map(|(comment, _)| comment.as_str()).collect();
    assert_eq!(comments, ["a E=-1.0000000000", "b E=-2.0000000000"]);
    let out = stdout(&run);
    assert!(!out.contains("collected"), "{out}");
    assert!(!out.contains("merged"), "{out}");
    assert!(!out.contains("filtered"), "{out}");
}

#[test] // rq-21313a28
fn an_ok_record_without_a_geometry_is_skipped_and_counted() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[sample_geometry_line("a", -1.0), sample_line("b", -2.0)],
    );
    let run = cjkqc_in(&dir, &[&db, "--movie"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let frames = movie_frames_of(&dir.join("movie.xyz"));
    assert_eq!(frames.len(), 1);
    assert!(stderr(&run).contains("excluded 1 records from movie.xyz (no geometry or not ok)"));
}

#[test] // rq-d8fed97e
fn a_print_only_run_skips_a_failed_record_with_a_geometry() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[sample_geometry_line("a", -1.0), failed_geometry_line("f")],
    );
    let run = cjkqc_in(&dir, &[&db, "--movie"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let frames = movie_frames_of(&dir.join("movie.xyz"));
    let comments: Vec<&str> = frames.iter().map(|(comment, _)| comment.as_str()).collect();
    assert_eq!(comments, ["a E=-1.0000000000"]);
    assert!(stderr(&run).contains("excluded 1 records from movie.xyz (no geometry or not ok)"));
}

#[test] // rq-b756a018
fn no_skip_note_when_every_record_becomes_a_frame() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            sample_geometry_line("a", -1.0),
            sample_geometry_line("b", -2.0),
        ],
    );
    let run = cjkqc_in(&dir, &[&db, "--movie"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(movie_frames_of(&dir.join("movie.xyz")).len(), 2);
    assert!(!stderr(&run).contains("excluded"), "{}", stderr(&run));
}

#[test] // rq-4aab0ef8
fn movie_alone_is_a_visible_outcome() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_geometry_line("a", -1.0)]);
    let run = cjkqc_in(&dir, &[&db, "--movie"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(movie_frames_of(&dir.join("movie.xyz")).len() == 1);
    assert!(
        stdout(&run).contains("wrote 1 frames into movie.xyz"),
        "{}",
        stdout(&run)
    );
}

#[test] // rq-d82889ab
fn movie_combines_with_out() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            sample_geometry_line("a", -40.0),
            sample_geometry_line("b", -40.001),
            sample_geometry_line("c", -38.0),
        ],
    );
    let run = cjkqc_in(
        &dir,
        &[&db, "--cutr", "el", "5", "--movie", "--out", "kept.jsonl"],
    );
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(loaded(&dir.join("kept.jsonl")).len(), 2);
    let out = stdout(&run);
    let filtered = out
        .find("filtered 2 records into kept.jsonl")
        .expect("filtered line");
    let wrote = out
        .find("wrote 2 frames into movie.xyz")
        .expect("wrote line");
    assert!(filtered < wrote, "{out}");
}

#[test] // rq-1e523f8f
fn zero_survivors_write_an_empty_movie() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_geometry_line("a", -1.0)]);
    let run = cjkqc_in(&dir, &[&db, "--extract", "9w", "--movie"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(
        fs::read(dir.join("movie.xyz")).expect("read movie.xyz"),
        b""
    );
    assert!(
        stdout(&run).contains("wrote 0 frames into movie.xyz"),
        "{}",
        stdout(&run)
    );
}

#[test] // rq-146b84dd
fn replacing_movie_xyz_follows_the_out_confirmation() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_geometry_line("a", -1.0)]);
    fs::write(dir.join("movie.xyz"), "sentinel\n").expect("seed movie.xyz");
    let run = cjkqc_in(&dir, &[&db, "--movie"]);
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
    assert_eq!(
        fs::read_to_string(dir.join("movie.xyz")).expect("read movie.xyz"),
        "sentinel\n"
    );
    assert!(stderr(&run).contains("error: movie.xyz exists; not interactive; use --fout to force"));
    let forced = cjkqc_in(&dir, &[&db, "--movie", "--fout"]);
    assert_eq!(forced.status.code(), Some(0), "{}", stderr(&forced));
    let frames = movie_frames_of(&dir.join("movie.xyz"));
    assert_eq!(frames.len(), 1);
    assert!(
        !stderr(&forced).contains("overwrite?"),
        "{}",
        stderr(&forced)
    );
}

#[test] // rq-d75cccd1
fn add_correction_with_movie_uses_pipeline_semantics() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[sample_geometry_line("a", -38.51), failed_geometry_line("f")],
    );
    let corr = db_with(&dir, "corr.jsonl", &[sample_line("a", -50.0)]);
    let run = cjkqc_in(&dir, &[&db, "--add-correction", &corr, "--movie"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let frames = movie_frames_of(&dir.join("movie.xyz"));
    let comments: Vec<&str> = frames.iter().map(|(comment, _)| comment.as_str()).collect();
    assert_eq!(comments, ["a E=-38.5100000000"]);
}

#[test] // rq-42da75bb
fn a_refusal_at_out_ends_the_run_before_movie_xyz() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            sample_geometry_line("a", -40.0),
            sample_geometry_line("b", -40.001),
            sample_geometry_line("c", -38.0),
        ],
    );
    write_jsonl(
        &dir.join("kept.jsonl"),
        &[sample_line("OLD", -1.0).as_str()],
    );
    fs::write(dir.join("movie.xyz"), "sentinel\n").expect("seed movie.xyz");
    let run = cjkqc_in(
        &dir,
        &[&db, "--cutr", "el", "5", "--out", "kept.jsonl", "--movie"],
    );
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
    assert_eq!(
        fs::read_to_string(dir.join("movie.xyz")).expect("read movie.xyz"),
        "sentinel\n"
    );
    assert_eq!(
        loaded(&dir.join("kept.jsonl"))[0].name.as_deref(),
        Some("OLD")
    );
    assert!(stderr(&run).contains("error: kept.jsonl exists; not interactive; use --fout to force"));
}

// --- per-structure xyz export scenarios (rqm/movie.md) ---

/// The `.xyz` file names in a directory, sorted, so a test asserts the
/// exact export set without depending on directory order.
fn xyz_files_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("read directory")
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().into_string().ok()?;
            name.ends_with(".xyz").then_some(name)
        })
        .collect();
    names.sort();
    names
}

/// A database line for a record with an optional name and a standard
/// two-atom geometry.
fn optional_name_geometry_line(name: Option<&str>, energy: f64) -> String {
    let record = Record {
        name: name.map(str::to_string),
        electronic_energy: Some(energy),
        geometry: Some(Geometry {
            symbols: vec!["N".to_string(), "H".to_string()],
            coordinates: vec![0.0, 0.0, 0.0, 0.0, 0.0, 1.0],
        }),
        ..Record::default()
    };
    serde_json::to_string(&record).expect("serialization")
}

#[test] // rq-6b0a7ad4
fn xyz_files_are_the_pipeline_survivors_in_run_order() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            sample_geometry_line("a", -40.0),
            sample_geometry_line("b", -40.001),
            sample_geometry_line("c", -38.0),
        ],
    );
    let run = cjkqc_in(&dir, &[&db, "--cutr", "el", "5", "--write-xyz"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(xyz_files_in(&dir), ["a.xyz", "b.xyz"]);
    assert!(!dir.join("c.xyz").exists());
}

#[test] // rq-ca43482f
fn a_print_only_run_exports_every_loaded_ok_named_record() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            sample_geometry_line("a", -1.0),
            sample_geometry_line("b", -2.0),
            failed_geometry_line("f"),
        ],
    );
    let run = cjkqc_in(&dir, &[&db, "--write-xyz"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(xyz_files_in(&dir), ["a.xyz", "b.xyz"]);
    let out = stdout(&run);
    assert!(!out.contains("collected"), "{out}");
    assert!(!out.contains("merged"), "{out}");
    assert!(!out.contains("filtered"), "{out}");
}

#[test] // rq-dd71a15a
fn the_exported_file_carries_the_movie_frame_text() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_geometry_line("1dma", -38.51)]);
    let run = cjkqc_in(&dir, &[&db, "--write-xyz"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let frames = movie_frames_of(&dir.join("1dma.xyz"));
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].0, "1dma E=-38.5100000000");
    assert_eq!(
        frames[0].1,
        [
            "N 0.000000 0.000000 0.000000",
            "H 0.000000 0.000000 1.000000"
        ]
    );
}

#[test] // rq-a0dfabb1
fn a_nameless_record_is_skipped_and_counted() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            sample_geometry_line("a", -1.0),
            optional_name_geometry_line(None, -2.0),
        ],
    );
    let run = cjkqc_in(&dir, &[&db, "--write-xyz"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(xyz_files_in(&dir), ["a.xyz"]);
    assert!(stderr(&run)
        .contains("excluded 1 records from --write-xyz (no name, no geometry, or not ok)"));
}

#[test] // rq-f1a120cc
fn an_ok_record_without_a_geometry_is_skipped_and_counted_for_xyz() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[sample_geometry_line("a", -1.0), sample_line("b", -2.0)],
    );
    let run = cjkqc_in(&dir, &[&db, "--write-xyz"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(xyz_files_in(&dir), ["a.xyz"]);
    assert!(stderr(&run)
        .contains("excluded 1 records from --write-xyz (no name, no geometry, or not ok)"));
}

#[test] // rq-dc092b01
fn a_print_only_run_skips_a_failed_record_with_a_geometry_for_xyz() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[sample_geometry_line("a", -1.0), failed_geometry_line("f")],
    );
    let run = cjkqc_in(&dir, &[&db, "--write-xyz"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(xyz_files_in(&dir), ["a.xyz"]);
    assert!(stderr(&run)
        .contains("excluded 1 records from --write-xyz (no name, no geometry, or not ok)"));
}

#[test] // rq-b56902e0
fn no_skip_note_when_every_record_becomes_an_xyz_file() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            sample_geometry_line("a", -1.0),
            sample_geometry_line("b", -2.0),
        ],
    );
    let run = cjkqc_in(&dir, &[&db, "--write-xyz"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(xyz_files_in(&dir), ["a.xyz", "b.xyz"]);
    assert!(!stderr(&run).contains("excluded"), "{}", stderr(&run));
}

#[test] // rq-94142c9a
fn an_unsafe_record_name_is_skipped_and_counted() {
    let dir = temp_dir();
    let outside = format!(
        "cjkqc-escape-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let outside_path = dir
        .parent()
        .expect("temp parent")
        .join(format!("{outside}.xyz"));
    let name = format!("../{outside}");
    let db = database_in(
        &dir,
        &[
            sample_geometry_line("a", -1.0),
            optional_name_geometry_line(Some(&name), -2.0),
        ],
    );
    let run = cjkqc_in(&dir, &[&db, "--write-xyz"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(xyz_files_in(&dir), ["a.xyz"]);
    assert!(!outside_path.exists(), "{}", outside_path.display());
    assert!(stderr(&run)
        .contains("excluded 1 records from --write-xyz (no name, no geometry, or not ok)"));
}

#[test] // rq-4a8815f8
fn equal_names_among_survivors_are_a_usage_error() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            sample_geometry_line("1dma", -1.0),
            sample_geometry_line("1dma", -2.0),
        ],
    );
    let run = cjkqc_in(&dir, &[&db, "--write-xyz"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
    assert!(xyz_files_in(&dir).is_empty(), "{:?}", xyz_files_in(&dir));
    assert!(stderr(&run).contains("error: duplicate xyz file name 1dma.xyz; nothing written"));
}

#[test] // rq-cde11874
fn write_xyz_alone_is_a_visible_outcome() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_geometry_line("a", -1.0)]);
    let run = cjkqc_in(&dir, &[&db, "--write-xyz"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(xyz_files_in(&dir), ["a.xyz"]);
    assert!(
        stdout(&run).contains("wrote 1 xyz files into the current directory"),
        "{}",
        stdout(&run)
    );
}

#[test] // rq-24ffa417
fn write_xyz_combines_with_out_and_movie() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            sample_geometry_line("a", -40.0),
            sample_geometry_line("b", -40.001),
            sample_geometry_line("c", -38.0),
        ],
    );
    let run = cjkqc_in(
        &dir,
        &[
            &db,
            "--cutr",
            "el",
            "5",
            "--write-xyz",
            "--movie",
            "--out",
            "kept.jsonl",
        ],
    );
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(loaded(&dir.join("kept.jsonl")).len(), 2);
    assert!(dir.join("a.xyz").exists());
    assert!(dir.join("b.xyz").exists());
    assert_eq!(movie_frames_of(&dir.join("movie.xyz")).len(), 2);
    let out = stdout(&run);
    let filtered = out
        .find("filtered 2 records into kept.jsonl")
        .expect("filtered line");
    let files = out
        .find("wrote 2 xyz files into the current directory")
        .expect("xyz summary line");
    let movie = out
        .find("wrote 2 frames into movie.xyz")
        .expect("movie summary line");
    assert!(filtered < files && files < movie, "{out}");
}

#[test] // rq-faaf6c4c
fn zero_survivors_write_no_xyz_files() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_geometry_line("a", -1.0)]);
    let run = cjkqc_in(&dir, &[&db, "--extract", "9w", "--write-xyz"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(xyz_files_in(&dir).is_empty(), "{:?}", xyz_files_in(&dir));
    assert!(
        stdout(&run).contains("wrote 0 xyz files into the current directory"),
        "{}",
        stdout(&run)
    );
}

#[test] // rq-92d9ed7f
fn replacing_existing_xyz_files_follows_the_out_confirmation() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            sample_geometry_line("1dma", -1.0),
            sample_geometry_line("2dma", -2.0),
            sample_geometry_line("3dma", -3.0),
        ],
    );
    fs::write(dir.join("1dma.xyz"), "sentinel one\n").expect("seed 1dma.xyz");
    fs::write(dir.join("2dma.xyz"), "sentinel two\n").expect("seed 2dma.xyz");
    let run = cjkqc_in(&dir, &[&db, "--write-xyz"]);
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
    assert_eq!(
        fs::read_to_string(dir.join("1dma.xyz")).expect("read 1dma.xyz"),
        "sentinel one\n"
    );
    assert_eq!(
        fs::read_to_string(dir.join("2dma.xyz")).expect("read 2dma.xyz"),
        "sentinel two\n"
    );
    assert!(!dir.join("3dma.xyz").exists());
    assert!(stderr(&run).contains("error: 1dma.xyz exists; not interactive; use --fout to force"));

    let forced = cjkqc_in(&dir, &[&db, "--write-xyz", "--fout"]);
    assert_eq!(forced.status.code(), Some(0), "{}", stderr(&forced));
    assert_eq!(xyz_files_in(&dir), ["1dma.xyz", "2dma.xyz", "3dma.xyz"]);
    assert_ne!(
        fs::read_to_string(dir.join("1dma.xyz")).expect("read 1dma.xyz"),
        "sentinel one\n"
    );
    assert_ne!(
        fs::read_to_string(dir.join("2dma.xyz")).expect("read 2dma.xyz"),
        "sentinel two\n"
    );
    assert!(
        !stderr(&forced).contains("overwrite?"),
        "{}",
        stderr(&forced)
    );
}

#[test] // rq-5b65c269
fn duplicate_names_abort_before_the_out_database_is_written() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            sample_geometry_line("1dma", -1.0),
            sample_geometry_line("1dma", -2.0),
        ],
    );
    let run = cjkqc_in(&dir, &[&db, "--out", "kept.jsonl", "--write-xyz"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
    assert!(!dir.join("kept.jsonl").exists());
    assert!(xyz_files_in(&dir).is_empty(), "{:?}", xyz_files_in(&dir));
    assert!(stderr(&run).contains("duplicate xyz file name 1dma.xyz; nothing written"));
}

#[test] // rq-26470d18
fn names_differing_only_by_case_are_duplicates() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            sample_geometry_line("Water", -1.0),
            sample_geometry_line("water", -2.0),
        ],
    );
    let run = cjkqc_in(&dir, &[&db, "--write-xyz"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
    assert!(xyz_files_in(&dir).is_empty(), "{:?}", xyz_files_in(&dir));
    assert!(stderr(&run).contains("duplicate xyz file name water.xyz; nothing written"));
}

#[test] // rq-7bdc15be
fn a_record_named_movie_cannot_collide_with_movie_xyz() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_geometry_line("movie", -1.0)]);
    let run = cjkqc_in(&dir, &[&db, "--movie", "--write-xyz"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
    assert!(xyz_files_in(&dir).is_empty(), "{:?}", xyz_files_in(&dir));
    assert!(
        stderr(&run).contains("xyz file name movie.xyz collides with movie.xyz; nothing written")
    );
}

#[test] // rq-5040a356
fn a_record_cannot_collide_with_the_out_path() {
    let dir = temp_dir();
    let db = database_in(&dir, &[sample_geometry_line("kept", -1.0)]);
    let run = cjkqc_in(&dir, &[&db, "--out", "kept.xyz", "--write-xyz"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
    assert!(!dir.join("kept.xyz").exists());
    assert!(xyz_files_in(&dir).is_empty(), "{:?}", xyz_files_in(&dir));
    assert!(stderr(&run)
        .contains("xyz file name kept.xyz collides with --out kept.xyz; nothing written"));
}

#[test] // rq-20104820
fn a_name_with_a_control_character_is_skipped_and_counted() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[optional_name_geometry_line(Some("bad\nname"), -1.0)],
    );
    let run = cjkqc_in(&dir, &[&db, "--write-xyz"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert!(xyz_files_in(&dir).is_empty(), "{:?}", xyz_files_in(&dir));
    assert!(stderr(&run)
        .contains("excluded 1 records from --write-xyz (no name, no geometry, or not ok)"));
}

// --- parallel collection (rqm/collection.md, Parallel Collection) ---

// rq-c974ee61
#[test]
fn a_parallel_run_writes_the_sequential_runs_database() {
    let dir = temp_dir();
    let inputs = [
        fixture("xtb/1dma.log"),
        fixture("orca/1pinic1sa-110_1_9-str9.out"),
        fixture("gaussian/2cha-020_1_0-str3.log"),
    ];
    let sequential = dir.join("sequential.jsonl");
    let parallel = dir.join("parallel.jsonl");
    let plain = cjkqc(&[
        &inputs[0],
        &inputs[1],
        &inputs[2],
        "--out",
        &sequential.to_string_lossy(),
    ]);
    assert_eq!(plain.status.code(), Some(0), "{}", stderr(&plain));
    let run = cjkqc(&[
        &inputs[0],
        &inputs[1],
        &inputs[2],
        "--out",
        &parallel.to_string_lossy(),
        "--parallel",
        "3",
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let plain_bytes = fs::read(&sequential).expect("sequential database");
    let parallel_bytes = fs::read(&parallel).expect("parallel database");
    assert_eq!(
        plain_bytes, parallel_bytes,
        "the parallel database must be byte-identical to the sequential one"
    );
}

// rq-7286bd48
#[test]
fn records_keep_argument_order_at_any_worker_count() {
    let dir = temp_dir();
    let out = dir.join("db.jsonl");
    let run = cjkqc(&[
        &fixture("xtb/1dma.log"),
        &fixture("orca/1pinic1sa-110_1_9-str9.out"),
        &fixture("gaussian/2cha-020_1_0-str3.log"),
        "--out",
        &out.to_string_lossy(),
        "--parallel",
        "3",
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let records = loaded(&out);
    let programs: Vec<_> = records
        .iter()
        .filter_map(|record| record.program.as_deref())
        .collect();
    assert_eq!(programs, vec!["xtb", "orca", "gaussian"]);
}

// rq-23b4fd18
#[test]
fn a_parallel_run_reports_the_argument_order_first_error() {
    let dir = temp_dir();
    let out = dir.join("db.jsonl");
    let junk = dir.join("junk.txt");
    fs::write(&junk, "not a program output\n").expect("write junk");
    let junk_text = junk.to_string_lossy().into_owned();
    let out_text = out.to_string_lossy().into_owned();
    let run = cjkqc(&[
        &fixture("xtb/1dma.log"),
        &junk_text,
        &fixture("orca/1pinic1sa-110_1_9-str9.out"),
        &junk_text,
        &fixture("gaussian/2cha-020_1_0-str3.log"),
        "--out",
        &out_text,
        "--parallel",
        "4",
    ]);
    assert_eq!(run.status.code(), Some(1));
    let error = stderr(&run);
    assert!(error.contains("junk.txt"), "{error}");
    assert!(
        !out.exists(),
        "nothing is written when an input is unrecognized"
    );
}

// rq-ecff2560
#[test]
fn a_parallel_run_writes_nothing_when_any_input_fails() {
    let dir = temp_dir();
    let out = dir.join("db.jsonl");
    let missing = dir.join("does-not-exist.out");
    let run = cjkqc(&[
        &fixture("xtb/1dma.log"),
        &fixture("orca/1pinic1sa-110_1_9-str9.out"),
        &missing.to_string_lossy(),
        "--out",
        &out.to_string_lossy(),
        "--parallel",
        "4",
    ]);
    assert_eq!(run.status.code(), Some(1));
    assert!(stderr(&run).contains("does-not-exist.out"));
    assert!(
        !out.exists(),
        "nothing is written when an input cannot be read"
    );
}

// rq-27e5a6b2
#[test]
fn a_zero_worker_count_is_a_usage_error() {
    let dir = temp_dir();
    let out = dir.join("db.jsonl");
    let run = cjkqc(&[
        &fixture("xtb/1dma.log"),
        "--out",
        &out.to_string_lossy(),
        "--parallel",
        "0",
    ]);
    assert_eq!(run.status.code(), Some(2));
    assert!(
        stderr(&run).contains("--parallel must be at least 1"),
        "{}",
        stderr(&run)
    );
}

// rq-60a23fe4
#[test]
fn merge_ignores_the_worker_count() {
    let dir = temp_dir();
    let a = dir.join("a.jsonl");
    write_jsonl(&a, &[&sample_line("A1", -1.0), &sample_line("A2", -2.0)]);
    let merged = dir.join("merged.jsonl");
    let run = cjkqc(&[
        &a.to_string_lossy(),
        "--out",
        &merged.to_string_lossy(),
        "--parallel",
        "4",
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let records = loaded(&merged);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].name.as_deref(), Some("A1"));
    assert_eq!(records[1].name.as_deref(), Some("A2"));
    assert!(
        stdout(&run).contains("merged 2 records into"),
        "{}",
        stdout(&run)
    );
}

// rq-c8e29272
#[test]
fn mixed_inputs_are_refused_identically_in_a_parallel_run() {
    let dir = temp_dir();
    let a = dir.join("a.jsonl");
    write_jsonl(&a, &[&sample_line("A1", -1.0)]);
    let out = dir.join("out.jsonl");
    let run = cjkqc(&[
        &a.to_string_lossy(),
        &fixture("xtb/1dma.log"),
        &fixture("orca/1pinic1sa-110_1_9-str9.out"),
        "--out",
        &out.to_string_lossy(),
        "--parallel",
        "3",
    ]);
    assert_eq!(run.status.code(), Some(2));
    let error = stderr(&run);
    assert!(error.contains("a.jsonl"), "{error}");
    assert!(error.contains("1dma.log"), "{error}");
    assert!(error.contains("1pinic1sa"), "{error}");
    assert!(!out.exists(), "nothing is written when inputs are mixed");
}

// --- open-file limits (rqm/collection.md, Parallel Collection) ---

/// The binary under a lowered process open-file limit: the limit is a
/// property of the process, so the test wraps the invocation in a shell
/// that sets it before the exec. The arguments are absolute paths, passed
/// through verbatim.
fn cjkqc_limited(limit: u32, args: &[&str]) -> Output {
    let script = format!("ulimit -n {limit}; exec \"$0\" \"$@\"");
    Command::new("bash")
        .arg("-c")
        .arg(&script)
        .arg(env!("CARGO_BIN_EXE_CJKQC"))
        .args(args)
        .output()
        .expect("failed to start CJKQC under an open-file limit")
}

/// `count` copies of the plain-xyz fixture under `dir/batch`, their
/// absolute paths in name order.
fn batch_of(dir: &Path, count: usize) -> Vec<String> {
    let batch = dir.join("batch");
    fs::create_dir(&batch).expect("create batch dir");
    for index in 1..=count {
        fs::copy(
            fixture("xyz/h2o.xyz"),
            batch.join(format!("h2o-{index:03}.xyz")),
        )
        .expect("copy fixture");
    }
    let mut paths: Vec<String> = fs::read_dir(&batch)
        .expect("read batch dir")
        .map(|entry| {
            entry
                .expect("batch entry")
                .path()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    paths.sort();
    paths
}

// rq-0c8eff8b
#[test]
fn a_batch_larger_than_the_open_file_limit_collects() {
    let dir = temp_dir();
    let inputs = batch_of(&dir, 600);
    let out = dir.join("db.jsonl");
    let out_text = out.to_string_lossy().into_owned();
    let mut args: Vec<&str> = inputs.iter().map(String::as_str).collect();
    args.extend(["--out", &out_text, "--parallel", "8"]);
    let run = cjkqc_limited(256, &args);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(loaded(&out).len(), 600);
}

// rq-9b736232
#[test]
fn a_mixture_is_reported_over_an_earlier_parse_failure() {
    let dir = temp_dir();
    let junk = dir.join("junk.txt");
    fs::write(&junk, "not a program output\n").expect("write junk");
    let a = dir.join("a.jsonl");
    write_jsonl(&a, &[&sample_line("A1", -1.0)]);
    let out = dir.join("db.jsonl");
    let run = cjkqc(&[
        &junk.to_string_lossy(),
        &a.to_string_lossy(),
        "--out",
        &out.to_string_lossy(),
        "--parallel",
        "2",
    ]);
    assert_eq!(run.status.code(), Some(2));
    let error = stderr(&run);
    assert!(error.contains("cannot mix"), "{error}");
    assert!(error.contains("junk.txt"), "{error}");
    assert!(error.contains("a.jsonl"), "{error}");
    assert!(!out.exists(), "nothing is written for a mixture");
}

// rq-8617d3fb
#[test]
fn an_open_file_limit_failure_names_the_limit() {
    let dir = temp_dir();
    // 128 outputs whose parses overlap: geometry tables large enough that
    // a worker holds its input handle for milliseconds, so 128 workers
    // under a 24-fd limit are mid-task together and the opens fail at
    // the limit.
    let batch = dir.join("batch");
    fs::create_dir(&batch).expect("create batch dir");
    let mut table = String::from("30000\nopen-file limit batch\n");
    for row in 0..30000 {
        table.push_str(&format!("H {row}.0 0.0 0.0\n"));
    }
    let mut inputs = Vec::with_capacity(128);
    for index in 1..=128 {
        let path = batch.join(format!("big-{index:03}.xyz"));
        fs::write(&path, &table).expect("write batch input");
        inputs.push(path.to_string_lossy().into_owned());
    }
    let out = dir.join("db.jsonl");
    let out_text = out.to_string_lossy().into_owned();
    let mut args: Vec<&str> = inputs.iter().map(String::as_str).collect();
    args.extend(["--out", &out_text, "--parallel", "128"]);
    let run = cjkqc_limited(24, &args);
    assert_eq!(run.status.code(), Some(1));
    let error = stderr(&run);
    assert!(error.contains("Too many open files"), "{error}");
    assert!(error.contains("open-file limit"), "{error}");
    assert!(error.contains("workers"), "{error}");
    assert!(!out.exists(), "nothing is written when an open fails");
}

// ---------------------------------------------------------------------------
// Rename scenarios from `rqm/composition.md` and `rqm/filtering.md`.
// ---------------------------------------------------------------------------

#[test] // rq-357f1f14
fn rename_rewrites_a_written_database_compositions() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[comp_line(
            "3sa1am1p",
            None,
            &[("sa", 3), ("am", 1), ("p", 1)],
            RecordStatus::Ok,
        )],
    );
    let out = dir.join("out.jsonl");
    let out_text = out.to_string_lossy().into_owned();
    let run = cjkqc(&[&db, "--rename", "p", "a", "--out", &out_text]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let records = loaded(&out);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].name.as_deref(), Some("3sa1am1p"));
    let expected: Components = [
        ("sa".to_string(), 3),
        ("am".to_string(), 1),
        ("a".to_string(), 1),
    ]
    .into_iter()
    .collect();
    assert_eq!(records[0].components, Some(expected));
}

#[test] // rq-6cc74288
fn repeated_rename_flags_chain_in_command_order() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[comp_line("one-p", None, &[("p", 1)], RecordStatus::Ok)],
    );
    let out = dir.join("out.jsonl");
    let out_text = out.to_string_lossy().into_owned();
    let run = cjkqc(&[
        &db, "--rename", "p", "a", "--rename", "a", "sa", "--out", &out_text,
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let records = loaded(&out);
    let expected: Components = [("sa".to_string(), 1)].into_iter().collect();
    assert_eq!(records[0].components, Some(expected));
}

#[test] // rq-f3a08add
fn renaming_composes_before_extract() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            comp_line("one-p", None, &[("p", 1)], RecordStatus::Ok),
            comp_line("one-a", None, &[("a", 1)], RecordStatus::Ok),
        ],
    );
    let run = cjkqc(&[&db, "--rename", "p", "a", "--extract", "1a", "--filename"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    assert!(text.contains("one-p"), "{text}");
    assert!(text.contains("one-a"), "{text}");
}

#[test] // rq-03690666
fn a_record_without_components_is_unchanged_by_rename() {
    let dir = temp_dir();
    let db = database_in(&dir, &[comp_line("plain", None, &[], RecordStatus::Ok)]);
    let out = dir.join("out.jsonl");
    let out_text = out.to_string_lossy().into_owned();
    let run = cjkqc(&[&db, "--rename", "p", "a", "--out", &out_text]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let records = loaded(&out);
    assert_eq!(records[0].components, None);
}

#[test] // rq-d6684b33
fn the_filename_keeps_its_original_stem_after_a_rename() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[comp_line(
            "3sa1am1p",
            None,
            &[("sa", 3), ("am", 1), ("p", 1)],
            RecordStatus::Ok,
        )],
    );
    let run = cjkqc(&[&db, "--rename", "p", "a", "--filename", "--name"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let table = rows(&text);
    assert_eq!(
        table,
        vec![vec!["filename", "name"], vec!["3sa1am1p", "1a1am3sa"]]
    );
}

#[test] // rq-d8ec51b8
fn an_empty_rename_code_is_a_usage_error() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[comp_line("one-p", None, &[("p", 1)], RecordStatus::Ok)],
    );
    let out = dir.join("out.jsonl");
    let out_text = out.to_string_lossy().into_owned();
    let run = cjkqc(&[&db, "--rename", "", "a", "--out", &out_text]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test] // rq-4bb3deda
fn a_non_letter_rename_code_is_a_usage_error() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[comp_line("one-p", None, &[("p", 1)], RecordStatus::Ok)],
    );
    let out = dir.join("out.jsonl");
    let out_text = out.to_string_lossy().into_owned();
    let run = cjkqc(&[&db, "--rename", "p", "a1", "--out", &out_text]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
}

#[test] // rq-4b48a995
fn a_rename_count_overflow_is_a_runtime_failure() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[comp_line(
            "overflowing",
            None,
            &[("p", 1), ("sa", u32::MAX)],
            RecordStatus::Ok,
        )],
    );
    let out = dir.join("out.jsonl");
    let out_text = out.to_string_lossy().into_owned();
    let run = cjkqc(&[&db, "--rename", "p", "sa", "--out", &out_text]);
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
    assert!(stderr(&run).contains("overflowing"), "{}", stderr(&run));
    assert!(!out.exists(), "nothing is written when a rename overflows");
}

#[test] // rq-7b3a3a9c
fn a_rename_overflow_names_the_offending_record_among_several() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            comp_line("fine", None, &[("p", 1)], RecordStatus::Ok),
            comp_line(
                "overflowing",
                None,
                &[("p", 1), ("sa", u32::MAX)],
                RecordStatus::Ok,
            ),
        ],
    );
    let out = dir.join("out.jsonl");
    let out_text = out.to_string_lossy().into_owned();
    let run = cjkqc(&[&db, "--rename", "p", "sa", "--out", &out_text]);
    assert_eq!(run.status.code(), Some(1), "{}", stderr(&run));
    let error = stderr(&run);
    assert!(error.contains("overflowing"), "{error}");
    assert!(!error.contains("fine"), "{error}");
    assert!(!out.exists(), "nothing is written when a rename overflows");
}

// --- arbalign scenarios (rqm/arbalign.md) ---

fn arbalign_record(name: &str, geometry: Option<Geometry>) -> Record {
    Record {
        name: Some(name.to_string()),
        geometry,
        ..Record::default()
    }
}

#[test] // rq-8ab03978 rq-6a0e38b6
fn arbalign_requires_a_visible_outcome() {
    let dir = temp_dir();
    let db = dir.join("db.jsonl");
    let line = serde_json::to_string(&arbalign_record("a", None)).expect("serialization");
    write_jsonl(&db, &[&line]);
    let db_text = db.to_string_lossy().into_owned();
    let out = cjkqc(&[&db_text, "--arbalign", "0.1"]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(stderr(&out).contains("a manipulation needs --out or at least one print flag"));
}

#[test] // rq-9cfce088
fn an_invalid_arbalign_threshold_is_a_usage_error() {
    let dir = temp_dir();
    let db = dir.join("db.jsonl");
    let line = serde_json::to_string(&arbalign_record("a", None)).expect("serialization");
    write_jsonl(&db, &[&line]);
    let db_text = db.to_string_lossy().into_owned();
    let out = cjkqc(&[&db_text, "--arbalign", "-0.1", "--filename"]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(stderr(&out).contains("negative"));
}

#[test] // rq-4ebb0210
fn arbalign_reports_records_without_usable_geometry() {
    let dir = temp_dir();
    let db = dir.join("db.jsonl");
    let missing = arbalign_record("missing", None);
    let usable = arbalign_record(
        "usable",
        Some(Geometry {
            symbols: vec!["H".to_string(), "H".to_string()],
            coordinates: vec![-0.5, 0.0, 0.0, 0.5, 0.0, 0.0],
        }),
    );
    let missing_line = serde_json::to_string(&missing).expect("serialization");
    let usable_line = serde_json::to_string(&usable).expect("serialization");
    write_jsonl(&db, &[&missing_line, &usable_line]);
    let db_text = db.to_string_lossy().into_owned();
    let out = cjkqc(&[&db_text, "--arbalign", "0.1", "--filename"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("arbalign: kept 1 records without usable geometry"),
        "{}",
        stderr(&out)
    );
}

// --- --align-within scenarios (rqm/arbalign.md, rqm/filtering.md) ---

fn arbalign_geom_line(
    name: Option<&str>,
    energy: Option<f64>,
    comps: Option<&[(&str, u32)]>,
    symbols: &[&str],
    coords: &[f64],
) -> String {
    let mut record = Record {
        name: name.map(str::to_string),
        electronic_energy: energy,
        ..Record::default()
    };
    if let Some(comps) = comps {
        record.components = Some(
            comps
                .iter()
                .map(|(code, count)| ((*code).to_string(), *count))
                .collect(),
        );
    }
    record.geometry = Some(compiled_jkqc::Geometry {
        symbols: symbols.iter().map(|symbol| (*symbol).to_string()).collect(),
        coordinates: coords.to_vec(),
    });
    serde_json::to_string(&record).expect("serialization")
}

fn h2_coordinates(bond: f64) -> Vec<f64> {
    let half = bond / 2.0;
    vec![-half, 0.0, 0.0, half, 0.0, 0.0]
}

#[test] // rq-8ac5fbd3 rq-9e0e3300
fn align_within_without_arbalign_is_a_usage_error() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[arbalign_geom_line(
            Some("a"),
            Some(-1.0),
            None,
            &["H", "H"],
            &h2_coordinates(1.0),
        )],
    );
    let out = cjkqc(&[&db, "--align-within", "el8", "--filename"]);
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("--align-within requires --arbalign"),
        "{}",
        stderr(&out)
    );
}

#[test] // rq-68964eb9
fn align_within_different_energy_subgroups_are_never_compared() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            arbalign_geom_line(
                Some("low"),
                Some(-1.000001),
                None,
                &["H", "H"],
                &h2_coordinates(1.0),
            ),
            arbalign_geom_line(
                Some("high"),
                Some(-1.0),
                None,
                &["H", "H"],
                &h2_coordinates(1.0),
            ),
        ],
    );
    let out = cjkqc(&[
        &db,
        "--align-within",
        "el8",
        "--arbalign",
        "1.0",
        "--filename",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "filename\nlow\nhigh");
}

#[test] // rq-fb0a8b46
fn align_within_same_energy_subgroup_is_filtered_by_rmsd() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            arbalign_geom_line(
                Some("low"),
                Some(-1.000000002),
                None,
                &["H", "H"],
                &h2_coordinates(1.0),
            ),
            arbalign_geom_line(
                Some("high"),
                Some(-1.000000001),
                None,
                &["H", "H"],
                &h2_coordinates(1.0),
            ),
        ],
    );
    let out = cjkqc(&[
        &db,
        "--align-within",
        "el8",
        "--arbalign",
        "0.1",
        "--filename",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "filename\nlow");
}

#[test] // rq-d0949c67
fn align_within_requires_every_selected_key_to_agree() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            arbalign_geom_line(
                Some("a"),
                Some(-1.0),
                None,
                &["H", "H"],
                &h2_coordinates(1.0),
            ),
            arbalign_geom_line(
                Some("b"),
                Some(-1.0),
                None,
                &["H", "H"],
                &h2_coordinates(1.2),
            ),
        ],
    );
    let out = cjkqc(&[
        &db,
        "--align-within",
        "el,rg",
        "--arbalign",
        "1.0",
        "--filename",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "filename\na\nb");
}

#[test] // rq-b827e2d4
fn align_within_dup_compares_only_records_with_the_same_name() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            arbalign_geom_line(
                Some("a"),
                Some(-1.0),
                None,
                &["H", "H"],
                &h2_coordinates(1.0),
            ),
            arbalign_geom_line(
                Some("b"),
                Some(-2.0),
                None,
                &["H", "H"],
                &h2_coordinates(1.0),
            ),
            arbalign_geom_line(
                Some("a"),
                Some(-3.0),
                None,
                &["H", "H"],
                &h2_coordinates(1.0),
            ),
        ],
    );
    let out = cjkqc(&[
        &db,
        "--align-within",
        "dup",
        "--arbalign",
        "0.1",
        "--filename",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "filename\na\nb");
}

#[test] // rq-dd8d9a63
fn align_within_dup_treats_nameless_records_as_singletons() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            arbalign_geom_line(None, Some(-2.0), None, &["H", "H"], &h2_coordinates(1.0)),
            arbalign_geom_line(None, Some(-1.0), None, &["H", "H"], &h2_coordinates(1.0)),
        ],
    );
    let out = cjkqc(&[
        &db,
        "--align-within",
        "dup",
        "--arbalign",
        "0.1",
        "--filename",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stdout(&out).trim().lines().count(), 3, "{}", stdout(&out));
}

#[test] // rq-52a3d5ea
fn align_within_undefined_metrics_are_mutually_equal() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            arbalign_geom_line(Some("a"), None, None, &["H", "H"], &h2_coordinates(1.0)),
            arbalign_geom_line(Some("b"), None, None, &["H", "H"], &h2_coordinates(1.0)),
        ],
    );
    let out = cjkqc(&[
        &db,
        "--align-within",
        "el8",
        "--arbalign",
        "0.1",
        "--filename",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stdout(&out).trim().lines().count(), 2, "{}", stdout(&out));
}

#[test] // rq-d08d83a4
fn align_within_never_crosses_composition_groups() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            arbalign_geom_line(
                Some("a"),
                Some(-1.0),
                Some(&[("x", 1)]),
                &["H", "H"],
                &h2_coordinates(1.0),
            ),
            arbalign_geom_line(
                Some("b"),
                Some(-1.0),
                Some(&[("y", 1)]),
                &["H", "H"],
                &h2_coordinates(1.0),
            ),
        ],
    );
    let out = cjkqc(&[
        &db,
        "--noname",
        "--align-within",
        "el8",
        "--arbalign",
        "10",
        "--filename",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "filename\na\nb");
}

#[test] // rq-98ece9e5
fn align_within_keeps_its_own_flag_value() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            arbalign_geom_line(
                Some("other"),
                Some(-2.0),
                None,
                &["H", "H"],
                &h2_coordinates(1.0),
            ),
            arbalign_geom_line(
                Some("low"),
                Some(-1.000000002),
                None,
                &["H", "H"],
                &h2_coordinates(1.0),
            ),
            arbalign_geom_line(
                Some("high"),
                Some(-1.000000001),
                None,
                &["H", "H"],
                &h2_coordinates(1.0),
            ),
        ],
    );
    let out = cjkqc(&[
        &db,
        "--align-within",
        "el8",
        "--arbalign",
        "0.1",
        "--filename",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "filename\nother\nlow");
}

#[test] // rq-06bb80f9
fn align_within_forms_subgroups_after_the_uniq_survivors() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[
            arbalign_geom_line(
                Some("x"),
                Some(-2.0),
                None,
                &["H", "H"],
                &h2_coordinates(1.0),
            ),
            arbalign_geom_line(
                Some("y"),
                Some(-1.000000002),
                None,
                &["H", "H"],
                &h2_coordinates(1.0),
            ),
            arbalign_geom_line(
                Some("z"),
                Some(-1.000000001),
                None,
                &["H", "H"],
                &h2_coordinates(1.0),
            ),
            arbalign_geom_line(
                Some("x"),
                Some(-1.0),
                None,
                &["H", "H"],
                &h2_coordinates(1.0),
            ),
        ],
    );
    let out = cjkqc(&[
        &db,
        "--uniq",
        "dup",
        "--align-within",
        "el8",
        "--arbalign",
        "0.1",
        "--filename",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "filename\nx\ny");
}
