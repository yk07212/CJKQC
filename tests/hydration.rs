//! Integration tests for hydration and solvation distributions, one per
//! Gherkin scenario of `rqm/hydration.md` exercised at the API level.
//! CLI-shaped scenarios live in `tests/cli.rs`.

use compiled_jkqc::filter::SortKey;
use compiled_jkqc::formation::{
    collapse_ensembles, corrected_collapse_ensembles, ConcConditions, ConcSpec, ConcUnit,
};
use compiled_jkqc::hydration::{
    corrected_hydration_states, corrected_hydration_states_from_ensembles, hydration_percentages,
    hydration_states, hydration_states_from_ensembles, print_hydration_section,
    saturation_pressure_water, HydrationGroup, HydrationState,
};
use compiled_jkqc::record::Correction;
use compiled_jkqc::thermo::{thermochemistry, ThermoConditions, ThermoMemo, E_HARTREE, K_B};
use compiled_jkqc::{Components, Geometry, Record, P_STANDARD};

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

/// k_B T in Eh at the default conditions.
fn kbt() -> f64 {
    K_B * 298.15 / E_HARTREE
}

/// The vapor term per water at relative humidity `rh`: the concentration
/// correction the implied `--conc w p` spec adds per water to the
/// formation value, in Eh — k_B T ln(p°/(rh·p_sat)) at the default
/// conditions (`rqm/hydration.md` Solvent Pressure).
fn vapor_term(rh: f64) -> f64 {
    K_B * 298.15 * (P_STANDARD / (rh * saturation_pressure_water(298.15))).ln() / E_HARTREE
}

/// The run's implied solvent spec at relative humidity `rh`: the CLI
/// appends `--conc w rh·p_sat` before the sections evaluate
/// (`rqm/hydration.md` Solvent Pressure).
fn concs_at(rh: f64) -> ConcConditions {
    ConcConditions {
        specs: vec![ConcSpec {
            code: "w".to_string(),
            magnitude: rh * saturation_pressure_water(298.15),
            unit: ConcUnit::Pa,
        }],
        self_consistent: false,
    }
}

/// The saturated-vapor default of a bare `--hydration` run.
fn saturated() -> ConcConditions {
    concs_at(1.0)
}

/// A full-thermochemistry record whose raw and corrected Gibbs free
/// energies hit the given targets exactly at the default conditions (the
/// electronic energy and the correction energy absorb the corrections,
/// which are linear in both). `None` targets leave the corresponding
/// level at the template's value or absent.
fn leveled(name: &str, comps: &[(&str, u32)], raw: Option<f64>, corrected: Option<f64>) -> Record {
    let mut record = Record {
        name: Some(name.to_string()),
        electronic_energy: Some(0.0),
        geometry: Some(hexagon()),
        vibrational_frequencies: Some(modes(12, 1000.0)),
        components: Some(components(comps)),
        ..Record::default()
    };
    let gibbs0 = thermochemistry(&record, &ThermoConditions::default())
        .expect("template has defined thermochemistry")
        .gibbs;
    if let Some(target) = raw {
        record.electronic_energy = Some(target - gibbs0);
    }
    if let Some(target) = corrected {
        record.correction = Some(Correction {
            energy: target - gibbs0,
            program: None,
            program_version: None,
            commands: None,
            kind: "sp".to_string(),
        });
    }
    record
}

/// A record like `leveled` but without frequencies, so its thermochemistry
/// and both Gibbs levels are undefined.
fn frequency_less(name: &str, comps: &[(&str, u32)]) -> Record {
    let mut record = leveled(name, comps, None, None);
    record.vibrational_frequencies = None;
    record
}

/// The Gibbs target for a state with the given reference values that
/// carries the distribution value `value` at relative humidity `rh`:
/// the standard-state formation value is `value` minus the vapor term of
/// its `waters` (`rqm/hydration.md` Solvent Pressure).
fn target_at(refs: &[f64], value: f64, waters: u32, rh: f64) -> f64 {
    refs.iter().sum::<f64>() + value - f64::from(waters) * vapor_term(rh)
}

/// The Gibbs target at the saturated-vapor default.
fn target(refs: &[f64], value: f64, waters: u32) -> f64 {
    target_at(refs, value, waters, 1.0)
}

const G_AM: f64 = -50.0;
const G_SA: f64 = -60.0;
const G_W: f64 = -76.0;

fn monomers() -> Vec<Record> {
    vec![
        leveled("am1", &[("am", 1)], Some(G_AM), None),
        leveled("sa1", &[("sa", 1)], Some(G_SA), None),
        leveled("w1", &[("w", 1)], Some(G_W), None),
    ]
}

fn percentage(cell: Option<f64>) -> f64 {
    cell.expect("the state's percentage is defined")
}

#[test] // rq-91512869
fn saturation_pressure_water_matches_its_reference_values() {
    let at_freezing = saturation_pressure_water(273.15);
    assert!(
        (at_freezing - 610.94).abs() < 1.0e-9,
        "610.94 Pa at 273.15 K, got {at_freezing}"
    );
    let at_room = saturation_pressure_water(298.15);
    assert!(
        (at_room - 3.162e3).abs() < 1.0,
        "about 3.162e3 Pa at 298.15 K, got {at_room}"
    );
}

#[test] // rq-a99a27e1
fn states_group_by_core_composition() {
    let mut records = vec![
        leveled("c1", &[("sa", 1)], Some(target(&[G_SA], 0.0, 0)), None),
        leveled(
            "c2",
            &[("sa", 1), ("w", 1)],
            Some(target(&[G_SA, G_W], 0.1, 1)),
            None,
        ),
        leveled(
            "c3",
            &[("sa", 1), ("w", 2)],
            Some(target(&[G_SA, G_W], 0.2, 2)),
            None,
        ),
        leveled(
            "c4",
            &[("am", 2), ("sa", 1)],
            Some(target(&[G_AM, G_AM, G_SA], 0.3, 0)),
            None,
        ),
        leveled(
            "c5",
            &[("am", 2), ("sa", 1), ("w", 1)],
            Some(target(&[G_AM, G_AM, G_SA, G_W], 0.4, 1)),
            None,
        ),
    ];
    records.extend(monomers());
    let groups = hydration_states(&records, SortKey::El, &memo(), &saturated(), "w");
    assert_eq!(groups.len(), 3, "sa core, 2am1sa core, and the am monomer");
    assert_eq!(groups[0].core, components(&[("sa", 1)]));
    let tokens: Vec<String> = groups[0]
        .states
        .iter()
        .map(|state| token(&state.components))
        .collect();
    assert_eq!(tokens, ["1sa", "1sa1w", "1sa2w"]);
    assert_eq!(groups[1].core, components(&[("am", 2), ("sa", 1)]));
    let second: Vec<String> = groups[1]
        .states
        .iter()
        .map(|state| token(&state.components))
        .collect();
    assert_eq!(second, ["2am1sa", "2am1sa1w"]);
}

fn token(components: &Components) -> String {
    components
        .iter()
        .map(|(code, count)| format!("{count}{code}"))
        .collect()
}

#[test] // rq-b9897f33
fn the_lowest_free_conformer_represents_a_state() {
    let mut records = vec![
        leveled("c1", &[("sa", 1)], Some(target(&[G_SA], 0.0, 0)), None),
        leveled(
            "hi",
            &[("sa", 1), ("w", 1)],
            Some(target(&[G_SA, G_W], 0.2, 1)),
            None,
        ),
        leveled(
            "lo",
            &[("sa", 1), ("w", 1)],
            Some(target(&[G_SA, G_W], 0.2 - kbt(), 1)),
            None,
        ),
    ];
    records.extend(monomers());
    let groups = hydration_states(&records, SortKey::El, &memo(), &saturated(), "w");
    assert_eq!(groups[0].states.len(), 2);
    // Both 1sa1w conformers share one components map: one state whose
    // value is the lower conformer's.
    let hydrate = &groups[0].states[1];
    assert!((hydrate.value.expect("defined") - (0.2 - kbt())).abs() < 1.0e-12);
}

#[test] // rq-282ee600
fn undefined_conformers_never_hide_a_state_s_defined_value() {
    let mut records = vec![
        leveled("c1", &[("sa", 1)], Some(target(&[G_SA], 0.0, 0)), None),
        frequency_less("blind", &[("sa", 1), ("w", 1)]),
        leveled(
            "seen",
            &[("sa", 1), ("w", 1)],
            Some(target(&[G_SA, G_W], 0.15, 1)),
            None,
        ),
    ];
    records.extend(monomers());
    let groups = hydration_states(&records, SortKey::El, &memo(), &saturated(), "w");
    let hydrate = &groups[0].states[1];
    assert!((hydrate.value.expect("defined") - 0.15).abs() < 1.0e-12);
}

#[test] // rq-ff1f09cb
fn a_state_without_any_defined_member_prints_a_dash_and_drops_out() {
    let mut records = vec![
        leveled("c1", &[("sa", 1)], Some(target(&[G_SA], 0.0, 0)), None),
        frequency_less("blind", &[("sa", 1), ("w", 1)]),
    ];
    records.extend(monomers());
    let groups = hydration_states(&records, SortKey::El, &memo(), &saturated(), "w");
    assert_eq!(groups[0].states[0].value, Some(0.0));
    assert_eq!(groups[0].states[1].value, None);
    let percentages = hydration_percentages(&groups[0], 298.15);
    assert_eq!(percentages, [Some(100.0), None]);
}

#[test] // rq-e0cba629
fn records_without_components_are_excluded() {
    let mut nameless = leveled("x", &[("sa", 1)], Some(target(&[G_SA], 0.5, 0)), None);
    nameless.components = None;
    let mut records = vec![
        nameless,
        leveled("c1", &[("sa", 1)], Some(target(&[G_SA], 0.3, 0)), None),
    ];
    records.extend(monomers());
    let groups = hydration_states(&records, SortKey::El, &memo(), &saturated(), "w");
    assert_eq!(groups.len(), 2, "the sa core and the am monomer core");
    assert_eq!(groups[0].states.len(), 1, "only the 1sa state");
    assert_eq!(groups[0].states[0].components, components(&[("sa", 1)]));
}

#[test] // rq-4b684f8d
fn pure_solvent_clusters_print_no_distribution() {
    let mut records = vec![
        leveled("w1", &[("w", 1)], Some(G_W), None),
        leveled("w2", &[("w", 2)], Some(target(&[G_W], -0.01, 2)), None),
        leveled("c1", &[("sa", 1)], Some(target(&[G_SA], 0.3, 0)), None),
    ];
    records.extend(monomers());
    let groups = hydration_states(&records, SortKey::El, &memo(), &saturated(), "w");
    assert_eq!(groups.len(), 2, "the sa core and the am monomer core");
    assert_eq!(groups[0].states.len(), 1);
    assert_eq!(groups[0].states[0].components, components(&[("sa", 1)]));
}

#[test] // rq-1c4d88c8
fn bavg_replaces_the_conformer_minimum_with_the_ensemble_value() {
    // Two 1sa1w conformers with equal electronic energies whose Gibbs
    // values differ by k_B T ln(3): the stiffer conformer's uniform mode
    // value is tuned until its (energy-free) correction supplies the gap.
    let gap = kbt() * 3.0_f64.ln();
    let correction_at = |mode_value: f64| {
        thermochemistry(
            &Record {
                electronic_energy: Some(0.0),
                geometry: Some(hexagon()),
                vibrational_frequencies: Some(modes(12, mode_value)),
                ..Record::default()
            },
            &ThermoConditions::default(),
        )
        .expect("defined")
        .gibbs
    };
    let base = correction_at(1000.0);
    let mut low = 1000.0_f64;
    let mut high = 40000.0_f64;
    for _ in 0..100 {
        let middle = (low + high) / 2.0;
        if correction_at(middle) - base < gap {
            low = middle;
        } else {
            high = middle;
        }
    }
    let nu = (low + high) / 2.0;
    let lo = leveled(
        "lo",
        &[("sa", 1), ("w", 1)],
        Some(target(&[G_SA, G_W], 0.5 * kbt(), 1)),
        None,
    );
    // Equal electronic energies; the Gibbs gap comes from the modes.
    let hi = Record {
        name: Some("hi".to_string()),
        electronic_energy: lo.electronic_energy,
        geometry: Some(hexagon()),
        vibrational_frequencies: Some(modes(12, nu)),
        components: Some(components(&[("sa", 1), ("w", 1)])),
        ..Record::default()
    };
    let gibbs = |record: &Record| {
        thermochemistry(record, &ThermoConditions::default())
            .expect("defined")
            .gibbs
    };
    assert!((gibbs(&hi) - gibbs(&lo) - gap).abs() < 1.0e-12);

    let records = vec![
        leveled("sa1", &[("sa", 1)], Some(G_SA), None),
        leveled("w1", &[("w", 1)], Some(G_W), None),
        lo,
        hi,
        leveled("am1", &[("am", 1)], Some(G_AM), None),
    ];
    let uncollapsed = hydration_states(&records, SortKey::El, &memo(), &saturated(), "w");
    let best = uncollapsed[0].states[1].value.expect("defined");
    assert!(
        (best - 0.5 * kbt()).abs() < 1.0e-12,
        "the lower conformer's value"
    );
    let ensembles = collapse_ensembles(&records, &memo());
    let averaged = hydration_states_from_ensembles(&ensembles, &memo(), &saturated(), "w");
    let ensemble_value = averaged[0].states[1].value.expect("defined");
    assert!(
        (ensemble_value - (best - kbt() * (4.0_f64 / 3.0).ln())).abs() < 1.0e-12,
        "the ensemble value is k_B T ln(4/3) below the best conformer"
    );
    let plain = hydration_percentages(&uncollapsed[0], 298.15);
    let averaged_percentages = hydration_percentages(&averaged[0], 298.15);
    assert!(
        percentage(averaged_percentages[1]) > percentage(plain[1]),
        "the averaged 1sa1w percentage is higher: {averaged_percentages:?} vs {plain:?}"
    );
}

#[test] // rq-7067d53d
fn the_corrected_level_reads_the_corrected_values() {
    let mut records = vec![
        leveled(
            "c1",
            &[("sa", 1)],
            Some(target(&[G_SA], 0.0, 0)),
            Some(target(&[G_SA], 0.0, 0)),
        ),
        leveled(
            "c2",
            &[("sa", 1), ("w", 1)],
            Some(target(&[G_SA, G_W], 0.2, 1)),
            Some(target(&[G_SA, G_W], -0.01, 1)),
        ),
    ];
    records.extend(vec![
        leveled("sa1", &[("sa", 1)], Some(G_SA), Some(G_SA)),
        leveled("w1", &[("w", 1)], Some(G_W), Some(G_W)),
    ]);
    let raw = hydration_states(&records, SortKey::El, &memo(), &saturated(), "w");
    let corrected = corrected_hydration_states(&records, SortKey::El, &memo(), &saturated(), "w");
    let raw_percentages = hydration_percentages(&raw[0], 298.15);
    let corrected_percentages = hydration_percentages(&corrected[0], 298.15);
    assert_ne!(raw_percentages, corrected_percentages);
    // The raw level favors 1sa, the corrected one 1sa1w.
    assert!(percentage(raw_percentages[0]) > 99.0);
    assert!(percentage(corrected_percentages[1]) > 99.0);
    // The corrected ensemble path reads the same values. The comparison
    // uses a reduced record set whose compositions never merge with a
    // monomer's in the collapse (rqm/formation.md Boltzmann-Averaged
    // Sections), so the two paths agree exactly.
    let reduced = vec![records[1].clone(), records[2].clone(), records[3].clone()];
    let expected = corrected_hydration_states(&reduced, SortKey::El, &memo(), &saturated(), "w");
    let ensembles = corrected_collapse_ensembles(&reduced, &memo());
    let averaged =
        corrected_hydration_states_from_ensembles(&ensembles, &memo(), &saturated(), "w");
    let expected_percentages = hydration_percentages(&expected[0], 298.15);
    let averaged_percentages = hydration_percentages(&averaged[0], 298.15);
    for (expected, ensemble) in expected_percentages.iter().zip(&averaged_percentages) {
        assert!((percentage(*expected) - percentage(*ensemble)).abs() < 1.0e-9);
    }
}

#[test] // rq-bd08eb6c
fn a_state_without_corrections_dashes_at_the_corrected_level() {
    let mut records = vec![
        leveled(
            "c1",
            &[("sa", 1)],
            Some(target(&[G_SA], 0.0, 0)),
            Some(target(&[G_SA], 0.0, 0)),
        ),
        leveled(
            "c2",
            &[("sa", 1), ("w", 1)],
            Some(target(&[G_SA, G_W], 0.1, 1)),
            None,
        ),
    ];
    records.extend(vec![
        leveled("sa1", &[("sa", 1)], Some(G_SA), Some(G_SA)),
        leveled("w1", &[("w", 1)], Some(G_W), Some(G_W)),
    ]);
    let corrected = corrected_hydration_states(&records, SortKey::El, &memo(), &saturated(), "w");
    let percentages = hydration_percentages(&corrected[0], 298.15);
    assert_eq!(percentages, [Some(100.0), None]);
}

#[test] // rq-9a225d69
fn two_states_one_kbt_apart_split_about_73_1_and_26_9() {
    let mut records = vec![
        leveled("c1", &[("sa", 1)], Some(target(&[G_SA], 0.0, 0)), None),
        leveled(
            "c2",
            &[("sa", 1), ("w", 1)],
            Some(target(&[G_SA, G_W], kbt(), 1)),
            None,
        ),
    ];
    records.extend(monomers());
    let groups = hydration_states(&records, SortKey::El, &memo(), &saturated(), "w");
    let percentages = hydration_percentages(&groups[0], 298.15);
    assert!(
        (percentage(percentages[0]) - 73.1).abs() < 0.05,
        "{percentages:?}"
    );
    assert!(
        (percentage(percentages[1]) - 26.9).abs() < 0.05,
        "{percentages:?}"
    );
}

#[test] // rq-0ccc07ee
fn a_state_far_above_the_minimum_underflows_to_zero() {
    let mut records = vec![
        leveled("c1", &[("sa", 1)], Some(target(&[G_SA], 0.0, 0)), None),
        leveled(
            "c2",
            &[("sa", 1), ("w", 1)],
            Some(target(&[G_SA, G_W], kbt(), 1)),
            None,
        ),
        leveled(
            "c3",
            &[("sa", 1), ("w", 2)],
            Some(target(&[G_SA, G_W], 400.0 * kbt(), 2)),
            None,
        ),
    ];
    records.extend(monomers());
    let groups = hydration_states(&records, SortKey::El, &memo(), &saturated(), "w");
    let percentages = hydration_percentages(&groups[0], 298.15);
    assert!(
        (percentage(percentages[0]) - 73.1).abs() < 0.05,
        "{percentages:?}"
    );
    assert!(
        (percentage(percentages[1]) - 26.9).abs() < 0.05,
        "{percentages:?}"
    );
    assert!(percentage(percentages[2]).abs() < 0.05, "{percentages:?}");
}

#[test] // rq-8ed6c203
fn a_group_without_defined_states_prints_all_dashes() {
    let records = vec![
        frequency_less("b1", &[("sa", 1), ("w", 1)]),
        frequency_less("b2", &[("sa", 1), ("w", 1)]),
        frequency_less("w1", &[("w", 1)]),
    ];
    let groups = hydration_states(&records, SortKey::El, &memo(), &saturated(), "w");
    assert_eq!(
        groups.len(),
        1,
        "only the sa core; the pure solvent is excluded"
    );
    let group = &groups[0];
    assert_eq!(group.states.len(), 1, "the two conformers share one state");
    assert!(group.states.iter().all(|state| state.value.is_none()));
    let percentages = hydration_percentages(group, 298.15);
    assert!(percentages.iter().all(|percentage| percentage.is_none()));
    let section = print_hydration_section(groups.as_slice(), 298.15);
    assert_eq!(section.lines().nth(1).expect("two rows").trim(), "-");
}

#[test] // rq-d60ca773
fn the_temperature_shifts_the_split() {
    let mut records = vec![
        leveled("c1", &[("sa", 1)], Some(target(&[G_SA], 0.0, 0)), None),
        leveled(
            "c2",
            &[("sa", 1), ("w", 1)],
            Some(target(&[G_SA, G_W], 1.0369e-3, 1)),
            None,
        ),
    ];
    records.extend(monomers());
    // The group is built once; only the percentage evaluation moves
    // across temperatures, so the 1.0369e-3 Eh gap stays fixed.
    let groups = hydration_states(&records, SortKey::El, &memo(), &saturated(), "w");
    let at_room = hydration_percentages(&groups[0], 298.15);
    assert!((percentage(at_room[0]) - 75.0).abs() < 0.05, "{at_room:?}");
    assert!((percentage(at_room[1]) - 25.0).abs() < 0.05, "{at_room:?}");
    let at_doubling = hydration_percentages(&groups[0], 596.3);
    assert!(
        (percentage(at_doubling[0]) - 63.4).abs() < 0.05,
        "{at_doubling:?}"
    );
    assert!(
        (percentage(at_doubling[1]) - 36.6).abs() < 0.05,
        "{at_doubling:?}"
    );
}

#[test] // rq-fea096b0
fn a_non_solvent_conc_spec_cancels_within_a_core_group() {
    let mut records = vec![
        leveled("c1", &[("sa", 1)], Some(target(&[G_SA], 0.3, 0)), None),
        leveled(
            "c2",
            &[("sa", 1), ("w", 1)],
            Some(target(&[G_SA, G_W], 0.1, 1)),
            None,
        ),
    ];
    records.extend(monomers());
    let plain = hydration_states(&records, SortKey::El, &memo(), &saturated(), "w");
    let mut specs = saturated().specs;
    specs.push(ConcSpec {
        code: "sa".to_string(),
        magnitude: 100.0,
        unit: ConcUnit::Ppt,
    });
    let with_sa = hydration_states(
        &records,
        SortKey::El,
        &memo(),
        &ConcConditions {
            specs,
            self_consistent: false,
        },
        "w",
    );
    let plain_percentages = hydration_percentages(&plain[0], 298.15);
    let with_sa_percentages = hydration_percentages(&with_sa[0], 298.15);
    for (plain, with) in plain_percentages.iter().zip(&with_sa_percentages) {
        assert!(
            (percentage(*plain) - percentage(*with)).abs() < 1.0e-9,
            "{plain_percentages:?} vs {with_sa_percentages:?}"
        );
    }
}

#[test] // rq-7b9426c0
fn cnt_scales_the_solvent_term_by_n_minus_one_over_n() {
    // Standard-state formation free energies equal: the targets are the
    // bare references, and the vapor term rides in the spec at rh 0.5.
    let mut records = vec![
        leveled("c1", &[("sa", 1)], Some(G_SA), None),
        leveled("c2", &[("sa", 1), ("w", 1)], Some(G_SA + G_W), None),
    ];
    records.extend(monomers());
    let spec = |self_consistent| ConcConditions {
        self_consistent,
        ..concs_at(0.5)
    };
    let plain = hydration_states(&records, SortKey::El, &memo(), &spec(false), "w");
    let cnt = hydration_states(&records, SortKey::El, &memo(), &spec(true), "w");
    let plain_percentages = hydration_percentages(&plain[0], 298.15);
    let cnt_percentages = hydration_percentages(&cnt[0], 298.15);
    assert!(
        percentage(plain_percentages[1]) < percentage(plain_percentages[0]),
        "{plain_percentages:?}"
    );
    assert!(
        percentage(cnt_percentages[1]) > percentage(plain_percentages[1]),
        "{cnt_percentages:?}"
    );
}

#[test] // rq-cd509d24
fn the_layout_is_a_token_row_and_a_percentage_row_per_group() {
    let mut records = vec![
        leveled("c1", &[("sa", 1)], Some(target(&[G_SA], 0.0, 0)), None),
        leveled(
            "c2",
            &[("sa", 1), ("w", 1)],
            Some(target(&[G_SA, G_W], 0.5 * kbt(), 1)),
            None,
        ),
        leveled(
            "c3",
            &[("sa", 1), ("w", 2)],
            Some(target(&[G_SA, G_W], 1.5 * kbt(), 2)),
            None,
        ),
    ];
    records.extend(monomers());
    let groups = hydration_states(&records, SortKey::El, &memo(), &saturated(), "w");
    let section = print_hydration_section(&groups, 298.15);
    let lines: Vec<&str> = section.lines().collect();
    assert_eq!(lines[0], " 1sa 1sa1w 1sa2w", "{section}");
    assert!(
        !lines[0].starts_with("name "),
        "no header line prints: {section}"
    );
    assert!(
        lines[1]
            .split(' ')
            .filter(|cell| !cell.is_empty())
            .all(|cell| cell.parse::<f64>().is_ok()),
        "{section}"
    );
}

#[test] // rq-e52f165b
fn a_blank_line_separates_the_groups() {
    let mut records = vec![
        leveled("c1", &[("sa", 1)], Some(target(&[G_SA], 0.0, 0)), None),
        leveled(
            "c2",
            &[("am", 2)],
            Some(target(&[G_AM, G_AM], 0.1, 0)),
            None,
        ),
    ];
    records.extend(monomers());
    let groups = hydration_states(&records, SortKey::El, &memo(), &saturated(), "w");
    assert_eq!(groups.len(), 3);
    let section = print_hydration_section(&groups, 298.15);
    // Every group's percentage row is followed by a blank line, so each
    // split part holds exactly one token row and one percentage row.
    let blocks: Vec<&str> = section.split("\n\n").collect();
    assert_eq!(blocks.len(), groups.len(), "{section:?}");
    for block in blocks {
        assert_eq!(block.lines().count(), 2, "{section:?}");
    }
}

#[test] // rq-2a926651
fn columns_align_within_a_group() {
    let group = HydrationGroup {
        core: components(&[("sa", 1)]),
        states: vec![
            HydrationState {
                components: components(&[("sa", 1)]),
                value: Some(0.0),
            },
            HydrationState {
                components: components(&[("sa", 1), ("w", 1)]),
                value: Some(20.0 * kbt()),
            },
        ],
    };
    let section = print_hydration_section(&[group], 298.15);
    assert_eq!(section, "  1sa 1sa1w\n100.0   0.0\n", "{section:?}");
}

#[test] // rq-3f2988b7
fn percentages_print_with_one_decimal() {
    let group = HydrationGroup {
        core: components(&[("sa", 1)]),
        states: vec![
            HydrationState {
                components: components(&[("sa", 1)]),
                value: Some(0.0),
            },
            HydrationState {
                components: components(&[("sa", 1), ("w", 1)]),
                value: Some(kbt()),
            },
        ],
    };
    let section = print_hydration_section(&[group], 298.15);
    let cells: Vec<&str> = section
        .lines()
        .nth(1)
        .expect("two rows")
        .split(' ')
        .filter(|cell| !cell.is_empty())
        .collect();
    assert_eq!(cells, ["73.1", "26.9"], "{section:?}");
}

#[test] // rq-6771a995
fn undefined_states_print_dashes_in_their_columns() {
    let group = HydrationGroup {
        core: components(&[("sa", 1)]),
        states: vec![
            HydrationState {
                components: components(&[("sa", 1)]),
                value: Some(0.0),
            },
            HydrationState {
                components: components(&[("sa", 1), ("w", 1)]),
                value: None,
            },
        ],
    };
    let section = print_hydration_section(&[group], 298.15);
    let percentage_row = section.lines().nth(1).expect("two rows");
    assert!(percentage_row.trim_end().ends_with('-'), "{section:?}");
}

#[test] // rq-fbfe30a4
fn rounded_percentages_need_not_sum_to_exactly_100() {
    let value = 0.123;
    let group = HydrationGroup {
        core: components(&[("sa", 1)]),
        states: vec![
            HydrationState {
                components: components(&[("sa", 1)]),
                value: Some(value),
            },
            HydrationState {
                components: components(&[("sa", 1), ("w", 1)]),
                value: Some(value),
            },
            HydrationState {
                components: components(&[("sa", 1), ("w", 2)]),
                value: Some(value),
            },
        ],
    };
    let percentages = hydration_percentages(&group, 298.15);
    for cell in &percentages {
        assert!((percentage(*cell) - 100.0 / 3.0).abs() < 1.0e-9);
    }
    let section = print_hydration_section(&[group], 298.15);
    let cells: Vec<f64> = section
        .lines()
        .nth(1)
        .expect("two rows")
        .split(' ')
        .filter(|cell| !cell.is_empty())
        .map(|cell| cell.parse::<f64>().expect("a percentage"))
        .collect();
    assert!(cells.iter().all(|cell| (*cell - 33.3).abs() < 0.05));
    let sum: f64 = cells.iter().sum();
    assert!(
        (sum - 99.9).abs() < 0.05 || (sum - 100.1).abs() < 0.05,
        "the displayed percentages may sum to 99.9 or 100.1: {cells:?}"
    );
}
