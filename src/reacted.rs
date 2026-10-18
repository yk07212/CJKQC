//! Reacted/exploded structure detection (see `rqm/detection.md`): within a
//! composition group, each structure's bonded-graph fragment fingerprint is
//! compared against the group's most frequent fingerprint. Deviants are
//! dropped (`--reacted`) or kept (`--reacted2`). The bond table is a
//! faithful mirror of the original JKQC procedure; no chemistry table is
//! consulted.

use std::collections::HashMap;

use crate::record::Record;

/// Which records a group's fragment consensus keeps: the majority pattern
/// (`--reacted`) or exactly the deviants (`--reacted2`).
///
/// (rq-ef146284)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReactedMode {
    /// Keep records whose signature equals the group consensus.
    Consensus,
    /// Keep only records whose signature deviates from the consensus.
    Deviants,
}

/// Are two atoms `dist` Å apart bonded? Fixed element-pair cutoffs mirroring
/// the original implementation; hydrogen bonds only to another hydrogen
/// below 0.80 Å; every other heavy pair bonds below 2.00 Å.
///
/// (rq-327095a8)
pub fn are_bonded(dist: f64, a: &str, b: &str) -> bool {
    if a == "H" || b == "H" {
        return a == "H" && b == "H" && dist < 0.80;
    }
    match (a, b) {
        ("C", "N") | ("N", "C") => dist < 1.75,
        ("N", "N") => dist < 1.50,
        ("S", "O") | ("O", "S") => dist < 1.90,
        ("O", "N") | ("N", "O") => dist < 1.90,
        _ => dist < 2.00,
    }
}

/// The record's fragment fingerprint: fragment formulas (element symbols
/// sorted and concatenated) of the bonded graph's connected components,
/// sorted alphabetically and joined with `+`. `None` when the record has no
/// geometry.
///
/// (rq-7e73da19)
pub fn fragment_signature(record: &Record) -> Option<String> {
    let geometry = record.geometry.as_ref()?;
    let atoms = geometry.symbols.len();
    if geometry.coordinates.len() != 3 * atoms {
        return None;
    }

    let mut parent: Vec<usize> = (0..atoms).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for i in 0..atoms {
        for j in i + 1..atoms {
            let dx = geometry.coordinates[3 * i] - geometry.coordinates[3 * j];
            let dy = geometry.coordinates[3 * i + 1] - geometry.coordinates[3 * j + 1];
            let dz = geometry.coordinates[3 * i + 2] - geometry.coordinates[3 * j + 2];
            let dist = (dx * dx + dy * dy + dz * dz).sqrt();
            if are_bonded(dist, &geometry.symbols[i], &geometry.symbols[j]) {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                if ri != rj {
                    parent[rj] = ri;
                }
            }
        }
    }

    let mut fragments: HashMap<usize, Vec<&str>> = HashMap::new();
    for (index, symbol) in geometry.symbols.iter().enumerate() {
        let root = find(&mut parent, index);
        fragments.entry(root).or_default().push(symbol.as_str());
    }
    let mut formulas: Vec<String> = fragments
        .values()
        .map(|members| {
            let mut sorted: Vec<&str> = members.clone();
            sorted.sort_unstable();
            sorted.concat()
        })
        .collect();
    formulas.sort_unstable();
    Some(formulas.join("+"))
}

/// Flags for one composition group: true for records kept under `mode`.
/// The consensus is the most frequent signature (a missing geometry counts
/// as the signature `(none)`); ties break by earliest occurrence.
pub(crate) fn reacted_mask(group: &[&Record], mode: ReactedMode) -> Vec<bool> {
    let signatures: Vec<String> = group
        .iter()
        .map(|record| fragment_signature(record).unwrap_or_else(|| "(none)".to_string()))
        .collect();

    let mut votes: HashMap<&str, (usize, usize)> = HashMap::new();
    for (index, signature) in signatures.iter().enumerate() {
        let entry = votes.entry(signature.as_str()).or_insert((0, index));
        entry.0 += 1;
    }
    let consensus = votes
        .into_iter()
        .max_by(|(_, (count_a, first_a)), (_, (count_b, first_b))| {
            count_a.cmp(count_b).then(first_b.cmp(first_a))
        })
        .map(|(signature, _)| signature)
        .unwrap_or("");

    signatures
        .iter()
        .map(|signature| match mode {
            ReactedMode::Consensus => signature == consensus,
            ReactedMode::Deviants => signature != consensus,
        })
        .collect()
}

/// Applies the consensus rule within each group and returns the kept
/// records in input order.
///
/// (rq-c6746a5e)
pub fn filter_reacted(groups: &[Vec<Record>], mode: ReactedMode) -> Vec<Record> {
    groups
        .iter()
        .flat_map(|group| {
            let refs: Vec<&Record> = group.iter().collect();
            reacted_mask(&refs, mode)
                .into_iter()
                .zip(group)
                .filter(|(keep, _)| *keep)
                .map(|(_, record)| record.clone())
                .collect::<Vec<Record>>()
        })
        .collect()
}
