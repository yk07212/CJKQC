//! Integration tests for the CJKQC command line: the scaffold behavior
//! from `rqm/scaffolding.md` and the collection scenarios from
//! `rqm/collection.md`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use complied_jkqc::db::load_database;
use complied_jkqc::{Components, Correction, Record, RecordStatus};

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
    let run = cjkqc(&[&db, "--extract", "2am3sa", "--name"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "name\nwide");
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
    let run = cjkqc(&[&db, "--nextract", "2am", "--name"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "name\nsasa");
}

#[test] // rq-f43b5dbf
fn an_unparsable_extract_query_is_a_usage_error() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[comp_line("a", None, &[("am", 1)], RecordStatus::Ok)],
    );
    let run = cjkqc(&[&db, "--extract", "2am$", "--name"]);
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
    let run = cjkqc(&[&db, "--noname", "--extract", "2am3sa-1conf", "--name"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "name\n2am3sa-1conf");
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
    let run = cjkqc(&[&db, "--sort", "el", "--name"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "name\na\nb");
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
    let run = cjkqc(&[&db, "--sort", "el", "--name", "--el", "--out", &out_text]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(loaded(&out).len(), 1);
    let text = stdout(&run);
    assert!(text.starts_with("name el\n"), "{text}");
    assert!(text.contains("1dma -38.5100000000"), "{text}");
}

#[test] // rq-acf714ae
fn print_columns_follow_canonical_order_regardless_of_flag_order() {
    let dir = temp_dir();
    let mut record = Record {
        name: Some("unknown".to_string()),
        ..Record::default()
    };
    record.geometry = Some(complied_jkqc::Geometry {
        symbols: vec!["Xx".to_string()],
        coordinates: vec![0.0, 0.0, 0.0],
    });
    let db = database_in(&dir, &[serde_json::to_string(&record).expect("line")]);
    let run = cjkqc(&[&db, "--mass", "--rg", "--name"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "name rg mass\nunknown - -");
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
    assert_eq!(stdout(&run).trim(), "lf\n-300.0");
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
    assert_eq!(stdout(&run).trim(), "lf\n-");
}

#[test] // rq-1f63ac38
fn ct_column_prints_components_without_separators() {
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
    let run = cjkqc(&[&db, "--ct"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "ct\n2am3sa");
}

#[test] // rq-7b0d8978
fn ct_column_is_undefined_without_components() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[serde_json::to_string(&Record::default()).expect("line")],
    );
    let run = cjkqc(&[&db, "--ct"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "ct\n-");
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
    let run = cjkqc(&[&db, "--status", "--name"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "name status\na ok\nb ok\nc failed");
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
fn arbalign_flags_remain_unknown_usage_errors() {
    let dir = temp_dir();
    let db = database_in(
        &dir,
        &[comp_line("a", Some(-1.0), &[("am", 1)], RecordStatus::Ok)],
    );
    let run = cjkqc(&[&db, "--arbalign", "0.38"]);
    assert_eq!(run.status.code(), Some(2), "{}", stderr(&run));
    assert!(stderr(&run).contains("arbalign"), "{}", stderr(&run));
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
    record.geometry = Some(complied_jkqc::Geometry {
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
    let run = cjkqc(&[&db, "--extract", "2am3sa_1conf", "--name"]);
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
    let run = cjkqc(&[&db, "--noname", "--extract", "conf-A", "--name"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "name\nconf-A");
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
    let dup = cjkqc(&[&db, "--uniq", "dup", "--name"]);
    assert_eq!(dup.status.code(), Some(0), "{}", stderr(&dup));
    assert_eq!(stdout(&dup).trim(), "name\nx\ny");
    let keys = cjkqc(&[&db, "--uniq", "el", "--name"]);
    assert_eq!(keys.status.code(), Some(0), "{}", stderr(&keys));
    // No two energies agree at 3 decimals, so uniq keeps all of them in
    // implicit el-ascending order.
    assert_eq!(stdout(&keys).trim(), "name\ny\nx\nx");
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
    let consensus = cjkqc(&[&db, "--reacted", "--name"]);
    assert_eq!(consensus.status.code(), Some(0), "{}", stderr(&consensus));
    assert_eq!(stdout(&consensus).trim(), "name\na\nb");
    let deviants = cjkqc(&[&db, "--reacted2", "--name"]);
    assert_eq!(deviants.status.code(), Some(0), "{}", stderr(&deviants));
    assert_eq!(stdout(&deviants).trim(), "name\nd");
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
    let first = cjkqc(&[&db, "--shuffle", "--seed", "7", "--name"]);
    let second = cjkqc(&[&db, "--shuffle", "--seed", "7", "--name"]);
    assert_eq!(stdout(&first), stdout(&second));
    assert_ne!(stdout(&first).trim(), "name\nn0\nn1\nn2\nn3\nn4");
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
    let selected = cjkqc(&[&db, "--select", "1", "--name"]);
    assert_eq!(selected.status.code(), Some(0), "{}", stderr(&selected));
    // Implicit el-ascending sort puts s (-9.0) before a (-3.0); each group
    // contributes its first record.
    assert_eq!(stdout(&selected).trim(), "name\ns\na");
    let reversed = cjkqc(&[&db, "--sort", "el", "--reverse", "--name"]);
    assert_eq!(reversed.status.code(), Some(0), "{}", stderr(&reversed));
    assert_eq!(stdout(&reversed).trim(), "name\nb\na\ns");
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
    let pass = cjkqc(&[&db, "--pass", "natoms", "2", "--name"]);
    assert_eq!(pass.status.code(), Some(0), "{}", stderr(&pass));
    assert_eq!(stdout(&pass).trim(), "name\nbig");

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
    let is_not = cjkqc(&[&freq_db, "--isnot", "nimag", "0", "--name"]);
    assert_eq!(is_not.status.code(), Some(0), "{}", stderr(&is_not));
    assert_eq!(stdout(&is_not).trim(), "name\nimag");
    let is_zero = cjkqc(&[&freq_db, "--is", "nimag", "0", "--name"]);
    assert_eq!(is_zero.status.code(), Some(0), "{}", stderr(&is_zero));
    assert_eq!(stdout(&is_zero).trim(), "name\nreal");
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
    let by_rg = cjkqc(&[&db, "--sort", "rg", "--name"]);
    assert_eq!(by_rg.status.code(), Some(0), "{}", stderr(&by_rg));
    assert_eq!(stdout(&by_rg).trim(), "name\ntight\nwide");
    let by_name = cjkqc(&[&db, "--sort", "name", "--name"]);
    assert_eq!(by_name.status.code(), Some(0), "{}", stderr(&by_name));
    assert_eq!(stdout(&by_name).trim(), "name\ntight\nwide");

    let mass_db = database_in(
        &dir,
        &[
            geom_line("carbon", &[("c", 1)], &["C"], &[0.0, 0.0, 0.0]),
            geom_line("hydrogen", &[("h", 1)], &["H"], &[0.0, 0.0, 0.0]),
        ],
    );
    let by_mass = cjkqc(&[&mass_db, "--sort", "mass", "--name"]);
    assert_eq!(by_mass.status.code(), Some(0), "{}", stderr(&by_mass));
    assert_eq!(stdout(&by_mass).trim(), "name\nhydrogen\ncarbon");
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
    let run = cjkqc(&[&db, "--sort", "name", "--name"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "name\n-\nnamed");
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
    let run = cjkqc(&[&db, &program.to_string_lossy(), "--status", "--name"]);
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
    let boundary = format!("{}", 0.001 * complied_jkqc::KCAL_MOL_PER_HARTREE);
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
        let run = cjkqc(&[db, flag, "el", threshold, "--name"]);
        assert_eq!(run.status.code(), Some(0), "{flag}: {}", stderr(&run));
        let want = if expected.is_empty() {
            "name".to_string()
        } else {
            format!("name\n{expected}")
        };
        assert_eq!(stdout(&run).trim(), want, "{flag} {threshold}");
    }
}

// ---------------------------------------------------------------------------
// Milestone 6: thermochemistry and formation scenarios from
// `rqm/thermo.md` and `rqm/formation.md` that are CLI-shaped.
// ---------------------------------------------------------------------------

use complied_jkqc::filter::SortKey;
use complied_jkqc::formation::{collect_monomers, formation_values, ConcConditions};
use complied_jkqc::thermo::{thermochemistry, ThermoConditions, ThermoMemo, E_HARTREE, K_B};
use complied_jkqc::Geometry;

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
        let cells: Vec<&str> = line.split(' ').collect();
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
    let run = cjkqc(&[&db, "--name", "--zpe", "--h", "--s", "--g"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "name zpe h s g\nsaddle - - - -");
}

#[test] // rq-d04cc134
fn a_degenerate_geometry_prints_dashes_not_infinities() {
    let dir = temp_dir();
    let collapsed = Record {
        name: Some("collapsed".to_string()),
        electronic_energy: Some(-75.99),
        geometry: Some(complied_jkqc::Geometry {
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
    let run = cjkqc(&[&db, "--name", "--zpe", "--h", "--s", "--g"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "name zpe h s g\ncollapsed - - - -");
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
            geometry: Some(complied_jkqc::Geometry {
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
    let run = cjkqc(&[&db, "--temp", "1e17", "--glob", "--name"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "name");
    // The same records also print - for g, like any undefined value.
    let printed = cjkqc(&[&db, "--temp", "1e17", "--name", "--g"]);
    assert_eq!(printed.status.code(), Some(0), "{}", stderr(&printed));
    assert_eq!(stdout(&printed).trim(), "name g\nhot -\ncooler -");
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
    let run = cjkqc(&[&db, "--name", "--g", "--makereal"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let row: Vec<&str> = text.trim().lines().nth(1).unwrap().split(' ').collect();
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
    let run = cjkqc(&[&db, "--name", "--nimag", "--g", "--makereal"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let row: Vec<&str> = text.trim().lines().nth(1).unwrap().split(' ').collect();
    assert_eq!(row[0], "saddle");
    assert_ne!(row[1], "-", "g derives under --makereal");
    assert!(row[1].parse::<f64>().is_ok());
    assert_eq!(row[2], "1", "nimag still counts the imaginary mode");
    // The lf cutoff property still sees the raw -300.0, so a window that
    // only admits values below -200 keeps the record.
    let kept = cjkqc(&[&db, "--name", "--filter-lt", "lf", "-200", "--makereal"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&kept));
    assert_eq!(stdout(&kept).trim(), "name\nsaddle");
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
    let run = cjkqc(&[&db, "--name", "--g", "--ignoreimag"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let row: Vec<&str> = text.trim().lines().nth(1).unwrap().split(' ').collect();
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
    let run = cjkqc(&[&db, "--name", "--nimag", "--g", "--ignoreimag"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let row: Vec<&str> = text.trim().lines().nth(1).unwrap().split(' ').collect();
    assert_eq!(row[0], "saddle");
    assert_ne!(row[1], "-", "g derives under --ignoreimag");
    assert!(row[1].parse::<f64>().is_ok());
    assert_eq!(row[2], "1", "nimag still counts the imaginary mode");
    // The lf cutoff property still sees the raw -300.0, so a window that
    // only admits values below -200 keeps the record.
    let kept = cjkqc(&[&db, "--name", "--filter-lt", "lf", "-200", "--ignoreimag"]);
    assert_eq!(kept.status.code(), Some(0), "{}", stderr(&kept));
    assert_eq!(stdout(&kept).trim(), "name\nsaddle");
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
    let run = cjkqc(&[&db, "--temp", "0", "--name"]);
    assert_eq!(run.status.code(), Some(2));
    assert!(stderr(&run).contains("--temp"), "{}", stderr(&run));
}

#[test] // rq-87c46fa4
fn a_negative_reference_frequency_is_a_usage_error() {
    let dir = temp_dir();
    let db = db_with(&dir, "db.jsonl", &[sample_line("A1", -1.0)]);
    let run = cjkqc(&[&db, "--fc", "-100", "--name"]);
    assert_eq!(run.status.code(), Some(2));
    assert!(stderr(&run).contains("--fc"), "{}", stderr(&run));
}

#[test] // rq-91d514c5
fn a_zero_frequency_scale_is_a_usage_error() {
    let dir = temp_dir();
    let db = db_with(&dir, "db.jsonl", &[sample_line("A1", -1.0)]);
    let run = cjkqc(&[&db, "--fscale", "0", "--name"]);
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
    let run = cjkqc(&[&db, "--name", "--g"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "name g\nundef -");
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
    let run = cjkqc(&[&db, "--sort", "g", "--name", "--g"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let text = text.trim();
    assert_eq!(
        text, "name g\ncold -100.5000000000\nwarm -100.0000000000\nempty -",
        "{text}"
    );
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
    let run = cjkqc(&[&db, "--glob", "--name"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "name\nlow\nalone");
}

#[test] // rq-58808413
fn glob_drops_a_group_whose_gibbs_is_all_undefined() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[thermo_record_json("bare", None, &[("am", 2)], None)],
    );
    let run = cjkqc(&[&db, "--glob", "--name"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    assert_eq!(stdout(&run).trim(), "name");
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
    let run = cjkqc(&[&db, "--uniq", "g", "--name"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    // The implicit el sort orders the records before the collapse; the
    // first of the agreeing pair survives, the second drops.
    assert_eq!(stdout(&run).trim(), "name\ndiffers\nagree1");
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
    let per_hartree = complied_jkqc::KCAL_MOL_PER_HARTREE;
    let expected = format!(
        "el zpe h s g\n{:.4} {:.4} {:.4} {:.4} {:.4}",
        -1.5 * per_hartree,
        thermo.zpe * per_hartree,
        thermo.enthalpy * per_hartree,
        thermo.entropy_term * per_hartree,
        thermo.gibbs * per_hartree
    );
    assert_eq!(stdout(&run).trim(), expected);
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
    let expected = format!("g\n{:.10}", thermo.gibbs);
    assert_eq!(stdout(&run).trim(), expected);
}

#[test] // rq-48b6dd65
fn a_value_passed_to_unit_is_a_usage_error() {
    let dir = temp_dir();
    let db = db_with(&dir, "db.jsonl", &[sample_line("A1", -1.0)]);
    let run = cjkqc(&[&db, "--unit", "kcal", "--name"]);
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
    assert_eq!(row[2], expected, "{:?}", row);
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
        "--name",
        "--el",
        "--formation",
        "--out",
        &out_text,
    ]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let text = stdout(&run);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "name el", "the table comes first");
    assert_eq!(lines[4], "ct el g", "the section follows the table");
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
    assert!(stdout(&run).starts_with("ct el g"), "{}", stdout(&run));
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
    assert_eq!(lines[0], "ct el g", "{text}");
    assert!(
        lines[1].starts_with("2am1sa "),
        "the cluster row leads with its composition token: {}",
        lines[1]
    );
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
    assert_eq!(stdout(&run).trim(), "ct el g\n- - -");
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
    assert_eq!(lines[1], "1am1sa - -", "{text}");
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
    assert_eq!(row[2], format!("{:.10}", formation.gibbs.unwrap()));
}

#[test] // rq-9c5b3a40
fn the_section_obeys_the_unit_setting() {
    let dir = temp_dir();
    // A 1am1sa cluster whose formation energy is exactly -0.01 Eh.
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
    let expected = format!("{:.4}", -0.01 * complied_jkqc::KCAL_MOL_PER_HARTREE);
    assert_eq!(row[1], expected, "{:?}", row);
}

#[test] // rq-e7f0ef7b
fn the_g_level_follows_the_run_s_temperature() {
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
    assert_ne!(warm_row[2], hot_row[2], "the g level follows it");
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
        .map(|line| line.split(' ').map(str::to_string).collect())
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
    let cells: Vec<&str> = row.split(' ').collect();
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
    let run = cjkqc(&[&db, "--cutr", "elcorr", "5", "--name"]);
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
    let run = cjkqc(&[&db, "--pass", "elcorr", "0", "--name"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let rows = data_rows(&stdout(&run));
    let names: Vec<&str> = rows.iter().map(|row| row[0].as_str()).collect();
    assert_eq!(names, vec!["corr"], "{}", stdout(&run));

    let run = cjkqc(&[&db, "--isnot", "elcorr", "0", "--name"]);
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
    let run = cjkqc(&[&db, "--sort", "elcorr", "--name"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let rows = data_rows(&stdout(&run));
    let names: Vec<&str> = rows.iter().map(|row| row[0].as_str()).collect();
    assert_eq!(names, vec!["low", "mid", "high"], "{}", stdout(&run));

    let run = cjkqc(&[&db, "--sort", "elcorr", "--name", "--reverse"]);
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
    let run = cjkqc(&[&db, "--sort", "gcorr", "--name"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let rows = data_rows(&stdout(&run));
    let names: Vec<&str> = rows.iter().map(|row| row[0].as_str()).collect();
    assert_eq!(names, vec!["one", "two", "bare"], "{}", stdout(&run));

    let run = cjkqc(&[&db, "--sort", "gcorr", "--name", "--reverse"]);
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
    let run = cjkqc(&[&db, "--cutr", "elcorr", "5", "--name"]);
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
    let raw = text.find("ct el g").expect("raw header");
    let corrected = text.find("ct elcorr gcorr").expect("corrected header");
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
    assert!(stdout(&run).contains("ct elcorr gcorr"), "{}", stdout(&run));
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
    assert!(text.contains("ct elcorr gcorr"), "{text}");
    // The cluster's corrected electronic formation: -167.5 - 2*(-76.5).
    let mut found = false;
    for line in text.lines().skip(1) {
        let cells: Vec<&str> = line.split(' ').collect();
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
        "--name",
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
    match stdout_text.find("ct el g") {
        Some(index) => &stdout_text[index..],
        None => panic!("no formation section in:\n{stdout_text}"),
    }
}

/// The stdout from the corrected formation section's header onward.
fn corrected_section(stdout_text: &str) -> &str {
    match stdout_text.find("ct elcorr gcorr") {
        Some(index) => &stdout_text[index..],
        None => panic!("no corrected formation section in:\n{stdout_text}"),
    }
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
    let g_base: f64 = base_row[2].parse().expect("base gibbs");
    let g_conc: f64 = row[2].parse().expect("conc gibbs");
    let expected = -kbt_eh(298.15) * (100.0_f64 * 1.0e-12).ln();
    assert!(
        (g_conc - g_base - expected).abs() < 1e-9,
        "{}",
        stdout(&run)
    );
    // The electronic level stays at the standard state.
    assert_eq!(row[1], base_row[1], "{}", stdout(&run));
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
    let g_base: f64 = formation_row(&stdout(&base), "2am1sa")[2]
        .parse()
        .expect("base gibbs");
    let g_conc: f64 = formation_row(&stdout(&run), "2am1sa")[2]
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
fn cnt_keeps_a_monomers_formation_free_energy_at_zero() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[thermo_line("sa1", -40.0, &[("sa", 1)], None)],
    );
    let run = cjkqc(&[&db, "--formation", "--cnt", "--conc", "sa", "100ppt"]);
    assert_eq!(run.status.code(), Some(0), "{}", stderr(&run));
    let row = formation_row(&stdout(&run), "1sa");
    assert_eq!(row[2], "0.0000000000", "{}", stdout(&run));
    // Without --cnt the monomer carries the full chemical-potential term.
    let plain = cjkqc(&[&db, "--formation", "--conc", "sa", "100ppt"]);
    assert_eq!(plain.status.code(), Some(0), "{}", stderr(&plain));
    let row = formation_row(&stdout(&plain), "1sa");
    let value: f64 = row[2].parse().expect("monomer gibbs");
    assert!(
        (value - ppt_shift(100.0)).abs() < 1e-9,
        "{}",
        stdout(&plain)
    );
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
    let g_base: f64 = formation_row(&stdout(&base), "2sa")[2]
        .parse()
        .expect("base gibbs");
    let g_conc: f64 = formation_row(&stdout(&run), "2sa")[2]
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
        .map(|line| line.split(' ').map(str::to_string).collect())
        .collect();
    assert_eq!(rows.len(), 2, "{}", stdout(&run));
    let reference: f64 = rows[0][2].parse().expect("reference monomer gibbs");
    assert!((reference - ppt_shift(100.0)).abs() < 1e-9);
    // The cluster: its own thermochemistry is undefined under --conc.
    assert_eq!(rows[1][2], "-", "{}", stdout(&run));
}

#[test] // rq-e8ea3b7f
fn the_corrected_section_shifts_gcorr_the_same_way() {
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
    let g_base: f64 = formation_row(corrected_section(&stdout(&base)), "1am1sa")[2]
        .parse()
        .expect("base gcorr");
    let g_conc: f64 = formation_row(corrected_section(&stdout(&run)), "1am1sa")[2]
        .parse()
        .expect("conc gcorr");
    let expected = -kbt_eh(298.15) * (100.0_f64 * 1.0e-12).ln();
    assert!(
        (g_conc - g_base - expected).abs() < 1e-9,
        "{}",
        stdout(&run)
    );
    // The corrected electronic level stays at the standard state.
    let base_row = formation_row(corrected_section(&stdout(&base)), "1am1sa");
    let row = formation_row(corrected_section(&stdout(&run)), "1am1sa");
    assert_eq!(row[1], base_row[1]);
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
}

#[test] // rq-75ac7572
fn conc_without_a_formation_section_is_an_accepted_no_op() {
    let dir = temp_dir();
    let db = db_with(
        &dir,
        "db.jsonl",
        &[thermo_line("am1", -50.0, &[("am", 1)], None)],
    );
    let plain = cjkqc(&[&db, "--name", "--el"]);
    let with_conc = cjkqc(&[&db, "--name", "--el", "--conc", "sa", "100ppt"]);
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
    let base = cjkqc(&[&db, "--ct", "--g", "--glob", "--formation"]);
    let run = cjkqc(&[
        &db,
        "--ct",
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
    let table_end = |text: &str| text.find("ct el g").expect("formation header");
    assert_eq!(
        &stdout(&base)[..table_end(&stdout(&base))],
        &stdout(&run)[..table_end(&stdout(&run))]
    );
    let g_base: f64 = formation_row(formation_section(&stdout(&base)), "2am1sa")[2]
        .parse()
        .expect("base gibbs");
    let g_conc: f64 = formation_row(formation_section(&stdout(&run)), "2am1sa")[2]
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
        formation_row(&stdout(out), "1am1sa")[2]
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
            formation_row(&stdout(&run), "1sa")[2],
            "-",
            "{value}: {}",
            stdout(&run)
        );
        assert_eq!(
            formation_row(&stdout(&run), "2am1sa")[2],
            "-",
            "{value}: {}",
            stdout(&run)
        );
    }
}
