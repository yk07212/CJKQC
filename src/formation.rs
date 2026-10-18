//! Formation energies from monomer references (see `rqm/formation.md`):
//! a run's final records are scanned for monomers (single-component,
//! count-one `ok` records), each component code gets one reference record
//! — the first of its monomers under the run's sort ordering — and the
//! `--formation` flag prints each printed record's formation energy and
//! formation free energy as a section after the normal property table.
//! Nothing is stored: values derive at request time under the run's
//! thermochemistry conditions.

use std::collections::BTreeMap;

use crate::filter::{format_energy, sort_value_for, Unit};
use crate::record::{Record, RecordStatus};
use crate::thermo::ThermoMemo;

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

/// Whether a record is a monomer reference candidate: status `ok` and a
/// components map of exactly one entry with count 1 (`rqm/formation.md`).
// rq-1b571f91
fn is_monomer(record: &Record) -> bool {
    record.status == RecordStatus::Ok
        && record.components.as_ref().is_some_and(|components| {
            components.len() == 1 && components.values().all(|count| *count == 1)
        })
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
    for (code, monomers) in candidates {
        // The first monomer under the ascending sort ordering; the sort is
        // stable, so ties keep load order (rqm/formation.md).
        let mut reference = monomers[0];
        for &monomer in &monomers[1..] {
            if sorts_before(monomer, reference, sort, memo) {
                reference = monomer;
            }
        }
        if let Some(electronic) = reference.electronic_energy {
            references.electronic.insert(code.to_string(), electronic);
        }
        if let Some(thermo) = memo.thermo(reference) {
            references.gibbs.insert(code.to_string(), thermo.gibbs);
        }
    }
    references
}

/// Whether `left` sorts strictly before `right` under the ascending
/// ordering of `sort`; `--reverse` never applies to the reference
/// (rqm/formation.md). Nameless monomers order first under `name`.
// rq-9e898298
fn sorts_before(
    left: &Record,
    right: &Record,
    sort: crate::filter::SortKey,
    memo: &ThermoMemo,
) -> bool {
    match sort {
        crate::filter::SortKey::Name => match (left.name.as_deref(), right.name.as_deref()) {
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
/// (`rqm/thermo.md`). A level is undefined when the record lacks
/// components, lacks the level's input energy, or any referenced code lacks
/// that level's reference.
///
/// (rq-6ffbf9aa rq-c3425396 rq-8f6da1c2)
pub fn formation_values(
    record: &Record,
    references: &MonomerReferences,
    memo: &ThermoMemo,
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
        Some(value)
    });
    Formation { electronic, gibbs }
}

/// The formation section: a `name el g` header row, then one row per record
/// in the given order; values follow the run's energy unit (kcal/mol under
/// `--unit`, hartree otherwise) and print through the same energy formatter
/// as the table columns, `-` when undefined or the name absent.
///
/// (rq-9503bcb2 rq-8dee4f4e)
pub fn print_formation_section(
    records: &[Record],
    references: &MonomerReferences,
    memo: &ThermoMemo,
    unit: Unit,
) -> String {
    let mut lines = vec!["name el g".to_string()];
    for record in records {
        let formation = formation_values(record, references, memo);
        let name = record.name.clone().unwrap_or_else(|| "-".to_string());
        let electronic = formation
            .electronic
            .map_or_else(|| "-".to_string(), |value| format_energy(value, unit));
        let gibbs = formation
            .gibbs
            .map_or_else(|| "-".to_string(), |value| format_energy(value, unit));
        lines.push(format!("{name} {electronic} {gibbs}"));
    }
    lines.join("\n")
}
