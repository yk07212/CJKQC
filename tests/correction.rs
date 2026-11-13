//! Integration tests for single-point energy corrections, one per Gherkin
//! scenario of `rqm/correction.md` exercised at the API level. CLI-shaped
//! scenarios (warnings, exit codes, summary lines, table formatting) live
//! in `tests/cli.rs`.

use compiled_jkqc::thermo::{derived_value, thermochemistry, ThermoConditions};
use compiled_jkqc::{
    attach_corrections, corrected_energy, Components, Correction, CorrectionError, Geometry,
    Record, RecordStatus,
};

fn components(pairs: &[(&str, u32)]) -> Components {
    pairs
        .iter()
        .map(|(code, count)| (code.to_string(), *count))
        .collect()
}

/// A nonlinear 6-atom geometry (a planar hexagon): 3N−6 = 12 modes.
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

fn modes(count: usize, value: f64) -> Vec<f64> {
    let mut values = vec![0.0; 6];
    values.resize(6 + count, value);
    values
}

/// An ok record with defined thermochemistry and a `name`.
fn named(name: &str, energy: f64) -> Record {
    Record {
        status: RecordStatus::Ok,
        name: Some(name.to_string()),
        electronic_energy: Some(energy),
        geometry: Some(hexagon()),
        vibrational_frequencies: Some(modes(12, 1000.0)),
        components: Some(components(&[("am", 1)])),
        ..Record::default()
    }
}

/// The same record with an attached single-point correction.
fn corrected(name: &str, energy: f64, correction: f64) -> Record {
    let mut record = named(name, energy);
    record.correction = Some(Correction {
        energy: correction,
        program: Some("orca".to_string()),
        program_version: Some("6.0.0".to_string()),
        commands: Some(vec!["! wB97x-3c def2-TZVPP Sp".to_string()]),
        kind: "sp".to_string(),
    });
    record
}

/// A correction-database record: the candidate side of an attach pass.
fn candidate(name: Option<&str>, energy: Option<f64>, status: RecordStatus) -> Record {
    Record {
        status,
        name: name.map(|name| name.to_string()),
        electronic_energy: energy,
        program: Some("orca".to_string()),
        program_version: Some("6.0.0".to_string()),
        command: Some("! wB97x-3c def2-TZVPP Sp".to_string()),
        ..Record::default()
    }
}

// --- Attachment ---

#[test] // rq-973be061
fn an_attached_correction_carries_its_provenance() {
    let input = named("1am", -38.51);
    let corrections = vec![candidate(Some("1am"), Some(-76.4109), RecordStatus::Ok)];
    let attachment = attach_corrections(std::slice::from_ref(&input), &corrections)
        .expect("one named candidate");
    let enriched = &attachment.records[0];
    let correction = enriched.correction.as_ref().expect("correction attached");
    assert_eq!(correction.energy, -76.4109);
    assert_eq!(correction.program.as_deref(), Some("orca"));
    assert_eq!(correction.program_version.as_deref(), Some("6.0.0"));
    assert_eq!(
        correction.commands.as_deref(),
        Some(&["! wB97x-3c def2-TZVPP Sp".to_string()][..])
    );
    assert_eq!(correction.kind, "sp");
    assert_eq!(enriched.electronic_energy, Some(-38.51));
    assert_eq!(attachment.attached, 1);
    assert_eq!(attachment.skipped, 0);
    assert_eq!(attachment.unmatched, 0);
}

#[test] // rq-6f572182
fn matching_is_exact_and_case_sensitive() {
    let input = named("1am", -38.51);
    let corrections = vec![candidate(Some("1AM"), Some(-76.4109), RecordStatus::Ok)];
    let attachment =
        attach_corrections(std::slice::from_ref(&input), &corrections).expect("unique names");
    assert!(attachment.records[0].correction.is_none());
    assert_eq!(attachment.attached, 0);
    assert_eq!(attachment.unmatched, 1);
}

#[test] // rq-c411edfa
fn every_record_sharing_the_name_receives_the_correction() {
    let mut second = named("1am-b", -38.52);
    second.name = Some("1am".to_string());
    let inputs = [named("1am", -38.51), second];
    let corrections = vec![candidate(Some("1am"), Some(-76.4109), RecordStatus::Ok)];
    let attachment = attach_corrections(&inputs, &corrections).expect("unique names");
    assert_eq!(attachment.attached, 2);
    for record in &attachment.records {
        assert_eq!(
            record.correction.as_ref().expect("attached").energy,
            -76.4109
        );
    }
}

#[test] // rq-c907b696
fn a_record_without_a_name_never_matches() {
    let input = named("1am", -38.51);
    let mut input = input;
    input.name = None;
    let corrections = vec![candidate(Some("1am"), Some(-76.4109), RecordStatus::Ok)];
    let attachment = attach_corrections(std::slice::from_ref(&input), &corrections)
        .expect("unique candidate names");
    assert!(attachment.records[0].correction.is_none());
    assert_eq!(attachment.attached, 0);
    assert_eq!(attachment.unmatched, 1);
}

#[test] // rq-ea21b840
fn a_candidate_without_a_name_is_unmatched() {
    let input = named("1am", -38.51);
    let corrections = vec![candidate(None, Some(-76.4109), RecordStatus::Ok)];
    let attachment =
        attach_corrections(std::slice::from_ref(&input), &corrections).expect("no candidates");
    assert!(attachment.records[0].correction.is_none());
    assert_eq!(attachment.attached, 0);
    assert_eq!(attachment.unmatched, 1);
    assert_eq!(attachment.skipped, 0);
}

#[test] // rq-0627beb4
fn a_new_match_replaces_a_stored_correction() {
    let input = corrected("1am", -38.51, -76.0);
    let corrections = vec![candidate(Some("1am"), Some(-76.4109), RecordStatus::Ok)];
    let attachment = attach_corrections(std::slice::from_ref(&input), &corrections)
        .expect("one named candidate");
    assert_eq!(
        attachment.records[0]
            .correction
            .as_ref()
            .expect("still attached")
            .energy,
        -76.4109
    );
}

#[test] // rq-53c8c5fc
fn an_unmatched_record_keeps_its_stored_correction() {
    let input = corrected("2am", -38.51, -76.0);
    let corrections = vec![candidate(Some("1am"), Some(-76.4109), RecordStatus::Ok)];
    let attachment = attach_corrections(std::slice::from_ref(&input), &corrections)
        .expect("unique candidate names");
    assert_eq!(
        attachment.records[0]
            .correction
            .as_ref()
            .expect("kept")
            .energy,
        -76.0
    );
    assert_eq!(attachment.attached, 0);
    assert_eq!(attachment.unmatched, 1);
}

#[test] // rq-e59918d8
fn a_failed_correction_record_is_skipped() {
    let input = named("1am", -38.51);
    let corrections = vec![candidate(Some("1am"), Some(-76.4109), RecordStatus::Failed)];
    let attachment = attach_corrections(std::slice::from_ref(&input), &corrections)
        .expect("no candidates, no duplicates");
    assert!(attachment.records[0].correction.is_none());
    assert_eq!(attachment.skipped, 1);
    assert_eq!(attachment.attached, 0);
}

#[test] // rq-d599d450
fn a_failed_rerun_does_not_collide_with_its_ok_replacement() {
    let input = named("1am", -38.51);
    let corrections = vec![
        candidate(Some("1am"), Some(-76.0), RecordStatus::Failed),
        candidate(Some("1am"), Some(-76.4109), RecordStatus::Ok),
    ];
    let attachment =
        attach_corrections(std::slice::from_ref(&input), &corrections).expect("one candidate");
    assert_eq!(
        attachment.records[0]
            .correction
            .as_ref()
            .expect("the ok record attaches")
            .energy,
        -76.4109
    );
    assert_eq!(attachment.skipped, 1);
}

#[test] // rq-a83c76ac
fn an_ok_correction_record_without_an_energy_is_skipped() {
    let input = named("1am", -38.51);
    let corrections = vec![candidate(Some("1am"), None, RecordStatus::Ok)];
    let attachment =
        attach_corrections(std::slice::from_ref(&input), &corrections).expect("no candidates");
    assert!(attachment.records[0].correction.is_none());
    assert_eq!(attachment.skipped, 1);
    assert_eq!(attachment.attached, 0);
}

#[test] // rq-e15a3e2d
fn a_correction_record_without_a_command_attaches_without_commands() {
    let input = named("1am", -38.51);
    let mut corrections = candidate(Some("1am"), Some(-76.4109), RecordStatus::Ok);
    corrections.command = None;
    let attachment =
        attach_corrections(std::slice::from_ref(&input), &[corrections]).expect("one candidate");
    let correction = attachment.records[0].correction.as_ref().expect("attached");
    assert_eq!(correction.energy, -76.4109);
    assert_eq!(correction.program.as_deref(), Some("orca"));
    assert_eq!(correction.kind, "sp");
    assert!(correction.commands.is_none());
}

#[test] // rq-74350863
fn duplicate_candidate_names_are_fatal() {
    let input = named("1am", -38.51);
    let corrections = vec![
        candidate(Some("1am"), Some(-76.4109), RecordStatus::Ok),
        candidate(Some("1am"), Some(-76.5), RecordStatus::Ok),
    ];
    let error = attach_corrections(std::slice::from_ref(&input), &corrections)
        .expect_err("duplicate names");
    assert_eq!(
        error,
        CorrectionError::DuplicateName {
            name: "1am".to_string()
        }
    );
}

#[test] // rq-a9dae115
fn attach_corrections_never_mutates_its_input() {
    let input = named("1am", -38.51);
    let corrections = vec![candidate(Some("1am"), Some(-76.4109), RecordStatus::Ok)];
    let before = input.clone();
    let _ = attach_corrections(std::slice::from_ref(&input), &corrections).expect("one candidate");
    assert_eq!(input, before);
}

// --- Corrected values ---

#[test] // rq-04a56097
fn the_corrected_thermochemistry_equals_the_correction_energy_plus_the_raw_thermal_correction() {
    let conditions = ThermoConditions::default();
    let record = corrected("1am", -38.51, -76.4109);
    let raw = thermochemistry(&record, &conditions).expect("defined thermochemistry");
    let el = corrected_energy(&record).expect("attached");
    let hcorr = derived_value("hcorr", &record, &conditions).expect("corrected enthalpy");
    let gcorr = derived_value("gcorr", &record, &conditions).expect("corrected Gibbs");
    let h = derived_value("h", &record, &conditions).expect("raw enthalpy");
    let g = derived_value("g", &record, &conditions).expect("raw Gibbs");
    assert!((hcorr - (el + (h - record.electronic_energy.unwrap()))).abs() < 1e-12);
    assert!((gcorr - (el + (g - record.electronic_energy.unwrap()))).abs() < 1e-12);
    // The corrected values are the corrected derivation's own terms, not
    // shifts of the raw derivation.
    assert!((hcorr - raw.enthalpy).abs() > 1e-6);
}

#[test] // rq-bca9f7b9
fn the_corrected_keys_return_none_without_a_correction() {
    let conditions = ThermoConditions::default();
    let record = named("1am", -38.51);
    assert_eq!(derived_value("elcorr", &record, &conditions), None);
    assert_eq!(derived_value("hcorr", &record, &conditions), None);
    assert_eq!(derived_value("gcorr", &record, &conditions), None);
}

#[test] // rq-5aca0e6b
fn hcorr_and_gcorr_are_undefined_without_defined_thermochemistry() {
    let conditions = ThermoConditions::default();
    let mut record = corrected("1am", -38.51, -76.4109);
    record.vibrational_frequencies = None;
    assert_eq!(derived_value("hcorr", &record, &conditions), None);
    assert_eq!(derived_value("gcorr", &record, &conditions), None);
}

#[test] // rq-8ce96e45
fn the_registry_exposes_the_corrected_keys() {
    let conditions = ThermoConditions::default();
    let record = corrected("1am", -38.51, -76.4109);
    assert_eq!(
        derived_value("elcorr", &record, &conditions),
        Some(-76.4109)
    );
    let thermo = thermochemistry_with_substituted_energy(&record, &conditions);
    assert_eq!(
        derived_value("hcorr", &record, &conditions),
        Some(thermo.enthalpy)
    );
    assert_eq!(
        derived_value("gcorr", &record, &conditions),
        Some(thermo.gibbs)
    );
}

/// The corrected derivation, reached through the public surface by
/// substituting the energy into a rebuilt record.
fn thermochemistry_with_substituted_energy(
    record: &Record,
    conditions: &ThermoConditions,
) -> compiled_jkqc::thermo::Thermo {
    let mut substituted = record.clone();
    substituted.electronic_energy = Some(corrected_energy(record).expect("attached"));
    thermochemistry(&substituted, conditions).expect("defined thermochemistry")
}
