//! Integration tests for extrapolation corrections, one per Gherkin
//! scenario of `rqm/extrapolation.md` exercised at the API level. CLI-shaped
//! scenarios (invocations, warnings, exit codes, summary lines, columns)
//! live in `tests/cli.rs`.

use std::path::PathBuf;

use compiled_jkqc::{
    blend_energy, extrapolate_corrections, parse_extrapolation, two_point_energy, ExtrapMode,
    ExtrapolationError, ExtrapolationSpec, Record, RecordStatus,
};

/// An ok source candidate: named, with an electronic energy and an
/// optional SCF energy.
fn candidate(name: &str, ee: f64, scf: Option<f64>) -> Record {
    Record {
        status: RecordStatus::Ok,
        name: Some(name.to_string()),
        electronic_energy: Some(ee),
        scf_energy: scf,
        ..Record::default()
    }
}

/// Adds provenance to a candidate.
fn provenance(
    mut record: Record,
    program: Option<&str>,
    version: Option<&str>,
    command: Option<&str>,
) -> Record {
    record.program = program.map(str::to_string);
    record.program_version = version.map(str::to_string);
    record.command = command.map(str::to_string);
    record
}

/// A run record: ok, named, carrying its own energy.
fn named(name: &str, energy: f64) -> Record {
    candidate(name, energy, None)
}

/// A parsed invocation from string values.
fn spec(values: &[&str]) -> ExtrapolationSpec {
    let owned: Vec<String> = values.iter().map(|value| value.to_string()).collect();
    parse_extrapolation(&owned).expect("valid invocation")
}

fn invocation(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

// --- Grammar (rq-0a61e5eb) ---

// rq-0a61e5eb
#[test]
fn parse_extrapolation_returns_the_three_forms() {
    let two_point = spec(&["2p", "low.jsonl", "high.jsonl", "2", "4.3", "2.51"]);
    assert_eq!(
        two_point.mode,
        ExtrapMode::TwoPoint {
            low: PathBuf::from("low.jsonl"),
            high: PathBuf::from("high.jsonl"),
            cardinal: 2,
            alpha: 4.3,
            beta: 2.51,
        }
    );
    let blend = spec(&["2s", "low.jsonl", "high.jsonl", "1.5"]);
    assert_eq!(
        blend.mode,
        ExtrapMode::SettingsBlend {
            low: PathBuf::from("low.jsonl"),
            high: PathBuf::from("high.jsonl"),
            fraction: 1.5,
        }
    );
    let combined = spec(&[
        "2p",
        "lbl.jsonl,lbh.jsonl",
        "hbl.jsonl,hbh.jsonl",
        "2",
        "4.3",
        "2.51",
        "1.5",
    ]);
    assert_eq!(
        combined.mode,
        ExtrapMode::TwoPointBlended {
            low_pair: (PathBuf::from("lbl.jsonl"), PathBuf::from("lbh.jsonl")),
            high_pair: (PathBuf::from("hbl.jsonl"), PathBuf::from("hbh.jsonl")),
            cardinal: 2,
            alpha: 4.3,
            beta: 2.51,
            fraction: 1.5,
        }
    );
}

// rq-fa4b7ff4
#[test]
fn parse_extrapolation_rejects_malformed_invocations() {
    for values in [
        invocation(&["3p", "low.jsonl", "high.jsonl", "2", "4.3", "2.51"]),
        invocation(&["2p", "low.jsonl", "high.jsonl", "2", "4.3"]),
        invocation(&["2s", "low.jsonl", "high.jsonl"]),
        invocation(&["2p", "low.jsonl", "high.jsonl", "0", "4.3", "2.51"]),
        invocation(&["2p", "low.jsonl", "high.jsonl", "2.5", "4.3", "2.51"]),
        invocation(&["2p", "low.jsonl", "high.jsonl", "2", "0", "2.51"]),
        invocation(&["2p", "low.jsonl", "high.jsonl", "2", "4.3", "-1"]),
        invocation(&["2s", "low.jsonl", "high.jsonl", "inf"]),
        invocation(&[]),
    ] {
        let error = parse_extrapolation(&values).expect_err("malformed invocation");
        assert!(
            error.contains("2p") || error.contains("2s") || error.contains("mode"),
            "usage message names the mode: {error}"
        );
    }
}

// --- Formulas ---

// rq-c5ea8cb9
#[test]
fn two_point_energy_matches_the_closed_form() {
    let energy = two_point_energy(-76.0, 0.0, -76.1, -0.05, 2, 4.3, 2.51);
    assert!((energy - -76.21251721253894).abs() < 1e-9, "{energy}");
}

// rq-19257e60
#[test]
fn blend_energy_scales_the_gap() {
    let energy = blend_energy(-76.0, -76.1, 1.5);
    assert!((energy - -76.15).abs() < 1e-12, "{energy}");
}

// rq-4c238015
#[test]
fn a_dft_records_zero_residual_extrapolates_through_the_scf_term() {
    let low = vec![candidate("1am", -76.0, Some(-76.0))];
    let high = vec![candidate("1am", -76.1, Some(-76.1))];
    let extrapolation = extrapolate_corrections(
        &[named("1am", -38.51)],
        &[&low, &high],
        &spec(&["2p", "low.jsonl", "high.jsonl", "2", "4.3", "2.51"]),
    )
    .expect("covered name");
    let correction = extrapolation.records[0]
        .correction
        .as_ref()
        .expect("attached");
    assert!(
        (correction.energy - -76.13421861395366).abs() < 1e-9,
        "{}",
        correction.energy
    );
}

// rq-a4fc7526
#[test]
fn a_non_finite_result_leaves_the_record_uncovered() {
    // ALPHA at the extreme of the accepted range collapses both SCF
    // exponential factors to zero, so the SCF term divides zero by zero.
    let low = vec![candidate("1am", -76.0, Some(-76.0))];
    let high = vec![candidate("1am", -76.1, Some(-76.1))];
    let owned: Vec<String> = ["2p", "low.jsonl", "high.jsonl", "2", "inf", "2.51"]
        .iter()
        .map(|value| value.to_string())
        .collect();
    let extrapolation_spec = parse_extrapolation(&owned).expect("alpha above zero");
    let extrapolation =
        extrapolate_corrections(&[named("1am", -38.51)], &[&low, &high], &extrapolation_spec)
            .expect("no fatal error");
    assert!(extrapolation.records[0].correction.is_none());
    assert_eq!(extrapolation.uncovered, 1);
    assert_eq!(extrapolation.attached, 0);
}

// --- Matching (rq-c68c02b0) ---

// rq-c8ad2edd
#[test]
fn a_source_list_of_the_wrong_length_is_an_error() {
    let source = vec![candidate("1am", -76.0, Some(-76.0))];
    let error = extrapolate_corrections(
        &[named("1am", -38.51)],
        &[&source],
        &spec(&["2p", "low.jsonl", "high.jsonl", "2", "4.3", "2.51"]),
    )
    .expect_err("one slice for a two-database mode");
    assert_eq!(
        error,
        ExtrapolationError::SourceCount {
            expected: 2,
            actual: 1,
        }
    );
    let error = extrapolate_corrections(
        &[named("1am", -38.51)],
        &[&source, &source, &source],
        &spec(&[
            "2p",
            "lbl.jsonl,lbh.jsonl",
            "hbl.jsonl,hbh.jsonl",
            "2",
            "4.3",
            "2.51",
            "1.5",
        ]),
    )
    .expect_err("three slices for a four-database mode");
    assert_eq!(
        error,
        ExtrapolationError::SourceCount {
            expected: 4,
            actual: 3,
        }
    );
}

// rq-fcd67083
#[test]
fn two_point_attaches_a_cbs_correction() {
    let run = named("1am", -38.51);
    let low = vec![provenance(
        candidate("1am", -76.0, Some(-76.0)),
        None,
        None,
        Some("! def2-SVP Sp"),
    )];
    let high = vec![provenance(
        candidate("1am", -76.15, Some(-76.1)),
        Some("orca"),
        Some("6.0.0"),
        Some("! def2-TZVPP Sp"),
    )];
    let extrapolation = extrapolate_corrections(
        &[run],
        &[&low, &high],
        &spec(&["2p", "low.jsonl", "high.jsonl", "2", "4.3", "2.51"]),
    )
    .expect("covered name");
    let correction = extrapolation.records[0]
        .correction
        .as_ref()
        .expect("attached");
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
    // The stored energy is never altered.
    assert_eq!(extrapolation.records[0].electronic_energy, Some(-38.51));
    assert_eq!(extrapolation.attached, 1);
    assert_eq!(extrapolation.uncovered, 0);
}

// rq-8f40cd00
#[test]
fn two_s_attaches_a_cps_correction() {
    let low = vec![candidate("1am", -76.0, None)];
    let high = vec![candidate("1am", -76.1, None)];
    let extrapolation = extrapolate_corrections(
        &[named("1am", -38.51)],
        &[&low, &high],
        &spec(&["2s", "low.jsonl", "high.jsonl", "1.5"]),
    )
    .expect("covered name");
    let correction = extrapolation.records[0]
        .correction
        .as_ref()
        .expect("attached");
    assert!(
        (correction.energy - -76.15).abs() < 1e-12,
        "{}",
        correction.energy
    );
    assert_eq!(correction.kind, "cps");
}

// rq-0f8f7fcd
#[test]
fn the_combined_form_attaches_a_cbs_cps_correction() {
    let lbl = vec![provenance(
        candidate("1am", -76.0, Some(-76.0)),
        None,
        None,
        Some("c1"),
    )];
    let hbl = vec![provenance(
        candidate("1am", -76.15, Some(-76.1)),
        None,
        None,
        Some("c2"),
    )];
    let lbh = vec![provenance(
        candidate("1am", -76.02, Some(-76.0)),
        None,
        None,
        Some("c3"),
    )];
    let hbh = vec![provenance(
        candidate("1am", -76.17, Some(-76.1)),
        Some("orca"),
        Some("6.0.0"),
        Some("c4"),
    )];
    let extrapolation = extrapolate_corrections(
        &[named("1am", -38.51)],
        &[&lbl, &hbl, &lbh, &hbh],
        &spec(&[
            "2p",
            "lbl.jsonl,lbh.jsonl",
            "hbl.jsonl,hbh.jsonl",
            "2",
            "4.3",
            "2.51",
            "1.5",
        ]),
    )
    .expect("covered name");
    let correction = extrapolation.records[0]
        .correction
        .as_ref()
        .expect("attached");
    assert!(
        (correction.energy - -76.24251721253893).abs() < 1e-9,
        "{}",
        correction.energy
    );
    assert_eq!(correction.kind, "cbs-cps");
    // The low-settings pair then the high-settings pair.
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
    // The last contributor's provenance.
    assert_eq!(correction.program.as_deref(), Some("orca"));
    assert_eq!(correction.program_version.as_deref(), Some("6.0.0"));
}

// rq-925967be
#[test]
fn a_name_missing_from_one_source_is_uncovered() {
    let low = vec![candidate("1am", -76.0, Some(-76.0))];
    let high = vec![candidate("2am", -76.1, Some(-76.1))];
    let extrapolation = extrapolate_corrections(
        &[named("1am", -38.51)],
        &[&low, &high],
        &spec(&["2p", "low.jsonl", "high.jsonl", "2", "4.3", "2.51"]),
    )
    .expect("unique names");
    assert!(extrapolation.records[0].correction.is_none());
    assert_eq!(extrapolation.uncovered, 1);
    assert_eq!(extrapolation.attached, 0);
    assert_eq!(extrapolation.unmatched, 1);
}

// rq-0ec3ddc5
#[test]
fn extrapolation_matching_is_exact_and_case_sensitive() {
    let low = vec![candidate("1AM", -76.0, None)];
    let high = vec![candidate("1AM", -76.1, None)];
    let extrapolation = extrapolate_corrections(
        &[named("1am", -38.51)],
        &[&low, &high],
        &spec(&["2s", "low.jsonl", "high.jsonl", "1.5"]),
    )
    .expect("unique names");
    assert!(extrapolation.records[0].correction.is_none());
    assert_eq!(extrapolation.uncovered, 1);
    assert_eq!(extrapolation.unmatched, 2);
}

// rq-41bd180a
#[test]
fn every_run_record_sharing_a_covered_name_receives_the_correction() {
    let low = vec![candidate("1am", -76.0, None)];
    let high = vec![candidate("1am", -76.1, None)];
    let extrapolation = extrapolate_corrections(
        &[named("1am", -38.51), named("1am", -38.52)],
        &[&low, &high],
        &spec(&["2s", "low.jsonl", "high.jsonl", "1.5"]),
    )
    .expect("unique names");
    assert_eq!(extrapolation.attached, 2);
    for record in &extrapolation.records {
        assert!(record.correction.is_some());
    }
}

// rq-7786c735
#[test]
fn a_run_record_without_a_name_never_matches() {
    let low = vec![candidate("1am", -76.0, None)];
    let high = vec![candidate("1am", -76.1, None)];
    let nameless = Record {
        status: RecordStatus::Ok,
        electronic_energy: Some(-38.51),
        ..Record::default()
    };
    let extrapolation = extrapolate_corrections(
        &[nameless],
        &[&low, &high],
        &spec(&["2s", "low.jsonl", "high.jsonl", "1.5"]),
    )
    .expect("unique names");
    assert!(extrapolation.records[0].correction.is_none());
    assert_eq!(extrapolation.uncovered, 1);
    assert_eq!(extrapolation.attached, 0);
}

// rq-73d8ed95
#[test]
fn duplicate_candidate_names_in_one_source_are_fatal() {
    let low = vec![
        candidate("1am", -76.0, Some(-76.0)),
        candidate("1am", -76.01, Some(-76.0)),
    ];
    let high = vec![candidate("1am", -76.1, Some(-76.1))];
    let error = extrapolate_corrections(
        &[named("1am", -38.51)],
        &[&low, &high],
        &spec(&["2p", "low.jsonl", "high.jsonl", "2", "4.3", "2.51"]),
    )
    .expect_err("duplicate names");
    assert_eq!(
        error,
        ExtrapolationError::DuplicateName {
            source: 0,
            name: "1am".to_string(),
        }
    );
}

// rq-ccc8ed3a
#[test]
fn mixed_programs_are_fatal() {
    let low = vec![provenance(
        candidate("1am", -76.0, Some(-76.0)),
        Some("orca"),
        None,
        None,
    )];
    let high = vec![provenance(
        candidate("1am", -76.1, Some(-76.1)),
        Some("mrcc"),
        None,
        None,
    )];
    let error = extrapolate_corrections(
        &[named("1am", -38.51)],
        &[&low, &high],
        &spec(&["2p", "low.jsonl", "high.jsonl", "2", "4.3", "2.51"]),
    )
    .expect_err("mixed programs");
    assert_eq!(
        error,
        ExtrapolationError::ProgramMismatch {
            name: "1am".to_string(),
            source_a: 0,
            source_b: 1,
            program_a: "orca".to_string(),
            program_b: "mrcc".to_string(),
        }
    );
}

// rq-c4c4e1d7
#[test]
fn the_combined_form_rejects_one_mixed_contributor() {
    let orca =
        |ee: f64, scf: f64| provenance(candidate("1am", ee, Some(scf)), Some("orca"), None, None);
    let lbl = vec![orca(-76.0, -76.0)];
    let hbl = vec![orca(-76.15, -76.1)];
    let lbh = vec![orca(-76.02, -76.0)];
    let hbh = vec![provenance(
        candidate("1am", -76.17, Some(-76.1)),
        Some("mrcc"),
        None,
        None,
    )];
    let error = extrapolate_corrections(
        &[named("1am", -38.51)],
        &[&lbl, &hbl, &lbh, &hbh],
        &spec(&[
            "2p",
            "lbl.jsonl,lbh.jsonl",
            "hbl.jsonl,hbh.jsonl",
            "2",
            "4.3",
            "2.51",
            "1.5",
        ]),
    )
    .expect_err("mixed programs");
    assert!(matches!(
        error,
        ExtrapolationError::ProgramMismatch { ref program_a, ref program_b, .. }
            if program_a == "orca" && program_b == "mrcc"
    ));
}

// rq-41d43353
#[test]
fn a_candidate_without_a_program_never_violates() {
    let low = vec![candidate("1am", -76.0, Some(-76.0))];
    let high = vec![provenance(
        candidate("1am", -76.1, Some(-76.1)),
        Some("orca"),
        None,
        None,
    )];
    let extrapolation = extrapolate_corrections(
        &[named("1am", -38.51)],
        &[&low, &high],
        &spec(&["2p", "low.jsonl", "high.jsonl", "2", "4.3", "2.51"]),
    )
    .expect("uniform provenance");
    let correction = extrapolation.records[0]
        .correction
        .as_ref()
        .expect("attached");
    assert_eq!(correction.program.as_deref(), Some("orca"));
}

// rq-5f78f8a6
#[test]
fn a_version_mismatch_is_fatal() {
    let low = vec![provenance(
        candidate("1am", -76.0, Some(-76.0)),
        Some("orca"),
        Some("6.0.0"),
        None,
    )];
    let high = vec![provenance(
        candidate("1am", -76.1, Some(-76.1)),
        Some("orca"),
        Some("6.1.0"),
        None,
    )];
    let error = extrapolate_corrections(
        &[named("1am", -38.51)],
        &[&low, &high],
        &spec(&["2p", "low.jsonl", "high.jsonl", "2", "4.3", "2.51"]),
    )
    .expect_err("mixed versions");
    assert_eq!(
        error,
        ExtrapolationError::VersionMismatch {
            name: "1am".to_string(),
            source_a: 0,
            source_b: 1,
            version_a: "6.0.0".to_string(),
            version_b: "6.1.0".to_string(),
        }
    );
}

// rq-7cb01a45
#[test]
fn matching_program_and_version_pass_the_uniformity_check() {
    let low = vec![provenance(
        candidate("1am", -76.0, Some(-76.0)),
        Some("orca"),
        Some("6.1.0"),
        None,
    )];
    let high = vec![provenance(
        candidate("1am", -76.1, Some(-76.1)),
        Some("orca"),
        Some("6.1.0"),
        None,
    )];
    let extrapolation = extrapolate_corrections(
        &[named("1am", -38.51)],
        &[&low, &high],
        &spec(&["2p", "low.jsonl", "high.jsonl", "2", "4.3", "2.51"]),
    )
    .expect("matching provenance");
    let correction = extrapolation.records[0]
        .correction
        .as_ref()
        .expect("attached");
    assert_eq!(correction.program_version.as_deref(), Some("6.1.0"));
}

// rq-edffc76e
#[test]
fn identical_electronic_energies_are_fatal() {
    let low = vec![candidate("1am", -76.0, Some(-76.0))];
    let high = vec![candidate("1am", -76.0, Some(-76.1))];
    let error = extrapolate_corrections(
        &[named("1am", -38.51)],
        &[&low, &high],
        &spec(&["2p", "low.jsonl", "high.jsonl", "2", "4.3", "2.51"]),
    )
    .expect_err("repeated calculation");
    assert_eq!(
        error,
        ExtrapolationError::DuplicateEnergy {
            name: "1am".to_string(),
            source_a: 0,
            source_b: 1,
        }
    );
}

// rq-62f744ee
#[test]
fn a_shared_scf_energy_inside_the_combined_form_is_not_a_violation() {
    let lbl = vec![candidate("1am", -76.05, Some(-76.0))];
    let lbh = vec![candidate("1am", -76.07, Some(-76.0))];
    let hbl = vec![candidate("1am", -76.15, Some(-76.1))];
    let hbh = vec![candidate("1am", -76.17, Some(-76.1))];
    let extrapolation = extrapolate_corrections(
        &[named("1am", -38.51)],
        &[&lbl, &hbl, &lbh, &hbh],
        &spec(&[
            "2p",
            "lbl.jsonl,lbh.jsonl",
            "hbl.jsonl,hbh.jsonl",
            "2",
            "4.3",
            "2.51",
            "1.5",
        ]),
    )
    .expect("shared SCF energies are legitimate");
    assert!(extrapolation.records[0].correction.is_some());
}

// rq-17e82092
#[test]
fn a_failed_rerun_does_not_collide_with_its_ok_replacement() {
    let low = vec![
        Record {
            status: RecordStatus::Failed,
            name: Some("1am".to_string()),
            ..Record::default()
        },
        candidate("1am", -76.0, Some(-76.0)),
    ];
    let high = vec![candidate("1am", -76.1, Some(-76.1))];
    let extrapolation = extrapolate_corrections(
        &[named("1am", -38.51)],
        &[&low, &high],
        &spec(&["2p", "low.jsonl", "high.jsonl", "2", "4.3", "2.51"]),
    )
    .expect("non-candidates never collide");
    assert!(extrapolation.records[0].correction.is_some());
    assert_eq!(extrapolation.skipped, 1);
}

// rq-69f2f193
#[test]
fn a_candidate_without_an_electronic_energy_is_skipped() {
    let low = vec![Record {
        status: RecordStatus::Ok,
        name: Some("1am".to_string()),
        scf_energy: Some(-76.0),
        ..Record::default()
    }];
    let high = vec![candidate("1am", -76.1, Some(-76.1))];
    let extrapolation = extrapolate_corrections(
        &[named("1am", -38.51)],
        &[&low, &high],
        &spec(&["2p", "low.jsonl", "high.jsonl", "2", "4.3", "2.51"]),
    )
    .expect("unique names");
    assert!(extrapolation.records[0].correction.is_none());
    assert_eq!(extrapolation.skipped, 1);
    assert_eq!(extrapolation.uncovered, 1);
}

// rq-5c26081b
#[test]
fn a_two_point_candidate_without_scf_energy_is_skipped() {
    let low = vec![Record {
        status: RecordStatus::Ok,
        name: Some("1am".to_string()),
        electronic_energy: Some(-76.0),
        ..Record::default()
    }];
    let high = vec![candidate("1am", -76.1, Some(-76.1))];
    let extrapolation = extrapolate_corrections(
        &[named("1am", -38.51)],
        &[&low, &high],
        &spec(&["2p", "low.jsonl", "high.jsonl", "2", "4.3", "2.51"]),
    )
    .expect("unique names");
    assert!(extrapolation.records[0].correction.is_none());
    assert_eq!(extrapolation.skipped, 1);
    // The same holds when the high database's candidate lacks scf_energy.
    let low = vec![candidate("1am", -76.0, Some(-76.0))];
    let high = vec![Record {
        status: RecordStatus::Ok,
        name: Some("1am".to_string()),
        electronic_energy: Some(-76.1),
        ..Record::default()
    }];
    let extrapolation = extrapolate_corrections(
        &[named("1am", -38.51)],
        &[&low, &high],
        &spec(&["2p", "low.jsonl", "high.jsonl", "2", "4.3", "2.51"]),
    )
    .expect("unique names");
    assert!(extrapolation.records[0].correction.is_none());
    assert_eq!(extrapolation.skipped, 1);
}

// rq-b92f7576
#[test]
fn two_s_needs_no_scf_energy() {
    let low = vec![Record {
        status: RecordStatus::Ok,
        name: Some("1am".to_string()),
        electronic_energy: Some(-76.0),
        ..Record::default()
    }];
    let high = vec![Record {
        status: RecordStatus::Ok,
        name: Some("1am".to_string()),
        electronic_energy: Some(-76.1),
        ..Record::default()
    }];
    let extrapolation = extrapolate_corrections(
        &[named("1am", -38.51)],
        &[&low, &high],
        &spec(&["2s", "low.jsonl", "high.jsonl", "1.5"]),
    )
    .expect("candidates without scf_energy");
    assert!(extrapolation.records[0].correction.is_some());
    assert_eq!(extrapolation.skipped, 0);
}

// rq-3bad508e
#[test]
fn candidates_no_run_record_names_are_counted_unmatched() {
    let low = vec![candidate("1am", -76.0, None), candidate("2am", -70.0, None)];
    let high = vec![candidate("1am", -76.1, None), candidate("2am", -70.1, None)];
    let extrapolation = extrapolate_corrections(
        &[named("1am", -38.51)],
        &[&low, &high],
        &spec(&["2s", "low.jsonl", "high.jsonl", "1.5"]),
    )
    .expect("unique names");
    assert!(extrapolation.records[0].correction.is_some());
    assert_eq!(extrapolation.unmatched, 2);
}

// rq-d516398b
#[test]
fn a_match_replaces_a_stored_correction() {
    let mut run = named("1am", -38.51);
    run.correction = Some(compiled_jkqc::Correction {
        energy: -76.0,
        program: None,
        program_version: None,
        commands: None,
        kind: "sp".to_string(),
    });
    let low = vec![candidate("1am", -76.0, None)];
    let high = vec![candidate("1am", -76.1, None)];
    let extrapolation = extrapolate_corrections(
        &[run],
        &[&low, &high],
        &spec(&["2s", "low.jsonl", "high.jsonl", "1.5"]),
    )
    .expect("unique names");
    let correction = extrapolation.records[0]
        .correction
        .as_ref()
        .expect("replaced");
    assert!(
        (correction.energy - -76.15).abs() < 1e-12,
        "{}",
        correction.energy
    );
    assert_eq!(correction.kind, "cps");
}

// rq-54c33c26
#[test]
fn an_uncovered_record_keeps_its_stored_correction() {
    let mut run = named("2am", -38.51);
    run.correction = Some(compiled_jkqc::Correction {
        energy: -76.0,
        program: None,
        program_version: None,
        commands: None,
        kind: "sp".to_string(),
    });
    let low = vec![candidate("1am", -76.0, None)];
    let high = vec![candidate("1am", -76.1, None)];
    let extrapolation = extrapolate_corrections(
        &[run],
        &[&low, &high],
        &spec(&["2s", "low.jsonl", "high.jsonl", "1.5"]),
    )
    .expect("unique names");
    let correction = extrapolation.records[0].correction.as_ref().expect("kept");
    assert_eq!(correction.energy, -76.0);
    assert_eq!(correction.kind, "sp");
    assert_eq!(extrapolation.uncovered, 1);
}

// rq-7de7b92a
#[test]
fn absent_commands_are_omitted() {
    let low = vec![candidate("1am", -76.0, Some(-76.0))];
    let high = vec![candidate("1am", -76.1, Some(-76.1))];
    let extrapolation = extrapolate_corrections(
        &[named("1am", -38.51)],
        &[&low, &high],
        &spec(&["2p", "low.jsonl", "high.jsonl", "2", "4.3", "2.51"]),
    )
    .expect("covered name");
    let correction = extrapolation.records[0]
        .correction
        .as_ref()
        .expect("attached");
    assert!(correction.commands.is_none());
}

// rq-a757e0f8
#[test]
fn extrapolate_corrections_never_mutates_its_input() {
    let run = named("1am", -38.51);
    let low = vec![candidate("1am", -76.0, Some(-76.0))];
    let high = vec![candidate("1am", -76.1, Some(-76.1))];
    let before = run.clone();
    let _ = extrapolate_corrections(
        std::slice::from_ref(&run),
        &[&low, &high],
        &spec(&["2p", "low.jsonl", "high.jsonl", "2", "4.3", "2.51"]),
    )
    .expect("covered name");
    assert_eq!(run, before);
}
