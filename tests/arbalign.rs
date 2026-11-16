//! Integration tests for `rqm/arbalign.md`, one per Gherkin scenario.

use std::fs;
use std::path::Path;

use compiled_jkqc::arbalign::{
    apply_arbalign, arbalign_mask, arbalign_rmsd, parse_arbalign_threshold, prepare_arbalign,
    ArbAlignStructure,
};
use compiled_jkqc::filter::{apply_filters, FilterSpec, Grouping, SortKey};
use compiled_jkqc::metrics::atomic_mass;
use compiled_jkqc::thermo::ThermoMemo;
use compiled_jkqc::{Components, Geometry, ParserRegistry, Record};

fn components(pairs: &[(&str, u32)]) -> Components {
    pairs
        .iter()
        .map(|(code, count)| ((*code).to_string(), *count))
        .collect()
}

fn geometry(symbols: &[&str], coordinates: &[f64]) -> Geometry {
    Geometry {
        symbols: symbols.iter().map(|symbol| (*symbol).to_string()).collect(),
        coordinates: coordinates.to_vec(),
    }
}

fn record(
    name: &str,
    symbols: &[&str],
    coordinates: &[f64],
    energy: Option<f64>,
    comps: Option<Components>,
) -> Record {
    Record {
        name: Some(name.to_string()),
        geometry: Some(geometry(symbols, coordinates)),
        electronic_energy: energy,
        components: comps,
        ..Record::default()
    }
}

fn water(name: &str, energy: Option<f64>) -> Record {
    record(
        name,
        &["O", "H", "H"],
        &[0.0, 0.0, 0.0, 0.96, 0.0, 0.0, -0.24, 0.93, 0.0],
        energy,
        Some(components(&[("w", 1)])),
    )
}

fn h2(name: &str, bond: f64, energy: Option<f64>) -> Record {
    let half = bond / 2.0;
    record(
        name,
        &["H", "H"],
        &[-half, 0.0, 0.0, half, 0.0, 0.0],
        energy,
        Some(components(&[("h", 1)])),
    )
}

fn prepared(record: &Record) -> ArbAlignStructure {
    prepare_arbalign(record).expect("preparable geometry")
}

fn structures(records: &[Record]) -> Vec<Option<ArbAlignStructure>> {
    records.iter().map(prepare_arbalign).collect::<Vec<_>>()
}

fn names(records: &[Record]) -> Vec<String> {
    records
        .iter()
        .map(|record| record.name.clone().expect("named record"))
        .collect()
}

fn run(records: &[Record], spec: &FilterSpec) -> Vec<Record> {
    apply_filters(records, spec, &ThermoMemo::default())
        .expect("pipeline")
        .records
}

fn close(left: f64, right: f64, tolerance: f64) -> bool {
    (left - right).abs() <= tolerance
}

// --- Threshold parsing ---

#[test] // rq-ef412fee
fn a_finite_non_negative_threshold_is_accepted() {
    assert_eq!(parse_arbalign_threshold("0.38"), Ok(0.38));
}

#[test] // rq-15621246
fn zero_is_a_valid_threshold() {
    assert_eq!(parse_arbalign_threshold("0"), Ok(0.0));
}

#[test] // rq-34493b93
fn a_negative_threshold_is_rejected() {
    assert!(parse_arbalign_threshold("-0.1").is_err());
}

#[test] // rq-29d2a817
fn non_finite_and_malformed_thresholds_are_rejected() {
    assert!(parse_arbalign_threshold("NaN").is_err());
    assert!(parse_arbalign_threshold("inf").is_err());
    assert!(parse_arbalign_threshold("abc").is_err());
}

// --- Alignment basics ---

#[test] // rq-48a54c37
fn identical_and_reordered_structures_have_zero_rmsd() {
    let straight = water("a", None);
    let mut reversed = straight.clone();
    reversed.geometry = Some(Geometry {
        symbols: vec!["H".to_string(), "H".to_string(), "O".to_string()],
        coordinates: vec![-0.24, 0.93, 0.0, 0.96, 0.0, 0.0, 0.0, 0.0, 0.0],
    });
    let rmsd = arbalign_rmsd(&prepared(&straight), &prepared(&reversed)).expect("comparable");
    assert!(close(rmsd, 0.0, 1e-9), "rmsd = {rmsd}");
}

#[test] // rq-9e72baf5
fn rotated_and_translated_structures_have_zero_rmsd() {
    let original = water("a", None);
    let rotated = record(
        "b",
        &["O", "H", "H"],
        &[1.2, -3.4, 9.0, 1.2, -4.36, 9.0, 2.13, -3.16, 9.0],
        None,
        Some(components(&[("w", 1)])),
    );
    let rmsd = arbalign_rmsd(&prepared(&original), &prepared(&rotated)).expect("comparable");
    assert!(close(rmsd, 0.0, 1e-9), "rmsd = {rmsd}");
}

#[test] // rq-f85b53e6
fn a_reflected_structure_is_matched_by_the_symmetry_search() {
    let original = water("a", None);
    let mirrored = record(
        "b",
        &["O", "H", "H"],
        &[0.0, 0.0, 0.0, -0.96, 0.0, 0.0, 0.24, 0.93, 0.0],
        None,
        Some(components(&[("w", 1)])),
    );
    let rmsd = arbalign_rmsd(&prepared(&original), &prepared(&mirrored)).expect("comparable");
    assert!(close(rmsd, 0.0, 1e-9), "rmsd = {rmsd}");
}

#[test] // rq-79b6a9ff
fn different_geometries_have_a_positive_rmsd() {
    let short = prepared(&h2("a", 1.0, None));
    let long = prepared(&h2("b", 1.2, None));
    let rmsd = arbalign_rmsd(&short, &long).expect("comparable");
    assert!(rmsd > 0.0, "rmsd = {rmsd}");
}

#[test] // rq-8f0d2e24
fn principal_axis_preprocessing_centers_on_the_center_of_mass() {
    let molecule = record(
        "hcl",
        &["H", "Cl"],
        &[0.0, 0.0, 0.0, 1.2746, 0.0, 0.0],
        None,
        Some(components(&[("m", 1)])),
    );
    let structure = prepared(&molecule);
    let mut weighted = [0.0; 3];
    let mut total = 0.0;
    for (index, symbol) in structure.symbols.iter().enumerate() {
        let mass = atomic_mass(symbol).expect("known element");
        total += mass;
        for (axis, value) in weighted.iter_mut().enumerate() {
            *value += mass * structure.coordinates[index][axis];
        }
    }
    for (axis, value) in weighted.iter().enumerate() {
        assert!(
            (value / total).abs() < 1e-12,
            "center-of-mass component {axis} = {}",
            value / total
        );
    }
}

// --- Comparability and non-preparable records ---

#[test] // rq-95322f48
fn different_element_multisets_are_not_compared() {
    let water = prepared(&water("a", None));
    let hydrogen_sulfide = prepared(&record(
        "b",
        &["S", "H", "H"],
        &[0.0, 0.0, 0.0, 1.34, 0.0, 0.0, -0.34, 1.30, 0.0],
        None,
        Some(components(&[("s", 1)])),
    ));
    assert_eq!(arbalign_rmsd(&water, &hydrogen_sulfide), None);
}

#[test] // rq-8f809f5d
fn a_record_without_geometry_is_left_untouched() {
    let mut missing = Record {
        name: Some("missing".to_string()),
        components: Some(components(&[("w", 1)])),
        ..Record::default()
    };
    let mut records = vec![missing.clone(), water("a", None), water("b", None)];
    let report = apply_arbalign(&mut records, 0.1, 1);
    assert_eq!(report.skipped, 1);
    assert_eq!(names(&records), ["missing", "a"]);
    missing.geometry = None;
    assert!(prepare_arbalign(&missing).is_none());
}

#[test] // rq-71cd97d3
fn an_unknown_element_leaves_a_record_untouched() {
    let unknown = record(
        "x",
        &["Xx", "H"],
        &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        None,
        Some(components(&[("x", 1)])),
    );
    assert!(prepare_arbalign(&unknown).is_none());
}

#[test] // rq-44f6407c
fn a_non_finite_coordinate_leaves_a_record_untouched() {
    let invalid = record(
        "x",
        &["H", "H"],
        &[0.0, 0.0, 0.0, f64::NAN, 0.0, 0.0],
        None,
        Some(components(&[("x", 1)])),
    );
    assert!(prepare_arbalign(&invalid).is_none());
}

// --- Elimination semantics ---

#[test] // rq-bfbfdd2b
fn a_pair_strictly_below_the_threshold_eliminates_the_later_record() {
    let records = [water("a", None), water("b", None)];
    let (mask, report) = arbalign_mask(&structures(&records), 0.1, 1);
    assert_eq!(report.skipped, 0);
    assert_eq!(mask, [true, false]);
}

#[test] // rq-8a55e4b4
fn a_pair_exactly_at_the_threshold_is_kept() {
    let records = [h2("a", 1.0, None), h2("b", 1.2, None)];
    let values = structures(&records);
    let rmsd = arbalign_rmsd(
        values[0].as_ref().expect("prepared"),
        values[1].as_ref().expect("prepared"),
    )
    .expect("comparable");
    assert!(close(rmsd, 0.1, 1e-9), "rmsd = {rmsd}");
    let (mask, _) = arbalign_mask(&values, rmsd, 1);
    assert_eq!(mask, [true, true]);
}

#[test] // rq-daef887e
fn threshold_zero_removes_nothing() {
    let records = [water("a", None), water("b", None)];
    let (mask, _) = arbalign_mask(&structures(&records), 0.0, 1);
    assert_eq!(mask, [true, true]);
}

#[test] // rq-45705445
fn the_first_record_in_order_always_wins() {
    let records = [h2("a", 1.0, None), h2("b", 1.1, None), h2("c", 1.25, None)];
    let (mask, _) = arbalign_mask(&structures(&records), 0.1, 1);
    assert_eq!(mask, [true, false, true]);
}

#[test] // rq-1fbb1d60
fn a_removed_record_cannot_eliminate_another_record() {
    let records = [h2("a", 1.0, None), h2("b", 1.1, None), h2("c", 1.25, None)];
    let values = structures(&records);
    let (mask, _) = arbalign_mask(&values, 0.1, 1);
    assert!(!mask[1]);
    let b = values[1].as_ref().expect("prepared");
    let c = values[2].as_ref().expect("prepared");
    let b_to_c = arbalign_rmsd(b, c).expect("comparable");
    assert!(b_to_c < 0.1, "b would remove c: {b_to_c}");
    assert!(mask[2], "c survives because b was already removed");
}

#[test] // rq-21a7b4fc
fn the_survivor_order_is_the_input_order() {
    let mut records = vec![
        h2("a", 1.0, None),
        h2("b", 1.06, None),
        h2("c", 1.5, None),
        h2("d", 1.7, None),
    ];
    apply_arbalign(&mut records, 0.09, 1);
    assert_eq!(names(&records), ["a", "c", "d"]);
}

// --- Pipeline integration ---

#[test] // rq-36e90144
fn arbalign_uses_the_sorted_order_to_choose_the_representative() {
    let records = [water("high", Some(-1.0)), water("low", Some(-2.0))];
    let spec = FilterSpec {
        sort: Some(SortKey::El),
        arbalign: Some(0.1),
        ..FilterSpec::default()
    };
    assert_eq!(names(&run(&records, &spec)), ["low"]);
}

#[test] // rq-27c77ee9
fn arbalign_triggers_the_implicit_energy_sort() {
    let records = [water("high", Some(-1.0)), water("low", Some(-2.0))];
    let spec = FilterSpec {
        arbalign: Some(0.1),
        ..FilterSpec::default()
    };
    assert_eq!(names(&run(&records, &spec)), ["low"]);
}

#[test] // rq-df401c1b rq-e753e60e
fn arbalign_runs_after_sort_and_after_select() {
    let records = [
        h2("a", 1.0, Some(-3.0)),
        h2("b", 1.05, Some(-2.0)),
        h2("c", 1.5, Some(-1.0)),
    ];
    let spec = FilterSpec {
        sort: Some(SortKey::El),
        select: Some(2),
        arbalign: Some(0.1),
        ..FilterSpec::default()
    };
    assert_eq!(names(&run(&records, &spec)), ["a"]);
}

#[test] // rq-b8a74911
fn arbalign_removes_records_before_glob_chooses_a_minimum() {
    let mut first = h2("high", 1.0, Some(-1.0));
    let mut second = h2("low", 2.0, Some(-2.0));
    first.vibrational_frequencies = Some(vec![4000.0]);
    second.vibrational_frequencies = Some(vec![4000.0]);
    let spec = FilterSpec {
        arbalign: Some(0.1),
        glob: true,
        ..FilterSpec::default()
    };
    assert_eq!(names(&run(&[first, second], &spec)), ["low"]);
}

#[test] // rq-da57e104
fn different_composition_groups_are_never_compared() {
    let mut water_a = water("water-a", None);
    let mut water_b = water("water-b", None);
    water_a.components = Some(components(&[("a", 1)]));
    water_b.components = Some(components(&[("a", 1)]));
    let mut sulfide_a = record(
        "sulfide-a",
        &["S", "H", "H"],
        &[0.0, 0.0, 0.0, 1.34, 0.0, 0.0, -0.34, 1.30, 0.0],
        None,
        Some(components(&[("b", 1)])),
    );
    let mut sulfide_b = sulfide_a.clone();
    sulfide_a.name = Some("sulfide-a".to_string());
    sulfide_b.name = Some("sulfide-b".to_string());
    let mut records = vec![water_a, water_b, sulfide_a, sulfide_b];
    apply_arbalign(&mut records, 1.0, 1);
    assert_eq!(names(&records), ["water-a", "sulfide-a"]);
}

#[test] // rq-dfca14c2
fn noname_does_not_make_different_compositions_comparable() {
    let water = water("water", Some(-1.0));
    let sulfide = record(
        "sulfide",
        &["S", "H", "H"],
        &[0.0, 0.0, 0.0, 2.0, 0.0, 0.0, -0.5, 1.8, 0.0],
        Some(-2.0),
        None,
    );
    let spec = FilterSpec {
        arbalign: Some(10.0),
        grouping: Grouping::Unit,
        ..FilterSpec::default()
    };
    let mut records = [water, sulfide];
    for record in &mut records {
        record.components = None;
    }
    assert_eq!(names(&run(&records, &spec)), ["sulfide", "water"]);
}

#[test] // rq-d72b5bc0 rq-4ebb0210
fn a_pooled_sweep_leaves_unpreparable_records_untouched() {
    let structures = vec![None, None];
    let (mask, report) = arbalign_mask(&structures, 0.1, 4);
    assert_eq!(mask, [true, true]);
    assert_eq!(report.skipped, 2);
}

#[test] // rq-4ebb0210
fn the_warning_counts_non_preparable_records_once() {
    let missing_a = Record {
        name: Some("missing-a".to_string()),
        components: Some(components(&[("w", 1)])),
        ..Record::default()
    };
    let missing_b = Record {
        name: Some("missing-b".to_string()),
        components: Some(components(&[("w", 1)])),
        ..Record::default()
    };
    let mut records = vec![missing_a, missing_b];
    let report = apply_arbalign(&mut records, 0.1, 1);
    assert_eq!(report.skipped, 2);
    assert_eq!(names(&records), ["missing-a", "missing-b"]);
}

// --- Parity oracle ---

#[test] // rq-975c0536
fn checked_in_corpus_survivor_set_matches_arbaligncpp_reference() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/arbalign_fixtures");
    let mut files: Vec<_> = fs::read_dir(dir.join("corpus"))
        .expect("corpus directory")
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "xyz"))
        .collect();
    files.sort();

    let registry = ParserRegistry::standard();
    let mut records = Vec::new();
    for path in files {
        let mut record = registry.parse_file(&path).expect("parse xyz");
        record.name = Some(
            path.file_name()
                .expect("file name")
                .to_string_lossy()
                .into_owned(),
        );
        records.push(record);
    }

    let reference = fs::read_to_string(dir.join("reference.txt")).expect("reference list");
    let expected: Vec<String> = reference
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();

    // reference.txt was generated with ArbAlignCPP --SORT ... 0.3 --MPA.
    let report = apply_arbalign(&mut records, 0.3, 1);
    assert_eq!(report.skipped, 0);
    assert_eq!(names(&records), expected);
}

// --- Determinism ---

#[test] // rq-d72b5bc0
fn worker_count_does_not_change_survivors() {
    let mut records = Vec::new();
    for index in 0..80 {
        let bond = 1.0 + 0.05 * (index % 8) as f64;
        records.push(h2(&format!("h{index}"), bond, None));
    }
    let values = structures(&records);
    let (sequential, _) = arbalign_mask(&values, 0.12, 1);
    let (parallel, _) = arbalign_mask(&values, 0.12, 4);
    assert_eq!(sequential, parallel);
}

#[test] // rq-72a1a9da
fn repeated_runs_produce_identical_prepared_coordinates() {
    let first = prepared(&water("a", None));
    let second = prepared(&water("a", None));
    assert_eq!(first.coordinates.len(), second.coordinates.len());
    for (left, right) in first.coordinates.iter().zip(&second.coordinates) {
        for axis in 0..3 {
            assert_eq!(left[axis].to_bits(), right[axis].to_bits());
        }
    }
    for axis in 0..3 {
        assert_eq!(first.center[axis].to_bits(), second.center[axis].to_bits());
    }
}
