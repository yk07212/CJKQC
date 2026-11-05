//! Formation energies from monomer references (see `rqm/formation.md`):
//! a run's final records are scanned for monomers (single-component,
//! count-one `ok` records), each component code gets one reference record
//! — the first of its monomers under the run's sort ordering — and the
//! `--formation` flag prints each printed record's formation energy and
//! formation free energy as a section after the normal property table.
//! The `--conc` and `--cnt` flags evaluate the free-energy level at
//! vapors of named components at given concentrations. Nothing is
//! stored: values derive at request time under the run's thermochemistry
//! conditions.
//!
//! (rq-10341821)

use std::collections::BTreeMap;

use crate::filter::{
    align_table, composition_token, format_energy, sort_value_for, Alignment, Grouping, Unit,
};
use crate::population::{boltzmann_weights, pop_level_value, PopLevel};
use crate::record::{Components, Record, RecordStatus};
use crate::thermo::{ThermoMemo, E_HARTREE, K_B, P_STANDARD};

/// The per-code monomer references in hartree, derived from the reference
/// records' observables.
///
/// (rq-a14c7897)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MonomerReferences {
    /// Electronic energy of each code's reference monomer.
    pub electronic: BTreeMap<String, f64>,
    /// Gibbs free energy of each code's reference monomer at the run's
    /// conditions.
    pub gibbs: BTreeMap<String, f64>,
}

/// The per-code corrected references in hartree: the same selection as
/// [`collect_monomers`], read through the corrected keys `elcorr`/`gcorr`
/// (`rqm/correction.md`); a code appears only when its selected monomer
/// defines that level's corrected value.
///
/// (rq-67b44e63)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CorrectedMonomerReferences {
    /// Correction energy of each code's reference monomer.
    pub electronic: BTreeMap<String, f64>,
    /// Corrected Gibbs free energy of each code's reference monomer at the
    /// run's conditions.
    pub gibbs: BTreeMap<String, f64>,
}

/// A record's two derived formation levels, both in hartree.
///
/// (rq-103447d2)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Formation {
    /// Formation energy (electronic level).
    pub electronic: Option<f64>,
    /// Formation free energy (Gibbs level).
    pub gibbs: Option<f64>,
}

/// The VALUE grammar's units (`rqm/formation.md` Formation at Given
/// Concentrations): Pa plain, `ppt`/`ppb` mole fractions of the standard
/// atmosphere, `cmmc` molecules per cm³.
///
/// (rq-7d64cce1)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConcUnit {
    /// A partial pressure in Pa.
    Pa,
    /// A mole fraction of 10⁻¹² of the standard atmosphere.
    Ppt,
    /// A mole fraction of 10⁻⁹ of the standard atmosphere.
    Ppb,
    /// Molecules per cm³, converted through the ideal-gas law at the
    /// run's temperature.
    Cmmc,
}

/// One `--conc` spec: a component code and a concentration magnitude in
/// the given unit.
///
/// (rq-c92b9ff1)
#[derive(Debug, Clone, PartialEq)]
pub struct ConcSpec {
    /// The component code the spec covers.
    pub code: String,
    /// The concentration magnitude quoted in `unit`.
    pub magnitude: f64,
    /// The unit `magnitude` is quoted in.
    pub unit: ConcUnit,
}

impl ConcSpec {
    /// The spec's partial pressure in Pa: `Pa` the magnitude itself,
    /// `Ppt`/`Ppb` magnitude·10⁻¹²/10⁻⁹·p°, `Cmmc` the number density
    /// converted through the ideal-gas law at the given temperature.
    ///
    /// (rq-c92b9ff1)
    pub fn pressure(&self, temperature: f64) -> f64 {
        match self.unit {
            ConcUnit::Pa => self.magnitude,
            ConcUnit::Ppt => self.magnitude * 1.0e-12 * P_STANDARD,
            ConcUnit::Ppb => self.magnitude * 1.0e-9 * P_STANDARD,
            ConcUnit::Cmmc => self.magnitude * 1.0e6 * K_B * temperature,
        }
    }
}

/// The run's concentration settings: the parsed `--conc` specs and the
/// `--cnt` self-consistent switch. The default — no specs, `--cnt` off —
/// leaves the formation free energies at the 1 atm standard state.
///
/// (rq-a0748032)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConcConditions {
    /// The parsed `--conc CODE VALUE` specs in command order.
    pub specs: Vec<ConcSpec>,
    /// Whether `--cnt` scales each named component's term by (N−1)/N.
    pub self_consistent: bool,
}

/// One collapsed composition group (Boltzmann-Averaged Sections): the
/// ensemble of one composition's records, with `components` `None` for
/// the unnamed ensemble of the records without components. The two values
/// in Eh — the population-weighted electronic mean and the
/// partition-function free energy — are `Some` together or `None`
/// together; an ensemble without values is a group whose records all lack
/// the section's Gibbs level.
///
/// (rq-35671b9e)
#[derive(Debug, Clone, PartialEq)]
pub struct Ensemble {
    /// The group's components map, `None` for the unnamed ensemble.
    pub components: Option<Components>,
    /// The population-weighted mean of the members' electronic-level
    /// inputs.
    pub electronic: Option<f64>,
    /// The ensemble free energy −k_B·T·ln(Σⱼ exp(−gⱼ/(k_B·T))).
    pub gibbs: Option<f64>,
}

/// The VALUE grammar of `--conc`: a decimal or scientific-notation
/// number, optionally suffixed lowercase with `ppt`, `ppb`, or `cmmc`.
/// `None` for a value outside the grammar, non-finite, or not positive —
/// the CLI turns `None` into a usage error (`rqm/formation.md`).
///
/// (rq-f8a2fb33)
pub fn parse_conc_value(text: &str) -> Option<(f64, ConcUnit)> {
    let (number, unit) = if let Some(number) = text.strip_suffix("ppt") {
        (number, ConcUnit::Ppt)
    } else if let Some(number) = text.strip_suffix("ppb") {
        (number, ConcUnit::Ppb)
    } else if let Some(number) = text.strip_suffix("cmmc") {
        (number, ConcUnit::Cmmc)
    } else {
        (text, ConcUnit::Pa)
    };
    let magnitude = number.parse::<f64>().ok()?;
    if !magnitude.is_finite() || magnitude <= 0.0 {
        return None;
    }
    Some((magnitude, unit))
}

/// The concentration term subtracted from a defined formation free
/// energy, in Eh: the sum over the specs whose code is in the components
/// map of nᵢ,eff · k_B T ln(cᵢ/p°) / E_h, with nᵢ,eff = nᵢ or, under
/// `self_consistent`, nᵢ·(N−1)/N for the record's total molecule count N.
/// A spec whose code is absent from the components contributes nothing,
/// so with no matching spec the term is 0.
///
/// (rq-2333d9cc)
pub fn concentration_correction(
    components: &Components,
    concs: &ConcConditions,
    temperature: f64,
) -> f64 {
    let total = components.values().copied().sum::<u32>();
    let mut shift = 0.0;
    for spec in &concs.specs {
        let Some(count) = components.get(&spec.code) else {
            continue;
        };
        let count = f64::from(*count);
        let effective = if concs.self_consistent {
            count * (f64::from(total) - 1.0) / f64::from(total)
        } else {
            count
        };
        shift += effective * K_B * temperature * (spec.pressure(temperature) / P_STANDARD).ln()
            / E_HARTREE;
    }
    shift
}

/// Whether a components map is monomer-shaped: exactly one entry with
/// count 1 (`rqm/formation.md` Monomer References) — the shape test
/// shared by the record-level and ensemble-level monomer checks.
// rq-1b571f91 rq-35671b9e
fn is_monomer_shaped(components: &Components) -> bool {
    components.len() == 1 && components.values().all(|count| *count == 1)
}

/// Whether a record is a monomer reference candidate: status `ok` and a
/// monomer-shaped components map (`rqm/formation.md`).
// rq-1b571f91
fn is_monomer(record: &Record) -> bool {
    record.status == RecordStatus::Ok && record.components.as_ref().is_some_and(is_monomer_shaped)
}

/// The reference scan over the given records (the run passes its final
/// record set, `rqm/filtering.md`): per component code, the first of that
/// code's monomers under the ascending sort ordering of `sort` — undefined
/// sort-key values order after defined ones — supplies both reference
/// values; a level the chosen record lacks has no reference. The memo
/// carries the run's thermochemistry conditions and derives each record
/// at most once (`rqm/thermo.md` Performance).
///
/// (rq-9e898298 rq-8b44ad8d)
pub fn collect_monomers(
    records: &[Record],
    memo: &ThermoMemo,
    sort: crate::filter::SortKey,
) -> MonomerReferences {
    let mut references = MonomerReferences::default();
    for (code, reference) in select_references(records, memo, sort) {
        if let Some(electronic) = reference.electronic_energy {
            references.electronic.insert(code.to_string(), electronic);
        }
        if let Some(thermo) = memo.thermo(reference) {
            references.gibbs.insert(code.to_string(), thermo.gibbs);
        }
    }
    references
}

/// The per-code reference selection shared by the raw and corrected
/// sections: per component code, the first of that code's monomers under
/// the ascending sort ordering of `sort`; the sort is stable, so ties keep
/// load order (`rqm/formation.md`).
// rq-9e898298
fn select_references<'a>(
    records: &'a [Record],
    memo: &ThermoMemo,
    sort: crate::filter::SortKey,
) -> BTreeMap<String, &'a Record> {
    let mut candidates: BTreeMap<&str, Vec<&Record>> = BTreeMap::new();
    for record in records {
        if is_monomer(record) {
            let code = record
                .components
                .as_ref()
                .and_then(|components| components.keys().next())
                .map(String::as_str);
            if let Some(code) = code {
                candidates.entry(code).or_default().push(record);
            }
        }
    }
    let mut selection = BTreeMap::new();
    for (code, monomers) in candidates {
        // The first monomer under the ascending sort ordering; the sort is
        // stable, so ties keep load order (rqm/formation.md).
        let mut reference = monomers[0];
        for &monomer in &monomers[1..] {
            if sorts_before(monomer, reference, sort, memo) {
                reference = monomer;
            }
        }
        selection.insert(code.to_string(), reference);
    }
    selection
}

/// Whether `left` sorts strictly before `right` under the ascending
/// ordering of `sort`; `--reverse` never applies to the reference
/// (rqm/formation.md). Nameless monomers order first under `filename`.
// rq-9e898298
fn sorts_before(
    left: &Record,
    right: &Record,
    sort: crate::filter::SortKey,
    memo: &ThermoMemo,
) -> bool {
    match sort {
        crate::filter::SortKey::Filename => match (left.name.as_deref(), right.name.as_deref()) {
            (None, Some(_)) => true,
            (Some(a), Some(b)) => a < b,
            _ => false,
        },
        _ => match (
            sort_value_for(left, sort, memo),
            sort_value_for(right, sort, memo),
        ) {
            (Some(a), Some(b)) => a < b,
            (Some(_), None) => true, // defined values order before undefined
            _ => false,
        },
    }
}

/// A record's two formation levels with their independent definedness:
/// electronic energy minus the stoichiometric mix of electronic references,
/// Gibbs free energy minus the mix of Gibbs references. The record's Gibbs
/// value resolves through the derived-property registry on the run's memo
/// (`rqm/thermo.md`); when defined, it has the concentration term of
/// `concentration_correction` under the run's `ConcConditions` subtracted,
/// and a correction that turns non-finite (an absurd input magnitude)
/// leaves the level undefined rather than infinite. A level is undefined
/// when the record lacks components, lacks the level's input energy, or
/// any referenced code lacks that level's reference.
///
/// (rq-6ffbf9aa rq-c3425396 rq-8f6da1c2 rq-2333d9cc)
pub fn formation_values(
    record: &Record,
    references: &MonomerReferences,
    memo: &ThermoMemo,
    concs: &ConcConditions,
) -> Formation {
    let Some(components) = record.components.as_ref() else {
        return Formation {
            electronic: None,
            gibbs: None,
        };
    };
    let electronic = record.electronic_energy.and_then(|energy| {
        let mut value = energy;
        for (code, count) in components {
            value -= f64::from(*count) * references.electronic.get(code)?;
        }
        Some(value)
    });
    let gibbs = memo.value("g", record).and_then(|free_energy| {
        let mut value = free_energy;
        for (code, count) in components {
            value -= f64::from(*count) * references.gibbs.get(code)?;
        }
        let value = value - concentration_correction(components, concs, memo.temperature());
        Some(value).filter(|value| value.is_finite())
    });
    Formation { electronic, gibbs }
}

/// The corrected reference scan (`rqm/formation.md`): the same monomer
/// set and the same sort-based selection as the raw section; a code's
/// corrected electronic reference is that monomer's correction energy and
/// its corrected Gibbs reference the monomer's `gcorr` value, and a
/// level's reference is undefined when the selected monomer carries no
/// correction (or its corrected Gibbs is undefined).
///
/// (rq-789635d3)
pub fn corrected_monomer_references(
    records: &[Record],
    memo: &ThermoMemo,
    sort: crate::filter::SortKey,
) -> CorrectedMonomerReferences {
    let mut references = CorrectedMonomerReferences::default();
    for (code, reference) in select_references(records, memo, sort) {
        if let Some(energy) = crate::correction::corrected_energy(reference) {
            references.electronic.insert(code.clone(), energy);
        }
        if let Some(gibbs) = memo.value("gcorr", reference) {
            references.gibbs.insert(code, gibbs);
        }
    }
    references
}

/// A record's two corrected formation levels with their independent
/// definedness: the correction energy minus the stoichiometric mix of
/// corrected electronic references, the corrected Gibbs free energy minus
/// the mix of corrected Gibbs references. A level is defined iff the
/// record's own corrected value at that level is defined and every
/// referenced code has a defined corrected reference at that level —
/// corrected formation numbers never mix levels (`rqm/formation.md`).
/// The corrected Gibbs value, when defined, has the same concentration
/// term as the raw section subtracted, with the same non-finite guard.
///
/// (rq-fb892986 rq-2333d9cc)
pub fn corrected_formation_values(
    record: &Record,
    references: &CorrectedMonomerReferences,
    memo: &ThermoMemo,
    concs: &ConcConditions,
) -> Formation {
    let Some(components) = record.components.as_ref() else {
        return Formation {
            electronic: None,
            gibbs: None,
        };
    };
    let electronic = crate::correction::corrected_energy(record).and_then(|energy| {
        let mut value = energy;
        for (code, count) in components {
            value -= f64::from(*count) * references.electronic.get(code)?;
        }
        Some(value)
    });
    let gibbs = memo.value("gcorr", record).and_then(|free_energy| {
        let mut value = free_energy;
        for (code, count) in components {
            value -= f64::from(*count) * references.gibbs.get(code)?;
        }
        let value = value - concentration_correction(components, concs, memo.temperature());
        Some(value).filter(|value| value.is_finite())
    });
    Formation { electronic, gibbs }
}

/// The formation section: a `name el g` header row, then one row per record
/// in the given order. The `name` cell is the record's composition token
/// (`rqm/filtering.md` Printing), `-` without components; values follow
/// the run's energy unit (kcal/mol under `--unit`, hartree otherwise) and
/// print through the same energy formatter as the table columns, `-` when
/// undefined.
///
/// (rq-9503bcb2 rq-8dee4f4e)
pub fn print_formation_section(
    records: &[Record],
    references: &MonomerReferences,
    memo: &ThermoMemo,
    unit: Unit,
    concs: &ConcConditions,
) -> String {
    let rows: Vec<(String, Formation)> = records
        .iter()
        .map(|record| {
            let token =
                composition_token(record.components.as_ref()).unwrap_or_else(|| "-".to_string());
            let formation = formation_values(record, references, memo, concs);
            (token, formation)
        })
        .collect();
    print_formation_table(&rows, unit, &["name", "el", "g"])
}

/// The corrected formation section (`rqm/formation.md`): the raw
/// section's table with the corrected levels — header `name elcorr gcorr`,
/// the same composition tokens, formatting, and `-` rules.
///
/// (rq-4f20e170)
pub fn print_formation_corr_section(
    records: &[Record],
    references: &CorrectedMonomerReferences,
    memo: &ThermoMemo,
    unit: Unit,
    concs: &ConcConditions,
) -> String {
    let rows: Vec<(String, Formation)> = records
        .iter()
        .map(|record| {
            let token =
                composition_token(record.components.as_ref()).unwrap_or_else(|| "-".to_string());
            let formation = corrected_formation_values(record, references, memo, concs);
            (token, formation)
        })
        .collect();
    print_formation_table(&rows, unit, &["name", "elcorr", "gcorr"])
}

/// The shared section table: a header row, then one `name el g`-shaped row
/// per `(token, formation)` pair in the given order — shared with the
/// collapsed sections (`rqm/formation.md` Boltzmann-Averaged Sections).
/// Aligned like the main table (`rqm/filtering.md` Printing): `name` a
/// text column (left), the energy columns numeric (right), header cells
/// aligning with their columns.
// rq-9503bcb2
fn print_formation_table(rows: &[(String, Formation)], unit: Unit, header: &[&str]) -> String {
    let header_cells: Vec<String> = header.iter().map(|cell| cell.to_string()).collect();
    let mut table = vec![header_cells];
    for (token, formation) in rows {
        let electronic = formation
            .electronic
            .map_or_else(|| "-".to_string(), |value| format_energy(value, unit));
        let gibbs = formation
            .gibbs
            .map_or_else(|| "-".to_string(), |value| format_energy(value, unit));
        table.push(vec![token.clone(), electronic, gibbs]);
    }
    let aligns = [Alignment::Left, Alignment::Right, Alignment::Right];
    align_table(&table, &aligns)
}

/// Whether an ensemble is a code's monomer ensemble: its components map
/// is monomer-shaped (Boltzmann-Averaged Sections).
// rq-35671b9e
fn is_monomer_ensemble(ensemble: &Ensemble) -> bool {
    ensemble.components.as_ref().is_some_and(is_monomer_shaped)
}

/// The electronic level paired with the collapse's Gibbs level: the raw
/// section reads the record's electronic energy, the corrected one the
/// correction energy (`rqm/formation.md` Boltzmann-Averaged Sections).
// rq-dd65e289
fn electronic_level(level: PopLevel) -> PopLevel {
    match level {
        PopLevel::G => PopLevel::El,
        PopLevel::GCorr => PopLevel::ElCorr,
        _ => unreachable!("the collapse collapses at a Gibbs level"),
    }
}

/// The shared collapse behind both sections (Boltzmann-Averaged
/// Sections): the records group by components map in first-appearance
/// order, with the records without components last as the unnamed
/// ensemble — `--noname` never pools the rows (`rqm/filtering.md`
/// Grouping). A group's members are its `ok` records that define the
/// Gibbs level of `level` — the derived Gibbs free energy for the raw
/// section, `gcorr` for the corrected one — resolved through the shared
/// registry resolver of `rqm/population.md`: failed and truncated
/// records contribute nothing exactly as the pipeline excludes them
/// (`rqm/filtering.md`), and an `ok` record without that value
/// contributes nothing (the definedness rule of `rqm/population.md`); a
/// group with no members yields an ensemble without values. The ensemble
/// values are the members' population-weighted mean of the paired
/// electronic level — every member defines that level too, since the
/// Gibbs derives from the electronic energy — and the ensemble free
/// energy g_min − k_B·T·ln Z over the members' Gibbs values, computed
/// against the members' minimum as `rqm/population.md` Numerics
/// prescribes, at the memo's temperature.
// rq-18dba668
fn collapse(records: &[Record], memo: &ThermoMemo, level: PopLevel) -> Vec<Ensemble> {
    // The grouping reuses the pipeline's composition groups; the unnamed
    // group rotates to the end (Boltzmann-Averaged Sections).
    // rq-dd65e289
    let mut groups = crate::filter::group_indices(records, Grouping::Composition);
    if let Some(position) = groups
        .iter()
        .position(|members| records[members[0]].components.is_none())
    {
        let unnamed = groups.remove(position);
        groups.push(unnamed);
    }
    let kbt = K_B * memo.temperature() / E_HARTREE;
    let mut ensembles = Vec::with_capacity(groups.len());
    for indices in groups {
        // The ensemble's components map is the group's own — independent
        // of which records survive as members.
        let components = components_of(records, &indices);
        // The members: the group's `ok` records that define the Gibbs
        // level of `level` (Boltzmann-Averaged Sections).
        // rq-dd65e289
        let members: Vec<usize> = indices
            .into_iter()
            .filter(|&index| {
                records[index].status == RecordStatus::Ok
                    && pop_level_value(&records[index], level, memo).is_some()
            })
            .collect();
        let values: Vec<Option<f64>> = members
            .iter()
            .map(|&index| pop_level_value(&records[index], level, memo))
            .collect();
        let electronic_values: Vec<Option<f64>> = members
            .iter()
            .map(|&index| pop_level_value(&records[index], electronic_level(level), memo))
            .collect();
        let ensemble = match boltzmann_weights(&values, kbt) {
            // No member defines the level: the ensemble carries no values.
            None => Ensemble {
                components,
                electronic: None,
                gibbs: None,
            },
            Some((weights, partition)) => {
                // The population-weighted mean of the members' electronic
                // inputs; a member without one — unreachable, since the
                // Gibbs derives from the electronic energy — leaves the
                // ensemble without values rather than half-defined.
                let electronic = weights.iter().zip(&electronic_values).try_fold(
                    0.0_f64,
                    |sum, (weight, value)| match (weight, value) {
                        (Some(weight), Some(value)) => Some(sum + weight * value),
                        (Some(_), None) => None,
                        (None, _) => Some(sum),
                    },
                );
                match electronic {
                    Some(mean) => {
                        let minimum = values
                            .iter()
                            .filter_map(|value| *value)
                            .reduce(f64::min)
                            .expect("the weights exist only when some member defines the level");
                        Ensemble {
                            components,
                            electronic: Some(mean),
                            gibbs: Some(minimum - kbt * partition.ln()),
                        }
                    }
                    None => Ensemble {
                        components,
                        electronic: None,
                        gibbs: None,
                    },
                }
            }
        };
        ensembles.push(ensemble);
    }
    ensembles
}

/// The group's components map: the group's first record's own, `None`
/// for the unnamed ensemble of records without components.
// rq-35671b9e
fn components_of(records: &[Record], indices: &[usize]) -> Option<Components> {
    records[indices[0]].components.clone()
}

/// The raw section's collapse (Boltzmann-Averaged Sections): groups by
/// components map in first-appearance order, the records without
/// components last as the unnamed ensemble; the members are the `ok`
/// records defining the derived Gibbs free energy and the electronic
/// inputs are the records' electronic energies.
///
/// (rq-18dba668)
pub fn collapse_ensembles(records: &[Record], memo: &ThermoMemo) -> Vec<Ensemble> {
    collapse(records, memo, PopLevel::G)
}

/// The corrected section's collapse (Boltzmann-Averaged Sections): the
/// members and electronic inputs resolve through `gcorr` and the
/// correction energy (`rqm/correction.md`), otherwise identical to
/// [`collapse_ensembles`].
///
/// (rq-f162476c)
pub fn corrected_collapse_ensembles(records: &[Record], memo: &ThermoMemo) -> Vec<Ensemble> {
    collapse(records, memo, PopLevel::GCorr)
}

/// The reference scan over ensembles (Boltzmann-Averaged Sections): each
/// monomer ensemble — components exactly one entry with count 1 —
/// supplies both reference values for its code; ensembles without values
/// contribute nothing. The collapse produces at most one ensemble per
/// components map, so the scan never overwrites a code.
///
/// (rq-27a3b339)
pub fn ensemble_references(ensembles: &[Ensemble]) -> MonomerReferences {
    let mut references = MonomerReferences::default();
    for ensemble in ensembles {
        if !is_monomer_ensemble(ensemble) {
            continue;
        }
        // The two values are Some together or not at all.
        let (Some(electronic), Some(gibbs)) = (ensemble.electronic, ensemble.gibbs) else {
            continue;
        };
        if let Some(code) = ensemble
            .components
            .as_ref()
            .and_then(|components| components.keys().next())
        {
            references.electronic.insert(code.clone(), electronic);
            references.gibbs.insert(code.to_string(), gibbs);
        }
    }
    references
}

/// The same scan read as the corrected references (Boltzmann-Averaged
/// Sections): the monomer ensemble's electronic mean is the
/// correction-energy reference and its Gibbs value the `gcorr` reference
/// (`rqm/correction.md`).
///
/// (rq-4b8a9f98)
pub fn corrected_ensemble_references(ensembles: &[Ensemble]) -> CorrectedMonomerReferences {
    let mut references = CorrectedMonomerReferences::default();
    for ensemble in ensembles {
        if !is_monomer_ensemble(ensemble) {
            continue;
        }
        // The two values are Some together or not at all.
        let (Some(electronic), Some(gibbs)) = (ensemble.electronic, ensemble.gibbs) else {
            continue;
        };
        if let Some(code) = ensemble
            .components
            .as_ref()
            .and_then(|components| components.keys().next())
        {
            references.electronic.insert(code.clone(), electronic);
            references.gibbs.insert(code.to_string(), gibbs);
        }
    }
    references
}

/// The two levels of one collapsed row (Boltzmann-Averaged Sections):
/// the ensemble's values minus the stoichiometric references, the Gibbs
/// level minus the concentration term of `concentration_correction` with
/// the same non-finite guard as the record-level values; both levels
/// undefined when the ensemble has no values or lacks components.
///
/// (rq-eafc5e8e)
pub fn ensemble_formation_values(
    ensemble: &Ensemble,
    references: &MonomerReferences,
    memo: &ThermoMemo,
    concs: &ConcConditions,
) -> Formation {
    let empty = Formation {
        electronic: None,
        gibbs: None,
    };
    let Some(components) = ensemble.components.as_ref() else {
        return empty;
    };
    // The two values are Some together or not at all.
    let (Some(ensemble_electronic), Some(ensemble_gibbs)) = (ensemble.electronic, ensemble.gibbs)
    else {
        return empty;
    };
    let electronic = components
        .iter()
        .try_fold(ensemble_electronic, |value, (code, count)| {
            Some(value - f64::from(*count) * references.electronic.get(code)?)
        });
    let gibbs = components
        .iter()
        .try_fold(ensemble_gibbs, |value, (code, count)| {
            Some(value - f64::from(*count) * references.gibbs.get(code)?)
        })
        .map(|value| value - concentration_correction(components, concs, memo.temperature()))
        .filter(|value| value.is_finite());
    Formation { electronic, gibbs }
}

/// The corrected row's two levels (Boltzmann-Averaged Sections): the same
/// rule as [`ensemble_formation_values`] against the corrected
/// references (`rqm/correction.md`).
///
/// (rq-f5583f17)
pub fn corrected_ensemble_formation_values(
    ensemble: &Ensemble,
    references: &CorrectedMonomerReferences,
    memo: &ThermoMemo,
    concs: &ConcConditions,
) -> Formation {
    let empty = Formation {
        electronic: None,
        gibbs: None,
    };
    let Some(components) = ensemble.components.as_ref() else {
        return empty;
    };
    // The two values are Some together or not at all.
    let (Some(ensemble_electronic), Some(ensemble_gibbs)) = (ensemble.electronic, ensemble.gibbs)
    else {
        return empty;
    };
    let electronic = components
        .iter()
        .try_fold(ensemble_electronic, |value, (code, count)| {
            Some(value - f64::from(*count) * references.electronic.get(code)?)
        });
    let gibbs = components
        .iter()
        .try_fold(ensemble_gibbs, |value, (code, count)| {
            Some(value - f64::from(*count) * references.gibbs.get(code)?)
        })
        .map(|value| value - concentration_correction(components, concs, memo.temperature()))
        .filter(|value| value.is_finite());
    Formation { electronic, gibbs }
}

/// The collapsed raw section (Boltzmann-Averaged Sections): header
/// `name el g`, one row per ensemble in the given order, `-` for undefined
/// values and for the unnamed ensemble's token.
///
/// (rq-30ea9ebc)
pub fn print_formation_ensembles(
    ensembles: &[Ensemble],
    references: &MonomerReferences,
    memo: &ThermoMemo,
    unit: Unit,
    concs: &ConcConditions,
) -> String {
    let rows: Vec<(String, Formation)> = ensembles
        .iter()
        .map(|ensemble| {
            let token =
                composition_token(ensemble.components.as_ref()).unwrap_or_else(|| "-".to_string());
            let formation = ensemble_formation_values(ensemble, references, memo, concs);
            (token, formation)
        })
        .collect();
    print_formation_table(&rows, unit, &["name", "el", "g"])
}

/// The collapsed corrected section (Boltzmann-Averaged Sections): the
/// collapsed raw section's table with the corrected levels — header
/// `name elcorr gcorr`.
///
/// (rq-4c1bcb87)
pub fn print_formation_corr_ensembles(
    ensembles: &[Ensemble],
    references: &CorrectedMonomerReferences,
    memo: &ThermoMemo,
    unit: Unit,
    concs: &ConcConditions,
) -> String {
    let rows: Vec<(String, Formation)> = ensembles
        .iter()
        .map(|ensemble| {
            let token =
                composition_token(ensemble.components.as_ref()).unwrap_or_else(|| "-".to_string());
            let formation = corrected_ensemble_formation_values(ensemble, references, memo, concs);
            (token, formation)
        })
        .collect();
    print_formation_table(&rows, unit, &["name", "elcorr", "gcorr"])
}
