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
use complied_jkqc::formation::{collect_monomers, formation_values};
use complied_jkqc::thermo::{thermochemistry, ThermoConditions, ThermoMemo};
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

fn formation_row(stdout_text: &str, name: &str) -> Vec<String> {
    for line in stdout_text.lines().skip(1) {
        let cells: Vec<&str> = line.split(' ').collect();
        if cells.first() == Some(&name) {
            return cells.iter().map(|cell| cell.to_string()).collect();
        }
    }
    panic!("no formation row for {name} in:\n{stdout_text}");
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
    let row = formation_row(&stdout(&run), "cluster");
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
    assert_eq!(lines[4], "name el g", "the section follows the table");
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
    assert!(stdout(&run).starts_with("name el g"), "{}", stdout(&run));
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
    let row = formation_row(&stdout(&run), "cluster");
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
    assert_eq!(lines[1], "cluster - -", "{text}");
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
    let row = formation_row(&stdout(&run), "cluster");
    assert_ne!(row[1], "-", "{:?}: both references are loaded", row);
    assert_ne!(row[2], "-", "{:?}", row);
    // And the values are the stoichiometric ones.
    let record: Record = serde_json::from_str(&db_line(&db, 0)).expect("record deserializes");
    let references = collect_monomers(&loaded_records(&db), &ThermoMemo::default(), SortKey::El);
    let formation = formation_values(&record, &references, &ThermoMemo::default());
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
    let row = formation_row(&stdout(&run), "cluster");
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
    let warm_row = formation_row(&stdout(&warm), "cluster");
    let hot_row = formation_row(&stdout(&hot), "cluster");
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
