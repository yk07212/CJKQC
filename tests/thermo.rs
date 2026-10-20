//! Integration tests for the thermochemistry model, one per Gherkin
//! scenario of `rqm/thermo.md` that is exercised at the API level.
//! CLI-shaped scenarios (flags, columns, units) live in `tests/cli.rs`.
//! Hand-computed reference values come from an independent evaluation of
//! the `rqm/thermo.md` formulas with CODATA 2018 constants.

use std::path::PathBuf;

use complied_jkqc::thermo::{
    classify_species, derived_value, rotational_constants, thermochemistry, SpeciesClass,
    ThermoConditions, ThermoMemo,
};
use complied_jkqc::{Geometry, ParserRegistry, Record};

fn conditions(temperature: f64) -> ThermoConditions {
    ThermoConditions {
        temperature,
        ..ThermoConditions::default()
    }
}

fn geometry(symbols: &[&str], coordinates: &[f64]) -> Geometry {
    Geometry {
        symbols: symbols.iter().map(|symbol| symbol.to_string()).collect(),
        coordinates: coordinates.to_vec(),
    }
}

/// Water-like bent triatomic: `["O", "H", "H"]` at a realistic geometry.
fn water_record() -> Record {
    Record {
        electronic_energy: Some(-76.0),
        geometry: Some(geometry(
            &["O", "H", "H"],
            &[0.0, 0.0, 0.0, 0.0, 0.0, 0.9572, 0.9259, 0.0, -0.2423],
        )),
        vibrational_frequencies: Some(vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
        ..Record::default()
    }
}

/// A collinear triatomic on the x axis: `Linear` under the moment rule.
fn linear3_record() -> Record {
    Record {
        electronic_energy: Some(-76.0),
        geometry: Some(geometry(
            &["H", "O", "H"],
            &[-1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        )),
        vibrational_frequencies: Some(vec![
            0.0, 0.0, 0.0, 0.0, 0.0, 1500.0, 1500.0, 3800.0, 3900.0,
        ]),
        ..Record::default()
    }
}

/// The H2 diatomic with one real mode at 100 cm⁻¹ — the reference
/// frequency — under the default conditions.
fn hh_record(mode: f64) -> Record {
    Record {
        electronic_energy: Some(-1.0),
        geometry: Some(geometry(&["H", "H"], &[0.0, 0.0, 0.0, 0.0, 0.0, 0.7414])),
        vibrational_frequencies: Some(vec![0.0, 0.0, 0.0, 0.0, 0.0, mode]),
        ..Record::default()
    }
}

fn fixture_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(relative)
}

fn parse_fixture(relative: &str) -> Record {
    let registry = ParserRegistry::standard();
    registry
        .parse_file(&fixture_path(relative))
        .expect("fixture parses")
}

// ---------------------------------------------------------------------------
// Conformance against ORCA.
// ---------------------------------------------------------------------------

#[test] // rq-9936f168
fn orca_fixture_reproduces_orcas_quasi_rrho_thermochemistry() {
    let record = parse_fixture("orca/1pinic1sa-110_1_9-str9.out");
    let thermo = thermochemistry(&record, &ThermoConditions::default())
        .expect("the fixture record has defined thermochemistry");
    // ORCA's printed values (tests/fixtures/orca quasi-RRHO block), within
    // 1e-4 Eh: the fixture prints frequencies to 2 decimals, so the
    // residual is frequency rounding, not model error.
    let expected = [
        (thermo.zpe, 0.27547281, "zpe"),
        (
            thermo.thermal_vibrational,
            0.01541878,
            "thermal_vibrational",
        ),
        (thermo.thermal_rotational, 0.00141627, "thermal_rotational"),
        (
            thermo.thermal_translational,
            0.00141627,
            "thermal_translational",
        ),
        (thermo.enthalpy, -199.34739714, "enthalpy"),
        (thermo.ts_translational, 0.02035094, "ts_translational"),
        (thermo.ts_rotational, 0.01583464, "ts_rotational"),
        (thermo.ts_vibrational, 0.02738792, "ts_vibrational"),
        (thermo.entropy_term, 0.06357350, "entropy_term"),
        (thermo.gibbs, -199.41097065, "gibbs"),
    ];
    for (actual, want, what) in expected {
        assert!(
            (actual - want).abs() <= 1e-4,
            "{what}: {actual} vs ORCA {want}"
        );
    }
}

#[test] // rq-3e2a7cc5
fn rotational_constants_recompute_from_the_stored_geometry() {
    let record = parse_fixture("orca/1pinic1sa-110_1_9-str9.out");
    let constants = rotational_constants(&record).expect("constants defined");
    // The return order is descending B̃ alongside the ascending moments —
    // the same order ORCA prints (0.024157, 0.008371, 0.007528 cm^-1).
    let expected = [0.024157, 0.008371, 0.007528];
    for (actual, want) in constants.iter().zip(expected) {
        assert!(
            (actual - want).abs() <= 1e-5,
            "rotational constant {actual} vs ORCA {want}"
        );
    }
}

// ---------------------------------------------------------------------------
// Mode selection.
// ---------------------------------------------------------------------------

#[test] // rq-e1c07994
fn near_zero_modes_do_not_participate() {
    let thermo = thermochemistry(&water_record(), &ThermoConditions::default()).unwrap();
    // ZPE of exactly the 500/1000/1500 modes: 0.006834502879 Eh.
    assert!(
        (thermo.zpe - 0.006834502879).abs() <= 1e-9,
        "zpe {}",
        thermo.zpe
    );
}

#[test] // rq-3f95d2bc
fn an_imaginary_frequency_leaves_thermochemistry_undefined() {
    let mut record = water_record();
    record.vibrational_frequencies = Some(vec![1000.0, -300.0]);
    assert!(thermochemistry(&record, &ThermoConditions::default()).is_none());
}

#[test] // rq-f6d24cca
fn makereal_flips_imaginary_modes_into_the_sums() {
    let mut flipped = water_record();
    flipped.vibrational_frequencies = Some(vec![1000.0, -300.0]);
    let mut real = water_record();
    real.vibrational_frequencies = Some(vec![1000.0, 300.0]);
    let with_makereal = thermochemistry(
        &flipped,
        &ThermoConditions {
            makereal: true,
            ..ThermoConditions::default()
        },
    )
    .unwrap();
    let without = thermochemistry(&real, &ThermoConditions::default()).unwrap();
    assert_eq!(with_makereal.zpe, without.zpe);
    assert_eq!(
        with_makereal.thermal_vibrational,
        without.thermal_vibrational
    );
    assert_eq!(with_makereal.ts_vibrational, without.ts_vibrational);
}

#[test] // rq-cc73801c
fn makereal_applies_the_participation_test_after_flipping() {
    let mut record = water_record();
    record.vibrational_frequencies = Some(vec![0.0, -0.5, 1200.0]);
    let thermo = thermochemistry(
        &record,
        &ThermoConditions {
            makereal: true,
            ..ThermoConditions::default()
        },
    )
    .unwrap();
    // Only the 1200 mode participates: zpe = 0.5 * h c 1200 / Eh.
    assert!(
        (thermo.zpe - 0.002733801152).abs() <= 1e-9,
        "{}",
        thermo.zpe
    );
}

#[test] // rq-903b5a1f
fn ignoreimag_drops_imaginary_modes_from_the_sums() {
    let mut skipped = water_record();
    skipped.vibrational_frequencies = Some(vec![1000.0, -300.0]);
    let mut real = water_record();
    real.vibrational_frequencies = Some(vec![1000.0]);
    let with_ignoreimag = thermochemistry(
        &skipped,
        &ThermoConditions {
            ignoreimag: true,
            ..ThermoConditions::default()
        },
    )
    .unwrap();
    let without = thermochemistry(&real, &ThermoConditions::default()).unwrap();
    assert_eq!(with_ignoreimag.zpe, without.zpe);
    assert_eq!(
        with_ignoreimag.thermal_vibrational,
        without.thermal_vibrational
    );
    assert_eq!(with_ignoreimag.ts_vibrational, without.ts_vibrational);
}

#[test] // rq-ef692e86
fn a_record_with_only_imaginary_modes_is_undefined_under_ignoreimag() {
    let mut record = water_record();
    record.vibrational_frequencies = Some(vec![-50.0, -300.0, -1200.0]);
    assert!(thermochemistry(
        &record,
        &ThermoConditions {
            ignoreimag: true,
            ..ThermoConditions::default()
        }
    )
    .is_none());
}

#[test] // rq-ec8fbd92
fn a_monatomic_record_with_a_stray_imaginary_mode_is_defined_under_ignoreimag() {
    let record = Record {
        electronic_energy: Some(-128.0),
        geometry: Some(geometry(&["Ne"], &[0.0, 0.0, 0.0])),
        vibrational_frequencies: Some(vec![-300.0]),
        ..Record::default()
    };
    let thermo = thermochemistry(
        &record,
        &ThermoConditions {
            ignoreimag: true,
            ..ThermoConditions::default()
        },
    )
    .expect("a monatomic record has no vibrational sums to lose");
    assert_eq!(thermo.zpe, 0.0);
    assert_eq!(thermo.ts_vibrational, 0.0);
}

#[test] // rq-65b1e3cd
fn a_single_mode_above_the_cutoff_carries_the_thermochemistry() {
    let mut record = water_record();
    record.vibrational_frequencies = Some(vec![0.5, 1200.0]);
    let thermo = thermochemistry(&record, &ThermoConditions::default()).unwrap();
    assert!(
        (thermo.zpe - 0.002733801152).abs() <= 1e-9,
        "{}",
        thermo.zpe
    );
}

// ---------------------------------------------------------------------------
// Species classes.
// ---------------------------------------------------------------------------

#[test] // rq-273399bb
fn a_monatomic_record_has_defined_thermochemistry_without_frequencies() {
    let record = Record {
        electronic_energy: Some(-128.0),
        geometry: Some(geometry(&["Ne"], &[0.0, 0.0, 0.0])),
        ..Record::default()
    };
    let thermo = thermochemistry(&record, &ThermoConditions::default()).unwrap();
    assert_eq!(thermo.zpe, 0.0);
    assert_eq!(thermo.thermal_rotational, 0.0);
    assert_eq!(thermo.ts_rotational, 0.0);
    assert_eq!(thermo.ts_vibrational, 0.0);
    // Hand-computed: G = -128.014244102256 Eh.
    assert!((thermo.gibbs - -128.014244102256).abs() <= 1e-9);
}

#[test] // rq-274f8c7e
fn a_linear_species_uses_the_linear_rotational_terms() {
    let thermo = thermochemistry(&linear3_record(), &ThermoConditions::default()).unwrap();
    // k_B T in Eh at 298.15 K.
    let kbt = 1.380649e-23 * 298.15 / 4.3597447222071e-18;
    assert!(
        (thermo.thermal_rotational - kbt).abs() <= 1e-12,
        "{}",
        thermo.thermal_rotational
    );
    // T R [ln(k_B T / (sigma h c B)) + 1] / (N_A Eh) with B from the
    // stored geometry (8.361919 cm^-1): 0.003975127231 Eh.
    assert!(
        (thermo.ts_rotational - 0.003975127231).abs() <= 1e-9,
        "{}",
        thermo.ts_rotational
    );
}

#[test] // rq-a2ed39c8
fn a_slightly_bent_triatomic_is_nonlinear() {
    // Smallest principal moment ~1e-3 of the largest: above the 1e-4
    // linear threshold.
    let record = Record {
        geometry: Some(geometry(
            &["O", "H", "H"],
            &[0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.06, 1.0],
        )),
        ..Record::default()
    };
    assert_eq!(classify_species(&record), Some(SpeciesClass::Nonlinear));
}

// ---------------------------------------------------------------------------
// Entropy pieces.
// ---------------------------------------------------------------------------

#[test] // rq-f03f140d
fn the_symmetry_number_divides_the_rotational_partition_function() {
    let mut sigma_two = water_record();
    sigma_two.rotational_symmetry_number = Some(2);
    let one = thermochemistry(&water_record(), &ThermoConditions::default()).unwrap();
    let two = thermochemistry(&sigma_two, &ThermoConditions::default()).unwrap();
    let difference = one.ts_rotational - two.ts_rotational;
    let tr_ln2 = 298.15 * (6.02214076e23 * 1.380649e-23) * 2.0f64.ln()
        / (6.02214076e23 * 4.3597447222071e-18);
    assert!(
        (difference - tr_ln2).abs() <= 1e-12,
        "{difference} vs {tr_ln2}"
    );
}

#[test] // rq-16a6fe88
fn electronic_entropy_carries_the_spin_degeneracy() {
    let mut doublet = water_record();
    doublet.multiplicity = Some(2);
    let thermo = thermochemistry(&doublet, &ThermoConditions::default()).unwrap();
    let tr_ln2 = 298.15 * (6.02214076e23 * 1.380649e-23) * 2.0f64.ln()
        / (6.02214076e23 * 4.3597447222071e-18);
    assert!(
        (thermo.ts_electronic - tr_ln2).abs() <= 1e-12,
        "{}",
        thermo.ts_electronic
    );
}

#[test] // rq-1c86c6f3
fn quasi_harmonic_entropy_never_exceeds_harmonic_entropy() {
    let mut record = water_record();
    record.vibrational_frequencies = Some(vec![0.0; 6]);
    record
        .vibrational_frequencies
        .as_mut()
        .unwrap()
        .extend([80.0, 120.0, 3000.0]);
    let quasi = thermochemistry(&record, &ThermoConditions::default()).unwrap();
    let harmonic = thermochemistry(
        &record,
        &ThermoConditions {
            freq_cutoff: 0.0,
            ..ThermoConditions::default()
        },
    )
    .unwrap();
    assert!(quasi.ts_vibrational <= harmonic.ts_vibrational);
    assert!(quasi.gibbs >= harmonic.gibbs);
}

#[test] // rq-18e4b628
fn the_reference_frequency_shifts_the_interpolation() {
    let mut record = water_record();
    record.vibrational_frequencies = Some(vec![0.0; 6]);
    record
        .vibrational_frequencies
        .as_mut()
        .unwrap()
        .extend([80.0, 120.0, 3000.0]);
    let at_100 = thermochemistry(&record, &ThermoConditions::default()).unwrap();
    let at_200 = thermochemistry(
        &record,
        &ThermoConditions {
            freq_cutoff: 200.0,
            ..ThermoConditions::default()
        },
    )
    .unwrap();
    assert_ne!(at_100.ts_vibrational, at_200.ts_vibrational);
}

#[test] // rq-927eb1c6
fn a_mode_at_the_reference_frequency_weighs_half() {
    // The H2 record's single mode sits exactly at the 100 cm⁻¹ reference,
    // so the vibrational entropy term is the exact half-mix of the
    // harmonic-oscillator and free-rotor expressions: 0.001386682027 Eh.
    let thermo = thermochemistry(&hh_record(100.0), &ThermoConditions::default()).unwrap();
    assert!(
        (thermo.ts_vibrational - 0.001386682027).abs() <= 1e-9,
        "{}",
        thermo.ts_vibrational
    );
}

// ---------------------------------------------------------------------------
// Conditions.
// ---------------------------------------------------------------------------

#[test] // rq-ed4da76f
fn temperature_changes_every_thermal_term_but_not_zpe() {
    let warm = thermochemistry(&water_record(), &ThermoConditions::default()).unwrap();
    let cold = thermochemistry(&water_record(), &conditions(273.15)).unwrap();
    assert_eq!(warm.zpe, cold.zpe);
    assert_ne!(warm.gibbs, cold.gibbs);
    // Hand-computed cold values.
    assert!((cold.enthalpy - -75.989503024101).abs() <= 1e-9);
    assert!((cold.gibbs - -76.009683256558).abs() <= 1e-9);
}

#[test] // rq-06cfcbfe
fn an_absurd_temperature_leaves_thermochemistry_undefined() {
    // A mode just above the 1 cm^-1 participation cutoff at 1e17 K puts
    // x below f64 resolution: e^{−x} rounds to exactly 1.0, the
    // vibrational sums diverge, and the derivation is undefined rather
    // than infinite (rqm/thermo.md Definedness).
    let record = Record {
        electronic_energy: Some(-75.99),
        geometry: Some(geometry(
            &["O", "H", "H"],
            &[0.0, 0.0, 0.0, 0.0, 0.0, 0.9572, 0.9259, 0.0, -0.2423],
        )),
        vibrational_frequencies: Some(vec![1.001, 1600.0, 3700.0]),
        ..Record::default()
    };
    assert!(thermochemistry(&record, &ThermoConditions::default()).is_some());
    let absurd = ThermoConditions {
        temperature: 1e17,
        ..ThermoConditions::default()
    };
    assert_eq!(thermochemistry(&record, &absurd), None);
    assert_eq!(derived_value("g", &record, &absurd), None);
    assert_eq!(ThermoMemo::new(absurd).value("g", &record), None);
}

#[test] // rq-cb57c609
fn the_frequency_scale_multiplies_zpe_linearly() {
    let scaled = thermochemistry(
        &water_record(),
        &ThermoConditions {
            freq_scale: 0.95,
            ..ThermoConditions::default()
        },
    )
    .unwrap();
    let unscaled = thermochemistry(&water_record(), &ThermoConditions::default()).unwrap();
    assert!((scaled.zpe - 0.95 * unscaled.zpe).abs() <= 1e-12);
    assert_ne!(scaled.ts_vibrational, unscaled.ts_vibrational);
}

// ---------------------------------------------------------------------------
// Definedness.
// ---------------------------------------------------------------------------

#[test] // rq-7fe12cf9
fn a_missing_electronic_energy_leaves_thermochemistry_undefined() {
    let mut record = water_record();
    record.electronic_energy = None;
    assert!(thermochemistry(&record, &ThermoConditions::default()).is_none());
}

#[test] // rq-7be2650a
fn a_non_monatomic_record_without_frequencies_is_undefined() {
    let mut record = water_record();
    record.vibrational_frequencies = None;
    assert!(thermochemistry(&record, &ThermoConditions::default()).is_none());
}

#[test] // rq-66b6ca1d
fn an_unknown_element_symbol_is_undefined() {
    let mut record = water_record();
    record.geometry = Some(geometry(
        &["Xx", "H", "H"],
        &[0.0, 0.0, 0.0, 0.0, 0.0, 0.9572, 0.9259, 0.0, -0.2423],
    ));
    assert!(thermochemistry(&record, &ThermoConditions::default()).is_none());
    assert!(classify_species(&record).is_none());
}

#[test] // rq-aceaf96d
fn a_degenerate_geometry_is_undefined() {
    // Every atom at identical coordinates: all principal moments vanish,
    // so there is no rotational axis and the derivation must be
    // undefined rather than produce infinities.
    let record = Record {
        electronic_energy: Some(-75.99),
        geometry: Some(geometry(&["O", "O"], &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0])),
        vibrational_frequencies: Some(vec![1000.0, 2000.0]),
        ..Record::default()
    };
    assert!(thermochemistry(&record, &ThermoConditions::default()).is_none());
    assert!(classify_species(&record).is_none());
}

// ---------------------------------------------------------------------------
// Registry.
// ---------------------------------------------------------------------------

#[test] // rq-df4e27e8
fn the_registry_exposes_the_five_energy_keys() {
    let record = water_record();
    let thermo = thermochemistry(&record, &ThermoConditions::default()).unwrap();
    let default = ThermoConditions::default();
    assert_eq!(
        derived_value("el", &record, &default),
        record.electronic_energy
    );
    assert_eq!(derived_value("zpe", &record, &default), Some(thermo.zpe));
    assert_eq!(derived_value("h", &record, &default), Some(thermo.enthalpy));
    assert_eq!(
        derived_value("s", &record, &default),
        Some(thermo.entropy_term)
    );
    assert_eq!(derived_value("g", &record, &default), Some(thermo.gibbs));
    assert_eq!(derived_value("nope", &record, &default), None);
}

#[test] // rq-8b44ad8d
fn the_memo_serves_the_registry_from_at_most_one_derivation_per_record() {
    let record = water_record();
    let conditions = ThermoConditions::default();
    let memo = ThermoMemo::new(conditions);
    // Repeated hits agree with the uncached derivation…
    for _ in 0..2 {
        assert_eq!(memo.thermo(&record), thermochemistry(&record, &conditions));
    }
    // …every registry key agrees with the pure lookup…
    for key in ["el", "zpe", "h", "s", "g", "nope"] {
        assert_eq!(
            memo.value(key, &record),
            derived_value(key, &record, &conditions)
        );
    }
    // …and a re-clone of the same observables yields the same values.
    let duplicate = record.clone();
    assert_eq!(memo.thermo(&duplicate), memo.thermo(&record));
    let undefined = Record::default();
    assert_eq!(memo.thermo(&undefined), None);
    assert_eq!(memo.value("g", &undefined), None);
}
