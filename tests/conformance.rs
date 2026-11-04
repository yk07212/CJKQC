//! Manifest-driven conformance suite over `tests/fixtures/`
//! (`rqm/parsers/conformance.md`): the standard registry must route,
//! status-classify, and extract every declared fixture exactly as the
//! manifest says. Verbatim floats compare bit-for-bit; only observables
//! flagged `converted` (bohr→Å geometry, Debye→a.u. dipole, expected
//! values from an independent source) carry the 1e-8 tolerance.

use std::fs;
use std::path::{Path, PathBuf};

use complied_jkqc::{ParserRegistry, Program, Record, RecordStatus};
use serde_json::Value;

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn manifest() -> Vec<Value> {
    let text = fs::read_to_string(fixtures_dir().join("manifest.json")).expect("manifest");
    serde_json::from_str(&text).expect("manifest is a JSON array")
}

fn tolerance(entry: &Value, observable: &str) -> f64 {
    let converted = entry
        .get("converted")
        .and_then(Value::as_array)
        .map(|names| names.iter().any(|name| name.as_str() == Some(observable)))
        .unwrap_or(false);
    if converted {
        1e-8
    } else {
        0.0
    }
}

fn within(actual: f64, expected: &Value, tol: f64, what: &str) {
    let expected = expected
        .as_f64()
        .unwrap_or_else(|| panic!("{what} expected a number"));
    assert!(
        (actual - expected).abs() <= tol,
        "{what}: {actual} vs expected {expected} (tolerance {tol})"
    );
}

fn expect_float(field: Option<f64>, expected: &Value, tol: f64, what: &str) {
    if expected.is_null() {
        assert!(field.is_none(), "{what} should be absent");
    } else {
        within(
            field.unwrap_or_else(|| panic!("{what} should be present")),
            expected,
            tol,
            what,
        );
    }
}

fn expect_int(field: Option<i64>, expected: &Value, what: &str) {
    if expected.is_null() {
        assert!(field.is_none(), "{what} should be absent");
    } else {
        assert_eq!(
            field,
            Some(expected.as_i64().expect("integer expected")),
            "{what}"
        );
    }
}

fn expect_text<'a>(field: Option<&'a String>, expected: &'a Value, what: &str) {
    if expected.is_null() {
        assert!(field.is_none(), "{what} should be absent");
    } else {
        assert_eq!(field.map(String::as_str), expected.as_str(), "{what}");
    }
}

fn expect_floats(field: Option<&Vec<f64>>, expected: &Value, tol: f64, what: &str) {
    match expected.as_array() {
        Some(values) => {
            let actual = field.unwrap_or_else(|| panic!("{what} should be present"));
            assert_eq!(actual.len(), values.len(), "{what} length");
            for (index, pair) in actual.iter().zip(values).enumerate() {
                within(*pair.0, pair.1, tol, &format!("{what}[{index}]"));
            }
        }
        None => {
            assert!(expected.is_null(), "unexpected {what} expectation");
            assert!(field.is_none(), "{what} should be absent");
        }
    }
}

fn expect_geometry(record: &Record, expected: &Value, tol: f64) {
    if expected.get("atoms").is_some() {
        let atoms = expected["atoms"].as_u64().expect("atoms count");
        let actual = record
            .geometry
            .as_ref()
            .unwrap_or_else(|| panic!("geometry should hold {atoms} atoms"));
        assert_eq!(actual.symbols.len(), atoms as usize, "geometry atom count");
        return;
    }
    if expected.is_null() {
        assert!(record.geometry.is_none(), "geometry should be absent");
        return;
    }
    let actual = record
        .geometry
        .as_ref()
        .expect("geometry should be present");
    let symbols = expected["symbols"].as_array().expect("symbols list");
    assert_eq!(
        actual.symbols,
        symbols
            .iter()
            .map(|symbol| symbol.as_str().expect("symbol string").to_string())
            .collect::<Vec<String>>(),
        "geometry symbols"
    );
    let coordinates = expected["coordinates"]
        .as_array()
        .expect("coordinates list");
    assert_eq!(actual.coordinates.len(), coordinates.len(), "geometry size");
    for (index, pair) in actual.coordinates.iter().zip(coordinates).enumerate() {
        within(
            *pair.0,
            pair.1,
            tol,
            &format!("geometry.coordinates[{index}]"),
        );
    }
}

fn check(entry: &Value) -> Record {
    let file = entry["file"].as_str().expect("file");
    let path = fixtures_dir().join(file);
    let record = ParserRegistry::standard()
        .parse_file(&path)
        .unwrap_or_else(|error| panic!("{file}: {error}"));
    assert_eq!(
        record.program.as_deref(),
        entry["program"].as_str(),
        "{file}: routing"
    );
    let expected_status = match entry["status"].as_str() {
        Some("ok") => RecordStatus::Ok,
        Some("failed") => RecordStatus::Failed,
        Some("truncated") => RecordStatus::Truncated,
        other => panic!("{file}: bad status {other:?}"),
    };
    assert_eq!(record.status, expected_status, "{file}: status");

    let expect = &entry["expect"];
    for (key, field) in [
        ("program_version", record.program_version.as_ref()),
        ("command", record.command.as_ref()),
    ] {
        if let Some(expected) = expect.get(key) {
            expect_text(field, expected, &format!("{file}: {key}"));
        }
    }
    if let Some(expected) = expect.get("charge") {
        expect_int(record.charge.map(i64::from), expected, file);
    }
    if let Some(expected) = expect.get("multiplicity") {
        expect_int(record.multiplicity.map(i64::from), expected, file);
    }
    if let Some(expected) = expect.get("electronic_energy") {
        expect_float(
            record.electronic_energy,
            expected,
            tolerance(entry, "electronic_energy"),
            &format!("{file}: electronic_energy"),
        );
    }
    if let Some(expected) = expect.get("scf_energy") {
        expect_float(
            record.scf_energy,
            expected,
            tolerance(entry, "scf_energy"),
            &format!("{file}: scf_energy"),
        );
    }
    if let Some(expected) = expect.get("geometry") {
        expect_geometry(&record, expected, tolerance(entry, "geometry"));
    }
    if let Some(frequencies) = expect.get("vibrational_frequencies") {
        if let Some(count) = frequencies.get("count").and_then(Value::as_u64) {
            let actual = record
                .vibrational_frequencies
                .as_ref()
                .unwrap_or_else(|| panic!("{file}: frequencies expected"));
            assert_eq!(actual.len() as u64, count, "{file}: frequency count");
        } else {
            expect_floats(
                record.vibrational_frequencies.as_ref(),
                frequencies,
                tolerance(entry, "vibrational_frequencies"),
                &format!("{file}: frequencies"),
            );
        }
    }
    if let Some(dipole) = expect.get("dipole_moment") {
        if dipole.is_null() {
            assert!(
                record.dipole_moment.is_none(),
                "{file}: dipole should be absent"
            );
        } else {
            let actual = record
                .dipole_moment
                .unwrap_or_else(|| panic!("{file}: dipole expected"));
            for (index, value) in actual.iter().enumerate() {
                within(
                    *value,
                    &dipole[index],
                    tolerance(entry, "dipole_moment"),
                    &format!("{file}: dipole[{index}]"),
                );
            }
        }
    }
    if let Some(expected) = expect.get("mulliken_charges") {
        expect_floats(
            record.mulliken_charges.as_ref(),
            expected,
            tolerance(entry, "mulliken_charges"),
            &format!("{file}: mulliken_charges"),
        );
    }
    if let Some(expected) = expect.get("rotational_symmetry_number") {
        expect_int(
            record.rotational_symmetry_number.map(i64::from),
            expected,
            file,
        );
    }
    record
}

// rq-441c138f
#[test]
fn standard_registry_holds_all_five_parsers_in_detection_order() {
    assert_eq!(
        ParserRegistry::standard().programs(),
        vec![
            Program::XTB,
            Program::ORCA,
            Program::GAUSSIAN,
            Program::MRCC,
            Program::XYZ,
        ]
    );
}

// rq-33130099
#[test]
fn every_fixture_file_is_declared_exactly_once() {
    let mut files: Vec<String> = Vec::new();
    let mut stack = vec![fixtures_dir()];
    while let Some(dir) = stack.pop() {
        for item in fs::read_dir(&dir).expect("read fixture dir") {
            let path = item.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name() != Some("manifest.json".as_ref())
                && path.extension().and_then(|ext| ext.to_str()) != Some("pkl")
            {
                files.push(
                    path.strip_prefix(fixtures_dir())
                        .expect("relative")
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    let entries = manifest();
    let declared: Vec<&str> = entries
        .iter()
        .map(|entry| entry["file"].as_str().expect("file"))
        .collect();
    files.sort();
    let mut declared_sorted = declared.clone();
    declared_sorted.sort_unstable();
    let mut deduped = declared_sorted.clone();
    deduped.dedup();
    assert_eq!(
        deduped.len(),
        declared_sorted.len(),
        "duplicate manifest entries"
    );
    assert_eq!(
        files, declared_sorted,
        "the manifest and the corpus must declare the same files"
    );
}

// rq-d2ba6fbf
// Ignored until genuine failed-run fixtures are supplied for every format
// (`rqm/parsers/conformance.md` pins parser failure banners to them).
#[test]
#[ignore = "awaiting genuine failed-run fixtures for each format"]
fn every_format_directory_covers_all_three_outcomes() {
    for program in ["xtb", "orca", "gaussian", "mrcc", "xyz"] {
        for status in ["ok", "failed", "truncated"] {
            assert!(
                manifest().iter().any(|entry| {
                    entry["program"] == program
                        && entry["status"] == status
                        && entry["file"]
                            .as_str()
                            .unwrap()
                            .starts_with(&format!("{program}/"))
                }),
                "{program} has no {status} fixture"
            );
        }
    }
}

// rq-991e31a2
#[test]
fn declared_fixtures_conform() {
    for entry in manifest() {
        check(&entry);
    }
}

// rq-2a0d8029
#[test]
fn gaussian_corpus_fixture_reproduces_the_jkqc_ground_truth() {
    let entry = manifest()
        .into_iter()
        .find(|entry| entry["file"] == "gaussian/2cha-020_1_0-str3.log")
        .expect("gaussian entry");
    let record = check(&entry);
    assert_eq!(record.program.as_deref(), Some("gaussian"));
    assert_eq!(record.status, RecordStatus::Ok);
    assert_eq!(record.electronic_energy, Some(-1526.66937796));
    assert_eq!(record.charge, Some(0));
    assert_eq!(record.multiplicity, Some(1));
}

// rq-646cd2be
#[test]
fn mrcc_corpus_fixture_reproduces_the_jkqc_ground_truth() {
    let entry = manifest()
        .into_iter()
        .find(|entry| entry["file"] == "mrcc/1dma.out")
        .expect("mrcc entry");
    let record = check(&entry);
    assert_eq!(record.electronic_energy, Some(-134.936294839321));
}

// rq-d2239555
#[test]
fn xtb_corpus_fixture_reproduces_the_jkqc_ground_truth() {
    let entry = manifest()
        .into_iter()
        .find(|entry| entry["file"] == "xtb/1dma.log")
        .expect("xtb entry");
    let record = check(&entry);
    assert_eq!(record.electronic_energy, Some(-135.137949826927));
}

// rq-1475e428
#[test]
fn orca_corpus_fixture_reproduces_the_jkqc_ground_truth() {
    let entry = manifest()
        .into_iter()
        .find(|entry| entry["file"] == "orca/1pinic1sa-110_1_9-str9.out")
        .expect("orca entry");
    let record = check(&entry);
    assert_eq!(record.electronic_energy, Some(-199.642065483666));
}

// rq-71a3e979
#[test]
fn an_orca_run_embedding_an_xtb_banner_routes_to_orca() {
    // The GFN2-xTB version banner sits below the ORCA banner in the head,
    // so the earliest-signature rule routes the file to ORCA and the record
    // keeps a geometry (defined thermochemistry).
    let entry = manifest()
        .into_iter()
        .find(|entry| entry["file"] == "orca/1sa1am.out")
        .expect("orca-embedding-xtb entry");
    let record = check(&entry);
    assert_eq!(record.program.as_deref(), Some("orca"));
    assert_eq!(record.status, RecordStatus::Ok);
    assert!(record.geometry.is_some(), "geometry should be parsed");
}

// rq-d56ab087
#[test]
fn companion_xyz_files_route_to_the_plain_xyz_parser() {
    for entry in manifest() {
        let file = entry["file"].as_str().expect("file");
        if file.ends_with(".xyz") && !file.starts_with("xyz/") {
            assert_eq!(entry["program"], "xyz", "{file} declared as xyz");
            let record = check(&entry);
            assert_eq!(record.program.as_deref(), Some("xyz"));
        }
    }
}

// rq-f1754e10
#[test]
fn failed_and_truncated_fixtures_yield_status_records() {
    for entry in manifest() {
        let status = entry["status"].as_str().expect("status");
        if matches!(status, "failed" | "truncated") {
            let record = check(&entry);
            if status == "failed" {
                assert!(
                    record
                        .error
                        .as_deref()
                        .is_some_and(|error| !error.is_empty()),
                    "failed records carry an error"
                );
            }
        }
    }
}
