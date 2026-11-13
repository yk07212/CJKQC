//! Integration tests for Boltzmann population columns, one per Gherkin
//! scenario of `rqm/population.md` exercised at the API level. CLI-shaped
//! scenarios live in `tests/cli.rs`.

use compiled_jkqc::filter::Grouping;
use compiled_jkqc::population::{parse_pop_level, pop_level_value, populations, PopLevel};
use compiled_jkqc::thermo::{thermochemistry, ThermoConditions, ThermoMemo, E_HARTREE, K_B};
use compiled_jkqc::{Components, Correction, Geometry, Record, RecordStatus};

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

fn memo() -> ThermoMemo {
    ThermoMemo::default()
}

/// k_B T in Eh at the default 298.15 K (`rqm/thermo.md` Constants).
fn kbt() -> f64 {
    K_B * 298.15 / E_HARTREE
}

/// A conformer-shaped ok record with geometry and twelve modes at
/// `mode_value`, so its thermochemistry is defined.
fn conformer(name: &str, electronic_energy: Option<f64>, code: Option<&str>) -> Record {
    Record {
        name: Some(name.to_string()),
        electronic_energy,
        geometry: Some(hexagon()),
        vibrational_frequencies: Some(modes(12, 1000.0)),
        components: code.map(|code| components(&[(code, 1)])),
        ..Record::default()
    }
}

/// The same shape as `template`, with the electronic energy set so the
/// Gibbs free energy lands exactly on `target` (the thermal correction is
/// independent of the electronic energy).
fn with_gibbs(name: &str, target: f64) -> Record {
    let mut record = conformer(name, Some(0.0), None);
    let correction = thermochemistry(&record, &ThermoConditions::default())
        .expect("template has defined thermochemistry")
        .gibbs;
    record.electronic_energy = Some(target - correction);
    record
}

#[test]
// rq-7efc4513
fn two_conformers_split_by_their_boltzmann_factors() {
    let low = conformer("low", Some(-50.0), Some("am"));
    let high = conformer("high", Some(-50.0 + kbt() * 3.0f64.ln()), Some("am"));
    let populations = populations(&[low, high], PopLevel::G, &memo(), Grouping::Composition);
    assert!((populations[0].unwrap() - 0.75).abs() < 1e-12);
    assert!((populations[1].unwrap() - 0.25).abs() < 1e-12);
}

#[test]
// rq-49b5e21e
fn populations_are_per_composition_group() {
    let a_low = conformer("a1", Some(-50.0), Some("am"));
    let a_high = conformer("a2", Some(-50.0 + kbt()), Some("am"));
    let b_low = conformer("b1", Some(-60.0), Some("sa"));
    let b_high = conformer("b2", Some(-60.0 + kbt()), Some("sa"));
    let populations = populations(
        &[a_low, a_high, b_low, b_high],
        PopLevel::G,
        &memo(),
        Grouping::Composition,
    );
    let expected_low = 1.0 / (1.0 + (-1.0_f64).exp());
    for group in [&populations[0..2], &populations[2..4]] {
        assert!((group[0].unwrap() - expected_low).abs() < 1e-12);
        assert!((group[1].unwrap() - (1.0 - expected_low)).abs() < 1e-12);
        assert!((group[0].unwrap() + group[1].unwrap() - 1.0).abs() < 1e-12);
    }
}

#[test]
// rq-ae4f8a69
fn ties_share_the_population() {
    let first = conformer("first", Some(-50.0), Some("am"));
    let second = conformer("second", Some(-50.0), Some("am"));
    let populations = populations(
        &[first, second],
        PopLevel::G,
        &memo(),
        Grouping::Composition,
    );
    assert_eq!(populations[0], Some(0.5));
    assert_eq!(populations[1], Some(0.5));
}

#[test]
// rq-94d96047
fn records_without_components_form_their_own_group() {
    let first = conformer("first", Some(-50.0), None);
    let second = conformer("second", Some(-50.0 + kbt() * 3.0f64.ln()), None);
    let monomer = conformer("monomer", Some(-60.0), Some("am"));
    let populations = populations(
        &[first, second, monomer],
        PopLevel::El,
        &memo(),
        Grouping::Composition,
    );
    assert!((populations[0].unwrap() - 0.75).abs() < 1e-12);
    assert!((populations[1].unwrap() - 0.25).abs() < 1e-12);
    assert_eq!(populations[2], Some(1.0));
}

#[test]
// rq-33dafbb7
fn a_record_without_an_electronic_energy_is_undefined_at_every_level() {
    let lacking = Record {
        name: Some("lacking".to_string()),
        status: RecordStatus::Ok,
        ..Record::default()
    };
    let defined = conformer("defined", Some(-50.0), Some("am"));
    for level in [PopLevel::El, PopLevel::G] {
        let populations = populations(
            &[lacking.clone(), defined.clone()],
            level,
            &memo(),
            Grouping::Composition,
        );
        assert_eq!(populations[0], None);
        assert_eq!(populations[1], Some(1.0));
    }
}

#[test]
// rq-b19284dd
fn a_record_without_frequencies_has_defined_popel_and_undefined_pop() {
    let low = Record {
        name: Some("low".to_string()),
        electronic_energy: Some(-50.0),
        ..Record::default()
    };
    let high = Record {
        name: Some("high".to_string()),
        electronic_energy: Some(-50.0 + kbt() * 3.0f64.ln()),
        ..Record::default()
    };
    let electronic = populations(
        &[low.clone(), high.clone()],
        PopLevel::El,
        &memo(),
        Grouping::Composition,
    );
    assert!((electronic[0].unwrap() - 0.75).abs() < 1e-12);
    assert!((electronic[1].unwrap() - 0.25).abs() < 1e-12);
    let gibbs = populations(&[low, high], PopLevel::G, &memo(), Grouping::Composition);
    assert_eq!(gibbs[0], None);
    assert_eq!(gibbs[1], None);
}

#[test]
// rq-1ee82da7
fn a_record_without_a_correction_is_undefined_at_the_corrected_levels_alone() {
    let mut corrected = conformer("corrected", Some(-50.0), Some("am"));
    corrected.correction = Some(Correction {
        energy: -49.0,
        program: None,
        program_version: None,
        commands: None,
        kind: "sp".to_string(),
    });
    let plain = conformer("plain", Some(-50.5), Some("am"));
    let populations = populations(
        &[corrected, plain],
        PopLevel::GCorr,
        &memo(),
        Grouping::Composition,
    );
    assert_eq!(populations[0], Some(1.0));
    assert_eq!(populations[1], None);
}

#[test]
// rq-d0b32669
fn a_group_where_no_record_defines_the_level_prints_all_dashes() {
    let first = Record {
        name: Some("first".to_string()),
        ..Record::default()
    };
    let second = Record {
        name: Some("second".to_string()),
        ..Record::default()
    };
    let populations = populations(
        &[first, second],
        PopLevel::G,
        &memo(),
        Grouping::Composition,
    );
    assert_eq!(populations[0], None);
    assert_eq!(populations[1], None);
}

#[test]
// rq-5a5c97f2
fn pop_level_value_resolves_all_four_levels_through_one_path() {
    let mut record = conformer("corrected", Some(-50.0), Some("am"));
    record.correction = Some(Correction {
        energy: -49.0,
        program: None,
        program_version: None,
        commands: None,
        kind: "sp".to_string(),
    });
    let memo = memo();
    assert_eq!(pop_level_value(&record, PopLevel::El, &memo), Some(-50.0));
    assert_eq!(
        pop_level_value(&record, PopLevel::G, &memo),
        thermochemistry(&record, &ThermoConditions::default()).map(|thermo| thermo.gibbs)
    );
    assert_eq!(
        pop_level_value(&record, PopLevel::ElCorr, &memo),
        Some(-49.0)
    );
    // The corrected Gibbs is the standard derivation with the correction
    // energy in place of the stored one (`rqm/correction.md`).
    let mut corrected_basis = record.clone();
    corrected_basis.electronic_energy = Some(-49.0);
    assert_eq!(
        pop_level_value(&record, PopLevel::GCorr, &memo),
        thermochemistry(&corrected_basis, &ThermoConditions::default()).map(|thermo| thermo.gibbs)
    );
}

#[test]
// rq-4c429ba4
fn the_corrected_basis_splits_like_the_plain_basis_at_equal_gaps() {
    let plain_low = conformer("plain_low", Some(-50.0), Some("am"));
    let plain_high = conformer("plain_high", Some(-50.0 + kbt() * 3.0f64.ln()), Some("am"));
    let mut corrected_low = conformer("corrected_low", Some(0.0), Some("am"));
    corrected_low.correction = Some(Correction {
        energy: -49.0,
        program: None,
        program_version: None,
        commands: None,
        kind: "sp".to_string(),
    });
    let mut corrected_high = conformer("corrected_high", Some(0.0), Some("am"));
    corrected_high.correction = Some(Correction {
        energy: -49.0 + kbt() * 3.0f64.ln(),
        program: None,
        program_version: None,
        commands: None,
        kind: "sp".to_string(),
    });
    let plain = populations(
        &[plain_low, plain_high],
        PopLevel::G,
        &memo(),
        Grouping::Composition,
    );
    let corrected = populations(
        &[corrected_low, corrected_high],
        PopLevel::GCorr,
        &memo(),
        Grouping::Composition,
    );
    assert!((plain[0].unwrap() - corrected[0].unwrap()).abs() < 1e-12);
    assert!((plain[1].unwrap() - corrected[1].unwrap()).abs() < 1e-12);
}

#[test]
// rq-cd221e62
fn parse_pop_level_accepts_only_the_four_names() {
    assert_eq!(parse_pop_level("el"), Some(PopLevel::El));
    assert_eq!(parse_pop_level("g"), Some(PopLevel::G));
    assert_eq!(parse_pop_level("elcorr"), Some(PopLevel::ElCorr));
    assert_eq!(parse_pop_level("gcorr"), Some(PopLevel::GCorr));
    for nonsense in ["", "G", "EL", "pop", "g corr", "2"] {
        assert_eq!(parse_pop_level(nonsense), None, "{nonsense}");
    }
}

#[test]
// rq-540b4c18
fn populations_align_index_wise_and_pool_globally_under_unit_grouping() {
    let am = conformer("am", Some(-50.0), Some("am"));
    let sa = conformer("sa", Some(-60.0), Some("sa"));
    let pooled = populations(&[am, sa], PopLevel::G, &memo(), Grouping::Unit);
    // One global pool: each population is defined and both sum to 1.
    assert!(pooled.iter().all(|population| population.is_some()));
    assert!((pooled[0].unwrap() + pooled[1].unwrap() - 1.0).abs() < 1e-12);
}

#[test]
// rq-b7d07857
fn a_gibbs_gap_of_one_kbt_prints_the_exact_boltzmann_split() {
    let low = with_gibbs("low", -50.0);
    let high = with_gibbs("high", -50.0 + kbt());
    let populations = populations(&[low, high], PopLevel::G, &memo(), Grouping::Composition);
    assert!((populations[0].unwrap() - 0.731_058_578_630_004_9).abs() < 1e-12);
    assert!((populations[1].unwrap() - 0.268_941_421_369_995_1).abs() < 1e-12);
}
