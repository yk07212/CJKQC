//! Integration tests for the reacted/exploded detection, one per Gherkin
//! scenario in `rqm/detection.md`.

use complied_jkqc::{
    are_bonded, filter_reacted, fragment_signature, Geometry, ReactedMode, Record,
};

fn geom(symbols: &[&str], coordinates: &[f64]) -> Record {
    Record {
        geometry: Some(Geometry {
            symbols: symbols.iter().map(|symbol| symbol.to_string()).collect(),
            coordinates: coordinates.to_vec(),
        }),
        ..Record::default()
    }
}

fn paired_nitrogens(distance: f64) -> Record {
    geom(&["N", "N"], &[0.0, 0.0, 0.0, distance, 0.0, 0.0])
}

#[test] // rq-00285b24
fn hydrogen_bonds_only_under_080_a_to_another_h() {
    let record = geom(
        &["H", "H", "O"],
        &[0.0, 0.0, 0.0, 0.75, 0.0, 0.0, 0.0, 0.0, 0.97],
    );
    assert_eq!(fragment_signature(&record).as_deref(), Some("HH+O"));
}

#[test] // rq-239fd4b8
fn hydrogens_never_join_the_heavy_skeleton() {
    let record = geom(
        &["O", "H", "H"],
        &[0.0, 0.0, 0.0, 0.97, 0.0, 0.0, -0.2335, 0.9416, 0.0],
    );
    assert_eq!(fragment_signature(&record).as_deref(), Some("H+H+O"));
}

#[test] // rq-dfb86bb9
fn a_reacted_heavy_skeleton_deviates_from_the_consensus() {
    let majority = [paired_nitrogens(3.0), paired_nitrogens(3.0)];
    let deviant = paired_nitrogens(1.4);
    let group = vec![majority[0].clone(), majority[1].clone(), deviant.clone()];
    let kept = filter_reacted(&[group], ReactedMode::Consensus);
    assert_eq!(kept.len(), 2);
    assert!(kept.iter().all(|record| record != &deviant));
}

#[test] // rq-516d24b1
fn deviants_mode_keeps_exactly_the_deviants() {
    let majority = [paired_nitrogens(3.0), paired_nitrogens(3.0)];
    let deviant = paired_nitrogens(1.4);
    let group = vec![majority[0].clone(), majority[1].clone(), deviant.clone()];
    let kept = filter_reacted(&[group], ReactedMode::Deviants);
    assert_eq!(kept, vec![deviant]);
}

#[test] // rq-e2e0998d rq-fe68801f
fn distance_exactly_at_a_cutoff_is_unbonded() {
    assert!(!are_bonded(1.75, "C", "N"));
    assert!(are_bonded(1.749, "C", "N"));
}

#[test] // rq-f2821d27
fn the_two_a_default_applies_to_unlisted_heavy_pairs() {
    assert!(are_bonded(1.95, "C", "C"));
    assert!(!are_bonded(2.05, "C", "C"));
}

#[test] // rq-775c5278
fn groups_vote_independently() {
    let group_one = vec![
        paired_nitrogens(3.0),
        paired_nitrogens(3.0),
        paired_nitrogens(3.0),
        paired_nitrogens(1.4),
    ];
    let group_two = vec![paired_nitrogens(3.0), paired_nitrogens(3.0)];
    let kept = filter_reacted(&[group_one, group_two], ReactedMode::Consensus);
    assert_eq!(kept.len(), 5);
}

#[test] // rq-bfd2765b
fn tie_in_frequency_breaks_by_earliest_occurrence() {
    let group = vec![
        paired_nitrogens(3.0),
        paired_nitrogens(1.4),
        paired_nitrogens(3.0),
        paired_nitrogens(1.4),
    ];
    let kept = filter_reacted(&[group], ReactedMode::Consensus);
    assert_eq!(kept.len(), 2);
    assert_eq!(
        fragment_signature(&kept[0]).as_deref(),
        Some("N+N"),
        "the earliest signature wins the tie"
    );
}

#[test] // rq-cc3c275c
fn a_geometryless_record_survives_only_with_a_none_consensus() {
    let group = vec![
        Record::default(),
        Record::default(),
        Record::default(),
        Record::default(),
        paired_nitrogens(3.0),
        paired_nitrogens(3.0),
        paired_nitrogens(3.0),
    ];
    let kept = filter_reacted(&[group], ReactedMode::Consensus);
    assert_eq!(kept.len(), 4);
    assert!(kept.iter().all(|record| record.geometry.is_none()));
}

#[test] // rq-6203c24a
fn detection_never_panics_on_hostile_geometries() {
    // Duplicate positions (zero distance) and a far-out atom.
    let hostile = geom(
        &["H", "H", "C"],
        &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0e6, 1.0e6, 1.0e6],
    );
    assert_eq!(fragment_signature(&hostile).as_deref(), Some("C+HH"));
    let group = vec![
        hostile.clone(),
        paired_nitrogens(3.0),
        paired_nitrogens(3.0),
    ];
    let kept = filter_reacted(&[group], ReactedMode::Consensus);
    assert_eq!(kept.len(), 2);
    assert!(kept.iter().all(|record| record != &hostile));
}
