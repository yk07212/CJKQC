//! Library-level integration tests for the collection pipeline:
//! classification and `collect_inputs` over the fixture corpus and
//! temporary databases (see `rqm/collection.md`).

use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use complied_jkqc::db::DbError;
use complied_jkqc::parser::WindowBudget;
use complied_jkqc::{
    classify_inputs, collect_inputs, collect_inputs_with, CollectError, CollectOutcome,
    CollectionMode, Record,
};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

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

fn temp_dir() -> TempDir {
    let dir = std::env::temp_dir().join(format!(
        "cjkqc-collect-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    TempDir(dir)
}

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(relative)
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

fn refs(paths: &[PathBuf]) -> Vec<&Path> {
    paths.iter().map(AsRef::as_ref).collect()
}

// rq-550ff715
#[test]
fn program_outputs_classify_as_build() {
    let inputs = vec![fixture("xtb/1dma.log")];
    assert_eq!(classify_inputs(&refs(&inputs)), Ok(CollectionMode::Build));
}

// rq-550ff715
#[test]
fn databases_classify_as_merge() {
    let dir = temp_dir();
    let db = dir.join("a.jsonl");
    write_jsonl(&db, &[sample_line("A1", -1.0).as_str()]);
    assert_eq!(classify_inputs(&refs(&[db])), Ok(CollectionMode::Merge));
}

// rq-550ff715
#[test]
fn empty_file_classifies_as_program_output() {
    let dir = temp_dir();
    let empty = dir.join("empty.log");
    fs::write(&empty, "").expect("write empty file");
    assert_eq!(classify_inputs(&refs(&[empty])), Ok(CollectionMode::Build));
}

// rq-550ff715
#[test]
fn mixed_inputs_are_reported_with_both_kinds() {
    let dir = temp_dir();
    let db = dir.join("a.jsonl");
    write_jsonl(&db, &[sample_line("A1", -1.0).as_str()]);
    let log = fixture("xtb/1dma.log");
    let error = match classify_inputs(&refs(&[db.clone(), log.clone()])) {
        Err(error @ CollectError::Mixed { .. }) => error,
        other => panic!("expected Mixed, got {other:?}"),
    };
    match error {
        CollectError::Mixed { databases, outputs } => {
            assert_eq!(databases, vec![db]);
            assert_eq!(outputs, vec![log]);
        }
        _ => unreachable!(),
    }
}

// rq-550ff715
#[test]
fn missing_input_fails_classification_with_io() {
    let missing = temp_dir().join("nope.log");
    match classify_inputs(&refs(&[missing])) {
        Err(CollectError::Io { kind, .. }) => {
            assert_eq!(kind, std::io::ErrorKind::NotFound);
        }
        other => panic!("expected Io, got {other:?}"),
    }
}

// rq-9d8725d5
#[test]
fn build_replaces_the_output_and_resolves_compositions() {
    let dir = temp_dir();
    let out = dir.join("db.jsonl");
    let xtb = fixture("xtb/1dma.log");
    let orca = fixture("orca/1pinic1sa-110_1_9-str9.out");

    let outcome = collect_inputs(&refs(&[xtb]), &out).expect("first build");
    assert!(matches!(
        outcome,
        CollectOutcome::Build {
            written: 1,
            failed: 0,
            truncated: 0,
        }
    ));

    // A second run replaces the database rather than appending.
    collect_inputs(&refs(&[orca]), &out).expect("second build");
    let loaded = complied_jkqc::db::load_database(&out).expect("load");
    assert_eq!(loaded.records.len(), 1);
    assert_eq!(
        loaded.records[0].name.as_deref(),
        Some("1pinic1sa-110_1_9-str9")
    );

    // And a two-input run keeps argument order with resolved compositions.
    collect_inputs(
        &refs(&[fixture("xtb/1dma.log"), fixture("xyz/h2o.xyz")]),
        &out,
    )
    .expect("ordered build");
    let loaded = complied_jkqc::db::load_database(&out).expect("load");
    assert_eq!(loaded.records.len(), 2);
    assert_eq!(loaded.records[0].program.as_deref(), Some("xtb"));
    assert_eq!(
        loaded.records[0]
            .components
            .as_ref()
            .and_then(|c| c.get("dma")),
        Some(&1)
    );
    assert_eq!(loaded.records[1].components, None);
}

// rq-9d8725d5 rq-989e6417
#[test]
fn unrecognized_input_aborts_without_writing() {
    let dir = temp_dir();
    let junk = dir.join("junk.txt");
    fs::write(&junk, "not a QC output at all\n").expect("write junk");
    let out = dir.join("db.jsonl");
    match collect_inputs(&refs(&[junk]), &out) {
        Err(CollectError::Unrecognized { .. }) => {}
        other => panic!("expected Unrecognized, got {other:?}"),
    }
    assert!(!out.exists());
}

// rq-9d8725d5 rq-989e6417
#[test]
fn empty_input_aborts_without_writing() {
    let dir = temp_dir();
    let empty = dir.join("empty.log");
    fs::write(&empty, "").expect("write empty file");
    let out = dir.join("db.jsonl");
    match collect_inputs(&refs(&[empty]), &out) {
        Err(CollectError::Unrecognized { .. }) => {}
        other => panic!("expected Unrecognized, got {other:?}"),
    }
    assert!(!out.exists());
}

// rq-9d8725d5
#[test]
fn missing_input_fails_the_build_without_writing() {
    let dir = temp_dir();
    let out = dir.join("db.jsonl");
    let missing = dir.join("nope.log");
    match collect_inputs(&refs(&[missing, fixture("xtb/1dma.log")]), &out) {
        Err(CollectError::Io { kind, .. }) => {
            assert_eq!(kind, std::io::ErrorKind::NotFound);
        }
        other => panic!("expected Io, got {other:?}"),
    }
    assert!(!out.exists());
}

// rq-9d8725d5
#[test]
fn merge_copies_records_verbatim() {
    let dir = temp_dir();
    let a = dir.join("a.jsonl");
    // A name that WOULD resolve: merge must not fill in components.
    write_jsonl(
        &a,
        &[r#"{"status":"ok","name":"2am3sa","electronic_energy":-1.0}"#],
    );
    let out = dir.join("merged.jsonl");
    let outcome = collect_inputs(&refs(&[a]), &out).expect("merge");
    assert!(matches!(outcome, CollectOutcome::Merge { written: 1, .. }));
    let loaded = complied_jkqc::db::load_database(&out).expect("load");
    assert_eq!(loaded.records[0].components, None);
}

// rq-9d8725d5
#[test]
fn merge_reports_skipped_lines_as_problems() {
    let dir = temp_dir();
    let a = dir.join("a.jsonl");
    write_jsonl(&a, &[sample_line("A1", -1.0).as_str(), "not valid json"]);
    let out = dir.join("merged.jsonl");
    let outcome = collect_inputs(&refs(&[a]), &out).expect("merge");
    match outcome {
        CollectOutcome::Merge { written, problems } => {
            assert_eq!(written, 1);
            assert_eq!(problems.len(), 1);
            assert_eq!(problems[0].line, 2);
        }
        other => panic!("expected Merge, got {other:?}"),
    }
}

// rq-9d8725d5
#[test]
fn merge_to_a_missing_parent_directory_fails() {
    let dir = temp_dir();
    let a = dir.join("a.jsonl");
    write_jsonl(&a, &[sample_line("A1", -1.0).as_str()]);
    let out = dir.join("no/such/dir/merged.jsonl");
    match collect_inputs(&refs(&[a]), &out) {
        Err(CollectError::Io { .. }) => {}
        other => panic!("expected Io, got {other:?}"),
    }
}

// rq-9d8725d5
#[test]
fn merge_io_maps_to_collect_io() {
    let dir = temp_dir();
    let missing = dir.join("missing.jsonl");
    let out = dir.join("merged.jsonl");
    match collect_inputs(&refs(&[missing]), &out) {
        Err(CollectError::Io { kind, .. }) => {
            assert_eq!(kind, std::io::ErrorKind::NotFound);
        }
        other => panic!("expected Io, got {other:?}"),
    }
    // Sanity: the same failure is an Io in the database layer.
    let merged = complied_jkqc::db::merge_databases(&[dir.join("missing.jsonl").as_path()], &out);
    assert!(matches!(merged, Err(DbError::Io { .. })));
}

// rq-8918146b
#[test]
fn collect_inputs_with_does_not_consult_the_hook_for_a_new_output() {
    let dir = temp_dir();
    let consulted = Cell::new(0);
    let mut hook = || {
        consulted.set(consulted.get() + 1);
        false
    };

    // Build mode: a fresh output is written even though the hook declines.
    let inputs = vec![fixture("xtb/1dma.log")];
    let fresh = dir.join("fresh.jsonl");
    collect_inputs_with(&refs(&inputs), &fresh, &WindowBudget::default(), &mut hook)
        .expect("build into a fresh output without consulting the hook");
    assert_eq!(consulted.get(), 0);
    assert!(fresh.exists());

    // Merge mode: the same rule.
    let db = dir.join("a.jsonl");
    write_jsonl(&db, &[sample_line("A1", -1.0).as_str()]);
    let merged = dir.join("merged.jsonl");
    collect_inputs_with(&refs(&[db]), &merged, &WindowBudget::default(), &mut hook)
        .expect("merge into a fresh output without consulting the hook");
    assert_eq!(consulted.get(), 0);
    assert!(merged.exists());
}

// rq-3b5861a5
#[test]
fn collect_inputs_with_consults_the_hook_for_an_existing_output() {
    let dir = temp_dir();
    let db = dir.join("a.jsonl");
    write_jsonl(&db, &[sample_line("A1", -1.0).as_str()]);
    let out = dir.join("merged.jsonl");
    write_jsonl(&out, &[sample_line("OLD", 0.0).as_str()]);
    let consulted = Cell::new(0);
    let mut hook = || {
        consulted.set(consulted.get() + 1);
        false
    };
    match collect_inputs_with(&refs(&[db]), &out, &WindowBudget::default(), &mut hook) {
        Err(CollectError::NotConfirmed) => {}
        other => panic!("expected NotConfirmed, got {other:?}"),
    }
    assert_eq!(consulted.get(), 1);
    // The declined write leaves the previous content intact.
    let kept = fs::read_to_string(&out).expect("read declined output");
    assert!(kept.contains("OLD"), "previous content replaced: {kept}");
}
