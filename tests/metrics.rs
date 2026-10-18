//! Integration tests for the derived metrics and property uniqueness, one
//! per Gherkin scenario in `rqm/uniqueness.md`.

use complied_jkqc::thermo::{ThermoConditions, ThermoMemo};
use complied_jkqc::{
    atomic_mass, dedup_names, metric_dipole_magnitude, metric_radius_of_gyration,
    metric_total_mass, parse_uniq_keys, unique_by_properties, Geometry, Record, UniqKey,
    UniqMetric,
};

fn geom(symbols: &[&str], coordinates: &[f64]) -> Geometry {
    Geometry {
        symbols: symbols.iter().map(|symbol| symbol.to_string()).collect(),
        coordinates: coordinates.to_vec(),
    }
}

fn geometry_record(symbols: &[&str], coordinates: &[f64]) -> Record {
    Record {
        geometry: Some(geom(symbols, coordinates)),
        ..Record::default()
    }
}

fn energy_record(energy: f64) -> Record {
    Record {
        electronic_energy: Some(energy),
        ..Record::default()
    }
}

fn key(metric: UniqMetric, decimals: f64) -> UniqKey {
    UniqKey { metric, decimals }
}

#[test] // rq-a38b5428 rq-d7383be9
fn rg_of_two_atom_system_matches_hand_verified_value() {
    let length = 1.2746;
    let record = geometry_record(&["H", "Cl"], &[0.0, 0.0, 0.0, length, 0.0, 0.0]);
    let hand_verified = {
        let m_h = 1.008f64;
        let m_cl = 35.45f64;
        let center = m_cl * length / (m_h + m_cl);
        ((m_h * center * center + m_cl * (length - center) * (length - center)) / (m_h + m_cl))
            .sqrt()
    };
    let computed = metric_radius_of_gyration(&record).expect("metric defined");
    assert!(
        (computed - hand_verified).abs() < 1e-9,
        "{computed} vs {hand_verified}"
    );
    assert_eq!(atomic_mass("H"), Some(1.008));
    assert_eq!(atomic_mass("Cl"), Some(35.45));
}

#[test] // rq-d59cd7df
fn mass_weighted_rg_differs_from_unweighted_for_mixed_masses() {
    // H2O with an asymmetric layout.
    let record = geometry_record(
        &["O", "H", "H"],
        &[0.0, 0.0, 0.0, 0.97, 0.0, 0.0, 0.30, 0.92, 0.0],
    );
    let weighted = metric_radius_of_gyration(&record).expect("metric defined");
    let unweighted = {
        let points: Vec<[f64; 3]> = (0..3)
            .map(|atom| {
                [
                    record.geometry.as_ref().unwrap().coordinates[3 * atom],
                    record.geometry.as_ref().unwrap().coordinates[3 * atom + 1],
                    record.geometry.as_ref().unwrap().coordinates[3 * atom + 2],
                ]
            })
            .collect();
        let center = [
            points.iter().map(|p| p[0]).sum::<f64>() / 3.0,
            points.iter().map(|p| p[1]).sum::<f64>() / 3.0,
            points.iter().map(|p| p[2]).sum::<f64>() / 3.0,
        ];
        let mean_square = points
            .iter()
            .map(|p| {
                (p[0] - center[0]).powi(2) + (p[1] - center[1]).powi(2) + (p[2] - center[2]).powi(2)
            })
            .sum::<f64>()
            / 3.0;
        mean_square.sqrt()
    };
    assert!((weighted - unweighted).abs() > 1e-6);
}

#[test] // rq-ea25c7ce
fn total_mass_sums_atomic_weights() {
    let record = geometry_record(
        &["O", "H", "H"],
        &[0.0, 0.0, 0.0, 0.97, 0.0, 0.0, -0.23, 0.94, 0.0],
    );
    let total = metric_total_mass(&record).expect("metric defined");
    assert!((total - (2.0 * 1.008 + 15.999)).abs() < 1e-12);
}

#[test] // rq-5f4593c6
fn unknown_element_leaves_mass_based_metrics_undefined() {
    let record = geometry_record(&["Xx"], &[0.0, 0.0, 0.0]);
    assert_eq!(atomic_mass("Xx"), None);
    assert_eq!(metric_total_mass(&record), None);
    assert_eq!(metric_radius_of_gyration(&record), None);
}

#[test] // rq-fe2a60bf
fn missing_geometry_yields_no_metrics() {
    let record = Record::default();
    assert_eq!(metric_total_mass(&record), None);
    assert_eq!(metric_radius_of_gyration(&record), None);
}

#[test] // rq-80e048db
fn dipole_magnitude_from_stored_vector() {
    let record = Record {
        dipole_moment: Some([0.0, 3.0, 4.0]),
        ..Record::default()
    };
    assert_eq!(metric_dipole_magnitude(&record), Some(5.0));
}

#[test] // rq-23365bec
fn duplicate_names_collapse_to_first_occurrence() {
    let records = ["a", "b", "a"]
        .iter()
        .map(|name| Record {
            name: Some(name.to_string()),
            ..Record::default()
        })
        .collect::<Vec<_>>();
    let kept: Vec<String> = dedup_names(&records)
        .iter()
        .map(|record| record.name.clone().unwrap())
        .collect();
    assert_eq!(kept, ["a".to_string(), "b".to_string()]);
}

#[test] // rq-04bf58f3
fn nameless_records_are_never_duplicates() {
    let records = vec![Record::default(), Record::default()];
    assert_eq!(dedup_names(&records).len(), 2);
}

#[test] // rq-74c176bd
fn energies_within_rounding_agree_and_collapse() {
    let records = vec![energy_record(-38.5123), energy_record(-38.5124)];
    let default_key = parse_uniq_keys("el").expect("default key parses");
    assert_eq!(default_key[0].decimals, 3.0);
    let kept = unique_by_properties(&records, &default_key, &ThermoMemo::default());
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].electronic_energy, Some(-38.5123));
}

#[test] // rq-d54c69c6
fn energies_one_rounding_step_apart_both_survive() {
    let records = vec![energy_record(-38.5123), energy_record(-38.5099)];
    let kept = unique_by_properties(
        &records,
        &[key(UniqMetric::El, 3.0)],
        &ThermoMemo::default(),
    );
    assert_eq!(kept.len(), 2);
}

#[test] // rq-9ea7dfb5
fn rounding_is_floor_based_for_negative_values() {
    let records = vec![energy_record(-38.5124), energy_record(-38.5131)];
    let kept = unique_by_properties(
        &records,
        &[key(UniqMetric::El, 3.0)],
        &ThermoMemo::default(),
    );
    assert_eq!(kept.len(), 2);
}

#[test] // rq-627522b6
fn multi_key_signature_requires_agreement_on_every_component() {
    let short = geometry_record(&["H", "Cl"], &[0.0, 0.0, 0.0, 10.0, 0.0, 0.0]);
    let long = geometry_record(&["H", "Cl"], &[0.0, 0.0, 0.0, 10.05, 0.0, 0.0]);
    let mut records = vec![short, long];
    for record in &mut records {
        record.electronic_energy = Some(-38.5123);
    }
    let kept = unique_by_properties(
        &records,
        &[key(UniqMetric::El, 3.0), key(UniqMetric::Rg, 2.0)],
        &ThermoMemo::default(),
    );
    assert_eq!(kept.len(), 2);
}

#[test] // rq-ca0185f6
fn undefined_metrics_are_mutually_equal() {
    let records = vec![energy_record(-1.0), energy_record(-1.0)];
    let kept = unique_by_properties(
        &records,
        &[key(UniqMetric::El, 3.0), key(UniqMetric::Rg, 2.0)],
        &ThermoMemo::default(),
    );
    assert_eq!(kept.len(), 1);
}

#[test] // rq-03784756
fn identical_keys_collapse_differing_decimals_do_not() {
    let keys = parse_uniq_keys("el2,el,el2").expect("parses");
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[0], key(UniqMetric::El, 2.0));
    assert_eq!(keys[1], key(UniqMetric::El, 3.0));
}

#[test] // rq-c74aa6d8
fn custom_decimals_loosen_the_signature() {
    let records = vec![energy_record(-38.5119), energy_record(-38.5121)];
    let default3 = unique_by_properties(
        &records,
        &[key(UniqMetric::El, 3.0)],
        &ThermoMemo::default(),
    );
    assert_eq!(default3.len(), 2);
    let at2 = unique_by_properties(
        &records,
        &[key(UniqMetric::El, 2.0)],
        &ThermoMemo::default(),
    );
    assert_eq!(at2.len(), 1);
    assert_eq!(at2[0].electronic_energy, Some(-38.5119));
}

#[test] // rq-46ff4c56
fn fractional_decimal_counts_are_accepted() {
    let records = vec![energy_record(-38.5100), energy_record(-38.5101)];
    let at2 = unique_by_properties(
        &records,
        &[key(UniqMetric::El, 2.0)],
        &ThermoMemo::default(),
    );
    assert_eq!(at2.len(), 2);
    let keys = parse_uniq_keys("el2.4").expect("fractional decimals parse");
    assert_eq!(keys[0].decimals, 2.4);
    let at24 = unique_by_properties(&records, &keys, &ThermoMemo::default());
    assert_eq!(at24.len(), 1);
    assert_eq!(at24[0].electronic_energy, Some(-38.51));
}

#[test] // rq-ebb500d9
fn negative_decimal_count_is_rejected() {
    assert!(parse_uniq_keys("el-1").is_err());
}

// ---------------------------------------------------------------------------
// Milestone 6: the Gibbs metric (rqm/uniqueness.md).
// ---------------------------------------------------------------------------

use complied_jkqc::thermo::thermochemistry;

/// A water-like nonlinear record whose Gibbs free energy hits `target`
/// exactly: the electronic energy absorbs the thermochemical correction.
fn gibbs_record(target: f64) -> Record {
    let mut record = Record {
        electronic_energy: Some(0.0),
        geometry: Some(geom(
            &["O", "H", "H"],
            &[0.0, 0.0, 0.0, 0.0, 0.0, 0.9572, 0.9259, 0.0, -0.2423],
        )),
        vibrational_frequencies: Some(vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
        ..Record::default()
    };
    let correction = thermochemistry(&record, &ThermoConditions::default())
        .expect("template defined")
        .gibbs;
    record.electronic_energy = Some(target - correction);
    record
}

#[test] // rq-8237e2b7
fn gibbs_values_within_the_rounding_agree_and_collapse() {
    let records = vec![gibbs_record(-100.0012), gibbs_record(-100.0013)];
    let keys = parse_uniq_keys("g").expect("g key parses");
    assert_eq!(keys[0].metric, UniqMetric::G);
    assert_eq!(keys[0].decimals, 3.0);
    let kept = unique_by_properties(&records, &keys, &ThermoMemo::default());
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].electronic_energy, records[0].electronic_energy);
}

#[test] // rq-ad851478
fn the_gibbs_metric_follows_the_run_s_conditions() {
    // Two diatomics: a stiff H2 and a soft HCl whose low mode shifts the
    // Gibbs correction between 298.15 K and 273.15 K. The electronic
    // energies below are adjusted constants; the assertions that follow
    // check the property they were tuned for — same 3-decimal Gibbs bucket
    // at 298.15 K, Gibbs values a rounding step apart at 273.15 K — so
    // the constants are verified rather than trusted.
    let stiff = Record {
        electronic_energy: Some(-99.996245887066),
        geometry: Some(geom(&["H", "H"], &[0.0, 0.0, 0.0, 0.0, 0.0, 0.7414])),
        vibrational_frequencies: Some(vec![0.0, 0.0, 0.0, 0.0, 0.0, 3000.0]),
        ..Record::default()
    };
    let soft = Record {
        electronic_energy: Some(-99.983186262918),
        geometry: Some(geom(&["H", "Cl"], &[0.0, 0.0, 0.0, 0.0, 0.0, 1.2746])),
        vibrational_frequencies: Some(vec![0.0, 0.0, 0.0, 0.0, 0.0, 120.0]),
        ..Record::default()
    };
    let warm = ThermoMemo::default();
    let cold = ThermoMemo::new(ThermoConditions {
        temperature: 273.15,
        ..ThermoConditions::default()
    });
    let warm_stiff = warm.value("g", &stiff).expect("defined");
    let warm_soft = warm.value("g", &soft).expect("defined");
    assert_eq!(
        (warm_stiff * 1000.0).floor(),
        (warm_soft * 1000.0).floor(),
        "the tuned energies must put both Gibbs values in one 3-decimal bucket at 298.15 K"
    );
    let cold_stiff = cold.value("g", &stiff).expect("defined");
    let cold_soft = cold.value("g", &soft).expect("defined");
    assert_ne!(
        (cold_stiff * 1000.0).floor(),
        (cold_soft * 1000.0).floor(),
        "the temperature shift must land the two Gibbs values in different 3-decimal buckets at 273.15 K"
    );
    let keys = &[key(UniqMetric::G, 3.0)];
    let collapsed = unique_by_properties(&[stiff.clone(), soft.clone()], keys, &warm);
    assert_eq!(collapsed.len(), 1, "the 298.15 K run collapses the pair");
    let cold_run = unique_by_properties(&[stiff, soft], keys, &cold);
    assert_eq!(cold_run.len(), 2, "the 273.15 K run keeps both");
}

#[test] // rq-96a03a44
fn undefined_gibbs_metrics_are_mutually_equal() {
    // No electronic energy: undefined thermochemistry, NaN-like signature
    // components that compare equal to each other.
    let records = vec![Record::default(), Record::default()];
    let kept = unique_by_properties(&records, &[key(UniqMetric::G, 3.0)], &ThermoMemo::default());
    assert_eq!(kept.len(), 1);
}
