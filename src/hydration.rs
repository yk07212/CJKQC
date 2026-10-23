//! Hydration and solvation distributions (see `rqm/hydration.md`): the
//! `--hydration`/`--solvation` sections print, per core composition, the
//! Boltzmann distribution of the solvent count over the hydration states
//! `(core)(solvent)ₙ`. The distribution values are the concentration-
//! corrected formation free energies the formation sections compute, with
//! the solvent's chemical potential set by the run's implied `--conc`
//! spec, so every non-solvent term cancels in the normalization and the
//! weights reduce to JKQC's `(p/p°)ⁿ·exp(−(G − n·G_solvent)/(k_B T))`.
//! Nothing is stored: values derive at request time under the run's
//! thermochemistry conditions.
//!
//! (rq-206d4784)

use crate::filter::{composition_token, SortKey};
use crate::formation::{
    collect_monomers, corrected_ensemble_formation_values, corrected_ensemble_references,
    corrected_formation_values, corrected_monomer_references, ensemble_formation_values,
    ensemble_references, formation_values, ConcConditions, Ensemble,
};
use crate::population::boltzmann_weights;
use crate::record::{Components, Record, RecordStatus};
use crate::thermo::{ThermoMemo, E_HARTREE, K_B};
use std::collections::HashMap;

/// The run's resolved solvent setting: the solvent component code and its
/// partial pressure in Pa.
///
/// (rq-af47884f)
#[derive(Debug, Clone, PartialEq)]
pub struct SolventConditions {
    /// The solvent component code, case-significant and letter-only.
    pub code: String,
    /// The solvent's partial pressure in Pa.
    pub pressure: f64,
}

/// One hydration state: a components map and its distribution value in
/// hartree — the section's level's formation value.
///
/// (rq-6bc74daa)
#[derive(Debug, Clone, PartialEq)]
pub struct HydrationState {
    /// The state's components map, solvent included.
    pub components: Components,
    /// The distribution value in Eh, `None` when no member of the state
    /// defines the level.
    pub value: Option<f64>,
}

/// One core group: the states sharing one core composition, in
/// first-appearance order.
///
/// (rq-0b5e2729)
#[derive(Debug, Clone, PartialEq)]
pub struct HydrationGroup {
    /// The group's core: the components map minus the solvent code.
    pub core: Components,
    /// The group's states in first-appearance order.
    pub states: Vec<HydrationState>,
}

/// The water saturation-vapor pressure in Pa: the Magnus correlation
/// p_sat(T) = 610.94·exp(17.625·τ/(243.04+τ)) with τ = T − 273.15
/// (Alduchov–Eskridge 1996), pinned by 610.94 Pa at 273.15 K and about
/// 3.162e3 Pa at 298.15 K (`rqm/hydration.md` Solvent Pressure).
///
/// (rq-7e9e7176)
pub fn saturation_pressure_water(temperature: f64) -> f64 {
    let tau = temperature - 273.15;
    610.94 * (17.625 * tau / (243.04 + tau)).exp()
}

/// The core of a components map: the map minus the solvent code
/// (`rqm/hydration.md` Hydration States).
// rq-42cdecff
fn core_of(components: &Components, solvent: &str) -> Components {
    components
        .iter()
        .filter(|(code, _)| code.as_str() != solvent)
        .map(|(code, count)| (code.clone(), *count))
        .collect()
}

/// The core groups over `(components, value)` pairs in first-appearance
/// order: core groups in first-appearance order of their core, states
/// within a group in first-appearance order of their components map. A
/// pair with an empty core — a pure solvent cluster — is excluded
/// (`rqm/hydration.md` Hydration States); a `None` value creates its
/// state without contributing to the value, so a state with no defined
/// member stays undefined.
// rq-42cdecff
fn group_by_core<I>(pairs: I, solvent: &str) -> Vec<HydrationGroup>
where
    I: Iterator<Item = (Components, Option<f64>)>,
{
    // First-appearance grouping through hash maps, the idiom of
    // `filter::group_indices`: a state's components map determines its
    // core, so one map keyed by the full map assigns both the group and
    // the state; cores get their own map for the groups born with a
    // later member.
    let mut groups: Vec<HydrationGroup> = Vec::new();
    let mut core_ids: HashMap<Components, usize> = HashMap::new();
    let mut state_ids: HashMap<Components, (usize, usize)> = HashMap::new();
    for (components, value) in pairs {
        let core = core_of(&components, solvent);
        if core.is_empty() {
            continue;
        }
        let (group_position, state_position) = match state_ids.get(&components) {
            Some(&(group_position, state_position)) => (group_position, state_position),
            None => {
                let group_position = match core_ids.get(&core) {
                    Some(&group_position) => group_position,
                    None => {
                        let group_position = groups.len();
                        groups.push(HydrationGroup {
                            core: core.clone(),
                            states: Vec::new(),
                        });
                        core_ids.insert(core.clone(), group_position);
                        group_position
                    }
                };
                let state_position = groups[group_position].states.len();
                groups[group_position].states.push(HydrationState {
                    components: components.clone(),
                    value: None,
                });
                state_ids.insert(components, (group_position, state_position));
                (group_position, state_position)
            }
        };
        let state = &mut groups[group_position].states[state_position];
        if let Some(value) = value {
            state.value = Some(match state.value {
                Some(existing) => existing.min(value),
                None => value,
            });
        }
    }
    groups
}

/// The record-level state builder shared by the raw and corrected paths:
/// each record with components joins its core group, and its level value
/// — produced by `value_of` — contributes to the state's minimum when the
/// record is `ok`. Failed and truncated records contribute nothing
/// exactly as the pipeline excludes them (`rqm/filtering.md`), even when
/// they still carry observables (`rqm/hydration.md` Hydration States).
// rq-42cdecff
fn states_from_records<F>(records: &[Record], solvent: &str, value_of: F) -> Vec<HydrationGroup>
where
    F: Fn(&Record) -> Option<f64>,
{
    let pairs = records.iter().filter_map(|record| {
        let components = record.components.clone()?;
        let value = if record.status == RecordStatus::Ok {
            value_of(record)
        } else {
            None
        };
        Some((components, value))
    });
    group_by_core(pairs, solvent)
}

/// The ensemble-level state builder shared by the raw and corrected
/// `--bavg` paths: each named ensemble is a state, its value the
/// ensemble's level value (`rqm/hydration.md` Hydration States).
// rq-42cdecff
fn states_from_ensembles<F>(
    ensembles: &[Ensemble],
    solvent: &str,
    value_of: F,
) -> Vec<HydrationGroup>
where
    F: Fn(&Ensemble) -> Option<f64>,
{
    let pairs = ensembles.iter().filter_map(|ensemble| {
        let components = ensemble.components.clone()?;
        Some((components, value_of(ensemble)))
    });
    group_by_core(pairs, solvent)
}

/// The uncollapsed raw hydration states over the given records (the run
/// passes its final record set and sort ordering,
/// `rqm/hydration.md` Hydration States): the monomer references of
/// `collect_monomers` under the sort ordering, each state's value the
/// lowest free conformer — the minimum over the state's members' defined
/// formation free energies.
///
/// (rq-2959f663)
pub fn hydration_states(
    records: &[Record],
    sort: SortKey,
    memo: &ThermoMemo,
    concs: &ConcConditions,
    solvent: &str,
) -> Vec<HydrationGroup> {
    let references = collect_monomers(records, memo, sort);
    states_from_records(records, solvent, |record| {
        formation_values(record, &references, memo, concs).gibbs
    })
}

/// The corrected twin of [`hydration_states`]: the same selection and
/// grouping, read through `corrected_monomer_references` and
/// `corrected_formation_values` (`rqm/formation.md`); identical
/// otherwise (`rqm/hydration.md` Hydration States).
///
/// (rq-0cfc6832)
pub fn corrected_hydration_states(
    records: &[Record],
    sort: SortKey,
    memo: &ThermoMemo,
    concs: &ConcConditions,
    solvent: &str,
) -> Vec<HydrationGroup> {
    let references = corrected_monomer_references(records, memo, sort);
    states_from_records(records, solvent, |record| {
        corrected_formation_values(record, &references, memo, concs).gibbs
    })
}

/// The `--bavg` raw hydration states over the collapsed ensembles
/// (`rqm/hydration.md` Hydration States): each ensemble is a state, its
/// value the ensemble's formation free energy — the collapse replaces
/// the conformer minimum, conformational entropy included.
///
/// (rq-1b2f8c5e)
pub fn hydration_states_from_ensembles(
    ensembles: &[Ensemble],
    memo: &ThermoMemo,
    concs: &ConcConditions,
    solvent: &str,
) -> Vec<HydrationGroup> {
    let references = ensemble_references(ensembles);
    states_from_ensembles(ensembles, solvent, |ensemble| {
        ensemble_formation_values(ensemble, &references, memo, concs).gibbs
    })
}

/// The corrected twin of [`hydration_states_from_ensembles`] over the
/// `corrected_collapse_ensembles` output; identical otherwise
/// (`rqm/hydration.md` Hydration States). The raw and corrected
/// spellings of each state builder share one level-parameterized
/// implementation (the Level Parity rule of `rqm/population.md`).
///
/// (rq-366add70)
pub fn corrected_hydration_states_from_ensembles(
    ensembles: &[Ensemble],
    memo: &ThermoMemo,
    concs: &ConcConditions,
    solvent: &str,
) -> Vec<HydrationGroup> {
    let references = corrected_ensemble_references(ensembles);
    states_from_ensembles(ensembles, solvent, |ensemble| {
        corrected_ensemble_formation_values(ensemble, &references, memo, concs).gibbs
    })
}

/// The normalized percentage of each defined state in the group, aligned
/// with `states`: 100 · exp(−(v − v_min)/(k_B T)) / Σ exp over the
/// defined states, with v_min the group's minimum defined value and T
/// the given temperature; `None` for undefined states and when no state
/// of the group defines the level (`rqm/hydration.md` The Distribution).
/// The minimum subtraction keeps every exponent ≤ 0, so all factors lie
/// in [0, 1] and no overflow is reachable (`rqm/population.md`
/// Numerics); a state far above the minimum underflows to 0.0 without
/// affecting the rest.
///
/// (rq-0a350842)
pub fn hydration_percentages(group: &HydrationGroup, temperature: f64) -> Vec<Option<f64>> {
    let kbt = K_B * temperature / E_HARTREE;
    let values: Vec<Option<f64>> = group.states.iter().map(|state| state.value).collect();
    match boltzmann_weights(&values, kbt) {
        None => vec![None; group.states.len()],
        Some((weights, _)) => weights
            .into_iter()
            .map(|weight| weight.map(|weight| weight * 100.0))
            .collect(),
    }
}

/// The paired-row layout of one or more core groups
/// (`rqm/hydration.md` Printing): per group the states' composition
/// tokens — the same alphabetical pairs the `--ct` column prints — then
/// their percentages with one decimal, `-` for an undefined state. No
/// header row; columns align within a group, each as wide as the wider
/// of its token and percentage cell, cells right-aligned, single space
/// between columns; a blank line follows every group's percentage row.
///
/// (rq-70efc347)
pub fn print_hydration_section(groups: &[HydrationGroup], temperature: f64) -> String {
    let mut blocks: Vec<String> = Vec::with_capacity(groups.len());
    for group in groups {
        let percentages = hydration_percentages(group, temperature);
        let tokens: Vec<String> = group
            .states
            .iter()
            .map(|state| {
                composition_token(Some(&state.components))
                    .expect("a state's components map is non-empty")
            })
            .collect();
        let cells: Vec<String> = percentages
            .iter()
            .map(|percentage| {
                percentage.map_or_else(|| "-".to_string(), |percentage| format!("{percentage:.1}"))
            })
            .collect();
        // Columns align within a group: each column is as wide as the
        // wider of its token and percentage cell (Printing).
        // rq-2a926651
        let widths: Vec<usize> = tokens
            .iter()
            .zip(&cells)
            .map(|(token, cell)| token.len().max(cell.len()))
            .collect();
        let token_row: Vec<String> = tokens
            .iter()
            .zip(&widths)
            .map(|(token, width)| format!("{token:>width$}"))
            .collect();
        let cell_row: Vec<String> = cells
            .iter()
            .zip(&widths)
            .map(|(cell, width)| format!("{cell:>width$}"))
            .collect();
        // A blank line follows every group's percentage row, so groups
        // read as blocks (Printing).
        // rq-e52f165b
        blocks.push(format!("{}\n{}\n", token_row.join(" "), cell_row.join(" ")));
    }
    blocks.join("\n")
}
