//! Integration tests for the record schema, one per Gherkin scenario in
//! `rqm/record.md`.

use complied_jkqc::{Components, Geometry, Record, RecordInvalidity, RecordStatus};

fn geometry(symbols: &[&str], coordinates: &[f64]) -> Geometry {
    Geometry {
        symbols: symbols.iter().map(|symbol| symbol.to_string()).collect(),
        coordinates: coordinates.to_vec(),
    }
}

fn components(pairs: &[(&str, u32)]) -> Components {
    pairs
        .iter()
        .map(|(code, count)| (code.to_string(), *count))
        .collect()
}

#[test] // rq-636984f8
fn round_trip_a_record_with_every_observable_set() {
    let record = Record {
        status: RecordStatus::Ok,
        error: None,
        name: Some("2am3sa-1conf".to_string()),
        source_path: Some("xtb/2am3sa-1conf.log".to_string()),
        program: Some("xtb".to_string()),
        program_version: Some("6.7.1".to_string()),
        command: Some("xtb 1dma.xyz --gxtb --acc 0.01".to_string()),
        electronic_energy: Some(-38.51),
        charge: Some(0),
        multiplicity: Some(1),
        geometry: Some(geometry(
            &["N", "H", "H"],
            &[0.0, 0.0, 0.0, 0.0, 0.0, 1.01, 0.5, 0.87, 0.0],
        )),
        vibrational_frequencies: Some(vec![3421.1, 1689.7, 1023.4, -80.2, 1490.3, 1100.9]),
        gradient: Some(vec![0.0; 9]),
        dipole_moment: Some([0.1, 0.2, 0.3]),
        mulliken_charges: Some(vec![-0.3, 0.15, 0.15]),
        rotational_symmetry_number: Some(1),
        components: Some(components(&[("am", 2), ("sa", 3)])),
        correction: None,
    };
    assert!(record.validate().is_ok());
    let line = serde_json::to_string(&record).expect("serialization");
    let deserialized: Record = serde_json::from_str(&line).expect("deserialization");
    assert_eq!(deserialized, record);
}

#[test] // rq-9b8f2e7b
fn minimal_record_serializes_to_exact_canonical_line() {
    let record = Record {
        electronic_energy: Some(-1.5),
        ..Record::default()
    };
    let line = serde_json::to_string(&record).expect("serialization");
    assert_eq!(line, r#"{"status":"ok","electronic_energy":-1.5}"#);
}

#[test] // rq-c8bfb90d
fn absent_observables_are_omitted_never_null() {
    let record = Record::default();
    let line = serde_json::to_string(&record).expect("serialization");
    assert!(!line.contains("dipole_moment"));
    assert!(!line.contains("null"));
}

#[test] // rq-009c7dab
fn geometry_serializes_as_parallel_arrays() {
    let record = Record {
        geometry: Some(geometry(
            &["O", "H", "H"],
            &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        )),
        ..Record::default()
    };
    let line = serde_json::to_string(&record).expect("serialization");
    assert!(
        line.contains(
            r#""geometry":{"symbols":["O","H","H"],"coordinates":[0.0,0.0,0.0,1.0,0.0,0.0,0.0,1.0,0.0]}"#
        )
    );
}

#[test] // rq-d5e85a47
fn unknown_keys_are_ignored_on_read() {
    let record: Record =
        serde_json::from_str(r#"{"status":"ok","electronic_energy":-1.5,"zpe":0.42}"#)
            .expect("deserialization");
    assert_eq!(record.status, RecordStatus::Ok);
    assert_eq!(record.electronic_energy, Some(-1.5));
}

#[test] // rq-f8d87123
fn missing_status_key_reads_as_ok() {
    let record: Record =
        serde_json::from_str(r#"{"electronic_energy":-1.5}"#).expect("deserialization");
    assert_eq!(record.status, RecordStatus::Ok);
}

#[test] // rq-9c420eee
fn unknown_status_value_fails_deserialization() {
    let result = serde_json::from_str::<Record>(r#"{"status":"weird"}"#);
    assert!(result.is_err());
}

#[test] // rq-0d722861
fn validate_rejects_non_finite_energy() {
    let record = Record {
        electronic_energy: Some(f64::NAN),
        ..Record::default()
    };
    assert_eq!(
        record.validate(),
        Err(RecordInvalidity::NonFiniteFloat {
            field: "electronic_energy"
        })
    );
}

#[test] // rq-c4aff078
fn validate_rejects_geometry_coordinates_not_three_per_atom() {
    let record = Record {
        geometry: Some(geometry(&["N", "H"], &[0.0; 5])),
        ..Record::default()
    };
    assert_eq!(
        record.validate(),
        Err(RecordInvalidity::MalformedGeometry {
            symbols: 2,
            coordinates: 5
        })
    );
}

#[test] // rq-d9b83cf2
fn validate_rejects_empty_geometry() {
    let record = Record {
        geometry: Some(geometry(&[], &[])),
        ..Record::default()
    };
    assert_eq!(
        record.validate(),
        Err(RecordInvalidity::MalformedGeometry {
            symbols: 0,
            coordinates: 0
        })
    );
}

#[test] // rq-3a6b7e46
fn validate_rejects_gradient_length_mismatch() {
    let record = Record {
        geometry: Some(geometry(&["O", "H", "H"], &[0.0; 9])),
        gradient: Some(vec![0.0; 8]),
        ..Record::default()
    };
    assert_eq!(
        record.validate(),
        Err(RecordInvalidity::LengthMismatch {
            field: "gradient",
            expected: 9,
            actual: 8
        })
    );
}

#[test] // rq-a0202680
fn validate_rejects_mulliken_charges_mismatch() {
    let record = Record {
        geometry: Some(geometry(&["O", "H", "H"], &[0.0; 9])),
        mulliken_charges: Some(vec![0.0; 2]),
        ..Record::default()
    };
    assert_eq!(
        record.validate(),
        Err(RecordInvalidity::LengthMismatch {
            field: "mulliken_charges",
            expected: 3,
            actual: 2
        })
    );
}

#[test] // rq-ffeb5456
fn validate_accepts_gradient_without_geometry() {
    let record = Record {
        gradient: Some(vec![0.0; 9]),
        ..Record::default()
    };
    assert_eq!(record.validate(), Ok(()));
}

#[test] // rq-f9547ca9
fn validate_rejects_failed_record_without_error() {
    let record = Record {
        status: RecordStatus::Failed,
        ..Record::default()
    };
    assert_eq!(record.validate(), Err(RecordInvalidity::MissingError));
}

#[test] // rq-b2bab408
fn validate_accepts_truncated_record_with_partial_observables() {
    let record = Record {
        status: RecordStatus::Truncated,
        electronic_energy: Some(-38.51),
        ..Record::default()
    };
    assert_eq!(record.validate(), Ok(()));
}

#[test] // rq-26e2fe07
fn validate_accepts_failed_record_with_only_status_and_error() {
    let record = Record {
        status: RecordStatus::Failed,
        error: Some("SCF did not converge".to_string()),
        ..Record::default()
    };
    assert_eq!(record.validate(), Ok(()));
}

#[test] // rq-af1baabc
fn validate_rejects_multiplicity_zero() {
    let record = Record {
        multiplicity: Some(0),
        ..Record::default()
    };
    assert!(matches!(
        record.validate(),
        Err(RecordInvalidity::InvalidScalar {
            field: "multiplicity",
            ..
        })
    ));
}

#[test] // rq-b37a708d
fn components_map_round_trips() {
    let record = Record {
        components: Some(components(&[("am", 2), ("sa", 3)])),
        ..Record::default()
    };
    assert!(record.validate().is_ok());
    let line = serde_json::to_string(&record).expect("serialization");
    let back: Record = serde_json::from_str(&line).expect("deserialization");
    assert_eq!(back, record);
}

#[test] // rq-4501ef81
fn components_serialize_alphabetically_regardless_of_insertion_order() {
    let mut map = Components::new();
    map.insert("sa".to_string(), 3);
    map.insert("am".to_string(), 2);
    let record = Record {
        components: Some(map),
        ..Record::default()
    };
    let line = serde_json::to_string(&record).expect("serialization");
    assert!(line.contains(r#""components":{"am":2,"sa":3}"#));
}

#[test] // rq-2e483977
fn components_read_from_a_json_object() {
    let record: Record = serde_json::from_str(r#"{"status":"ok","components":{"am":2,"sa":3}}"#)
        .expect("deserialization");
    assert_eq!(record.components, Some(components(&[("am", 2), ("sa", 3)])));
}

#[test] // rq-2d06ae4a
fn validate_rejects_empty_components_map() {
    let record = Record {
        components: Some(Components::new()),
        ..Record::default()
    };
    assert!(matches!(
        record.validate(),
        Err(RecordInvalidity::InvalidComponents)
    ));
}

#[test] // rq-219c9bd9
fn validate_rejects_component_code_holding_a_digit() {
    let record = Record {
        components: Some(components(&[("a1", 1)])),
        ..Record::default()
    };
    assert!(matches!(
        record.validate(),
        Err(RecordInvalidity::InvalidComponents)
    ));
}

#[test] // rq-819d16cd
fn validate_rejects_component_count_below_one() {
    let record = Record {
        components: Some(components(&[("am", 0)])),
        ..Record::default()
    };
    assert!(matches!(
        record.validate(),
        Err(RecordInvalidity::InvalidComponents)
    ));
}

#[test] // rq-19cf3c0b
fn failed_record_without_program_is_unrecognized() {
    let record = Record {
        status: RecordStatus::Failed,
        error: Some("unrecognized output format".to_string()),
        ..Record::default()
    };
    assert!(record.is_unrecognized());
}

#[test] // rq-c7356b3e
fn failed_record_from_a_matched_parser_is_not_unrecognized() {
    let record = Record {
        status: RecordStatus::Failed,
        error: Some("SCF did not converge".to_string()),
        program: Some("xtb".to_string()),
        ..Record::default()
    };
    assert!(!record.is_unrecognized());
}

#[test] // rq-5aa5a715
fn ok_record_is_not_unrecognized() {
    assert!(!Record::default().is_unrecognized());
}

// --- correction field (rqm/correction.md) ---

use complied_jkqc::Correction;

fn corrected_record() -> Record {
    Record {
        name: Some("2am3sa-1conf".to_string()),
        electronic_energy: Some(-38.51),
        components: Some(components(&[("am", 2), ("sa", 3)])),
        correction: Some(Correction {
            energy: -76.4,
            program: Some("orca".to_string()),
            program_version: Some("6.0.0".to_string()),
            commands: Some(vec!["! wB97x-3c def2-TZVPP Sp".to_string()]),
            kind: "sp".to_string(),
        }),
        ..Record::default()
    }
}

#[test] // rq-3b0ad5a2
fn a_correction_round_trips() {
    let record = corrected_record();
    assert!(record.validate().is_ok());
    let line = serde_json::to_string(&record).expect("serialization");
    let deserialized: Record = serde_json::from_str(&line).expect("deserialization");
    assert_eq!(deserialized, record);
}

#[test] // rq-5a78fd5d
fn a_correction_object_reads_from_a_json_object() {
    let line = concat!(
        r#"{"status":"ok","name":"1dma","electronic_energy":-76.0,"#,
        r#""correction":{"energy":-76.4,"program":"orca","program_version":"6.0.0","#,
        r#""commands":["! wB97x-3c def2-TZVPP Sp"],"kind":"sp"}}"#
    );
    let record: Record = serde_json::from_str(line).expect("deserialization");
    let correction = record.correction.expect("correction present");
    assert_eq!(correction.energy, -76.4);
    assert_eq!(correction.program.as_deref(), Some("orca"));
    assert_eq!(correction.program_version.as_deref(), Some("6.0.0"));
    assert_eq!(
        correction.commands.as_deref(),
        Some(&["! wB97x-3c def2-TZVPP Sp".to_string()][..])
    );
    assert_eq!(correction.kind, "sp");
}

#[test] // rq-f03405ec
fn validate_rejects_a_non_finite_correction_energy() {
    let mut record = corrected_record();
    record.correction.as_mut().unwrap().energy = f64::NAN;
    assert_eq!(
        record.validate(),
        Err(RecordInvalidity::NonFiniteFloat {
            field: "correction.energy"
        })
    );
}

#[test] // rq-0b4ab1a5
fn validate_rejects_an_empty_correction_kind() {
    let mut record = corrected_record();
    record.correction.as_mut().unwrap().kind = String::new();
    assert_eq!(
        record.validate(),
        Err(RecordInvalidity::InvalidScalar {
            field: "correction.kind",
            reason: "must not be empty".to_string(),
        })
    );
}
