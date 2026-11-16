//! Integration tests for composition resolution, one per Gherkin scenario
//! in `rqm/composition.md`.

use compiled_jkqc::{
    parse_jkqc_name, rename_component, resolve_components, Components, Record, RecordStatus,
    RenameError,
};

fn components(pairs: &[(&str, u32)]) -> Components {
    pairs
        .iter()
        .map(|(code, count)| (code.to_string(), *count))
        .collect()
}

fn assert_parsed(name: &str, expected: Components) {
    assert_eq!(parse_jkqc_name(name), Some(expected), "name {name:?}");
}

fn assert_unresolved(name: &str) {
    assert_eq!(parse_jkqc_name(name), None, "name {name:?}");
}

#[test] // rq-3a95d0f8
fn two_component_name_resolves() {
    assert_parsed("2am3sa", components(&[("am", 2), ("sa", 3)]));
}

#[test] // rq-522c8a7d
fn conformer_label_after_hyphen_is_ignored() {
    assert_parsed("2am3sa-1conf", components(&[("am", 2), ("sa", 3)]));
}

#[test] // rq-18ba8ffb
fn label_with_hyphens_and_underscores_is_ignored() {
    assert_parsed(
        "1pinic1sa-110_1_9-str9",
        components(&[("pinic", 1), ("sa", 1)]),
    );
}

#[test] // rq-306e6285
fn proton_adduct_resolves() {
    assert_parsed("1am1p", components(&[("am", 1), ("p", 1)]));
}

#[test] // rq-cae3826b
fn single_component_name_resolves() {
    assert_parsed("1dma", components(&[("dma", 1)]));
}

#[test] // rq-fdd7be52
fn repeated_codes_sum() {
    assert_parsed("1am1am", components(&[("am", 2)]));
}

#[test] // rq-9d155d29
fn leading_zeros_in_count_are_accepted() {
    assert_parsed("02am", components(&[("am", 2)]));
}

#[test] // rq-8197fd08
fn multi_part_fixture_name_resolves_via_its_stem() {
    assert_parsed("2cha-020_1_0-str3", components(&[("cha", 2)]));
}

#[test] // rq-1adfe3e9
fn implicit_count_does_not_resolve() {
    assert_unresolved("am2sa");
}

#[test] // rq-3091cb1e
fn uppercase_codes_resolve_and_differ_from_lowercase() {
    assert_parsed("2Am3sa", components(&[("Am", 2), ("sa", 3)]));
}

#[test] // rq-5d9fe801
fn codes_differing_only_in_case_are_distinct() {
    assert_parsed("1am1Am", components(&[("am", 1), ("Am", 1)]));
}

#[test] // rq-fb64102b
fn chemical_formula_does_not_resolve() {
    assert_unresolved("h2o");
}

#[test] // rq-3275eeb1
fn descriptive_stem_does_not_resolve() {
    assert_unresolved("trimer-truncated");
}

#[test] // rq-a1b00922
fn digit_only_name_does_not_resolve() {
    assert_unresolved("123");
}

#[test] // rq-160a5957
fn zero_count_makes_name_unresolvable() {
    assert_unresolved("0am1p");
}

#[test] // rq-dcf4a545
fn count_beyond_u32_max_makes_name_unresolvable() {
    assert_unresolved("99999999999am");
}

#[test] // rq-cb100db2
fn empty_name_does_not_resolve() {
    assert_unresolved("");
}

#[test] // rq-6eeaa8ed
fn empty_composition_part_does_not_resolve() {
    assert_unresolved("-1conf");
}

#[test] // rq-c66abc39
fn non_ascii_character_in_code_does_not_resolve() {
    assert_unresolved("1ãm");
}

#[test] // rq-a3a948bc
fn record_gains_components_from_resolvable_name() {
    let mut record = Record {
        name: Some("2am3sa-1conf".to_string()),
        ..Record::default()
    };
    resolve_components(&mut record);
    assert_eq!(record.components, Some(components(&[("am", 2), ("sa", 3)])));
}

#[test] // rq-9215f506
fn record_carried_composition_wins_over_name() {
    let mut record = Record {
        name: Some("2am3sa".to_string()),
        components: Some(components(&[("w", 1)])),
        ..Record::default()
    };
    resolve_components(&mut record);
    assert_eq!(record.components, Some(components(&[("w", 1)])));
}

#[test] // rq-b992e561
fn nameless_record_is_left_unchanged() {
    let mut record = Record::default();
    resolve_components(&mut record);
    assert_eq!(record.components, None);
}

#[test] // rq-ab9f329c
fn unresolvable_name_leaves_components_absent() {
    let mut record = Record {
        name: Some("h2o".to_string()),
        ..Record::default()
    };
    resolve_components(&mut record);
    assert_eq!(record.components, None);
}

#[test] // rq-0f1c73be
fn failed_record_still_gains_its_composition() {
    let mut record = Record {
        status: RecordStatus::Failed,
        error: Some("SCF did not converge".to_string()),
        name: Some("1dma".to_string()),
        ..Record::default()
    };
    resolve_components(&mut record);
    assert_eq!(record.components, Some(components(&[("dma", 1)])));
}

#[test] // rq-4dac0963
fn renaming_a_code_moves_its_count() {
    let mut comps = components(&[("sa", 3), ("am", 1), ("p", 1)]);
    rename_component(&mut comps, "p", "a").expect("rename succeeds");
    assert_eq!(comps, components(&[("sa", 3), ("am", 1), ("a", 1)]));
}

#[test] // rq-340fd4c2
fn renaming_into_an_existing_code_merges_the_counts() {
    let mut comps = components(&[("sa", 3), ("am", 1), ("p", 1)]);
    rename_component(&mut comps, "p", "sa").expect("rename succeeds");
    assert_eq!(comps, components(&[("sa", 4), ("am", 1)]));
}

#[test] // rq-b1b28e9d
fn renaming_an_absent_code_leaves_the_map_unchanged() {
    let mut comps = components(&[("am", 2), ("sa", 3)]);
    rename_component(&mut comps, "p", "a").expect("rename succeeds");
    assert_eq!(comps, components(&[("am", 2), ("sa", 3)]));
}

#[test] // rq-1404f4d5
fn renaming_a_code_to_itself_is_a_no_op() {
    let mut comps = components(&[("am", 2), ("p", 1)]);
    rename_component(&mut comps, "p", "p").expect("rename succeeds");
    assert_eq!(comps, components(&[("am", 2), ("p", 1)]));
}

#[test] // rq-5c0d989f
fn rename_codes_are_case_sensitive() {
    let mut comps = components(&[("p", 1)]);
    rename_component(&mut comps, "P", "a").expect("rename succeeds");
    assert_eq!(comps, components(&[("p", 1)]));
}

#[test] // rq-870c7feb
fn a_count_merge_beyond_u32_max_is_refused() {
    let mut comps = components(&[("p", 1), ("sa", u32::MAX)]);
    let result = rename_component(&mut comps, "p", "sa");
    assert_eq!(
        result,
        Err(RenameError::CountOverflow {
            code: "sa".to_string()
        })
    );
    assert_eq!(comps, components(&[("p", 1), ("sa", u32::MAX)]));
}
