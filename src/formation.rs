//! Formation energies from monomer references (see `rqm/formation.md`):
//! a run's final records are scanned for monomers (single-component,
//! count-one `ok` records), each component code gets one reference record
//! — the first of its monomers under the run's sort ordering — and the
//! `--formation` flag prints each printed record's formation energy,
//! formation enthalpy, formation entropy term, and formation free energy
//! as a section after the normal property table. The `--conc` and `--cnt`
//! flags shift the free-energy and entropy-term levels at vapors of named
//! components at given concentrations. Nothing is stored: values derive
//! at request time under the run's thermochemistry conditions.
//!
//! (rq-10341821)

use std::collections::BTreeMap;
use std::fmt;

use crate::filter::{
    align_table, format_energy, sort_value_for, Alignment, Grouping, SortKey, Unit,
};
use crate::ions::{acdc_composition_token, IonDeclarations, IonSpec, ProtonKind};
use crate::population::{boltzmann_weights, pop_level_value, PopLevel};
use crate::record::{Components, Record, RecordStatus};
use crate::thermo::{ThermoMemo, E_HARTREE, K_B, P_STANDARD};

/// The per-code monomer references in hartree, derived from the reference
/// records' observables. The enthalpy, entropy-term, and Gibbs maps carry
/// the same key set — the codes whose selected monomer defines
/// thermochemistry — so the three thermal levels are defined together.
///
/// (rq-a14c7897)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MonomerReferences {
    /// Electronic energy of each code's reference monomer.
    pub electronic: BTreeMap<String, f64>,
    /// Enthalpy of each code's reference monomer at the run's conditions.
    pub enthalpy: BTreeMap<String, f64>,
    /// T·S entropy term of each code's reference monomer at the run's
    /// conditions.
    pub entropy_term: BTreeMap<String, f64>,
    /// Gibbs free energy of each code's reference monomer at the run's
    /// conditions.
    pub gibbs: BTreeMap<String, f64>,
}

/// The per-code corrected references in hartree: the same selection as
/// [`collect_monomers`], read through the corrected keys
/// `elcorr`/`hcorr`/`gcorr` (`rqm/correction.md`) and the raw T·S
/// entropy term; a code appears only when its selected monomer defines
/// that level's corrected value (`rqm/correction.md`). The enthalpy,
/// entropy-term, and Gibbs maps carry the same key set, so the three
/// thermal levels are defined together.
///
/// (rq-67b44e63)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CorrectedMonomerReferences {
    /// Correction energy of each code's reference monomer.
    pub electronic: BTreeMap<String, f64>,
    /// Corrected enthalpy of each code's reference monomer at the run's
    /// conditions.
    pub enthalpy: BTreeMap<String, f64>,
    /// T·S entropy term of each code's reference monomer — the correction
    /// does not alter any thermal term — at the run's conditions.
    pub entropy_term: BTreeMap<String, f64>,
    /// Corrected Gibbs free energy of each code's reference monomer at
    /// the run's conditions.
    pub gibbs: BTreeMap<String, f64>,
}

/// A record's four derived formation levels, all in hartree: the
/// electronic level is independent; `enthalpy`, `entropy_term`, and
/// `gibbs` are `Some` together or `None` together, since all three need
/// defined thermochemistry and their references come from the same
/// selected monomer — except that a non-finite concentration term (an
/// absurd magnitude) leaves the enthalpy defined while the entropy
/// term and Gibbs go undefined (`rqm/formation.md` Formation at Given
/// Concentrations).
///
/// (rq-103447d2)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Formation {
    /// Formation energy (electronic level).
    pub electronic: Option<f64>,
    /// Formation enthalpy.
    pub enthalpy: Option<f64>,
    /// Formation entropy term (the T·S energy term, the same quantity the
    /// main table's `s` column prints).
    pub entropy_term: Option<f64>,
    /// Formation free energy (Gibbs level).
    pub gibbs: Option<f64>,
}

impl Formation {
    /// The all-undefined formation of a record that forms at no level.
    // rq-103447d2
    const NONE: Formation = Formation {
        electronic: None,
        enthalpy: None,
        entropy_term: None,
        gibbs: None,
    };
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

/// A polarity's reference ion has no `ok`, single-component monomer in the
/// run's final record set (`rqm/formation.md` Definedness and Validation).
///
/// (rq-b879564f)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IonReferenceError {
    /// The polarity whose reference ion is missing.
    pub kind: ProtonKind,
    /// The reference ion's component code.
    pub code: String,
}

impl fmt::Display for IonReferenceError {
    // rq-b879564f
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the reference ion `{}` has no ok monomer in the record set",
            self.code
        )
    }
}

impl std::error::Error for IonReferenceError {}

/// The run's formation presentation settings: the concentration
/// conditions, ion declarations, output unit, Boltzmann-collapse switch,
/// and reference-sort ordering (`rqm/formation.md` Feature API). `Default`
/// is the standard state, no ion declarations, hartree, no collapse, and
/// the `el` sort.
///
/// (rq-2c95cff0)
#[derive(Debug, Clone, PartialEq)]
pub struct FormationSettings {
    /// The run's concentration settings.
    pub concs: ConcConditions,
    /// The run's ion declarations.
    pub ions: IonDeclarations,
    /// The run's output energy unit.
    pub unit: Unit,
    /// Whether each formation section collapses to ensembles.
    pub bavg: bool,
    /// The sort ordering that selects the reference monomers.
    pub sort: SortKey,
}

impl Default for FormationSettings {
    // rq-2c95cff0
    fn default() -> Self {
        Self {
            concs: ConcConditions::default(),
            ions: IonDeclarations::default(),
            unit: Unit::Au,
            bavg: false,
            sort: SortKey::El,
        }
    }
}

/// One collapsed composition group (Boltzmann-Averaged Sections): the
/// ensemble of one composition's records, with `components` `None` for
/// the unnamed ensemble of the records without components. The four
/// values in Eh — the population-weighted electronic and enthalpy means,
/// the partition-function free energy, and the entropy term H_ens −
/// G_ens — are `Some` together or `None` together; an ensemble without
/// values is a group whose records all lack the section's Gibbs level.
///
/// (rq-35671b9e)
#[derive(Debug, Clone, PartialEq)]
pub struct Ensemble {
    /// The group's components map, `None` for the unnamed ensemble.
    pub components: Option<Components>,
    /// The population-weighted mean of the members' electronic-level
    /// inputs.
    pub electronic: Option<f64>,
    /// The population-weighted mean of the members' enthalpy-level
    /// values.
    pub enthalpy: Option<f64>,
    /// The entropy-term value H_ens − G_ens, so the collapse keeps
    /// g_ens = h_ens − st_ens.
    pub entropy_term: Option<f64>,
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

/// The reference-ion check of Ionic References: for each polarity that has
/// declared ions, the first declaration's code must have an `ok`,
/// single-component monomer in the given final records.
///
/// (rq-8449c2a1)
pub fn validate_ion_references(
    ions: &IonDeclarations,
    records: &[Record],
) -> Result<(), IonReferenceError> {
    for kind in [ProtonKind::Added, ProtonKind::Removed] {
        if let Some(reference) = ions.reference(kind) {
            let found = records.iter().any(|record| {
                record.status == RecordStatus::Ok
                    && record.components.as_ref().is_some_and(|components| {
                        components.len() == 1 && components.get(reference.code.as_str()) == Some(&1)
                    })
            });
            if !found {
                return Err(IonReferenceError {
                    kind,
                    code: reference.code.clone(),
                });
            }
        }
    }
    Ok(())
}

/// The concentration term `c_term` in Eh (`rqm/formation.md` Formation at
/// Given Concentrations): the sum over the specs whose code is in the
/// components map of nᵢ,eff · k_B T ln(cᵢ/p°) / E_h, with nᵢ,eff = nᵢ
/// or, under `self_consistent`, nᵢ·(N−1)/N for the record's total
/// molecule count N. A spec whose code is absent from the components
/// contributes nothing, so with no matching spec the term is 0. The
/// formation values subtract it from the Gibbs level and add it to the
/// entropy-term level.
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
/// sort-key values order after defined ones — supplies every reference
/// value; a level the chosen record lacks has no reference. The memo
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
        // The enthalpy, entropy-term, and Gibbs references are defined
        // together: one derivation of the selected monomer supplies all
        // three (rqm/formation.md Monomer References).
        if let Some(thermo) = memo.thermo(reference) {
            references
                .enthalpy
                .insert(code.to_string(), thermo.enthalpy);
            references
                .entropy_term
                .insert(code.to_string(), thermo.entropy_term);
            references.gibbs.insert(code.to_string(), thermo.gibbs);
        }
    }
    references
}

/// The neutral parents' reference sum for an ion token at one level: the
/// stoichiometric mix of the token's neutral codes, or `None` when one is
/// absent (`rqm/formation.md` Reference Ions and Values).
// rq-ae446bcb
fn neutral_reference_sum(neutral: &Components, map: &BTreeMap<String, f64>) -> Option<f64> {
    neutral.iter().try_fold(0.0, |sum, (code, count)| {
        Some(sum + f64::from(*count) * map.get(code)?)
    })
}

/// One level's effective-reference map for a polarity's declared ion codes
/// (`rqm/formation.md` Reference Ions and Values): with one spec the code
/// keeps its own monomer reference; with several, the first spec is the
/// zero point, μ is its own reference minus its neutral parents per
/// proton, and every other spec reads its neutral parents plus its proton
/// count times μ. Undefined entries are omitted, so the standing
/// missing-reference rule prints `-`.
// rq-ae446bcb rq-2ea9b81f
fn acdc_reference_map(map: &BTreeMap<String, f64>, specs: &[&IonSpec]) -> BTreeMap<String, f64> {
    if specs.len() <= 1 {
        return map.clone();
    }
    let mut out = map.clone();
    for spec in specs {
        out.remove(&spec.code);
    }
    let reference = specs[0];
    let mu = map.get(&reference.code).copied().and_then(|own| {
        neutral_reference_sum(&reference.token.neutral, map)
            .map(|parents| (own - parents) / f64::from(reference.token.protons))
    });
    for spec in specs {
        let effective = if spec.code == reference.code {
            // The zero point reads its own monomer even when a neutral
            // parent is absent; only the other codes lose their scale.
            map.get(&spec.code).copied()
        } else {
            mu.and_then(|mu| {
                neutral_reference_sum(&spec.token.neutral, map)
                    .map(|parents| parents + f64::from(spec.token.protons) * mu)
            })
        };
        if let Some(value) = effective {
            out.insert(spec.code.clone(), value);
        }
    }
    out
}

/// The effective-reference map of Reference Ions and Values: each declared
/// ion code's entry at each level becomes its neutral-parent mix plus the
/// polarity's proton chemical potential (or its own monomer reference when
/// it is its polarity's only ion code); undeclared codes are unchanged.
///
/// (rq-ae446bcb)
pub fn acdc_monomer_references(
    references: &MonomerReferences,
    ions: &IonDeclarations,
) -> MonomerReferences {
    let added = ions.of_kind(ProtonKind::Added);
    let removed = ions.of_kind(ProtonKind::Removed);
    let transform = |map: &BTreeMap<String, f64>| {
        let map = acdc_reference_map(map, &added);
        acdc_reference_map(&map, &removed)
    };
    MonomerReferences {
        electronic: transform(&references.electronic),
        enthalpy: transform(&references.enthalpy),
        entropy_term: transform(&references.entropy_term),
        gibbs: transform(&references.gibbs),
    }
}

/// The corrected section's effective-reference map: the same transformation
/// read through the corrected reference maps (`rqm/formation.md` Reference
/// Ions and Values).
///
/// (rq-aef2862b)
pub fn acdc_corrected_monomer_references(
    references: &CorrectedMonomerReferences,
    ions: &IonDeclarations,
) -> CorrectedMonomerReferences {
    let added = ions.of_kind(ProtonKind::Added);
    let removed = ions.of_kind(ProtonKind::Removed);
    let transform = |map: &BTreeMap<String, f64>| {
        let map = acdc_reference_map(map, &added);
        acdc_reference_map(&map, &removed)
    };
    CorrectedMonomerReferences {
        electronic: transform(&references.electronic),
        enthalpy: transform(&references.enthalpy),
        entropy_term: transform(&references.entropy_term),
        gibbs: transform(&references.gibbs),
    }
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

/// The thermal reference maps of the formation fold: the enthalpy,
/// entropy-term, and Gibbs maps, one per level.
type ThermalReferences<'a> = (
    &'a BTreeMap<String, f64>,
    &'a BTreeMap<String, f64>,
    &'a BTreeMap<String, f64>,
);

/// The shared thermal fold of the formation values, record-level and
/// ensemble alike: the level's own enthalpy, entropy term, and Gibbs
/// value — as the `(enthalpy, entropy_term, gibbs)` trio, `None` when
/// the level has no values at all — each minus the stoichiometric mix
/// of its matching reference map, with the concentration term
/// subtracted from the Gibbs value and added to the entropy-term value.
/// Each level folds against its own map, so with the scan's references
/// — whose thermal maps are defined together — the three levels are
/// defined together, while a hand-built reference set may define one
/// level alone. A non-finite concentration term (an absurd magnitude)
/// leaves the Gibbs and entropy levels undefined rather than infinite
/// (`rqm/formation.md` Formation Values, Formation at Given
/// Concentrations).
// rq-6ffbf9aa rq-fb892986 rq-eafc5e8e rq-f5583f17
fn thermal_formation(
    levels: Option<(f64, f64, f64)>,
    components: &Components,
    references: ThermalReferences<'_>,
    concs: &ConcConditions,
    temperature: f64,
) -> (Option<f64>, Option<f64>, Option<f64>) {
    let Some((enthalpy_value, entropy_term_value, gibbs_value)) = levels else {
        return (None, None, None);
    };
    let (enthalpy_reference, entropy_reference, gibbs_reference) = references;
    let fold = |mut value: f64, reference: &BTreeMap<String, f64>| {
        for (code, count) in components {
            value -= f64::from(*count) * reference.get(code)?;
        }
        Some(value)
    };
    let enthalpy = fold(enthalpy_value, enthalpy_reference);
    let correction = concentration_correction(components, concs, temperature);
    let entropy_term = fold(entropy_term_value, entropy_reference)
        .map(|value| value + correction)
        .filter(|value| value.is_finite());
    let gibbs = fold(gibbs_value, gibbs_reference)
        .map(|value| value - correction)
        .filter(|value| value.is_finite());
    (enthalpy, entropy_term, gibbs)
}

/// A record's four formation levels with their definedness: the
/// electronic energy minus the stoichiometric mix of electronic
/// references, and the enthalpy, entropy term, and Gibbs free energy —
/// from one whole-record thermochemistry derivation on the memo
/// (`rqm/thermo.md`) rather than three registry lookups, keeping the
/// three levels defined together — minus the stoichiometric mix of
/// their references. When the thermal levels are defined, the
/// concentration term of `concentration_correction` under the run's
/// `ConcConditions` is subtracted from the Gibbs value and added to the
/// entropy-term value, and a correction that turns non-finite (an absurd
/// input magnitude) leaves those two levels undefined rather than
/// infinite. The electronic level is undefined when the record lacks
/// components, lacks its input energy, or any referenced code lacks that
/// level's reference.
///
/// (rq-6ffbf9aa rq-c3425396 rq-8f6da1c2 rq-2333d9cc)
pub fn formation_values(
    record: &Record,
    references: &MonomerReferences,
    memo: &ThermoMemo,
    concs: &ConcConditions,
) -> Formation {
    let Some(components) = record.components.as_ref() else {
        return Formation::NONE;
    };
    let electronic = record.electronic_energy.and_then(|energy| {
        let mut value = energy;
        for (code, count) in components {
            value -= f64::from(*count) * references.electronic.get(code)?;
        }
        Some(value)
    });
    let thermal = thermal_formation(
        memo.thermo(record)
            .map(|thermo| (thermo.enthalpy, thermo.entropy_term, thermo.gibbs)),
        components,
        (
            &references.enthalpy,
            &references.entropy_term,
            &references.gibbs,
        ),
        concs,
        memo.temperature(),
    );
    let (enthalpy, entropy_term, gibbs) = thermal;
    Formation {
        electronic,
        enthalpy,
        entropy_term,
        gibbs,
    }
}

/// The corrected reference scan (`rqm/formation.md`): the same monomer
/// set and the same sort-based selection as the raw section; a code's
/// corrected electronic reference is that monomer's correction energy,
/// its corrected enthalpy reference the monomer's `hcorr`, its corrected
/// entropy-term reference the monomer's T·S — which the correction does
/// not alter — and its corrected Gibbs reference the monomer's `gcorr`
/// value. A reference is undefined when the selected monomer carries no
/// correction or its corrected thermochemistry is undefined; the
/// enthalpy, entropy-term, and Gibbs references are defined together.
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
        // The corrected enthalpy, entropy-term, and Gibbs references are
        // defined together: one corrected derivation of the selected
        // monomer supplies all three (rqm/formation.md).
        if let Some(thermo) = memo.corrected_thermo(reference) {
            references.enthalpy.insert(code.clone(), thermo.enthalpy);
            references
                .entropy_term
                .insert(code.to_string(), thermo.entropy_term);
            references.gibbs.insert(code, thermo.gibbs);
        }
    }
    references
}

/// A record's four corrected formation levels with their definedness:
/// the correction energy minus the stoichiometric mix of corrected
/// electronic references, and the corrected enthalpy, the T·S entropy
/// term — the correction does not alter any thermal term — and the
/// corrected Gibbs free energy minus the stoichiometric mixes of their
/// corrected references. A level is defined iff the record's own
/// corrected value at that level is defined and every referenced code has
/// a defined corrected reference at that level — corrected formation
/// numbers never mix levels (`rqm/formation.md`). A record without a
/// correction has no corrected value at any level. The corrected Gibbs
/// value, when defined, has the same concentration term as the raw
/// section subtracted and the corrected entropy-term value has it added,
/// with the same non-finite guard.
///
/// (rq-fb892986 rq-2333d9cc)
pub fn corrected_formation_values(
    record: &Record,
    references: &CorrectedMonomerReferences,
    memo: &ThermoMemo,
    concs: &ConcConditions,
) -> Formation {
    let Some(components) = record.components.as_ref() else {
        return Formation::NONE;
    };
    let electronic = crate::correction::corrected_energy(record).and_then(|energy| {
        let mut value = energy;
        for (code, count) in components {
            value -= f64::from(*count) * references.electronic.get(code)?;
        }
        Some(value)
    });
    let thermal = thermal_formation(
        memo.corrected_thermo(record)
            .map(|thermo| (thermo.enthalpy, thermo.entropy_term, thermo.gibbs)),
        components,
        (
            &references.enthalpy,
            &references.entropy_term,
            &references.gibbs,
        ),
        concs,
        memo.temperature(),
    );
    let (enthalpy, entropy_term, gibbs) = thermal;
    Formation {
        electronic,
        enthalpy,
        entropy_term,
        gibbs,
    }
}

/// The formation section: a `name el h st g` header row, then one row per
/// record in the given order. The `name` cell is the ACDC token when
/// `ions` declares the record's codes (ACDC Names), otherwise the record's
/// composition token (`rqm/filtering.md` Printing); `-` without
/// components; values
/// follow the run's energy unit (kcal/mol under `--unit`, hartree
/// otherwise) and print through the same energy formatter as the table
/// columns, `-` when undefined.
///
/// (rq-9503bcb2 rq-8dee4f4e)
pub fn print_formation_section(
    records: &[Record],
    references: &MonomerReferences,
    memo: &ThermoMemo,
    settings: &FormationSettings,
) -> String {
    let rows: Vec<(String, Formation)> = records
        .iter()
        .map(|record| {
            let token = acdc_composition_token(record.components.as_ref(), &settings.ions)
                .unwrap_or_else(|| "-".to_string());
            let formation = formation_values(record, references, memo, &settings.concs);
            (token, formation)
        })
        .collect();
    print_formation_table(&rows, settings.unit, &["name", "el", "h", "st", "g"])
}

/// The corrected formation section (`rqm/formation.md`): the raw
/// section's table with the corrected levels — header
/// `name elcorr hcorr stcorr gcorr`, the same `name` token rule,
/// formatting, and `-` rules.
///
/// (rq-4f20e170)
pub fn print_formation_corr_section(
    records: &[Record],
    references: &CorrectedMonomerReferences,
    memo: &ThermoMemo,
    settings: &FormationSettings,
) -> String {
    let rows: Vec<(String, Formation)> = records
        .iter()
        .map(|record| {
            let token = acdc_composition_token(record.components.as_ref(), &settings.ions)
                .unwrap_or_else(|| "-".to_string());
            let formation = corrected_formation_values(record, references, memo, &settings.concs);
            (token, formation)
        })
        .collect();
    print_formation_table(
        &rows,
        settings.unit,
        &["name", "elcorr", "hcorr", "stcorr", "gcorr"],
    )
}

/// The shared section table: a header row, then one `name el h st
/// g`-shaped row per `(token, formation)` pair in the given order —
/// shared with the collapsed sections (`rqm/formation.md`
/// Boltzmann-Averaged Sections). Aligned like the main table
/// (`rqm/filtering.md` Printing): `name` a text column (left), the
/// energy columns numeric (right), header cells aligning with their
/// columns.
// rq-9503bcb2
fn print_formation_table(rows: &[(String, Formation)], unit: Unit, header: &[&str]) -> String {
    debug_assert!(
        header.len() == 5,
        "the formation table body prints the name plus four level columns"
    );
    let header_cells: Vec<String> = header.iter().map(|cell| cell.to_string()).collect();
    let mut table = vec![header_cells];
    for (token, formation) in rows {
        let cell = |value: Option<f64>| {
            value.map_or_else(|| "-".to_string(), |value| format_energy(value, unit))
        };
        table.push(vec![
            token.clone(),
            cell(formation.electronic),
            cell(formation.enthalpy),
            cell(formation.entropy_term),
            cell(formation.gibbs),
        ]);
    }
    let aligns = [
        Alignment::Left,
        Alignment::Right,
        Alignment::Right,
        Alignment::Right,
        Alignment::Right,
    ];
    align_table(&table, &aligns)
}

/// Whether an ensemble is a code's monomer ensemble: its components map
/// is monomer-shaped (Boltzmann-Averaged Sections).
// rq-35671b9e
fn is_monomer_ensemble(ensemble: &Ensemble) -> bool {
    ensemble.components.as_ref().is_some_and(is_monomer_shaped)
}

/// The registry keys of the collapse's paired levels: the electronic key
/// of the electronic level — the record's `electronic_energy` for the raw
/// section, the correction energy (`rqm/correction.md`) for the corrected
/// one — and the enthalpy key of the enthalpy level; the Gibbs level
/// itself resolves through the shared population resolver
/// (`rqm/formation.md` Boltzmann-Averaged Sections).
// rq-dd65e289
fn collapse_keys(level: PopLevel) -> (&'static str, &'static str) {
    match level {
        PopLevel::G => ("el", "h"),
        PopLevel::GCorr => ("elcorr", "hcorr"),
        _ => unreachable!("the collapse collapses at a Gibbs level"),
    }
}

/// The members' population-weighted mean of `values`: a member without a
/// value — unreachable for the collapse's paired levels, since the
/// Gibbs level derives from both — leaves the mean undefined rather than
/// half-defined (Boltzmann-Averaged Sections).
// rq-18dba668
fn weighted_mean(weights: &[Option<f64>], values: &[Option<f64>]) -> Option<f64> {
    weights
        .iter()
        .zip(values)
        .try_fold(0.0_f64, |sum, (weight, value)| match (weight, value) {
            (Some(weight), Some(value)) => Some(sum + weight * value),
            (Some(_), None) => None,
            (None, _) => Some(sum),
        })
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
/// values are the members' population-weighted means of the paired
/// electronic and enthalpy levels — every member defines those levels
/// too, since they derive with the Gibbs level — the ensemble free
/// energy g_min − k_B·T·ln Z over the members' Gibbs values, computed
/// against the members' minimum as `rqm/population.md` Numerics
/// prescribes, and the entropy term H_ens − G_ens, at the memo's
/// temperature.
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
    let (electronic_key, enthalpy_key) = collapse_keys(level);
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
        let level_value = |key: &str, index: usize| {
            memo.value(key, &records[index])
                .filter(|value| value.is_finite())
        };
        let electronic_values: Vec<Option<f64>> = members
            .iter()
            .map(|&index| level_value(electronic_key, index))
            .collect();
        let enthalpy_values: Vec<Option<f64>> = members
            .iter()
            .map(|&index| level_value(enthalpy_key, index))
            .collect();
        let ensemble = match boltzmann_weights(&values, kbt) {
            // No member defines the level: the ensemble carries no values.
            None => Ensemble {
                components,
                electronic: None,
                enthalpy: None,
                entropy_term: None,
                gibbs: None,
            },
            Some((weights, partition)) => {
                // The population-weighted means of the members'
                // electronic and enthalpy inputs; a member without one —
                // unreachable, since both derive with the Gibbs level —
                // leaves the ensemble without values rather than
                // half-defined.
                let electronic = weighted_mean(&weights, &electronic_values);
                let enthalpy = weighted_mean(&weights, &enthalpy_values);
                match (electronic, enthalpy) {
                    (Some(mean_electronic), Some(mean_enthalpy)) => {
                        let minimum = values
                            .iter()
                            .filter_map(|value| *value)
                            .reduce(f64::min)
                            .expect("the weights exist only when some member defines the level");
                        // The entropy term H_ens − G_ens keeps
                        // g_ens = h_ens − st_ens, so the conformational
                        // stabilization appears in the entropy term
                        // rather than lowering the enthalpy.
                        let gibbs = minimum - kbt * partition.ln();
                        Ensemble {
                            components,
                            electronic: Some(mean_electronic),
                            enthalpy: Some(mean_enthalpy),
                            entropy_term: Some(mean_enthalpy - gibbs),
                            gibbs: Some(gibbs),
                        }
                    }
                    _ => Ensemble {
                        components,
                        electronic: None,
                        enthalpy: None,
                        entropy_term: None,
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
/// records defining the derived Gibbs free energy and the electronic and
/// enthalpy inputs are the records' electronic energies and enthalpies,
/// with the entropy-term value computed as H_ens − G_ens.
///
/// (rq-18dba668)
pub fn collapse_ensembles(records: &[Record], memo: &ThermoMemo) -> Vec<Ensemble> {
    collapse(records, memo, PopLevel::G)
}

/// The corrected section's collapse (Boltzmann-Averaged Sections): the
/// members and inputs resolve through `gcorr`, `hcorr`, and the
/// correction energy (`rqm/correction.md`), otherwise identical to
/// [`collapse_ensembles`].
///
/// (rq-f162476c)
pub fn corrected_collapse_ensembles(records: &[Record], memo: &ThermoMemo) -> Vec<Ensemble> {
    collapse(records, memo, PopLevel::GCorr)
}

/// The reference scan over ensembles (Boltzmann-Averaged Sections): each
/// monomer ensemble — components exactly one entry with count 1 —
/// supplies its four reference values for its code; ensembles without
/// values contribute nothing. The collapse produces at most one ensemble
/// per components map, so the scan never overwrites a code.
///
/// (rq-27a3b339)
pub fn ensemble_references(ensembles: &[Ensemble]) -> MonomerReferences {
    let mut references = MonomerReferences::default();
    for ensemble in ensembles {
        if !is_monomer_ensemble(ensemble) {
            continue;
        }
        // The four values are Some together or not at all.
        let (Some(electronic), Some(enthalpy), Some(entropy_term), Some(gibbs)) = (
            ensemble.electronic,
            ensemble.enthalpy,
            ensemble.entropy_term,
            ensemble.gibbs,
        ) else {
            continue;
        };
        if let Some(code) = ensemble
            .components
            .as_ref()
            .and_then(|components| components.keys().next())
        {
            references.electronic.insert(code.clone(), electronic);
            references.enthalpy.insert(code.clone(), enthalpy);
            references.entropy_term.insert(code.clone(), entropy_term);
            references.gibbs.insert(code.to_string(), gibbs);
        }
    }
    references
}

/// The same scan read as the corrected references (Boltzmann-Averaged
/// Sections): the monomer ensemble's electronic mean is the
/// correction-energy reference, its enthalpy mean the `hcorr` reference,
/// its entropy-term value the T·S reference, and its Gibbs value the
/// `gcorr` reference (`rqm/correction.md`).
///
/// (rq-4b8a9f98)
pub fn corrected_ensemble_references(ensembles: &[Ensemble]) -> CorrectedMonomerReferences {
    let mut references = CorrectedMonomerReferences::default();
    for ensemble in ensembles {
        if !is_monomer_ensemble(ensemble) {
            continue;
        }
        // The four values are Some together or not at all.
        let (Some(electronic), Some(enthalpy), Some(entropy_term), Some(gibbs)) = (
            ensemble.electronic,
            ensemble.enthalpy,
            ensemble.entropy_term,
            ensemble.gibbs,
        ) else {
            continue;
        };
        if let Some(code) = ensemble
            .components
            .as_ref()
            .and_then(|components| components.keys().next())
        {
            references.electronic.insert(code.clone(), electronic);
            references.enthalpy.insert(code.clone(), enthalpy);
            references.entropy_term.insert(code.clone(), entropy_term);
            references.gibbs.insert(code.to_string(), gibbs);
        }
    }
    references
}

/// The four levels of one collapsed row (Boltzmann-Averaged Sections):
/// the ensemble's values minus the stoichiometric references, with the
/// concentration term of `concentration_correction` subtracted from the
/// Gibbs value and added to the entropy-term value, using the same
/// non-finite guard as the record-level values; every level is undefined
/// when the ensemble has no values or lacks components.
///
/// (rq-eafc5e8e)
pub fn ensemble_formation_values(
    ensemble: &Ensemble,
    references: &MonomerReferences,
    memo: &ThermoMemo,
    concs: &ConcConditions,
) -> Formation {
    let empty = Formation::NONE;
    let Some(components) = ensemble.components.as_ref() else {
        return empty;
    };
    // The four values are Some together or not at all.
    let (
        Some(ensemble_electronic),
        Some(ensemble_enthalpy),
        Some(ensemble_entropy_term),
        Some(ensemble_gibbs),
    ) = (
        ensemble.electronic,
        ensemble.enthalpy,
        ensemble.entropy_term,
        ensemble.gibbs,
    )
    else {
        return empty;
    };
    let electronic = components
        .iter()
        .try_fold(ensemble_electronic, |value, (code, count)| {
            Some(value - f64::from(*count) * references.electronic.get(code)?)
        });
    let (enthalpy, entropy_term, gibbs) = thermal_formation(
        Some((ensemble_enthalpy, ensemble_entropy_term, ensemble_gibbs)),
        components,
        (
            &references.enthalpy,
            &references.entropy_term,
            &references.gibbs,
        ),
        concs,
        memo.temperature(),
    );
    Formation {
        electronic,
        enthalpy,
        entropy_term,
        gibbs,
    }
}

/// The corrected row's four levels (Boltzmann-Averaged Sections): the
/// same rule as [`ensemble_formation_values`] against the corrected
/// references (`rqm/correction.md`).
///
/// (rq-f5583f17)
pub fn corrected_ensemble_formation_values(
    ensemble: &Ensemble,
    references: &CorrectedMonomerReferences,
    memo: &ThermoMemo,
    concs: &ConcConditions,
) -> Formation {
    let empty = Formation::NONE;
    let Some(components) = ensemble.components.as_ref() else {
        return empty;
    };
    // The four values are Some together or not at all.
    let (
        Some(ensemble_electronic),
        Some(ensemble_enthalpy),
        Some(ensemble_entropy_term),
        Some(ensemble_gibbs),
    ) = (
        ensemble.electronic,
        ensemble.enthalpy,
        ensemble.entropy_term,
        ensemble.gibbs,
    )
    else {
        return empty;
    };
    let electronic = components
        .iter()
        .try_fold(ensemble_electronic, |value, (code, count)| {
            Some(value - f64::from(*count) * references.electronic.get(code)?)
        });
    let (enthalpy, entropy_term, gibbs) = thermal_formation(
        Some((ensemble_enthalpy, ensemble_entropy_term, ensemble_gibbs)),
        components,
        (
            &references.enthalpy,
            &references.entropy_term,
            &references.gibbs,
        ),
        concs,
        memo.temperature(),
    );
    Formation {
        electronic,
        enthalpy,
        entropy_term,
        gibbs,
    }
}

/// The collapsed raw section (Boltzmann-Averaged Sections): header
/// `name el h st g`, one row per ensemble in the given order, `-` for
/// undefined values and for the unnamed ensemble's token, the `name` cell
/// following the ACDC token rule for the ensemble's components.
///
/// (rq-30ea9ebc)
pub fn print_formation_ensembles(
    ensembles: &[Ensemble],
    references: &MonomerReferences,
    memo: &ThermoMemo,
    settings: &FormationSettings,
) -> String {
    let rows: Vec<(String, Formation)> = ensembles
        .iter()
        .map(|ensemble| {
            let token = acdc_composition_token(ensemble.components.as_ref(), &settings.ions)
                .unwrap_or_else(|| "-".to_string());
            let formation = ensemble_formation_values(ensemble, references, memo, &settings.concs);
            (token, formation)
        })
        .collect();
    print_formation_table(&rows, settings.unit, &["name", "el", "h", "st", "g"])
}

/// The collapsed corrected section (Boltzmann-Averaged Sections): the
/// collapsed raw section's table with the corrected levels — header
/// `name elcorr hcorr stcorr gcorr`, with the same `name` cell rule.
///
/// (rq-4c1bcb87)
pub fn print_formation_corr_ensembles(
    ensembles: &[Ensemble],
    references: &CorrectedMonomerReferences,
    memo: &ThermoMemo,
    settings: &FormationSettings,
) -> String {
    let rows: Vec<(String, Formation)> = ensembles
        .iter()
        .map(|ensemble| {
            let token = acdc_composition_token(ensemble.components.as_ref(), &settings.ions)
                .unwrap_or_else(|| "-".to_string());
            let formation =
                corrected_ensemble_formation_values(ensemble, references, memo, &settings.concs);
            (token, formation)
        })
        .collect();
    print_formation_table(
        &rows,
        settings.unit,
        &["name", "elcorr", "hcorr", "stcorr", "gcorr"],
    )
}
