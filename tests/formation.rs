//! Integration tests for formation energies from monomer references, one
//! per Gherkin scenario of `rqm/formation.md` exercised at the API level.
//! CLI-shaped scenarios live in `tests/cli.rs`.

use complied_jkqc::filter::SortKey;
use complied_jkqc::formation::{collect_monomers, formation_values, Formation, MonomerReferences};
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
    let formation = formation_values(&cluster, &references, &memo());
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
    let formation = formation_values(&monomer, &references, &memo());
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
    let formation = formation_values(&cluster, &references, &memo());
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
    let formation = formation_values(&cluster, &references, &memo());
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
    let formation = formation_values(&record, &MonomerReferences::default(), &memo());
    assert_eq!(
        formation,
        Formation {
            electronic: None,
            gibbs: None,
        }
    );
}
