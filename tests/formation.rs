//! Integration tests for formation energies from monomer references, one
//! per Gherkin scenario of `rqm/formation.md` exercised at the API level.
//! CLI-shaped scenarios live in `tests/cli.rs`.

use complied_jkqc::filter::{SortKey, Unit};
use complied_jkqc::formation::{
    collect_monomers, formation_values, ConcConditions, Formation, MonomerReferences,
};
use complied_jkqc::thermo::{thermochemistry, ThermoConditions, ThermoMemo};
use complied_jkqc::{Components, Geometry, Record, RecordStatus};

fn components(pairs: &[(&str, u32)]) -> Components {
    pairs
        .iter()
        .map(|(code, count)| (code.to_string(), *count))
        .collect()
}

fn geometry(symbols: &[&str], coordinates: &[f64]) -> Geometry {
    Geometry {
        symbols: symbols.iter().map(|symbol| symbol.to_string()).collect(),
        coordinates: coordinates.to_vec(),
    }
}

/// A nonlinear 6-atom geometry (a planar hexagon): 3N−6 = 12 modes.
fn hexagon() -> Geometry {
    let mut coordinates = Vec::new();
    for atom in 0..6 {
        let angle = atom as f64 * std::f64::consts::PI / 3.0;
        coordinates.extend([angle.cos(), angle.sin(), 0.0]);
    }
    geometry(&["C", "C", "C", "C", "C", "C"], &coordinates)
}

fn modes(count: usize, value: f64) -> Vec<f64> {
    let mut values = vec![0.0; 6];
    values.resize(6 + count, value);
    values
}

fn default_conditions() -> ThermoConditions {
    ThermoConditions::default()
}

fn memo() -> ThermoMemo {
    ThermoMemo::default()
}

/// A monomer-shaped ok record with geometry and `mode_value` for all its
/// vibrational modes.
fn monomer(name: &str, electronic_energy: Option<f64>, code: &str, mode_value: f64) -> Record {
    Record {
        name: Some(name.to_string()),
        electronic_energy,
        geometry: Some(hexagon()),
        vibrational_frequencies: Some(modes(12, mode_value)),
        components: Some(components(&[(code, 1)])),
        ..Record::default()
    }
}

/// A record with the same geometry and modes as `template`, but with the
/// electronic energy adjusted so its Gibbs free energy hits `target`
/// exactly (the correction is independent of the electronic energy).
fn with_gibbs(name: &str, code: &str, mode_value: f64, target: f64) -> Record {
    let mut record = monomer(name, Some(0.0), code, mode_value);
    let correction = thermochemistry(&record, &default_conditions())
        .expect("template has defined thermochemistry")
        .gibbs;
    record.electronic_energy = Some(target - correction);
    record
}

#[test] // rq-34845056
fn a_single_component_count_one_record_is_a_monomer() {
    let records = vec![monomer("am1", Some(-50.0), "am", 1000.0)];
    let references = collect_monomers(&records, &memo(), SortKey::El);
    assert_eq!(references.electronic.get("am"), Some(&-50.0));
    assert!(references.gibbs.contains_key("am"));
}

#[test] // rq-56585302
fn multi_component_and_multi_count_records_are_not_monomers() {
    let mut am_two = monomer("am2", Some(-50.0), "am", 1000.0);
    am_two.components = Some(components(&[("am", 2)]));
    let mut mixed = monomer("mix", Some(-60.0), "am", 1000.0);
    mixed.components = Some(components(&[("am", 1), ("sa", 1)]));
    let references = collect_monomers(&[am_two, mixed], &memo(), SortKey::El);
    assert!(references.electronic.is_empty());
    assert!(references.gibbs.is_empty());
}

#[test] // rq-64526ba5
fn without_sort_the_lowest_energy_conformer_is_the_reference() {
    let high = monomer("high", Some(-50.0), "am", 1000.0);
    let low = monomer("low", Some(-50.5), "am", 1000.0);
    let references = collect_monomers(&[high, low], &memo(), SortKey::El);
    assert_eq!(references.electronic.get("am"), Some(&-50.5));
}

#[test] // rq-64a72e10
fn sort_g_makes_the_lowest_gibbs_conformer_the_reference_for_both_levels() {
    // Conformer A has the lowest electronic energy; conformer B has the
    // lowest Gibbs free energy because its twelve soft modes carry far
    // more entropy.
    let a = monomer("A", Some(-50.005), "am", 2000.0);
    let b = monomer("B", Some(-50.0), "am", 30.0);
    let g_a = thermochemistry(&a, &default_conditions()).unwrap().gibbs;
    let g_b = thermochemistry(&b, &default_conditions()).unwrap().gibbs;
    assert!(a.electronic_energy.unwrap() < b.electronic_energy.unwrap());
    assert!(g_b < g_a, "B must hold the lowest gibbs: {g_b} vs {g_a}");
    let references = collect_monomers(&[a.clone(), b.clone()], &memo(), SortKey::G);
    // Both reference levels come from conformer B alone.
    assert_eq!(references.electronic.get("am"), Some(&-50.0));
    let b_gibbs = thermochemistry(&b, &default_conditions()).unwrap().gibbs;
    assert_eq!(references.gibbs.get("am"), Some(&b_gibbs));
}

#[test] // rq-90e2ea3e
fn sort_name_takes_the_nameless_monomer_as_the_reference() {
    // Under `--sort name` the undefined case is the nameless monomer, and
    // nameless orders before named (rqm/formation.md, the nameless-first
    // carve-out of `rqm/filtering.md` Sort) — even when the named monomer
    // has the lower electronic energy.
    let mut nameless = monomer("z", Some(-50.0), "am", 1000.0);
    nameless.name = None;
    let named = monomer("a", Some(-50.5), "am", 1000.0);
    let references = collect_monomers(&[nameless, named], &memo(), SortKey::Name);
    assert_eq!(references.electronic.get("am"), Some(&-50.0));
}

#[test] // rq-7da88087
fn the_reference_s_undefined_level_leaves_that_level_undefined() {
    // Two na monomers: the compact one wins the rg sort but carries no
    // frequencies, so its Gibbs level is absent.
    let mut compact = monomer("compact", Some(-10.0), "na", 1000.0);
    compact.vibrational_frequencies = None;
    compact.geometry = Some(geometry(
        &["H", "O", "H"],
        &[0.0, 0.0, 0.0, 0.0, 0.0, 0.6, 0.0, 0.0, 0.6],
    ));
    let mut extended = monomer("extended", Some(-10.5), "na", 1000.0);
    extended.geometry = Some(geometry(
        &["H", "O", "H"],
        &[0.0, 0.0, 0.0, 0.0, 0.0, 0.6, 5.0, 0.0, 0.6],
    ));
    let references = collect_monomers(&[compact, extended], &memo(), SortKey::Rg);
    assert_eq!(references.electronic.get("na"), Some(&-10.0));
    assert!(!references.gibbs.contains_key("na"));
}

#[test] // rq-536786ea
fn failed_monomer_records_never_qualify() {
    let mut failed = monomer("bad", Some(-99.0), "am", 1000.0);
    failed.status = RecordStatus::Failed;
    failed.error = Some("boom".to_string());
    let ok = monomer("good", Some(-50.0), "am", 1000.0);
    let references = collect_monomers(&[failed, ok], &memo(), SortKey::El);
    assert_eq!(references.electronic.get("am"), Some(&-50.0));
}

#[test] // rq-9063d573
fn a_monomer_without_thermochemistry_has_no_gibbs_reference() {
    let mut no_freqs = monomer("am1", Some(-50.0), "am", 1000.0);
    no_freqs.vibrational_frequencies = None;
    let references = collect_monomers(&[no_freqs], &memo(), SortKey::El);
    assert_eq!(references.electronic.get("am"), Some(&-50.0));
    assert!(!references.gibbs.contains_key("am"));
}

#[test] // rq-44b86475
fn a_2am1sa_cluster_forms_from_its_stoichiometric_monomers() {
    let mut references = MonomerReferences::default();
    references.electronic.insert("am".to_string(), -50.0);
    references.electronic.insert("sa".to_string(), -40.0);
    let cluster = Record {
        name: Some("cluster".to_string()),
        electronic_energy: Some(-141.0),
        components: Some(components(&[("am", 2), ("sa", 1)])),
        ..Record::default()
    };
    let formation = formation_values(&cluster, &references, &memo(), &ConcConditions::default());
    assert_eq!(
        formation,
        Formation {
            electronic: Some(-1.0),
            gibbs: None,
        }
    );
}

#[test] // rq-36de901e
fn a_monomer_s_own_formation_energy_is_zero() {
    let monomer = with_gibbs("am1", "am", 1000.0, -50.0);
    let references = collect_monomers(std::slice::from_ref(&monomer), &memo(), SortKey::El);
    let formation = formation_values(&monomer, &references, &memo(), &ConcConditions::default());
    assert_eq!(formation.electronic, Some(0.0));
    assert_eq!(formation.gibbs, Some(0.0));
}

#[test] // rq-1b5e6c7a
fn a_missing_code_reference_leaves_that_level_undefined() {
    let mut references = MonomerReferences::default();
    references.electronic.insert("am".to_string(), -50.0);
    let cluster = Record {
        name: Some("cluster".to_string()),
        electronic_energy: Some(-60.0),
        components: Some(components(&[("am", 1), ("na", 1)])),
        ..Record::default()
    };
    let formation = formation_values(&cluster, &references, &memo(), &ConcConditions::default());
    assert_eq!(formation.electronic, None);
}

#[test] // rq-650192f2
fn the_two_levels_are_independently_undefined() {
    // The am monomer supplies both levels; the na monomer has an energy
    // but no frequencies, so only the cluster's electronic level forms.
    let am = with_gibbs("am1", "am", 1000.0, -50.0);
    let mut na = monomer("na1", Some(-40.0), "na", 1000.0);
    na.vibrational_frequencies = None;
    let cluster = with_gibbs("cluster", "am", 1000.0, -95.0);
    let mut cluster = cluster;
    cluster.components = Some(components(&[("am", 1), ("na", 1)]));
    let references = collect_monomers(&[am, na], &memo(), SortKey::El);
    let formation = formation_values(&cluster, &references, &memo(), &ConcConditions::default());
    assert!(formation.electronic.is_some());
    assert_eq!(formation.gibbs, None);
}

#[test] // rq-b02bcba1
fn records_without_components_form_at_neither_level() {
    let record = Record {
        name: Some("bare".to_string()),
        electronic_energy: Some(-60.0),
        ..Record::default()
    };
    let formation = formation_values(
        &record,
        &MonomerReferences::default(),
        &memo(),
        &ConcConditions::default(),
    );
    assert_eq!(
        formation,
        Formation {
            electronic: None,
            gibbs: None,
        }
    );
}

// --- Corrected formation section (rqm/formation.md, rqm/correction.md) ---

use complied_jkqc::formation::{
    corrected_formation_values, corrected_monomer_references, print_formation_corr_section,
};
use complied_jkqc::thermo::KCAL_MOL_PER_HARTREE;
use complied_jkqc::Correction;

/// A monomer carrying an attached single-point correction.
fn corrected_monomer(name: &str, electronic_energy: f64, code: &str, correction: f64) -> Record {
    let mut record = monomer(name, Some(electronic_energy), code, 1000.0);
    record.correction = Some(Correction {
        energy: correction,
        program: Some("orca".to_string()),
        program_version: Some("6.0.0".to_string()),
        commands: None,
        kind: "sp".to_string(),
    });
    record
}

fn corrected_cluster(electronic_energy: f64, correction: f64) -> Record {
    let mut cluster = monomer("cluster", Some(electronic_energy), "am", 1000.0);
    cluster.components = Some(components(&[("am", 2)]));
    cluster.correction = Some(Correction {
        energy: correction,
        program: Some("orca".to_string()),
        program_version: Some("6.0.0".to_string()),
        commands: None,
        kind: "sp".to_string(),
    });
    cluster
}

#[test] // rq-39641660
fn the_corrected_section_evaluates_the_attached_corrections() {
    let mon = corrected_monomer("am1", -50.0, "am", -76.5);
    let cluster = corrected_cluster(-141.0, -167.5);
    let references = corrected_monomer_references(std::slice::from_ref(&mon), &memo(), SortKey::El);
    assert_eq!(references.electronic.get("am"), Some(&-76.5));
    let formation =
        corrected_formation_values(&cluster, &references, &memo(), &ConcConditions::default());
    assert_eq!(formation.electronic, Some(-167.5 - 2.0 * -76.5));
}

#[test] // rq-20ab3295
fn a_record_without_a_correction_has_dashes_in_the_corrected_section() {
    let mon = corrected_monomer("am1", -50.0, "am", -76.5);
    let references = corrected_monomer_references(std::slice::from_ref(&mon), &memo(), SortKey::El);
    // The cluster itself carries no correction: both levels are undefined.
    let cluster = monomer("cluster", Some(-141.0), "am", 1000.0);
    let mut cluster = cluster;
    cluster.components = Some(components(&[("am", 2)]));
    let formation =
        corrected_formation_values(&cluster, &references, &memo(), &ConcConditions::default());
    assert_eq!(
        formation,
        Formation {
            electronic: None,
            gibbs: None,
        }
    );
}

#[test] // rq-5ca4f95d
fn a_reference_monomer_without_a_correction_leaves_both_levels_undefined() {
    let mon = monomer("am1", Some(-50.0), "am", 1000.0);
    let references = corrected_monomer_references(std::slice::from_ref(&mon), &memo(), SortKey::El);
    assert!(references.electronic.is_empty());
    assert!(references.gibbs.is_empty());
    let cluster = corrected_cluster(-141.0, -167.5);
    let formation =
        corrected_formation_values(&cluster, &references, &memo(), &ConcConditions::default());
    assert_eq!(formation.electronic, None);
    assert_eq!(formation.gibbs, None);
}

#[test] // rq-b6409abd
fn the_corrected_section_reuses_the_raw_sections_reference_monomers() {
    // Two am monomers; the raw sort by el picks the first (more stable)
    // one, so the corrected references must come from that same monomer
    // and not from the other one.
    let first = corrected_monomer("am1", -50.0, "am", -76.5);
    let second = corrected_monomer("am2", -49.0, "am", -75.0);
    let references = corrected_monomer_references(&[first, second], &memo(), SortKey::El);
    assert_eq!(references.electronic.get("am"), Some(&-76.5));
    assert_ne!(references.electronic.get("am"), Some(&-75.0));
}

#[test] // rq-6e0560c2
fn the_corrected_section_prints_in_the_runs_unit() {
    let mon = corrected_monomer("am1", -50.0, "am", -76.5);
    let cluster = corrected_cluster(-141.0, -167.5);
    let references = corrected_monomer_references(std::slice::from_ref(&mon), &memo(), SortKey::El);
    let hartree = print_formation_corr_section(
        std::slice::from_ref(&cluster),
        &references,
        &memo(),
        Unit::Au,
        &ConcConditions::default(),
    );
    let kcal = print_formation_corr_section(
        std::slice::from_ref(&cluster),
        &references,
        &memo(),
        Unit::Kcal,
        &ConcConditions::default(),
    );
    // The corrected formation energy is -167.5 - 2*(-76.5) = -14.5 Eh.
    let expected = -14.5 * KCAL_MOL_PER_HARTREE;
    assert!(hartree.starts_with("ct elcorr gcorr"));
    let row = kcal.lines().nth(1).expect("one row");
    let value: f64 = row
        .split(' ')
        .nth(1)
        .expect("elcorr cell")
        .parse()
        .expect("number");
    assert!((value - expected).abs() < 1e-3, "{kcal}");
}

#[test] // rq-f99b56ad
fn filtered_away_monomers_serve_as_no_corrected_references() {
    // The run passes only its survivors: the monomer was filtered away,
    // so the cluster's corrected levels are undefined even though the
    // cluster carries a correction.
    let cluster = corrected_cluster(-141.0, -167.5);
    let references =
        corrected_monomer_references(std::slice::from_ref(&cluster), &memo(), SortKey::El);
    let formation =
        corrected_formation_values(&cluster, &references, &memo(), &ConcConditions::default());
    assert_eq!(formation.electronic, None);
    assert_eq!(formation.gibbs, None);
}

#[test] // rq-67b44e63
fn corrected_references_are_independent_per_level() {
    // The selected monomer carries a correction energy but undefined
    // corrected Gibbs (no frequencies beyond the silent six): the
    // electronic reference exists while the Gibbs reference does not.
    let mut mon = corrected_monomer("am1", -50.0, "am", -76.5);
    mon.vibrational_frequencies = None;
    let references = corrected_monomer_references(std::slice::from_ref(&mon), &memo(), SortKey::El);
    assert_eq!(references.electronic.get("am"), Some(&-76.5));
    assert!(!references.gibbs.contains_key("am"));
}

// --- Concentrations (rqm/formation.md, Formation at Given Concentrations) ---

use complied_jkqc::formation::{concentration_correction, parse_conc_value, ConcSpec, ConcUnit};
use complied_jkqc::thermo::{E_HARTREE, K_B, P_STANDARD};

/// The per-molecule R T in Eh at 298.15 K: k_B T / E_h.
fn kbt() -> f64 {
    K_B * 298.15 / E_HARTREE
}

#[test] // rq-e982f0e0
fn a_plain_value_is_a_partial_pressure_in_pa() {
    assert_eq!(parse_conc_value("0.00001"), Some((0.00001, ConcUnit::Pa)));
}

#[test] // rq-d7a0c24f
fn a_ppt_value_is_a_mole_fraction_of_the_standard_atmosphere() {
    let (magnitude, unit) = parse_conc_value("100ppt").expect("parses");
    assert_eq!(magnitude, 100.0);
    assert_eq!(unit, ConcUnit::Ppt);
    let spec = ConcSpec {
        code: "sa".to_string(),
        magnitude,
        unit,
    };
    assert_eq!(spec.pressure(298.15), 100.0 * 1.0e-12 * P_STANDARD);
    assert!((spec.pressure(298.15) - 1.01325e-5).abs() < 1e-20);
}

#[test] // rq-183b0f2b
fn a_ppb_value_is_a_mole_fraction_of_the_standard_atmosphere() {
    let (magnitude, unit) = parse_conc_value("10ppb").expect("parses");
    assert_eq!(magnitude, 10.0);
    assert_eq!(unit, ConcUnit::Ppb);
    let spec = ConcSpec {
        code: "sa".to_string(),
        magnitude,
        unit,
    };
    assert_eq!(spec.pressure(298.15), 10.0 * 1.0e-9 * P_STANDARD);
    assert!((spec.pressure(298.15) - 1.01325e-3).abs() < 1e-18);
}

#[test] // rq-0f5b209d
fn a_cmmc_value_converts_through_the_ideal_gas_law_at_the_runs_temperature() {
    let (magnitude, unit) = parse_conc_value("1e10cmmc").expect("parses");
    assert_eq!(magnitude, 1.0e10);
    assert_eq!(unit, ConcUnit::Cmmc);
    let spec = ConcSpec {
        code: "sa".to_string(),
        magnitude,
        unit,
    };
    assert_eq!(spec.pressure(298.15), 1.0e10 * 1.0e6 * K_B * 298.15);
    assert!((spec.pressure(298.15) - 4.1164e-5).abs() < 1e-9);
    assert_eq!(spec.pressure(350.0), 1.0e10 * 1.0e6 * K_B * 350.0);
    assert!((spec.pressure(350.0) - 4.8323e-5).abs() < 1e-9);
}

#[test] // rq-bbf4e72d
fn a_component_without_a_spec_stays_at_the_standard_state() {
    // A 2am1sa record with defined gibbs and one sa-only spec: the
    // correction carries exactly one sa term and no am term.
    let am = with_gibbs("am1", "am", 1000.0, -50.0);
    let sa = with_gibbs("sa1", "sa", 1000.0, -40.0);
    let mut cluster = with_gibbs("cluster", "am", 1000.0, -141.0);
    cluster.components = Some(components(&[("am", 2), ("sa", 1)]));
    let references = collect_monomers(&[am, sa], &memo(), SortKey::El);
    let concs = ConcConditions {
        specs: vec![ConcSpec {
            code: "sa".to_string(),
            magnitude: 100.0,
            unit: ConcUnit::Ppt,
        }],
        self_consistent: false,
    };
    let formation = formation_values(&cluster, &references, &memo(), &concs);
    let uncorrected = formation_values(&cluster, &references, &memo(), &ConcConditions::default());
    let shift = kbt() * (100.0_f64 * 1.0e-12).ln();
    assert_eq!(formation.gibbs, Some(uncorrected.gibbs.unwrap() - shift));
}

#[test] // rq-1b039c1f
fn cnt_scales_a_fully_covered_clusters_correction_by_n_minus_one_over_n() {
    // A 3sa record: without --cnt the correction is 3 R T ln(c/p°); with
    // --cnt it is 3·(3−1)/3 = 2 R T ln(c/p°).
    let sa = with_gibbs("sa1", "sa", 1000.0, -40.0);
    let mut cluster = with_gibbs("cluster", "sa", 1000.0, -121.0);
    cluster.components = Some(components(&[("sa", 3)]));
    let references = collect_monomers(std::slice::from_ref(&sa), &memo(), SortKey::El);
    let spec = ConcSpec {
        code: "sa".to_string(),
        magnitude: 100.0,
        unit: ConcUnit::Ppt,
    };
    let off = ConcConditions {
        specs: vec![spec.clone()],
        self_consistent: false,
    };
    let on = ConcConditions {
        specs: vec![spec],
        self_consistent: true,
    };
    let uncorrected = formation_values(&cluster, &references, &memo(), &ConcConditions::default())
        .gibbs
        .unwrap();
    let shift = kbt() * (100.0_f64 * 1.0e-12).ln();
    assert_eq!(
        formation_values(&cluster, &references, &memo(), &off).gibbs,
        Some(uncorrected - 3.0 * shift)
    );
    assert_eq!(
        formation_values(&cluster, &references, &memo(), &on).gibbs,
        Some(uncorrected - 2.0 * shift)
    );
}

#[test] // rq-2333d9cc
fn a_spec_whose_code_is_absent_contributes_nothing() {
    let components = components(&[("am", 2)]);
    let concs = ConcConditions {
        specs: vec![ConcSpec {
            code: "sa".to_string(),
            magnitude: 100.0,
            unit: ConcUnit::Ppt,
        }],
        self_consistent: true,
    };
    assert_eq!(concentration_correction(&components, &concs, 298.15), 0.0);
}

#[test] // rq-2333d9cc
fn cnt_sums_n_over_the_whole_components_map() {
    // 2am1sa with only sa spec'd: the un-spec'd am molecules still count
    // toward N, so the sa term scales by 1·(3−1)/3 = 2/3, not by 1.
    let components = components(&[("am", 2), ("sa", 1)]);
    let on = ConcConditions {
        specs: vec![ConcSpec {
            code: "sa".to_string(),
            magnitude: 100.0,
            unit: ConcUnit::Ppt,
        }],
        self_consistent: true,
    };
    let expected = (2.0f64 / 3.0) * K_B * 298.15 * (100.0e-12_f64).ln() / E_HARTREE;
    assert!((concentration_correction(&components, &on, 298.15) - expected).abs() < 1e-18);
}
