//! Integration tests for the record schema, one per Gherkin scenario in
//! `rqm/record.md`.

use complied_jkqc::{Geometry, Record, RecordInvalidity, RecordStatus};

fn geometry(symbols: &[&str], coordinates: &[f64]) -> Geometry {
    Geometry {
        symbols: symbols.iter().map(|symbol| symbol.to_string()).collect(),
        coordinates: coordinates.to_vec(),
    }
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
        method: Some("GFN2-xTB".to_string()),
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
        serde_json::from_str(r#"{"status":"ok","electronic_energy":-1.5,"components":{"am":2}}"#)
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
