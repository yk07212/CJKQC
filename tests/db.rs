//! Integration tests for the record database, one per Gherkin scenario in
//! `rqm/database.md`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use complied_jkqc::db::{append_records, load_database, merge_databases, DbError};
use complied_jkqc::{Record, RecordInvalidity};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "cjkqc-db-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn sample_record(name: &str, energy: f64) -> Record {
    Record {
        name: Some(name.to_string()),
        electronic_energy: Some(energy),
        ..Record::default()
    }
}

fn line_of(record: &Record) -> String {
    let mut line = serde_json::to_string(record).expect("serialization");
    line.push('\n');
    line
}

fn write_jsonl(path: &Path, records: &[Record]) {
    let content: String = records.iter().map(line_of).collect();
    fs::write(path, content).expect("write database");
}

fn io_error<T>(result: Result<T, DbError>) -> DbError {
    match result {
        Err(error @ DbError::Io { .. }) => error,
        Err(other) => panic!("expected DbError::Io, got {other:?}"),
        Ok(_) => panic!("expected DbError::Io, got Ok"),
    }
}

fn loaded_names(path: &Path) -> Vec<String> {
    load_database(path)
        .expect("load")
        .records
        .into_iter()
        .map(|record| record.name.expect("name"))
        .collect()
}

#[test] // rq-58f8e962
fn load_returns_all_records_in_file_order() {
    let dir = temp_dir();
    let path = dir.join("db.jsonl");
    write_jsonl(
        &path,
        &[
            sample_record("A1", -1.0),
            sample_record("A2", -2.0),
            sample_record("B1", -3.0),
        ],
    );
    let outcome = load_database(&path).expect("load");
    assert_eq!(outcome.records.len(), 3);
    assert_eq!(loaded_names(&path), ["A1", "A2", "B1"]);
    assert!(outcome.problems.is_empty());
    fs::remove_dir_all(&dir).ok();
}

#[test] // rq-f0bf0879
fn empty_database_loads_as_zero_records() {
    let dir = temp_dir();
    let path = dir.join("db.jsonl");
    fs::write(&path, "").expect("write empty file");
    let outcome = load_database(&path).expect("load");
    assert!(outcome.records.is_empty());
    assert!(outcome.problems.is_empty());
    fs::remove_dir_all(&dir).ok();
}

#[test] // rq-cb5c083c
fn blank_lines_are_ignored() {
    let dir = temp_dir();
    let path = dir.join("db.jsonl");
    let content = format!(
        "{}\n{}",
        line_of(&sample_record("A1", -1.0)),
        line_of(&sample_record("A2", -2.0))
    );
    fs::write(&path, content).expect("write database");
    let outcome = load_database(&path).expect("load");
    assert_eq!(outcome.records.len(), 2);
    assert!(outcome.problems.is_empty());
    fs::remove_dir_all(&dir).ok();
}

#[test] // rq-61efc73e
fn loading_a_missing_file_is_an_error() {
    let dir = temp_dir();
    let path = dir.join("missing.jsonl");
    let error = io_error(load_database(&path));
    assert!(matches!(
        error,
        DbError::Io {
            kind: std::io::ErrorKind::NotFound,
            ..
        }
    ));
    fs::remove_dir_all(&dir).ok();
}

#[test] // rq-8f1add9c
fn loading_a_directory_is_an_error() {
    let dir = temp_dir();
    let path = dir.join("somedir");
    fs::create_dir(&path).expect("create directory");
    let error = io_error(load_database(&path));
    assert!(matches!(error, DbError::Io { .. }));
    fs::remove_dir_all(&dir).ok();
}

#[test] // rq-2ff394b0
fn corrupt_line_is_skipped_and_reported() {
    let dir = temp_dir();
    let path = dir.join("db.jsonl");
    let content = format!(
        "{}not json\n{}",
        line_of(&sample_record("A1", -1.0)),
        line_of(&sample_record("A2", -2.0))
    );
    fs::write(&path, content).expect("write database");
    let outcome = load_database(&path).expect("load");
    assert_eq!(loaded_names(&path), ["A1", "A2"]);
    assert_eq!(outcome.problems.len(), 1);
    assert_eq!(outcome.problems[0].line, 2);
    assert!(outcome.problems[0].reason.contains("invalid JSON"));
    fs::remove_dir_all(&dir).ok();
}

#[test] // rq-50bb7e83
fn partially_written_last_line_is_skipped_and_reported() {
    let dir = temp_dir();
    let path = dir.join("db.jsonl");
    let mut content = format!(
        "{}{}",
        line_of(&sample_record("A1", -1.0)),
        line_of(&sample_record("A2", -2.0))
    );
    content.push_str(r#"{"status":"ok","elect"#);
    fs::write(&path, content).expect("write database");
    let outcome = load_database(&path).expect("load");
    assert_eq!(outcome.records.len(), 2);
    assert_eq!(outcome.problems.len(), 1);
    assert_eq!(outcome.problems[0].line, 3);
    fs::remove_dir_all(&dir).ok();
}

#[test] // rq-77c19ef4
fn invalid_utf8_line_is_skipped_and_reported() {
    let dir = temp_dir();
    let path = dir.join("db.jsonl");
    let mut content: Vec<u8> = line_of(&sample_record("A1", -1.0)).into_bytes();
    content.extend_from_slice(b"\xff\xfe broken\n");
    content.extend_from_slice(line_of(&sample_record("A2", -2.0)).as_bytes());
    fs::write(&path, content).expect("write database");
    let outcome = load_database(&path).expect("load");
    assert_eq!(outcome.records.len(), 2);
    assert_eq!(outcome.problems.len(), 1);
    assert_eq!(outcome.problems[0].line, 2);
    fs::remove_dir_all(&dir).ok();
}

#[test] // rq-1736b3f9
fn record_failing_validation_is_skipped_and_reported() {
    let dir = temp_dir();
    let path = dir.join("db.jsonl");
    fs::write(
        &path,
        r#"{"status":"ok","geometry":{"symbols":["H"],"coordinates":[0.0,0.0,0.0]},"gradient":[0.0,0.0]}"#,
    )
    .expect("write database");
    let outcome = load_database(&path).expect("load");
    assert!(outcome.records.is_empty());
    assert_eq!(outcome.problems.len(), 1);
    assert_eq!(outcome.problems[0].line, 1);
    assert!(outcome.problems[0].reason.contains("gradient"));
    fs::remove_dir_all(&dir).ok();
}

#[test] // rq-e4fad868
fn append_creates_missing_database() {
    let dir = temp_dir();
    let path = dir.join("new.jsonl");
    append_records(
        &path,
        &[sample_record("A1", -1.0), sample_record("A2", -2.0)],
    )
    .expect("append");
    let bytes = fs::read(&path).expect("read file");
    assert_eq!(bytes.iter().filter(|byte| **byte == b'\n').count(), 2);
    assert_eq!(bytes.last(), Some(&b'\n'));
    assert_eq!(loaded_names(&path), ["A1", "A2"]);
    fs::remove_dir_all(&dir).ok();
}

#[test] // rq-b1b26ed5
fn append_preserves_existing_content() {
    let dir = temp_dir();
    let path = dir.join("db.jsonl");
    write_jsonl(
        &path,
        &[sample_record("A1", -1.0), sample_record("A2", -2.0)],
    );
    let before = fs::read(&path).expect("read file");
    append_records(&path, &[sample_record("B1", -3.0)]).expect("append");
    let after = fs::read(&path).expect("read file");
    assert!(after.starts_with(&before));
    assert_eq!(after.iter().filter(|byte| **byte == b'\n').count(), 3);
    assert_eq!(loaded_names(&path), ["A1", "A2", "B1"]);
    fs::remove_dir_all(&dir).ok();
}

#[test] // rq-82900eac
fn append_seals_missing_final_newline() {
    let dir = temp_dir();
    let path = dir.join("db.jsonl");
    let original = r#"{"status":"ok","name":"A1"}"#;
    fs::write(&path, original).expect("write database");
    let record = sample_record("B1", -3.0);
    let expected = format!("{original}\n{}", line_of(&record));
    append_records(&path, &[record]).expect("append");
    let after = fs::read_to_string(&path).expect("read file");
    assert_eq!(after, expected);
    fs::remove_dir_all(&dir).ok();
}

#[test] // rq-9f86cf20
fn append_with_invalid_record_writes_nothing() {
    let dir = temp_dir();
    let path = dir.join("db.jsonl");
    write_jsonl(
        &path,
        &[sample_record("A1", -1.0), sample_record("A2", -2.0)],
    );
    let before = fs::read(&path).expect("read file");
    let invalid = Record {
        electronic_energy: Some(f64::NAN),
        ..Record::default()
    };
    let error = match append_records(&path, &[sample_record("B1", -3.0), invalid]) {
        Err(error) => error,
        Ok(()) => panic!("expected DbError::InvalidRecord, got Ok"),
    };
    assert_eq!(
        error,
        DbError::InvalidRecord(RecordInvalidity::NonFiniteFloat {
            field: "electronic_energy"
        })
    );
    assert_eq!(fs::read(&path).expect("read file"), before);
    fs::remove_dir_all(&dir).ok();
}

#[test] // rq-1d5bcfda
fn append_to_missing_parent_directory_fails() {
    let dir = temp_dir();
    let path = dir.join("no/such/dir/db.jsonl");
    let error = io_error(append_records(&path, &[sample_record("A1", -1.0)]));
    assert!(matches!(error, DbError::Io { .. }));
    fs::remove_dir_all(&dir).ok();
}

#[test] // rq-b177c1e4
fn appending_zero_records_to_missing_file_creates_nothing() {
    let dir = temp_dir();
    let path = dir.join("new.jsonl");
    append_records(&path, &[]).expect("append");
    assert!(!path.exists());
    fs::remove_dir_all(&dir).ok();
}

#[test] // rq-2bffd61d
fn appended_records_round_trip() {
    let dir = temp_dir();
    let path = dir.join("db.jsonl");
    let records = vec![
        sample_record("A1", -1.5),
        Record {
            status: complied_jkqc::RecordStatus::Failed,
            error: Some("SCF did not converge".to_string()),
            name: Some("A2".to_string()),
            ..Record::default()
        },
    ];
    append_records(&path, &records).expect("append");
    let outcome = load_database(&path).expect("load");
    assert_eq!(outcome.records, records);
    assert!(outcome.problems.is_empty());
    fs::remove_dir_all(&dir).ok();
}

#[test] // rq-95c630bc
fn merge_concatenates_inputs_in_order() {
    let dir = temp_dir();
    let a = dir.join("a.jsonl");
    let b = dir.join("b.jsonl");
    let c = dir.join("c.jsonl");
    write_jsonl(&a, &[sample_record("A1", -1.0), sample_record("A2", -2.0)]);
    write_jsonl(&b, &[sample_record("B1", -3.0)]);
    let a_before = fs::read(&a).expect("read file");
    let b_before = fs::read(&b).expect("read file");
    let outcome = merge_databases(&[&a, &b], &c).expect("merge");
    assert_eq!(outcome.records, 3);
    assert!(outcome.problems.is_empty());
    assert_eq!(loaded_names(&c), ["A1", "A2", "B1"]);
    assert_eq!(fs::read(&a).expect("read file"), a_before);
    assert_eq!(fs::read(&b).expect("read file"), b_before);
    fs::remove_dir_all(&dir).ok();
}

#[test] // rq-b032b41c
fn merge_can_write_onto_an_input_path() {
    let dir = temp_dir();
    let a = dir.join("a.jsonl");
    let b = dir.join("b.jsonl");
    write_jsonl(&a, &[sample_record("A1", -1.0)]);
    write_jsonl(&b, &[sample_record("B1", -3.0)]);
    let b_before = fs::read(&b).expect("read file");
    merge_databases(&[&a, &b], &a).expect("merge");
    assert_eq!(loaded_names(&a), ["A1", "B1"]);
    assert_eq!(fs::read(&b).expect("read file"), b_before);
    fs::remove_dir_all(&dir).ok();
}

#[test] // rq-15bf030c
fn merge_truncates_an_existing_output() {
    let dir = temp_dir();
    let a = dir.join("a.jsonl");
    let old = dir.join("old.jsonl");
    write_jsonl(&a, &[sample_record("A1", -1.0)]);
    write_jsonl(
        &old,
        &[
            sample_record("O1", -1.0),
            sample_record("O2", -2.0),
            sample_record("O3", -3.0),
            sample_record("O4", -4.0),
            sample_record("O5", -5.0),
        ],
    );
    merge_databases(&[&a], &old).expect("merge");
    assert_eq!(loaded_names(&old), ["A1"]);
    fs::remove_dir_all(&dir).ok();
}

#[test] // rq-bdd60556
fn merge_skips_and_reports_corrupt_lines_in_inputs() {
    let dir = temp_dir();
    let a = dir.join("a.jsonl");
    let b = dir.join("b.jsonl");
    let c = dir.join("c.jsonl");
    let mut a_content = line_of(&sample_record("A1", -1.0));
    a_content.push_str("corrupt\n");
    fs::write(&a, a_content).expect("write database");
    write_jsonl(&b, &[sample_record("B1", -3.0)]);
    let outcome = merge_databases(&[&a, &b], &c).expect("merge");
    assert_eq!(outcome.records, 2);
    assert_eq!(outcome.problems.len(), 1);
    assert_eq!(outcome.problems[0].input, a);
    assert_eq!(outcome.problems[0].line, 2);
    assert!(outcome.problems[0].reason.contains("invalid JSON"));
    assert_eq!(loaded_names(&c), ["A1", "B1"]);
    fs::remove_dir_all(&dir).ok();
}

#[test] // rq-23fa0de7
fn merge_with_missing_input_writes_nothing() {
    let dir = temp_dir();
    let a = dir.join("a.jsonl");
    let missing = dir.join("missing.jsonl");
    let c = dir.join("c.jsonl");
    write_jsonl(&a, &[sample_record("A1", -1.0)]);
    let error = io_error(merge_databases(&[&a, &missing], &c));
    assert!(matches!(error, DbError::Io { .. }));
    assert!(!c.exists());
    fs::remove_dir_all(&dir).ok();
}

#[test] // rq-62bf0467
fn merge_to_missing_parent_directory_fails() {
    let dir = temp_dir();
    let a = dir.join("a.jsonl");
    let c = dir.join("no/such/dir/c.jsonl");
    write_jsonl(&a, &[sample_record("A1", -1.0)]);
    let error = io_error(merge_databases(&[&a], &c));
    assert!(matches!(error, DbError::Io { .. }));
    fs::remove_dir_all(&dir).ok();
}

#[test] // rq-4dda3d71
fn merging_zero_inputs_creates_empty_output() {
    let dir = temp_dir();
    let c = dir.join("c.jsonl");
    let outcome = merge_databases(&[], &c).expect("merge");
    assert_eq!(outcome.records, 0);
    assert!(outcome.problems.is_empty());
    let metadata = fs::metadata(&c).expect("output exists");
    assert_eq!(metadata.len(), 0);
    fs::remove_dir_all(&dir).ok();
}
