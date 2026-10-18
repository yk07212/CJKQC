//! Integration tests for the CJKQC command line: the scaffold behavior
//! from `rqm/scaffolding.md` and the collection scenarios from
//! `rqm/collection.md`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use complied_jkqc::db::load_database;
use complied_jkqc::{Record, RecordStatus};

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
fn a_repeated_invocation_replaces_the_database_with_a_warning() {
    let dir = temp_dir();
    let out = dir.join("db.jsonl");
    let out_text = out.to_string_lossy().into_owned();
    let first = cjkqc(&[&fixture("xtb/1dma.log"), "--out", &out_text]);
    assert_eq!(first.status.code(), Some(0));
    let second = cjkqc(&[&fixture("xyz/h2o.xyz"), "--out", &out_text]);
    assert_eq!(second.status.code(), Some(0), "{}", stderr(&second));
    let records = loaded(&out);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].program.as_deref(), Some("xyz"));
    let warnings = stderr(&second);
    assert!(warnings.contains("warning"), "{warnings}");
    assert!(warnings.contains("db.jsonl"), "{warnings}");
}

#[test] // rq-503eebe7
fn fout_silences_the_replacement_warning() {
    let dir = temp_dir();
    let out = dir.join("db.jsonl");
    let out_text = out.to_string_lossy().into_owned();
    cjkqc(&[&fixture("xtb/1dma.log"), "--out", &out_text]);
    let second = cjkqc(&[&fixture("xyz/h2o.xyz"), "--out", &out_text, "--fout"]);
    assert_eq!(second.status.code(), Some(0), "{}", stderr(&second));
    assert_eq!(loaded(&out).len(), 1);
    assert!(!stderr(&second).contains("warning"), "{}", stderr(&second));
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
    let run = cjkqc(&[&a.to_string_lossy(), "--out", &old.to_string_lossy()]);
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
