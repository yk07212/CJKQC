//! Integration tests for the manipulation pipeline, one per Gherkin
//! scenario of `rqm/filtering.md` that is exercised at the API level.
//! CLI-shaped scenarios live in `tests/cli.rs`.

use compiled_jkqc::filter::{
    apply_filters, extract_matches, parse_extract_query, print_table, property_value, CmpOp,
    Column, Cutoff, ExtractMode, FilterSpec, Grouping, Mode, Property, SortKey, Unit,
};
use compiled_jkqc::thermo::{ThermoConditions, ThermoMemo};
use compiled_jkqc::{Components, Geometry, Record, RecordStatus, UniqKey, UniqMetric, UniqMode};

fn components(pairs: &[(&str, u32)]) -> Components {
    pairs
        .iter()
        .map(|(code, count)| (code.to_string(), *count))
        .collect()
}

fn record(name: &str, energy: Option<f64>, comps: Option<Components>) -> Record {
    Record {
        name: Some(name.to_string()),
        electronic_energy: energy,
        components: comps,
        ..Record::default()
    }
}

fn geometry_record(name: &str, symbols: &[&str], coordinates: &[f64]) -> Record {
    Record {
        name: Some(name.to_string()),
        geometry: Some(Geometry {
            symbols: symbols.iter().map(|symbol| symbol.to_string()).collect(),
            coordinates: coordinates.to_vec(),
        }),
        ..Record::default()
    }
}

fn cutoff(op: CmpOp, mode: Mode, property: Property, value: f64) -> Cutoff {
    Cutoff {
        op,
        mode,
        property,
        value,
    }
}

fn names(records: &[Record]) -> Vec<String> {
    records
        .iter()
        .map(|record| record.name.clone().unwrap())
        .collect()
}

fn run(records: &[Record], spec: &FilterSpec) -> Vec<Record> {
    apply_filters(records, spec, &ThermoMemo::default())
        .expect("pipeline")
        .records
}

#[test] // rq-9fa28264
fn a_comma_list_unions() {
    let patterns = parse_extract_query("2am3sa,1am1sa").expect("parses");
    assert_eq!(patterns.len(), 2);
    assert!(extract_matches(
        &patterns[0],
        Some(&components(&[("am", 2), ("sa", 3)]))
    ));
    assert!(extract_matches(
        &patterns[1],
        Some(&components(&[("am", 1), ("sa", 1)]))
    ));
    assert!(!extract_matches(
        &patterns[0],
        Some(&components(&[("am", 1), ("sa", 1)]))
    ));
}

#[test] // rq-5c68d6cb
fn a_count_range_expands_inclusively() {
    let patterns = parse_extract_query("1am0-2w").expect("parses");
    assert_eq!(patterns.len(), 3);
    let one_am: Option<&Components> = Some(&components(&[("am", 1)]));
    let one_am_one_w: Option<&Components> = Some(&components(&[("am", 1), ("w", 1)]));
    let one_am_two_w: Option<&Components> = Some(&components(&[("am", 1), ("w", 2)]));
    let two_am: Option<&Components> = Some(&components(&[("am", 2)]));
    assert!(extract_matches(&patterns[0], one_am));
    assert!(extract_matches(&patterns[1], one_am_one_w));
    assert!(extract_matches(&patterns[2], one_am_two_w));
    assert!(!extract_matches(&patterns[0], one_am_one_w));
    assert!(!extract_matches(&patterns[1], one_am));
    for pattern in &patterns {
        assert!(!extract_matches(pattern, two_am));
    }
}

#[test] // rq-767b9e05
fn a_choice_expands() {
    let patterns = parse_extract_query("(2,3)am1w").expect("parses");
    assert_eq!(patterns.len(), 2);
    assert!(extract_matches(
        &patterns[0],
        Some(&components(&[("am", 2), ("w", 1)]))
    ));
    assert!(extract_matches(
        &patterns[1],
        Some(&components(&[("am", 3), ("w", 1)]))
    ));
    assert!(!extract_matches(
        &patterns[0],
        Some(&components(&[("am", 4), ("w", 1)]))
    ));
    assert!(!extract_matches(
        &patterns[1],
        Some(&components(&[("am", 4), ("w", 1)]))
    ));
}

#[test] // rq-4dbf6ef0
fn a_wildcard_admits_any_extras() {
    let patterns = parse_extract_query("2am*w").expect("parses");
    assert!(extract_matches(
        &patterns[0],
        Some(&components(&[("am", 2)]))
    ));
    assert!(extract_matches(
        &patterns[0],
        Some(&components(&[("am", 2), ("w", 1)]))
    ));
    assert!(extract_matches(
        &patterns[0],
        Some(&components(&[("am", 2), ("w", 9)]))
    ));
    assert!(!extract_matches(
        &patterns[0],
        Some(&components(&[("am", 1), ("w", 1)]))
    ));
}

#[test] // rq-7fe408ee
fn a_bare_star_matches_everything() {
    let patterns = parse_extract_query("*").expect("parses");
    assert!(extract_matches(
        &patterns[0],
        Some(&components(&[("am", 5), ("sa", 2), ("w", 1)]))
    ));
    assert!(extract_matches(&patterns[0], None));
}

#[test] // rq-80b74fa7
fn an_exact_pattern_rejects_extras() {
    let patterns = parse_extract_query("2am").expect("parses");
    assert!(!extract_matches(
        &patterns[0],
        Some(&components(&[("am", 2), ("w", 1)]))
    ));
    assert!(extract_matches(
        &patterns[0],
        Some(&components(&[("am", 2)]))
    ));
}

#[test] // rq-49f1442c
fn a_label_comment_in_a_pattern_is_ignored() {
    let patterns = parse_extract_query("2am3sa-1conf").expect("parses");
    assert_eq!(patterns.len(), 1);
    assert!(extract_matches(
        &patterns[0],
        Some(&components(&[("am", 2), ("sa", 3)]))
    ));
}

#[test] // rq-60c7278b
fn a_count_star_code_wildcard_combines_both_restrictions() {
    let patterns = parse_extract_query("2am2*w").expect("parses");
    assert_eq!(patterns.len(), 1);
    assert!(extract_matches(
        &patterns[0],
        Some(&components(&[("am", 2), ("w", 2)]))
    ));
    assert!(
        !extract_matches(
            &patterns[0],
            Some(&components(&[("am", 2), ("w", 1), ("sa", 1)]))
        ),
        "extra codes outside the wildcard list are rejected"
    );
    assert!(
        !extract_matches(&patterns[0], Some(&components(&[("am", 2)]))),
        "extras must sum to exactly N"
    );
}

#[test] // rq-071ab7e7
fn an_underscore_in_a_pattern_is_a_parse_error() {
    assert!(parse_extract_query("2am3sa_1conf").is_err());
    assert!(parse_extract_query("2am_1conf-2w").is_err());
}

#[test] // rq-e1e64cd9
fn a_pathological_count_range_is_a_parse_error() {
    assert!(parse_extract_query("1am1-4294967295w").is_err());
    // A legal range still works.
    assert_eq!(parse_extract_query("1am0-2w").expect("parses").len(), 3);
}

#[test] // rq-03ad3478
fn the_count_range_cap_holds_exactly_one_thousand_values() {
    assert_eq!(
        parse_extract_query("1am1-1000w").expect("parses").len(),
        1000
    );
    assert!(parse_extract_query("1am1-1001w").is_err());
}

#[test] // rq-98d88fc4
fn a_reversed_count_range_is_a_parse_error() {
    assert!(parse_extract_query("1am3-1w").is_err());
}

#[test] // rq-bc3e1482
fn apply_filters_reports_unparsable_queries_instead_of_dropping_records() {
    let records = vec![record("a", Some(-1.0), Some(components(&[("am", 1)])))];
    let spec = FilterSpec {
        extract: Some((vec!["2am$".to_string()], ExtractMode::Keep)),
        ..FilterSpec::default()
    };
    assert!(apply_filters(&records, &spec, &ThermoMemo::default()).is_err());
}

#[test] // rq-f43b5dbf
fn an_unparsable_query_is_rejected() {
    assert!(parse_extract_query("2am$").is_err());
    assert!(parse_extract_query("").is_err());
    assert!(parse_extract_query("2am(3,sa").is_err());
}

#[test] // rq-77d0cf6c
fn a_relative_energy_window_is_per_composition_group() {
    let records = vec![
        record("a", Some(0.0), Some(components(&[("am", 1)]))),
        record("b", Some(0.01), Some(components(&[("am", 1)]))),
        record("c", Some(-5.0), Some(components(&[("sa", 1)]))),
    ];
    let spec = FilterSpec {
        cutoffs: vec![cutoff(CmpOp::Le, Mode::Relative, Property::El, 5.0)],
        ..FilterSpec::default()
    };
    assert_eq!(names(&run(&records, &spec)), ["a", "c"]);
}

#[test] // rq-15a24c78
fn an_absolute_energy_cutoff_uses_hartree() {
    let records = vec![
        record("low", Some(-38.0), Some(components(&[("am", 1)]))),
        record("high", Some(-37.0), Some(components(&[("am", 1)]))),
    ];
    let spec = FilterSpec {
        cutoffs: vec![cutoff(CmpOp::Le, Mode::Absolute, Property::El, -37.5)],
        ..FilterSpec::default()
    };
    assert_eq!(names(&run(&records, &spec)), ["low"]);
}

#[test] // rq-297b5bf7
fn boundary_values_sit_on_the_inclusive_side_of_their_operator() {
    let records = || {
        vec![
            record("low", Some(-1.0), Some(components(&[("am", 1)]))),
            record("at", Some(-0.5), Some(components(&[("am", 1)]))),
            record("high", Some(0.0), Some(components(&[("am", 1)]))),
        ]
    };
    let keep = |op: CmpOp| {
        let spec = FilterSpec {
            cutoffs: vec![cutoff(op, Mode::Absolute, Property::El, -0.5)],
            ..FilterSpec::default()
        };
        names(&run(&records(), &spec))
    };
    assert_eq!(keep(CmpOp::Le), ["low", "at"]);
    assert_eq!(keep(CmpOp::Lt), ["low"]);
    assert_eq!(keep(CmpOp::Gt), ["high"]);
    assert_eq!(keep(CmpOp::Ge), ["at", "high"]);
    assert_eq!(keep(CmpOp::Eq), ["at"]);
    assert_eq!(keep(CmpOp::Ne), ["low", "high"]);
}

#[test] // rq-4319b627
fn the_lowest_frequency_pass_drops_imaginary_modes() {
    let mut imaginary = record("imaginary", None, Some(components(&[("am", 1)])));
    imaginary.vibrational_frequencies = Some(vec![-120.0, 400.0]);
    let mut stable = record("stable", None, Some(components(&[("am", 1)])));
    stable.vibrational_frequencies = Some(vec![25.0, 400.0]);
    let spec = FilterSpec {
        cutoffs: vec![cutoff(CmpOp::Gt, Mode::Absolute, Property::Lf, 0.0)],
        ..FilterSpec::default()
    };
    assert_eq!(names(&run(&[imaginary, stable], &spec)), ["stable"]);
}

#[test] // rq-e64bbc62
fn undefined_values_fail_comparisons_except_inequality() {
    let records = vec![record("nodip", None, Some(components(&[("am", 1)])))];
    let pass = FilterSpec {
        cutoffs: vec![cutoff(CmpOp::Gt, Mode::Absolute, Property::Dip, 1.0)],
        ..FilterSpec::default()
    };
    assert!(run(&records, &pass).is_empty());
    let isnot = FilterSpec {
        cutoffs: vec![cutoff(CmpOp::Ne, Mode::Absolute, Property::Dip, 1.0)],
        ..FilterSpec::default()
    };
    assert_eq!(names(&run(&records, &isnot)), ["nodip"]);
}

#[test] // rq-6e0398ec
fn an_rg_cutoff_uses_angstroms() {
    let compact = geometry_record("compact", &["H", "H"], &[0.0, 0.0, 0.0, 1.4, 0.0, 0.0]);
    let extended = geometry_record("extended", &["H", "H"], &[0.0, 0.0, 0.0, 30.0, 0.0, 0.0]);
    let mut records = vec![compact, extended];
    for record in &mut records {
        record.components = Some(components(&[("h", 2)]));
    }
    let spec = FilterSpec {
        cutoffs: vec![cutoff(CmpOp::Le, Mode::Absolute, Property::Rg, 10.0)],
        ..FilterSpec::default()
    };
    assert_eq!(names(&run(&records, &spec)), ["compact"]);
}

#[test] // rq-2d073323
fn sorting_by_energy_ascending_with_reverse() {
    let records = vec![
        record("minus-two", Some(-2.0), Some(components(&[("am", 1)]))),
        record("minus-one", Some(-1.0), Some(components(&[("am", 1)]))),
        record("no-energy", None, Some(components(&[("am", 1)]))),
    ];
    let ascending = FilterSpec {
        sort: Some(SortKey::El),
        ..FilterSpec::default()
    };
    assert_eq!(
        names(&run(&records, &ascending)),
        ["minus-two", "minus-one", "no-energy"]
    );
    let descending = FilterSpec {
        sort: Some(SortKey::El),
        reverse: true,
        ..FilterSpec::default()
    };
    assert_eq!(
        names(&run(&records, &descending)),
        ["no-energy", "minus-one", "minus-two"]
    );
}

#[test] // rq-297da2a2
fn select_keeps_the_first_n_per_group() {
    let records = vec![
        record("a1", Some(-3.0), Some(components(&[("am", 1)]))),
        record("a2", Some(-2.0), Some(components(&[("am", 1)]))),
        record("a3", Some(-1.0), Some(components(&[("am", 1)]))),
        record("s1", Some(-4.0), Some(components(&[("sa", 1)]))),
    ];
    let spec = FilterSpec {
        select: Some(2),
        ..FilterSpec::default()
    };
    assert_eq!(names(&run(&records, &spec)).len(), 3);
}

#[test] // rq-2f2ca923
fn uniq_implies_sorting_by_energy() {
    let records = vec![
        record(
            "listed-first",
            Some(-38.5122),
            Some(components(&[("am", 1)])),
        ),
        record("lower", Some(-38.5123), Some(components(&[("am", 1)]))),
    ];
    let spec = FilterSpec {
        uniq: Some(UniqMode::Keys(vec![UniqKey {
            metric: UniqMetric::El,
            decimals: 3.0,
        }])),
        ..FilterSpec::default()
    };
    assert_eq!(names(&run(&records, &spec)), ["lower"]);
}

#[test] // rq-6a872802
fn shuffle_is_deterministic_per_seed() {
    let records: Vec<Record> = (0..5)
        .map(|index| {
            record(
                &format!("n{index}"),
                Some(index as f64),
                Some(components(&[("am", 1)])),
            )
        })
        .collect();
    let spec = FilterSpec {
        shuffle: true,
        seed: 7,
        ..FilterSpec::default()
    };
    let first = names(&run(&records, &spec));
    let second = names(&run(&records, &spec));
    assert_eq!(first, second);
    assert_ne!(first, ["n0", "n1", "n2", "n3", "n4"]);
    let other_seed = FilterSpec {
        seed: 8,
        ..spec.clone()
    };
    let mut third = names(&run(&records, &other_seed));
    let mut original = names(&records);
    third.sort();
    original.sort();
    assert_eq!(third, original);
}

#[test]
fn apply_filters_drops_non_ok_records() {
    let mut failed = record("failed", None, Some(components(&[("am", 1)])));
    failed.status = RecordStatus::Failed;
    failed.error = Some("boom".to_string());
    let spec = FilterSpec::default();
    assert_eq!(names(&run(&[failed], &spec)), Vec::<String>::new());
}

#[test] // rq-d1cfac2d rq-b21ce0b5
fn stage_order_is_fixed() {
    let records = vec![
        record("wanted", Some(-2.0), Some(components(&[("am", 2)]))),
        record("other", Some(-1.0), Some(components(&[("sa", 3)]))),
        record("light", Some(-1.0), Some(components(&[("am", 2)]))),
    ];
    let spec = FilterSpec {
        extract: Some((vec!["2am".to_string()], ExtractMode::Keep)),
        cutoffs: vec![cutoff(CmpOp::Le, Mode::Absolute, Property::El, -1.5)],
        ..FilterSpec::default()
    };
    assert_eq!(names(&run(&records, &spec)), ["wanted"]);
}

#[test] // rq-b76b0761
fn property_values_cover_the_cutoff_properties() {
    let mut record = record("all", Some(-38.0), Some(components(&[("am", 1)])));
    record.charge = Some(-1);
    record.multiplicity = Some(2);
    record.geometry = Some(Geometry {
        symbols: vec!["H".to_string(), "H".to_string()],
        coordinates: vec![0.0, 0.0, 0.0, 0.74, 0.0, 0.0],
    });
    record.vibrational_frequencies = Some(vec![-50.0, 100.0, 200.0]);
    assert_eq!(
        property_value(&record, Property::El, &ThermoMemo::default()),
        Some(-38.0)
    );
    assert_eq!(
        property_value(&record, Property::Lf, &ThermoMemo::default()),
        Some(-50.0)
    );
    assert_eq!(
        property_value(&record, Property::Charge, &ThermoMemo::default()),
        Some(-1.0)
    );
    assert_eq!(
        property_value(&record, Property::Multiplicity, &ThermoMemo::default()),
        Some(2.0)
    );
    assert_eq!(
        property_value(&record, Property::Natoms, &ThermoMemo::default()),
        Some(2.0)
    );
    assert_eq!(
        property_value(&record, Property::Nfreq, &ThermoMemo::default()),
        Some(3.0)
    );
    assert_eq!(
        property_value(&record, Property::Nimag, &ThermoMemo::default()),
        Some(1.0)
    );
    assert_eq!(
        property_value(&record, Property::Dip, &ThermoMemo::default()),
        None
    );
}

// ---------------------------------------------------------------------------
// Milestone 6: Gibbs-valued stages (rqm/thermo.md, rqm/filtering.md).
// ---------------------------------------------------------------------------

use compiled_jkqc::thermo::thermochemistry;

fn default_conditions() -> ThermoConditions {
    ThermoConditions::default()
}

/// A nonlinear water-like record with `components` whose Gibbs free energy
/// hits `target` exactly.
fn gibbs_record(name: &str, target: f64, comps: Option<Components>) -> Record {
    let mut record = Record {
        name: Some(name.to_string()),
        electronic_energy: Some(0.0),
        geometry: Some(Geometry {
            symbols: ["O", "H", "H"].iter().map(|s| s.to_string()).collect(),
            coordinates: vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.9572, 0.9259, 0.0, -0.2423],
        }),
        vibrational_frequencies: Some(vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]),
        components: comps,
        ..Record::default()
    };
    let correction = thermochemistry(&record, &default_conditions())
        .expect("template defined")
        .gibbs;
    record.electronic_energy = Some(target - correction);
    record
}

#[test] // rq-60d713bc
fn a_relative_gibbs_cutoff_window_works_per_group_in_kcal_mol() {
    let am = Some(components(&[("am", 2)]));
    let sa = Some(components(&[("sa", 1)]));
    let records = vec![
        gibbs_record("at-min", 0.0, am.clone()),
        gibbs_record("above", 0.02, am),
        gibbs_record("other-group", -5.0, sa),
    ];
    let spec = FilterSpec {
        cutoffs: vec![cutoff(CmpOp::Le, Mode::Relative, Property::G, 5.0)],
        ..FilterSpec::default()
    };
    // The 0.02 Eh record sits 12.6 kcal/mol above its group minimum and
    // drops; the other group's minimum is its own reference and survives.
    assert_eq!(names(&run(&records, &spec)), ["at-min", "other-group"]);
}

/// A planar ring of `atoms` carbons: a nonlinear geometry with 3N−6 modes.
fn ring_record(name: &str, atoms: usize, mode_value: f64, comps: Option<Components>) -> Record {
    let mut coordinates = Vec::new();
    for atom in 0..atoms {
        let angle = atom as f64 * 2.0 * std::f64::consts::PI / atoms as f64;
        coordinates.extend([(atom % 7) as f64, angle.cos() * 3.0, angle.sin() * 3.0]);
    }
    let modes = 3 * atoms - 6;
    let mut frequencies = vec![0.0; 6];
    frequencies.resize(6 + modes, mode_value);
    Record {
        name: Some(name.to_string()),
        electronic_energy: Some(0.0),
        geometry: Some(Geometry {
            symbols: vec!["C".to_string(); atoms],
            coordinates,
        }),
        vibrational_frequencies: Some(frequencies),
        components: comps,
        ..Record::default()
    }
}

#[test] // rq-658aba8c
fn glob_keeps_each_group_s_gibbs_minimum_among_the_selected() {
    // Group A: two stiff rings plus one very soft ring holding the group's
    // lowest gibbs — but its electronic energy sorts it last, so --select 2
    // (after the implicit el sort) excludes it before --glob runs.
    let am = Some(components(&[("am", 2)]));
    let sa = Some(components(&[("sa", 1)]));
    let correction = |record: &Record| {
        thermochemistry(record, &default_conditions())
            .expect("ring thermochemistry defined")
            .gibbs
    };
    let mut a = ring_record("a", 6, 2000.0, am.clone());
    let mut b = ring_record("b", 6, 2000.0, am.clone());
    let mut soft = ring_record("soft", 24, 2.0, am.clone());
    let alone = ring_record("alone", 6, 2000.0, sa);
    let soft_correction = correction(&soft); // with el = 0: gibbs == correction
    a.electronic_energy = Some(-50.05);
    b.electronic_energy = Some(-50.02);
    soft.electronic_energy = Some(-50.10 - soft_correction); // gibbs = -50.10
                                                             // Invariants the pipeline relies on, asserted for the record.
    assert!(
        soft.electronic_energy.unwrap() > b.electronic_energy.unwrap(),
        "the soft ring must sort last by el"
    );
    let (g_a, g_b, g_soft) = (correction(&a), correction(&b), correction(&soft));
    assert!(g_a < g_b, "the first selected ring holds the lower gibbs");
    assert!(g_soft < g_a, "the excluded ring holds the group minimum");

    let records = vec![a, b, soft, alone];
    let spec = FilterSpec {
        select: Some(2),
        glob: true,
        ..FilterSpec::default()
    };
    assert_eq!(names(&run(&records, &spec)), ["a", "alone"]);
}

#[test] // rq-58808413
fn glob_drops_records_with_undefined_gibbs() {
    // No electronic energy: undefined gibbs, so the group contributes
    // nothing.
    let mut bare = record("bare", None, Some(components(&[("am", 2)])));
    bare.geometry = Some(Geometry {
        symbols: vec!["H".to_string(), "H".to_string()],
        coordinates: vec![0.0, 0.0, 0.0, 0.74, 0.0, 0.0],
    });
    let spec = FilterSpec {
        glob: true,
        ..FilterSpec::default()
    };
    assert_eq!(names(&run(&[bare], &spec)), Vec::<String>::new());
}

#[test] // rq-4ea8ca35
fn energy_columns_print_kcal_mol_under_unit() {
    let record = gibbs_record("warm", -1.0, Some(components(&[("am", 1)])));
    let columns = [
        Column::Filename,
        Column::El,
        Column::Zpe,
        Column::H,
        Column::S,
        Column::G,
    ];
    let kcal = print_table(
        std::slice::from_ref(&record),
        &columns,
        &ThermoMemo::default(),
        Unit::Kcal,
        Grouping::Composition,
        &compiled_jkqc::IonDeclarations::default(),
    );
    let thermo = thermochemistry(&record, &default_conditions()).unwrap();
    let per_hartree = compiled_jkqc::KCAL_MOL_PER_HARTREE;
    let lines: Vec<Vec<String>> = kcal
        .lines()
        .map(|line| line.split_whitespace().map(str::to_string).collect())
        .collect();
    assert_eq!(
        lines[0],
        ["filename", "el", "zpe", "h", "s", "g"]
            .map(str::to_string)
            .to_vec()
    );
    assert_eq!(
        lines[1],
        [
            "warm",
            &format!("{:.4}", record.electronic_energy.unwrap() * per_hartree),
            &format!("{:.4}", thermo.zpe * per_hartree),
            &format!("{:.4}", thermo.enthalpy * per_hartree),
            &format!("{:.4}", thermo.entropy_term * per_hartree),
            &format!("{:.4}", thermo.gibbs * per_hartree),
        ]
        .map(str::to_string)
        .to_vec()
    );
}
