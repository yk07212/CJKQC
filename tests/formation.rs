//! Integration tests for formation energies from monomer references, one
//! per Gherkin scenario of `rqm/formation.md` exercised at the API level.
//! CLI-shaped scenarios live in `tests/cli.rs`.

use compiled_jkqc::filter::{SortKey, Unit};
use compiled_jkqc::formation::{
    collect_monomers, formation_values, ConcConditions, Formation, FormationSettings,
    MonomerReferences,
};
use compiled_jkqc::thermo::{thermochemistry, ThermoConditions, ThermoMemo};
use compiled_jkqc::{Components, Geometry, Record, RecordStatus};

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
    assert!(references.enthalpy.contains_key("am"));
    assert!(references.entropy_term.contains_key("am"));
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
    assert!(references.enthalpy.is_empty());
    assert!(references.entropy_term.is_empty());
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
fn sort_g_makes_the_lowest_gibbs_conformer_the_reference_for_every_level() {
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
    // Every reference level comes from conformer B alone.
    assert_eq!(references.electronic.get("am"), Some(&-50.0));
    let b_thermo = thermochemistry(&b, &default_conditions()).unwrap();
    assert_eq!(references.enthalpy.get("am"), Some(&b_thermo.enthalpy));
    assert_eq!(
        references.entropy_term.get("am"),
        Some(&b_thermo.entropy_term)
    );
    assert_eq!(references.gibbs.get("am"), Some(&b_thermo.gibbs));
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
    let references = collect_monomers(&[nameless, named], &memo(), SortKey::Filename);
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
    assert!(!references.enthalpy.contains_key("na"));
    assert!(!references.entropy_term.contains_key("na"));
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
fn a_monomer_without_thermochemistry_has_no_thermal_reference() {
    let mut no_freqs = monomer("am1", Some(-50.0), "am", 1000.0);
    no_freqs.vibrational_frequencies = None;
    let references = collect_monomers(&[no_freqs], &memo(), SortKey::El);
    assert_eq!(references.electronic.get("am"), Some(&-50.0));
    assert!(!references.enthalpy.contains_key("am"));
    assert!(!references.entropy_term.contains_key("am"));
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
            enthalpy: None,
            entropy_term: None,
            gibbs: None,
        }
    );
}

/// A cluster-shaped record with the same geometry and modes as the
/// monomer template, but with the electronic energy adjusted so its
/// enthalpy hits `target` exactly (the derivation is linear in the
/// electronic energy).
fn with_enthalpy(name: &str, pairs: &[(&str, u32)], mode_value: f64, target: f64) -> Record {
    let mut record = Record {
        name: Some(name.to_string()),
        electronic_energy: Some(0.0),
        geometry: Some(hexagon()),
        vibrational_frequencies: Some(modes(12, mode_value)),
        components: Some(components(pairs)),
        ..Record::default()
    };
    let thermal = thermochemistry(&record, &default_conditions())
        .expect("template has defined thermochemistry")
        .enthalpy;
    record.electronic_energy = Some(target - thermal);
    record
}

#[test] // rq-cfc7f6f3
fn a_2am1sa_cluster_has_a_stoichiometric_formation_enthalpy() {
    // Enthalpy references am = -49.9 and sa = -39.9; a cluster record
    // with components am = 2 sa = 1 and enthalpy -140.7: the formation
    // enthalpy is -140.7 - 2*(-49.9) - (-39.9) = -1.0 Eh.
    let mut references = MonomerReferences::default();
    references.enthalpy.insert("am".to_string(), -49.9);
    references.enthalpy.insert("sa".to_string(), -39.9);
    let cluster = with_enthalpy("cluster", &[("am", 2), ("sa", 1)], 1000.0, -140.7);
    let enthalpy = thermochemistry(&cluster, &default_conditions())
        .unwrap()
        .enthalpy;
    assert!((enthalpy - (-140.7)).abs() < 1e-12);
    let formation = formation_values(&cluster, &references, &memo(), &ConcConditions::default());
    assert!((formation.enthalpy.unwrap() - (-1.0)).abs() < 1e-12);
}

#[test] // rq-91cdbab5
fn a_2am1sa_cluster_has_a_stoichiometric_formation_entropy_term() {
    // T*S references am = 0.01 and sa = 0.02; a cluster whose own T*S is
    // its derivation's: the formation entropy term is that value minus
    // 2*0.01 + 0.02, and the formation free energy is the formation
    // enthalpy minus the formation entropy term.
    let mut references = MonomerReferences::default();
    references.enthalpy.insert("am".to_string(), -49.9);
    references.enthalpy.insert("sa".to_string(), -39.9);
    references.entropy_term.insert("am".to_string(), 0.01);
    references.entropy_term.insert("sa".to_string(), 0.02);
    references.gibbs.insert("am".to_string(), -49.9 - 0.01);
    references.gibbs.insert("sa".to_string(), -39.9 - 0.02);
    let cluster = with_enthalpy("cluster", &[("am", 2), ("sa", 1)], 1000.0, -140.7);
    let ts = thermochemistry(&cluster, &default_conditions())
        .expect("cluster has defined thermochemistry")
        .entropy_term;
    let formation = formation_values(&cluster, &references, &memo(), &ConcConditions::default());
    assert_eq!(formation.entropy_term, Some(ts - 2.0 * 0.01 - 0.02));
    // g_form = h_form - st_form (rqm/formation.md Formation Values).
    assert!(
        (formation.gibbs.unwrap()
            - (formation.enthalpy.unwrap() - formation.entropy_term.unwrap()))
        .abs()
            < 1e-12
    );
}

#[test] // rq-36de901e
fn a_monomer_s_own_formation_values_are_zero() {
    let monomer = with_gibbs("am1", "am", 1000.0, -50.0);
    let references = collect_monomers(std::slice::from_ref(&monomer), &memo(), SortKey::El);
    let formation = formation_values(&monomer, &references, &memo(), &ConcConditions::default());
    assert_eq!(formation.electronic, Some(0.0));
    assert_eq!(formation.enthalpy, Some(0.0));
    assert_eq!(formation.entropy_term, Some(0.0));
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
fn the_electronic_level_is_independent_of_the_thermal_levels() {
    // The am monomer supplies every level; the na monomer has an energy
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
    assert_eq!(formation.enthalpy, None);
    assert_eq!(formation.entropy_term, None);
    assert_eq!(formation.gibbs, None);
}

#[test] // rq-22dd2e0b
fn a_thermal_reference_gap_leaves_the_three_thermal_levels_undefined_together() {
    // The am monomer carries thermochemistry; the na monomer has an
    // energy but no frequencies, so its thermal references are absent:
    // the cluster's thermal levels are undefined together while the
    // electronic level still forms.
    let am = with_gibbs("am1", "am", 1000.0, -50.0);
    let mut na = monomer("na1", Some(-40.0), "na", 1000.0);
    na.vibrational_frequencies = None;
    let cluster = with_gibbs("cluster", "am", 1000.0, -95.0);
    let mut cluster = cluster;
    cluster.components = Some(components(&[("am", 1), ("na", 1)]));
    let references = collect_monomers(&[am, na], &memo(), SortKey::El);
    // The thermal reference gap: am has thermal references, na does not.
    assert!(references.enthalpy.contains_key("am"));
    assert!(!references.enthalpy.contains_key("na"));
    let formation = formation_values(&cluster, &references, &memo(), &ConcConditions::default());
    // Both electronic references exist, so the electronic level is Some.
    let electronic = formation.electronic.unwrap();
    assert!((electronic - (-5.0)).abs() < 1e-12);
    // The three thermal levels are undefined together.
    assert_eq!(formation.enthalpy, None);
    assert_eq!(formation.entropy_term, None);
    assert_eq!(formation.gibbs, None);
}

#[test] // rq-b02bcba1
fn records_without_components_form_at_no_level() {
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
            enthalpy: None,
            entropy_term: None,
            gibbs: None,
        }
    );
}

// --- Corrected formation section (rqm/formation.md, rqm/correction.md) ---

use compiled_jkqc::formation::{
    corrected_formation_values, corrected_monomer_references, print_formation_corr_section,
};
use compiled_jkqc::thermo::KCAL_MOL_PER_HARTREE;
use compiled_jkqc::Correction;

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
    // The cluster itself carries no correction: every level is undefined.
    let cluster = monomer("cluster", Some(-141.0), "am", 1000.0);
    let mut cluster = cluster;
    cluster.components = Some(components(&[("am", 2)]));
    let formation =
        corrected_formation_values(&cluster, &references, &memo(), &ConcConditions::default());
    assert_eq!(
        formation,
        Formation {
            electronic: None,
            enthalpy: None,
            entropy_term: None,
            gibbs: None,
        }
    );
}

#[test] // rq-5ca4f95d
fn a_reference_monomer_without_a_correction_leaves_every_level_undefined() {
    let mon = monomer("am1", Some(-50.0), "am", 1000.0);
    let references = corrected_monomer_references(std::slice::from_ref(&mon), &memo(), SortKey::El);
    assert!(references.electronic.is_empty());
    assert!(references.enthalpy.is_empty());
    assert!(references.entropy_term.is_empty());
    assert!(references.gibbs.is_empty());
    let cluster = corrected_cluster(-141.0, -167.5);
    let formation =
        corrected_formation_values(&cluster, &references, &memo(), &ConcConditions::default());
    assert_eq!(formation.electronic, None);
    assert_eq!(formation.enthalpy, None);
    assert_eq!(formation.entropy_term, None);
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
        &FormationSettings {
            unit: Unit::Au,
            ..FormationSettings::default()
        },
    );
    let kcal = print_formation_corr_section(
        std::slice::from_ref(&cluster),
        &references,
        &memo(),
        &FormationSettings {
            unit: Unit::Kcal,
            ..FormationSettings::default()
        },
    );
    // The corrected formation energy is -167.5 - 2*(-76.5) = -14.5 Eh.
    let expected = -14.5 * KCAL_MOL_PER_HARTREE;
    let header: Vec<&str> = hartree
        .lines()
        .next()
        .expect("header row")
        .split_whitespace()
        .collect();
    assert_eq!(header, ["name", "elcorr", "hcorr", "stcorr", "gcorr"]);
    let row = kcal.lines().nth(1).expect("one row");
    let value: f64 = row
        .split_whitespace()
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
    assert_eq!(formation.enthalpy, None);
    assert_eq!(formation.entropy_term, None);
    assert_eq!(formation.gibbs, None);
}

#[test] // rq-67b44e63
fn corrected_references_are_independent_per_level() {
    // The selected monomer carries a correction energy but undefined
    // corrected Gibbs (no frequencies beyond the silent six): the
    // electronic reference exists while the thermal references do not.
    let mut mon = corrected_monomer("am1", -50.0, "am", -76.5);
    mon.vibrational_frequencies = None;
    let references = corrected_monomer_references(std::slice::from_ref(&mon), &memo(), SortKey::El);
    assert_eq!(references.electronic.get("am"), Some(&-76.5));
    assert!(!references.enthalpy.contains_key("am"));
    assert!(!references.entropy_term.contains_key("am"));
    assert!(!references.gibbs.contains_key("am"));
}

#[test] // rq-cdb75b97
fn the_corrected_formation_enthalpy_subtracts_the_corrected_monomer_enthalpies() {
    // A corrected 1am1sa cluster against corrected am and sa monomers:
    // the corrected formation enthalpy is hcorr(cluster) - hcorr(am) -
    // hcorr(sa), and the corrected formation enthalpy minus the
    // corrected formation entropy term equals the corrected formation
    // free energy.
    let am = corrected_monomer("am1", -50.0, "am", -76.5);
    let sa = corrected_monomer("sa1", -40.0, "sa", -69.5);
    let mut cluster = corrected_cluster(-141.0, -167.5);
    cluster.components = Some(components(&[("am", 1), ("sa", 1)]));
    let memo = memo();
    let references = corrected_monomer_references(&[am.clone(), sa.clone()], &memo, SortKey::El);
    let formation =
        corrected_formation_values(&cluster, &references, &memo, &ConcConditions::default());
    let hcorr = |record: &Record| memo.value("hcorr", record).unwrap();
    assert_eq!(
        formation.enthalpy,
        Some(hcorr(&cluster) - hcorr(&am) - hcorr(&sa))
    );
    // The corrected levels satisfy gcorr_form = hcorr_form - stcorr_form
    // (rqm/formation.md Corrected Formation Section).
    assert!(
        (formation.gibbs.unwrap()
            - (formation.enthalpy.unwrap() - formation.entropy_term.unwrap()))
        .abs()
            < 1e-12
    );
}

#[test] // rq-4ca420a5
fn the_corrected_entropy_term_equals_the_raw_entropy_term() {
    // The correction does not alter any thermal term, so the corrected
    // section's st value equals the raw section's st whenever both are
    // defined — here through the same record's two formation values.
    let am = corrected_monomer("am1", -50.0, "am", -76.5);
    let sa = corrected_monomer("sa1", -40.0, "sa", -69.5);
    let mut cluster = corrected_cluster(-141.0, -167.5);
    cluster.components = Some(components(&[("am", 1), ("sa", 1)]));
    let memo = memo();
    let raw_references = collect_monomers(&[am.clone(), sa.clone()], &memo, SortKey::El);
    let corrected_references = corrected_monomer_references(&[am, sa], &memo, SortKey::El);
    let raw = formation_values(&cluster, &raw_references, &memo, &ConcConditions::default());
    let corrected = corrected_formation_values(
        &cluster,
        &corrected_references,
        &memo,
        &ConcConditions::default(),
    );
    assert_eq!(corrected.entropy_term, raw.entropy_term);
}

// --- Concentrations (rqm/formation.md, Formation at Given Concentrations) ---

use compiled_jkqc::formation::{concentration_correction, parse_conc_value, ConcSpec, ConcUnit};
use compiled_jkqc::thermo::{E_HARTREE, K_B, P_STANDARD};

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

// --- Boltzmann-averaged sections (rqm/formation.md, rq-dd65e289) ---

use compiled_jkqc::correction::corrected_energy;
use compiled_jkqc::formation::{
    collapse_ensembles, corrected_collapse_ensembles, corrected_ensemble_formation_values,
    corrected_ensemble_references, ensemble_formation_values, ensemble_references,
    print_formation_ensembles, Ensemble,
};

/// A cluster-shaped record whose Gibbs free energy hits `target` exactly
/// at the default conditions (the electronic energy absorbs the
/// correction).
fn cluster_with_gibbs(name: &str, pairs: &[(&str, u32)], mode_value: f64, target: f64) -> Record {
    let mut record = Record {
        name: Some(name.to_string()),
        electronic_energy: Some(0.0),
        geometry: Some(hexagon()),
        vibrational_frequencies: Some(modes(12, mode_value)),
        components: Some(components(pairs)),
        ..Record::default()
    };
    retarget_gibbs(&mut record, target);
    record
}

/// Re-points the record's electronic energy so its Gibbs free energy hits
/// `target` exactly at the default conditions (the derivation is linear
/// in the electronic energy).
fn retarget_gibbs(record: &mut Record, target: f64) {
    record.electronic_energy = Some(0.0);
    let terms = thermochemistry(record, &default_conditions())
        .expect("template has defined thermochemistry")
        .gibbs;
    record.electronic_energy = Some(target - terms);
}

/// A copy of `template` with the given spin multiplicity: the electronic
/// entropy term R·ln(m) shifts the Gibbs value by exactly k_B·T·ln(m)
/// while the electronic energy stays put (rqm/thermo.md).
fn with_multiplicity(template: &Record, multiplicity: u32) -> Record {
    let mut record = template.clone();
    record.multiplicity = Some(multiplicity);
    record
}

/// A record with an attached correction whose Gibbs free energy hits
/// `target` exactly at the default conditions.
fn corrected_with_gibbs(
    name: &str,
    pairs: &[(&str, u32)],
    mode_value: f64,
    target: f64,
    correction_energy: f64,
) -> Record {
    let mut record = cluster_with_gibbs(name, pairs, mode_value, target);
    record.correction = Some(Correction {
        energy: correction_energy,
        program: Some("orca".to_string()),
        program_version: Some("6.0.0".to_string()),
        commands: None,
        kind: "sp".to_string(),
    });
    record
}

/// Tolerance for the ensemble arithmetic: the Boltzmann weights carry a
/// few units in the last place, so the exact identities hold only to f64
/// rounding of the energies (~1e-14 at 100 Eh magnitudes).
const EPS: f64 = 1e-12;

#[test] // rq-7d43ccc9
fn the_collapsed_g_includes_the_conformational_entropy_term() {
    // Two 2am1sa conformers with equal electronic energies whose Gibbs
    // values differ by exactly kB*T*ln(3): the multiplicity-3 copy holds
    // R*ln(3) more electronic entropy. The collapse weights are 3/4 and
    // 1/4, so the stabilization is k_B*T*ln(4/3).
    let template = cluster_with_gibbs("a", &[("am", 2), ("sa", 1)], 1000.0, -91.0);
    let excited = with_multiplicity(&template, 3);
    let am = with_gibbs("am1", "am", 1000.0, -50.0);
    let sa = with_gibbs("sa1", "sa", 1000.0, -40.0);
    let memo = memo();
    // The multiplicity-3 copy holds R*ln(3) more electronic entropy, so
    // its Gibbs value sits exactly kB*T*ln(3) BELOW the template's — it
    // is the best conformer.
    let gap = memo.value("g", &excited).unwrap() - memo.value("g", &template).unwrap();
    assert!((gap + kbt() * 3.0f64.ln()).abs() < EPS, "fixture gap {gap}");
    let records = vec![template, excited, am, sa];
    let ensembles = collapse_ensembles(&records, &memo);
    assert_eq!(ensembles.len(), 3);
    let references = ensemble_references(&ensembles);
    let ensemble = ensemble_formation_values(
        &ensembles[0],
        &references,
        &memo,
        &ConcConditions::default(),
    );
    let record_references = collect_monomers(&records, &memo, SortKey::El);
    let conformer = formation_values(
        &records[1],
        &record_references,
        &memo,
        &ConcConditions::default(),
    );
    // The collapsed el equals either conformer's: equal electronic
    // energies make the population-weighted mean that energy.
    assert!((ensemble.electronic.unwrap() - conformer.electronic.unwrap()).abs() < EPS);
    // The collapsed h equals either conformer's: equal enthalpies make
    // the population-weighted mean that enthalpy.
    assert!((ensemble.enthalpy.unwrap() - conformer.enthalpy.unwrap()).abs() < EPS);
    // The collapsed g sits k_B*T*ln(4/3) below the best conformer's.
    assert!(
        (ensemble.gibbs.unwrap() - (conformer.gibbs.unwrap() - kbt() * (4.0f64 / 3.0).ln())).abs()
            < EPS,
        "ensemble g {:?} vs conformer g {:?}",
        ensemble.gibbs,
        conformer.gibbs
    );
    // The collapsed st carries the same k_B*T*ln(4/3) shift relative to
    // the best conformer's: the conformational stabilization appears in
    // the entropy term rather than lowering the enthalpy.
    assert!(
        (ensemble.entropy_term.unwrap()
            - (conformer.entropy_term.unwrap() + kbt() * (4.0f64 / 3.0).ln()))
        .abs()
            < EPS,
        "ensemble st {:?} vs conformer st {:?}",
        ensemble.entropy_term,
        conformer.entropy_term
    );
}

#[test] // rq-df7a2c88
fn the_collapsed_entropy_term_is_the_mean_enthalpy_minus_the_ensemble_gibbs() {
    // Two 2am1sa conformers with equal electronic energies and
    // enthalpies whose Gibbs values differ by exactly kB*T*ln(3): the
    // collapsed row's h is the population-weighted mean of the
    // conformers' h formation values, its st is that mean minus the
    // ensemble free energy, and its g is the ensemble free energy.
    let template = cluster_with_gibbs("a", &[("am", 2), ("sa", 1)], 1000.0, -91.0);
    let excited = with_multiplicity(&template, 3);
    let am = with_gibbs("am1", "am", 1000.0, -50.0);
    let sa = with_gibbs("sa1", "sa", 1000.0, -40.0);
    let memo = memo();
    let records = vec![template, excited, am, sa];
    let ensembles = collapse_ensembles(&records, &memo);
    let references = ensemble_references(&ensembles);
    let ensemble = ensemble_formation_values(
        &ensembles[0],
        &references,
        &memo,
        &ConcConditions::default(),
    );
    let record_references = collect_monomers(&records, &memo, SortKey::El);
    let formations: Vec<Formation> = records
        .iter()
        .take(2)
        .map(|record| {
            formation_values(
                record,
                &record_references,
                &memo,
                &ConcConditions::default(),
            )
        })
        .collect();
    // The members' Boltzmann populations: the multiplicity-3 conformer
    // (records[1]) holds the lower Gibbs value, so its weight is 3/4.
    let weights = [0.25, 0.75];
    let mean_h =
        weights[0] * formations[0].enthalpy.unwrap() + weights[1] * formations[1].enthalpy.unwrap();
    assert!(
        (ensemble.enthalpy.unwrap() - mean_h).abs() < EPS,
        "collapsed h {:?} vs mean {:?}",
        ensemble.enthalpy,
        mean_h
    );
    // st_ens = h_ens - g_ens (rqm/formation.md Boltzmann-Averaged
    // Sections).
    assert!(
        (ensemble.entropy_term.unwrap() - (ensemble.enthalpy.unwrap() - ensemble.gibbs.unwrap()))
            .abs()
            < EPS
    );
    // g_ens is the ensemble free energy: g_min - k_B*T*ln(4/3).
    assert!(
        (ensemble.gibbs.unwrap() - (formations[1].gibbs.unwrap() - kbt() * (4.0f64 / 3.0).ln()))
            .abs()
            < EPS
    );
}

#[test] // rq-f524bb50
fn the_collapsed_el_is_the_population_weighted_mean() {
    // Two 2am1sa conformers with equal Gibbs values and different
    // electronic energies: the weights are exactly 1/2 each, so the
    // collapsed el is the midpoint of the two conformers' el formation
    // values whatever their gap.
    let base = cluster_with_gibbs("a", &[("am", 2), ("sa", 1)], 1000.0, -91.0);
    let mut partner = base.clone();
    partner.name = Some("b".to_string());
    partner.multiplicity = Some(3);
    // Re-hit the same Gibbs target with the multiplicity shifted, so the
    // electronic energies come apart while the Gibbs values stay equal.
    retarget_gibbs(&mut partner, -91.0);
    let am = with_gibbs("am1", "am", 1000.0, -50.0);
    let sa = with_gibbs("sa1", "sa", 1000.0, -40.0);
    let memo = memo();
    assert!((memo.value("g", &partner).unwrap() - memo.value("g", &base).unwrap()).abs() < EPS);
    assert!(
        (partner.electronic_energy.unwrap() - base.electronic_energy.unwrap()).abs() > 1.0e-6,
        "the fixture's electronic energies must differ"
    );
    let records = vec![base, partner, am, sa];
    let ensembles = collapse_ensembles(&records, &memo);
    let references = ensemble_references(&ensembles);
    let ensemble = ensemble_formation_values(
        &ensembles[0],
        &references,
        &memo,
        &ConcConditions::default(),
    );
    let record_references = collect_monomers(&records, &memo, SortKey::El);
    let first = formation_values(
        &records[0],
        &record_references,
        &memo,
        &ConcConditions::default(),
    );
    let second = formation_values(
        &records[1],
        &record_references,
        &memo,
        &ConcConditions::default(),
    );
    let midpoint = (first.electronic.unwrap() + second.electronic.unwrap()) / 2.0;
    assert!((ensemble.electronic.unwrap() - midpoint).abs() < EPS);
}

#[test] // rq-f71670c3
fn monomer_rows_print_exactly_zero() {
    // Two am conformers with defined thermochemistry: the monomer
    // ensemble supplies its own reference, so the 1am row prints 0 at
    // every level.
    let first = with_gibbs("am1", "am", 1000.0, -50.0);
    let second = with_multiplicity(&first, 3);
    let cluster = cluster_with_gibbs("cluster", &[("am", 2)], 1000.0, -101.0);
    let memo = memo();
    let records = vec![first, second, cluster];
    let ensembles = collapse_ensembles(&records, &memo);
    let references = ensemble_references(&ensembles);
    let section = print_formation_ensembles(
        &ensembles,
        &references,
        &memo,
        &FormationSettings::default(),
    );
    let row = section
        .lines()
        .find(|line| line.starts_with("1am "))
        .expect("1am row");
    let values: Vec<f64> = row
        .split_whitespace()
        .skip(1)
        .map(|cell| cell.parse::<f64>().expect("number"))
        .collect();
    assert_eq!(values.len(), 4);
    assert!(values.iter().all(|value| value.abs() < EPS), "{section}");
}

#[test] // rq-f93d319b
fn undefined_members_never_poison_the_group() {
    // Two full conformers with the kB*T*ln(3) Gibbs gap plus one without
    // frequencies: the valueless record contributes nothing and the
    // surviving members define the ensemble.
    let template = cluster_with_gibbs("a", &[("am", 2), ("sa", 1)], 1000.0, -91.0);
    let excited = with_multiplicity(&template, 3);
    let mut valueless = cluster_with_gibbs("c", &[("am", 2), ("sa", 1)], 1000.0, -90.0);
    valueless.vibrational_frequencies = None;
    let am = with_gibbs("am1", "am", 1000.0, -50.0);
    let sa = with_gibbs("sa1", "sa", 1000.0, -40.0);
    let memo = memo();
    let records = vec![template, excited, valueless, am, sa];
    let ensembles = collapse_ensembles(&records, &memo);
    assert_eq!(ensembles.len(), 3);
    let references = ensemble_references(&ensembles);
    let ensemble = ensemble_formation_values(
        &ensembles[0],
        &references,
        &memo,
        &ConcConditions::default(),
    );
    let record_references = collect_monomers(&records, &memo, SortKey::El);
    // The multiplicity-3 copy holds the lower Gibbs value: it is the
    // best conformer.
    let best = formation_values(
        &records[1],
        &record_references,
        &memo,
        &ConcConditions::default(),
    );
    assert!(
        (ensemble.gibbs.unwrap() - (best.gibbs.unwrap() - kbt() * (4.0f64 / 3.0).ln())).abs() < EPS
    );
}

#[test] // rq-482ab490
fn failed_records_contribute_nothing_to_the_collapse() {
    // The failed record keeps its full observables: it is excluded by
    // status, not by definedness — and its lower energy would dominate
    // the ensemble if the ok-gate regressed.
    let failed = cluster_with_gibbs("bad", &[("am", 2), ("sa", 1)], 1000.0, -95.0);
    let mut failed = failed;
    failed.status = RecordStatus::Failed;
    failed.error = Some("boom".to_string());
    let ok = cluster_with_gibbs("good", &[("am", 2), ("sa", 1)], 1000.0, -91.5);
    let am = with_gibbs("am1", "am", 1000.0, -50.0);
    let sa = with_gibbs("sa1", "sa", 1000.0, -40.0);
    let memo = memo();
    let records = vec![failed, ok, am, sa];
    let ensembles = collapse_ensembles(&records, &memo);
    assert_eq!(ensembles.len(), 3);
    // The 2am1sa ensemble is the ok record alone: with exactly its
    // values, never an average that includes the failed record.
    assert_eq!(ensembles[0].electronic, records[1].electronic_energy);
    assert_eq!(ensembles[0].gibbs, memo.value("g", &records[1]));
}

#[test] // rq-deca4a24
fn the_corrected_section_collapses_at_its_own_level() {
    // Two corrected 1am1sa conformers with equal correction energies
    // whose gcorr values differ by exactly kB*T*ln(3) — the electronic
    // entropy of the multiplicity-3 copy — and corrected monomers.
    let template = corrected_with_gibbs("a", &[("am", 1), ("sa", 1)], 1000.0, -61.0, -5.0);
    let excited = with_multiplicity(&template, 3);
    let am = corrected_with_gibbs("am1", &[("am", 1)], 1000.0, -50.0, -5.0);
    let sa = corrected_with_gibbs("sa1", &[("sa", 1)], 1000.0, -40.0, -5.0);
    let memo = memo();
    // The multiplicity-3 copy holds R*ln(3) more electronic entropy, so
    // its gcorr value sits exactly kB*T*ln(3) BELOW the template's — it
    // is the best conformer; the correction energies are equal.
    let gap = memo.value("gcorr", &excited).unwrap() - memo.value("gcorr", &template).unwrap();
    assert!((gap + kbt() * 3.0f64.ln()).abs() < EPS, "fixture gap {gap}");
    let records = vec![template, excited, am, sa];
    let ensembles = corrected_collapse_ensembles(&records, &memo);
    assert_eq!(ensembles.len(), 3);
    let references = corrected_ensemble_references(&ensembles);
    let ensemble = corrected_ensemble_formation_values(
        &ensembles[0],
        &references,
        &memo,
        &ConcConditions::default(),
    );
    let record_references = corrected_monomer_references(&records, &memo, SortKey::El);
    let conformer = corrected_formation_values(
        &records[1],
        &record_references,
        &memo,
        &ConcConditions::default(),
    );
    // Equal correction energies make the collapsed elcorr either
    // conformer's; the collapsed gcorr sits k_B*T*ln(4/3) below the best.
    assert!((ensemble.electronic.unwrap() - conformer.electronic.unwrap()).abs() < EPS);
    assert!(
        (ensemble.gibbs.unwrap() - (conformer.gibbs.unwrap() - kbt() * (4.0f64 / 3.0).ln())).abs()
            < EPS
    );
    // Equal hcorr values make the collapsed hcorr either conformer's,
    // and the collapsed stcorr is its hcorr value minus its gcorr
    // value.
    assert!((ensemble.enthalpy.unwrap() - conformer.enthalpy.unwrap()).abs() < EPS);
    assert!(
        (ensemble.entropy_term.unwrap() - (ensemble.enthalpy.unwrap() - ensemble.gibbs.unwrap()))
            .abs()
            < EPS
    );
}

#[test] // rq-ade63c60
fn a_corrected_member_without_thermochemistry_contributes_nothing() {
    let full = corrected_with_gibbs("a", &[("am", 1), ("sa", 1)], 1000.0, -61.0, -5.0);
    let mut valueless = corrected_with_gibbs("b", &[("am", 1), ("sa", 1)], 1000.0, -60.5, -5.0);
    valueless.vibrational_frequencies = None;
    let am = corrected_with_gibbs("am1", &[("am", 1)], 1000.0, -50.0, -5.0);
    let sa = corrected_with_gibbs("sa1", &[("sa", 1)], 1000.0, -40.0, -5.0);
    let memo = memo();
    let records = vec![full.clone(), valueless, am, sa];
    let ensembles = corrected_collapse_ensembles(&records, &memo);
    assert_eq!(ensembles.len(), 3);
    // The single surviving member collapses to itself exactly.
    assert!((ensembles[0].gibbs.unwrap() - memo.value("gcorr", &full).unwrap()).abs() < 1e-15);
    assert!((ensembles[0].electronic.unwrap() - corrected_energy(&full).unwrap()).abs() < 1e-15);
    // And the collapsed row's values equal the full conformer's
    // uncollapsed row's values at every level.
    let references = corrected_ensemble_references(&ensembles);
    let ensemble = corrected_ensemble_formation_values(
        &ensembles[0],
        &references,
        &memo,
        &ConcConditions::default(),
    );
    let record_references = corrected_monomer_references(&records, &memo, SortKey::El);
    let conformer =
        corrected_formation_values(&full, &record_references, &memo, &ConcConditions::default());
    assert!((ensemble.electronic.unwrap() - conformer.electronic.unwrap()).abs() < 1e-15);
    assert!((ensemble.enthalpy.unwrap() - conformer.enthalpy.unwrap()).abs() < 1e-15);
    assert!((ensemble.entropy_term.unwrap() - conformer.entropy_term.unwrap()).abs() < 1e-15);
    assert!((ensemble.gibbs.unwrap() - conformer.gibbs.unwrap()).abs() < 1e-15);
}

#[test] // rq-0df193f0
fn a_monomer_group_without_any_gibbs_loses_its_reference_at_every_level() {
    let cluster = cluster_with_gibbs("cluster", &[("am", 2)], 1000.0, -101.0);
    let mut sp_am = with_gibbs("am1", "am", 1000.0, -50.0);
    sp_am.vibrational_frequencies = None;
    let sa = with_gibbs("sa1", "sa", 1000.0, -40.0);
    let memo = memo();
    let records = vec![cluster.clone(), sp_am, sa];
    let ensembles = collapse_ensembles(&records, &memo);
    assert_eq!(ensembles.len(), 3);
    // The am group's single record defines no Gibbs level, so its
    // ensemble carries no values and supplies no reference.
    assert_eq!(ensembles[1].electronic, None);
    assert_eq!(ensembles[1].enthalpy, None);
    assert_eq!(ensembles[1].entropy_term, None);
    assert_eq!(ensembles[1].gibbs, None);
    let references = ensemble_references(&ensembles);
    assert!(!references.electronic.contains_key("am"));
    assert!(!references.enthalpy.contains_key("am"));
    assert!(references.gibbs.contains_key("sa"));
    // The cluster loses its reference at every level at once.
    let ensemble = ensemble_formation_values(
        &ensembles[0],
        &references,
        &memo,
        &ConcConditions::default(),
    );
    assert_eq!(
        ensemble,
        Formation {
            electronic: None,
            enthalpy: None,
            entropy_term: None,
            gibbs: None,
        }
    );
    // Without the collapse the electronic level still forms.
    let record_references = collect_monomers(&records, &memo, SortKey::El);
    let uncollapsed = formation_values(
        &cluster,
        &record_references,
        &memo,
        &ConcConditions::default(),
    );
    assert!(uncollapsed.electronic.is_some());
    assert_eq!(uncollapsed.enthalpy, None);
    assert_eq!(uncollapsed.entropy_term, None);
    assert_eq!(uncollapsed.gibbs, None);
}

#[test] // rq-1ad3c076
fn records_without_components_form_one_unnamed_ensemble() {
    let bare = |name: &str| Record {
        name: Some(name.to_string()),
        electronic_energy: Some(-60.0),
        ..Record::default()
    };
    let am = with_gibbs("am1", "am", 1000.0, -50.0);
    let memo = memo();
    // A componentless record loads first, yet the unnamed ensemble comes
    // last: the section prints 1am first, then one row of dashes.
    let records = vec![bare("bare1"), am, bare("bare2")];
    let ensembles = collapse_ensembles(&records, &memo);
    assert_eq!(ensembles.len(), 2);
    assert_eq!(ensembles[0].components, Some(components(&[("am", 1)])));
    assert!(ensembles[0].gibbs.is_some());
    assert!(ensembles[0].enthalpy.is_some());
    assert!(ensembles[0].entropy_term.is_some());
    assert_eq!(ensembles[1].components, None);
    assert_eq!(ensembles[1].electronic, None);
    assert_eq!(ensembles[1].enthalpy, None);
    assert_eq!(ensembles[1].entropy_term, None);
    assert_eq!(ensembles[1].gibbs, None);
}

#[test] // rq-7721f6ff
fn the_collapse_preserves_first_appearance_order() {
    let mixed = cluster_with_gibbs("m1", &[("am", 1), ("sa", 1)], 1000.0, -61.0);
    let two_am = cluster_with_gibbs("c2", &[("am", 2)], 1000.0, -101.0);
    let mixed_again = cluster_with_gibbs("m2", &[("am", 1), ("sa", 1)], 1000.0, -61.5);
    let one_sa = cluster_with_gibbs("s1", &[("sa", 1)], 1000.0, -40.0);
    let memo = memo();
    let records = vec![mixed, two_am, mixed_again, one_sa];
    let ensembles = collapse_ensembles(&records, &memo);
    let tokens: Vec<Components> = ensembles
        .iter()
        .filter_map(|ensemble: &Ensemble| ensemble.components.clone())
        .collect();
    assert_eq!(
        tokens,
        vec![
            components(&[("am", 1), ("sa", 1)]),
            components(&[("am", 2)]),
            components(&[("sa", 1)]),
        ]
    );
}

// --- Ionic references (rqm/formation.md Ionic References) ---

use compiled_jkqc::formation::{
    acdc_corrected_monomer_references, acdc_monomer_references, CorrectedMonomerReferences,
};
use compiled_jkqc::ions::{
    acdc_composition_token, parse_acdc_token, validate_ion_declarations, IonDeclarationError,
    IonDeclarations, IonSpec, ProtonKind,
};

fn ion_declarations(pairs: &[(&str, &str)]) -> IonDeclarations {
    IonDeclarations {
        specs: pairs
            .iter()
            .map(|(code, token)| IonSpec {
                code: (*code).to_string(),
                token: parse_acdc_token(token).expect("test token parses"),
            })
            .collect(),
    }
}

fn electronic_references(pairs: &[(&str, f64)]) -> MonomerReferences {
    let mut references = MonomerReferences::default();
    for (code, value) in pairs {
        references.electronic.insert((*code).to_string(), *value);
    }
    references
}

#[test] // rq-1560f041
fn an_ion_token_parses_into_neutral_molecules_and_added_protons() {
    let token = parse_acdc_token("1am1P").expect("token parses");
    assert_eq!(token.neutral, components(&[("am", 1)]));
    assert_eq!(token.protons, 1);
    assert_eq!(token.kind, ProtonKind::Added);
}

#[test] // rq-32cc4472
fn an_ion_token_parses_removed_protons() {
    let token = parse_acdc_token("2sa2H").expect("token parses");
    assert_eq!(token.neutral, components(&[("sa", 2)]));
    assert_eq!(token.protons, 2);
    assert_eq!(token.kind, ProtonKind::Removed);
}

#[test] // rq-6bccfd36
fn a_bare_proton_token_is_valid() {
    let token = parse_acdc_token("1P").expect("token parses");
    assert!(token.neutral.is_empty());
    assert_eq!(token.protons, 1);
    assert_eq!(token.kind, ProtonKind::Added);
}

#[test] // rq-b2a7d478
fn a_token_without_a_proton_marker_is_invalid() {
    assert!(parse_acdc_token("1sa2").is_err());
}

#[test] // rq-5b7a95ea
fn a_token_mixing_p_and_h_is_invalid() {
    assert!(parse_acdc_token("1sa1H1P").is_err());
}

#[test] // rq-3a4da0c5
fn a_token_with_a_hyphenated_label_is_invalid() {
    assert!(parse_acdc_token("1sa1H-1conf").is_err());
}

#[test] // rq-35eab7ec
fn the_acdc_token_multiplies_the_ion_count() {
    let ions = ion_declarations(&[("isa", "1sa1H")]);
    assert_eq!(
        acdc_composition_token(Some(&components(&[("isa", 2)])), &ions),
        Some("2sa2H".to_string())
    );
}

#[test] // rq-8cfefe27
fn the_acdc_token_combines_an_ion_with_explicit_neutral_parents() {
    let ions = ion_declarations(&[("isa", "1sa1H")]);
    assert_eq!(
        acdc_composition_token(Some(&components(&[("isa", 1), ("sa", 1)])), &ions),
        Some("2sa1H".to_string())
    );
}

#[test] // rq-77c8e134
fn undeclared_codes_stay_in_the_acdc_token() {
    let ions = ion_declarations(&[("iam", "1am1P")]);
    assert_eq!(
        acdc_composition_token(Some(&components(&[("iam", 1), ("x", 1)])), &ions),
        Some("1am1x1P".to_string())
    );
}

#[test] // rq-eecbf103
fn a_lone_ion_code_is_its_own_reference_without_neutral_parents() {
    let ions = ion_declarations(&[("isa", "1sa1H")]);
    let references = electronic_references(&[("isa", -100.0)]);
    let effective = acdc_monomer_references(&references, &ions);
    assert_eq!(effective.electronic.get("isa"), Some(&-100.0));
}

#[test] // rq-a8c9d487
fn the_first_declared_positive_ion_is_the_zero_point() {
    let ions = ion_declarations(&[("iam", "1am1P"), ("idma", "1dma1P")]);
    let references = electronic_references(&[
        ("am", -50.0),
        ("dma", -40.0),
        ("iam", -49.5),
        ("idma", -39.0),
    ]);
    let effective = acdc_monomer_references(&references, &ions);
    // mu = ref(iam) - ref(am) = 0.5; eff(idma) = ref(dma) + 0.5.
    assert_eq!(effective.electronic.get("iam"), Some(&-49.5));
    assert_eq!(effective.electronic.get("idma"), Some(&-39.5));
    let cluster = Record {
        electronic_energy: Some(-39.0),
        components: Some(components(&[("idma", 1)])),
        ..Record::default()
    };
    let formation = formation_values(&cluster, &effective, &memo(), &ConcConditions::default());
    assert_eq!(formation.electronic, Some(0.5));
}

#[test] // rq-38ba7c86
fn declaration_order_not_sort_chooses_the_zero_point() {
    let ions = ion_declarations(&[("idma", "1dma1P"), ("iam", "1am1P")]);
    let references = electronic_references(&[
        ("am", -50.0),
        ("dma", -40.0),
        ("iam", -49.5),
        ("idma", -39.0),
    ]);
    let effective = acdc_monomer_references(&references, &ions);
    // mu = ref(idma) - ref(dma) = 1.0; eff(iam) = ref(am) + 1.0.
    assert_eq!(effective.electronic.get("idma"), Some(&-39.0));
    assert_eq!(effective.electronic.get("iam"), Some(&-49.0));
}

#[test] // rq-45390ea2
fn a_missing_neutral_parent_leaves_non_reference_ions_undefined() {
    let ions = ion_declarations(&[("iam", "1am1P"), ("idma", "1dma1P")]);
    // No am reference, so mu is undefined; the reference ion keeps its own
    // reference and the other ion loses its scale.
    let references = electronic_references(&[("dma", -40.0), ("iam", -49.5), ("idma", -39.0)]);
    let effective = acdc_monomer_references(&references, &ions);
    assert_eq!(effective.electronic.get("iam"), Some(&-49.5));
    assert!(!effective.electronic.contains_key("idma"));
}

#[test] // rq-33f96f23
fn multiply_charged_ions_scale_the_reference_potential() {
    let ions = ion_declarations(&[("isa", "1sa1H"), ("iisa", "1sa2H")]);
    let references = electronic_references(&[("sa", -90.0), ("isa", -100.0), ("iisa", -108.0)]);
    let effective = acdc_monomer_references(&references, &ions);
    // mu = ref(isa) - ref(sa) = -10; eff(iisa) = ref(sa) + 2*mu = -110.
    assert_eq!(effective.electronic.get("iisa"), Some(&-110.0));
    let cluster = Record {
        electronic_energy: Some(-108.0),
        components: Some(components(&[("iisa", 1)])),
        ..Record::default()
    };
    let formation = formation_values(&cluster, &effective, &memo(), &ConcConditions::default());
    assert_eq!(formation.electronic, Some(2.0));
}

#[test] // rq-0f165990
fn a_duplicate_ion_code_is_rejected() {
    let ions = ion_declarations(&[("iam", "1am1P"), ("iam", "1am1P")]);
    assert_eq!(
        validate_ion_declarations(&ions),
        Err(IonDeclarationError::DuplicateCode {
            code: "iam".to_string()
        })
    );
}

#[test] // rq-a91db93c
fn a_token_whose_neutral_part_names_a_declared_ion_is_rejected() {
    let ions = ion_declarations(&[("iam", "1am1P"), ("x", "1iam1P")]);
    assert_eq!(
        validate_ion_declarations(&ions),
        Err(IonDeclarationError::NeutralCodeIsIon {
            code: "iam".to_string()
        })
    );
}

#[test] // rq-eb7da353
fn bare_proton_references_use_the_proton_monomer_directly() {
    // The reference token 1P has no neutral parent, so mu is its own
    // reference and the other ion reads its neutral parent plus mu.
    let ions = ion_declarations(&[("p", "1P"), ("iam", "1am1P")]);
    let references = electronic_references(&[("p", -1.0), ("am", -50.0), ("iam", -49.5)]);
    let effective = acdc_monomer_references(&references, &ions);
    assert_eq!(effective.electronic.get("p"), Some(&-1.0));
    assert_eq!(effective.electronic.get("iam"), Some(&-51.0));
    let cluster = Record {
        electronic_energy: Some(-49.5),
        components: Some(components(&[("iam", 1)])),
        ..Record::default()
    };
    let formation = formation_values(&cluster, &effective, &memo(), &ConcConditions::default());
    assert_eq!(formation.electronic, Some(1.5));
}

fn corrected_electronic_references(pairs: &[(&str, f64)]) -> CorrectedMonomerReferences {
    let mut references = CorrectedMonomerReferences::default();
    for (code, value) in pairs {
        references.electronic.insert((*code).to_string(), *value);
    }
    references
}

#[test] // rq-aef2862b
fn the_corrected_reference_transform_reads_a_multi_ion_polarity() {
    let ions = ion_declarations(&[("iam", "1am1P"), ("idma", "1dma1P")]);
    let references = corrected_electronic_references(&[
        ("am", -50.0),
        ("dma", -40.0),
        ("iam", -49.5),
        ("idma", -39.0),
    ]);
    let effective = acdc_corrected_monomer_references(&references, &ions);
    assert_eq!(effective.electronic.get("iam"), Some(&-49.5));
    assert_eq!(effective.electronic.get("idma"), Some(&-39.5));
}
