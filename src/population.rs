//! Boltzmann population columns (`rqm/population.md`): one flag, `--pop`,
//! whose optional level argument picks the energy basis. Populations are
//! ensemble values computed at printing time over exactly the records the
//! run prints, grouped like the pipeline's composition groups, and never
//! stored (`rqm/ARCHITECTURE.md`).

use crate::filter::Grouping;
use crate::record::Record;
use crate::thermo::{ThermoMemo, E_HARTREE, K_B};

/// A population energy basis: `el`, `g`, `elcorr`, or `gcorr`; bare
/// `--pop` selects `G` (`rqm/population.md` Bases and Flags).
///
/// (rq-b954f3fb)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopLevel {
    /// Electronic energy.
    El,
    /// Gibbs free energy (the bare-flag default).
    G,
    /// Attached correction energy.
    ElCorr,
    /// Gibbs free energy from the attached correction.
    GCorr,
}

/// The level grammar: `el`, `g`, `elcorr`, `gcorr`, lowercase and exact;
/// `None` for anything else (the CLI turns `None` into a usage error,
/// `rqm/population.md` Bases and Flags).
///
/// (rq-cd221e62)
pub fn parse_pop_level(text: &str) -> Option<PopLevel> {
    match text {
        "el" => Some(PopLevel::El),
        "g" => Some(PopLevel::G),
        "elcorr" => Some(PopLevel::ElCorr),
        "gcorr" => Some(PopLevel::GCorr),
        _ => None,
    }
}

impl PopLevel {
    /// The levels in the population columns' canonical print order
    /// (`pop`, `popel`, `popelcorr`, `popgcorr` — `rqm/population.md`
    /// Bases and Flags).
    pub const CANONICAL: [PopLevel; 4] =
        [PopLevel::G, PopLevel::El, PopLevel::ElCorr, PopLevel::GCorr];

    /// The registry key the level resolves through: the single resolver
    /// all four levels share — the corrected levels run the identical
    /// path (Level Parity, `rqm/population.md`).
    fn registry_key(self) -> &'static str {
        match self {
            PopLevel::El => "el",
            PopLevel::G => "g",
            PopLevel::ElCorr => "elcorr",
            PopLevel::GCorr => "gcorr",
        }
    }
}

/// The level's basis value in Eh through the derived-property registry on
/// the run's memo (`rqm/thermo.md`): `El` the passthrough electronic
/// energy, `G` the derived Gibbs free energy, `ElCorr`/`GCorr` the
/// corrected values (`rqm/correction.md`). One resolver for all levels —
/// the corrected levels run the identical path (rqm/population.md Level
/// Parity). A non-finite registry value is undefined.
///
/// (rq-6c00aaee)
pub fn pop_level_value(record: &Record, level: PopLevel, memo: &ThermoMemo) -> Option<f64> {
    memo.value(level.registry_key(), record)
        .filter(|value| value.is_finite())
}

/// Per-record populations aligned index-wise with `records` over the
/// given (printed) set: groups per the run's grouping, Boltzmann weights
/// at the memo's temperature, `None` when the record's level value is
/// undefined or no member of its group defines the level
/// (`rqm/population.md` Computation).
///
/// p_i = exp(-(v_i - v_min)/(k_B T)) / sum_j exp(-(v_j - v_min)/(k_B T))
/// over the group's defined members, with v_min the group's minimum
/// defined value. The exponent is <= 0 by construction, so every factor
/// lies in [0, 1] and the minimum's own factor is exactly 1: the
/// partition sum is at least 1 and overflow and division by zero are
/// unreachable (`rqm/population.md` Numerics).
///
/// (rq-540b4c18 rq-12614633)
pub fn populations(
    records: &[Record],
    level: PopLevel,
    memo: &ThermoMemo,
    grouping: Grouping,
) -> Vec<Option<f64>> {
    let mut result: Vec<Option<f64>> = vec![None; records.len()];
    let values: Vec<Option<f64>> = records
        .iter()
        .map(|record| pop_level_value(record, level, memo))
        .collect();
    let kbt = K_B * memo.temperature() / E_HARTREE;
    for members in crate::filter::group_indices(records, grouping) {
        let defined: Vec<(usize, f64)> = members
            .iter()
            .filter_map(|&index| values[index].map(|value| (index, value)))
            .collect();
        let Some(minimum) = defined.iter().map(|&(_, value)| value).reduce(f64::min) else {
            continue;
        };
        let weights: Vec<(usize, f64)> = defined
            .iter()
            .map(|&(index, value)| (index, (-(value - minimum) / kbt).exp()))
            .collect();
        let sum: f64 = weights.iter().map(|&(_, weight)| weight).sum();
        for (index, weight) in weights {
            result[index] = Some(weight / sum);
        }
    }
    result
}
