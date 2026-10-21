//! The manipulation pipeline behind the filtering command line (see
//! `rqm/filtering.md`): extract, cutoffs, uniqueness, reacted detection,
//! sort, select, glob, and shuffle, all pure functions over records, plus
//! the JKQC-style per-column printing.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::metrics::{
    dedup_names_mask, metric_dipole_magnitude, metric_radius_of_gyration, metric_total_mass,
    uniqueness_mask, UniqMode,
};
use crate::reacted::{reacted_mask, ReactedMode};
use crate::record::{Components, Record, RecordStatus};
use crate::thermo::{ThermoMemo, KCAL_MOL_PER_HARTREE};

/// Whether `--extract` keeps matches (`Keep`) or `--nextract` drops them
/// (`Drop`).
///
/// (rq-b0d6f2f5)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractMode {
    /// Keep records matching the query.
    Keep,
    /// Keep records not matching the query.
    Drop,
}

/// How stages partition records: by resolved composition (unnamed records
/// form one separate group) or as a single global group (`--noname`).
///
/// (rq-2f996ba6)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Grouping {
    /// Records sharing the same `components` map form a group.
    #[default]
    Composition,
    /// One global group.
    Unit,
}

/// The comparison a cutoff applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    /// `<`
    Lt,
    /// `≤`
    Le,
    /// `>`
    Gt,
    /// `≥`
    Ge,
    /// `==`
    Eq,
    /// `≠`
    Ne,
}

impl CmpOp {
    fn holds(self, x: f64, threshold: f64) -> bool {
        match self {
            CmpOp::Lt => x < threshold,
            CmpOp::Le => x <= threshold,
            CmpOp::Gt => x > threshold,
            CmpOp::Ge => x >= threshold,
            CmpOp::Eq => x == threshold,
            CmpOp::Ne => x != threshold,
        }
    }
}

/// Absolute threshold or relative to the group minimum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Absolute.
    Absolute,
    /// Relative to the group minimum.
    Relative,
}

/// A record property a cutoff can compare: `el`, `g`, `elcorr`, `gcorr`,
/// `rg`, `lf`, `dip`, `mass`, `charge`, `multiplicity`, `natoms`,
/// `nfreq`, `nimag`.
///
/// (rq-d157ebb2)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Property {
    /// `electronic_energy` in hartree (kcal/mol in relative mode).
    El,
    /// Gibbs free energy under the run's thermochemistry conditions
    /// (`rqm/thermo.md`), hartree (kcal/mol in relative mode).
    G,
    /// Attached correction energy (`rqm/correction.md`), hartree (kcal/mol
    /// in relative mode); undefined without a correction.
    ElCorr,
    /// Gibbs free energy from the attached correction
    /// (`rqm/correction.md`), hartree (kcal/mol in relative mode);
    /// undefined without a correction or defined thermochemistry.
    GCorr,
    /// Mass-weighted radius of gyration in Å.
    Rg,
    /// Lowest vibrational frequency in cm⁻¹.
    Lf,
    /// Dipole magnitude in atomic units.
    Dip,
    /// Total mass in u.
    Mass,
    /// Net charge.
    Charge,
    /// Spin multiplicity.
    Multiplicity,
    /// Geometry atom count.
    Natoms,
    /// Frequency count.
    Nfreq,
    /// Count of negative frequencies.
    Nimag,
}

impl Property {
    /// Parses a cutoff property name; `None` for unknown names.
    pub fn parse(name: &str) -> Option<Property> {
        match name {
            "el" => Some(Property::El),
            "g" => Some(Property::G),
            "elcorr" => Some(Property::ElCorr),
            "gcorr" => Some(Property::GCorr),
            "rg" => Some(Property::Rg),
            "lf" => Some(Property::Lf),
            "dip" => Some(Property::Dip),
            "mass" => Some(Property::Mass),
            "charge" => Some(Property::Charge),
            "multiplicity" => Some(Property::Multiplicity),
            "natoms" => Some(Property::Natoms),
            "nfreq" => Some(Property::Nfreq),
            "nimag" => Some(Property::Nimag),
            _ => None,
        }
    }
}

/// One cutoff: `op property value`, absolute or relative.
///
/// (rq-8ad5e95a)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cutoff {
    /// Comparison.
    pub op: CmpOp,
    /// Absolute or relative mode.
    pub mode: Mode,
    /// Compared property.
    pub property: Property,
    /// Threshold value.
    pub value: f64,
}

/// A sort key: `el`, `g`, `elcorr`, `gcorr`, `rg`, `mass`, `name`.
///
/// (rq-953da4ce)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    /// `electronic_energy`.
    El,
    /// Gibbs free energy under the run's thermochemistry conditions
    /// (`rqm/thermo.md`).
    G,
    /// Attached correction energy (`rqm/correction.md`).
    ElCorr,
    /// Gibbs free energy from the attached correction
    /// (`rqm/correction.md`).
    GCorr,
    /// Radius of gyration.
    Rg,
    /// Total mass.
    Mass,
    /// Record name.
    Name,
}

impl SortKey {
    /// Parses a sort key name; `None` for unknown names.
    pub fn parse(name: &str) -> Option<SortKey> {
        match name {
            "el" => Some(SortKey::El),
            "g" => Some(SortKey::G),
            "elcorr" => Some(SortKey::ElCorr),
            "gcorr" => Some(SortKey::GCorr),
            "rg" => Some(SortKey::Rg),
            "mass" => Some(SortKey::Mass),
            "name" => Some(SortKey::Name),
            _ => None,
        }
    }
}

/// The extras rule of an extract pattern.
///
/// (rq-0186810c)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wildcard {
    /// No wildcard: no extras allowed.
    None,
    /// Bare `*`: any extras.
    Any,
    /// `N*`: extras sum to exactly `N`.
    Exactly(u32),
    /// `*code`: only the named codes as extras, any counts.
    Only(Vec<String>),
    /// `N*code`: only the named codes as extras, summing to exactly `N`.
    OnlyExactly(u32, Vec<String>),
}

/// A parsed extract pattern: pairs `(count, code)` (zero allowed, meaning
/// the code must be absent) plus a wildcard.
///
/// (rq-0186810c)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractPattern {
    /// Composition pairs; zero counts require the code to be absent.
    pub pairs: Vec<(u32, String)>,
    /// Extras rule.
    pub wildcard: Wildcard,
}

/// Why an extract query failed to parse (a usage error, exit 2).
///
/// (rq-881749dc)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractError(String);

impl fmt::Display for ExtractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ExtractError {}

/// The whole manipulation: the pipeline stages in `rqm/filtering.md`.
///
/// (rq-773db11b)
#[derive(Debug, Clone, Default)]
pub struct FilterSpec {
    /// `--extract`/`--nextract`: raw query strings and direction.
    pub extract: Option<(Vec<String>, ExtractMode)>,
    /// Cutoffs, chained in command order.
    pub cutoffs: Vec<Cutoff>,
    /// `--uniq` mode.
    pub uniq: Option<UniqMode>,
    /// `--reacted`/`--reacted2` mode.
    pub reacted: Option<ReactedMode>,
    /// `--sort` key.
    pub sort: Option<SortKey>,
    /// `--reverse`.
    pub reverse: bool,
    /// `--select` count.
    pub select: Option<usize>,
    /// `--glob`: keep each group's lowest-Gibbs record.
    pub glob: bool,
    /// `--shuffle`.
    pub shuffle: bool,
    /// `--seed`.
    pub seed: u64,
    /// `--noname` grouping.
    pub grouping: Grouping,
}

/// The print-boundary energy unit: `Kcal` when the `--unit` flag is
/// present, `Au` (hartree) otherwise.
///
/// (rq-8718aee5)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Unit {
    /// Hartree (the default).
    #[default]
    Au,
    /// kcal/mol (`--unit`).
    Kcal,
}

/// A printable column; the canonical print order is the order of
/// [`Column::ALL`], regardless of flag order on the command line.
///
/// (rq-38b619ff rq-4ea8ca35)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    /// Record name.
    Name,
    /// Status (`ok`, `failed`, `truncated`).
    Status,
    /// Program token and version (`xtb 6.7.1`).
    Program,
    /// Command line, double-quoted.
    Command,
    /// Correction provenance: kind, program and version, quoted commands
    /// (`rqm/correction.md`).
    ///
    /// (rq-38b619ff)
    Correction,
    /// Electronic energy, hartree, 10 decimals.
    El,
    /// Zero-point energy (`rqm/thermo.md`), like `el`.
    Zpe,
    /// Enthalpy (`rqm/thermo.md`), like `el`.
    H,
    /// T·S entropy term (`rqm/thermo.md`), like `el`.
    S,
    /// Gibbs free energy (`rqm/thermo.md`), like `el`.
    G,
    /// Attached correction energy (`rqm/correction.md`), like `el`.
    ///
    /// (rq-38b619ff)
    ElCorr,
    /// Enthalpy from the attached correction (`rqm/correction.md`), like
    /// `el`.
    ///
    /// (rq-38b619ff)
    HCorr,
    /// Gibbs free energy from the attached correction
    /// (`rqm/correction.md`), like `el`.
    ///
    /// (rq-38b619ff)
    GCorr,
    /// Radius of gyration, 3 decimals.
    Rg,
    /// Total mass, 3 decimals.
    Mass,
    /// Dipole magnitude, 3 decimals.
    Dip,
    /// Net charge.
    Charge,
    /// Spin multiplicity.
    Multiplicity,
    /// Atom count.
    Natoms,
    /// Lowest stored vibrational frequency in cm⁻¹, raw values, 1 decimal
    /// (`rqm/filtering.md` Printing).
    ///
    /// (rq-38b619ff)
    Lf,
    /// Frequency count.
    Nfreq,
    /// Negative frequency count.
    Nimag,
    /// Components as `2am+3sa`.
    Components,
    /// Composition token: alphabetical pairs without separators
    /// (`2am3sa`).
    ///
    /// (rq-38b619ff)
    Ct,
}

impl Column {
    /// All columns in canonical print order.
    pub const ALL: [Column; 24] = [
        Column::Name,
        Column::Status,
        Column::Program,
        Column::Command,
        Column::Correction,
        Column::El,
        Column::Zpe,
        Column::H,
        Column::S,
        Column::G,
        Column::ElCorr,
        Column::HCorr,
        Column::GCorr,
        Column::Rg,
        Column::Mass,
        Column::Dip,
        Column::Charge,
        Column::Multiplicity,
        Column::Natoms,
        Column::Lf,
        Column::Nfreq,
        Column::Nimag,
        Column::Components,
        Column::Ct,
    ];

    /// The derived-property registry key of an energy column
    /// (`rqm/thermo.md` Feature API), `None` for every other column.
    /// Energy cells all resolve through the registry — no column arm
    /// hand-wires a derivation.
    ///
    /// (rq-8f6da1c2)
    pub fn registry_key(&self) -> Option<&'static str> {
        match self {
            Column::El => Some("el"),
            Column::Zpe => Some("zpe"),
            Column::H => Some("h"),
            Column::S => Some("s"),
            Column::G => Some("g"),
            Column::ElCorr => Some("elcorr"),
            Column::HCorr => Some("hcorr"),
            Column::GCorr => Some("gcorr"),
            _ => None,
        }
    }

    /// Header label (also the print flag's long name).
    pub fn label(&self) -> &'static str {
        match self {
            Column::Name => "name",
            Column::Status => "status",
            Column::Program => "program",
            Column::Command => "command",
            Column::Correction => "correction",
            Column::El => "el",
            Column::Zpe => "zpe",
            Column::H => "h",
            Column::S => "s",
            Column::G => "g",
            Column::ElCorr => "elcorr",
            Column::HCorr => "hcorr",
            Column::GCorr => "gcorr",
            Column::Rg => "rg",
            Column::Mass => "mass",
            Column::Dip => "dip",
            Column::Charge => "charge",
            Column::Multiplicity => "multiplicity",
            Column::Natoms => "natoms",
            Column::Lf => "lf",
            Column::Nfreq => "nfreq",
            Column::Nimag => "nimag",
            Column::Components => "components",
            Column::Ct => "ct",
        }
    }
}

/// The record's defined value for a cutoff property; `None` marks it
/// undefined (absent observable, unknown element for mass metrics, or —
/// for `g` — undefined thermochemistry at the run's conditions,
/// `rqm/thermo.md`). The energy keys `el`, `g`, `elcorr`, and `gcorr`
/// resolve through the derived-property registry on the run's memo
/// (`rqm/correction.md` for the corrected keys).
///
/// (rq-b76b0761 rq-8f6da1c2)
pub fn property_value(record: &Record, property: Property, memo: &ThermoMemo) -> Option<f64> {
    match property {
        Property::El => memo.value("el", record),
        Property::G => memo.value("g", record),
        Property::ElCorr => memo.value("elcorr", record),
        Property::GCorr => memo.value("gcorr", record),
        Property::Rg => metric_radius_of_gyration(record),
        Property::Lf => record
            .vibrational_frequencies
            .as_ref()
            .and_then(|values| values.iter().min_by(|a, b| a.total_cmp(b)).copied()),
        Property::Dip => metric_dipole_magnitude(record),
        Property::Mass => metric_total_mass(record),
        Property::Charge => record.charge.map(|value| value as f64),
        Property::Multiplicity => record.multiplicity.map(|value| value as f64),
        Property::Natoms => record
            .geometry
            .as_ref()
            .map(|geometry| geometry.symbols.len() as f64),
        Property::Nfreq => record
            .vibrational_frequencies
            .as_ref()
            .map(|values| values.len() as f64),
        Property::Nimag => record
            .vibrational_frequencies
            .as_ref()
            .map(|values| values.iter().filter(|value| **value < 0.0).count() as f64),
    }
}

/// Parses a `--extract` query into patterns: comma terms, inclusive count
/// ranges, `(a,b)` choices, label comments after an unconsumed `-` or `_`,
/// zero counts, and the `*`/`N*`/`*code`/`N*code` wildcards.
///
/// (rq-2bc9762e rq-7b7e2114)
pub fn parse_extract_query(query: &str) -> Result<Vec<ExtractPattern>, ExtractError> {
    if query.trim().is_empty() {
        return Err(ExtractError("empty extract query".to_string()));
    }
    let mut patterns = Vec::new();
    for term in split_commas(query) {
        if term.trim().is_empty() {
            return Err(ExtractError("empty pattern in extract query".to_string()));
        }
        let tokens = tokenize(&term)?;
        for expanded in expand_ranges(tokens)? {
            for expanded in expand_choices(expanded)? {
                patterns.push(pattern_from_tokens(&expanded)?);
            }
        }
    }
    if patterns.is_empty() {
        return Err(ExtractError(
            "extract query expands to no patterns".to_string(),
        ));
    }
    Ok(patterns)
}

/// Whether a record's composition satisfies a pattern. Records without
/// `components` behave as if their composition were empty.
///
/// (rq-1e1f9c11)
pub fn extract_matches(pattern: &ExtractPattern, components: Option<&Components>) -> bool {
    let empty = Components::new();
    let map = components.unwrap_or(&empty);
    for (count, code) in &pattern.pairs {
        match map.get(code) {
            Some(record_count) if *count == 0 => return false,
            Some(record_count) if record_count != count => return false,
            Some(_) => {}
            None if *count != 0 => return false,
            None => {}
        }
    }
    let mentioned: HashSet<&str> = pattern
        .pairs
        .iter()
        .map(|(_, code)| code.as_str())
        .collect();
    let extras: Vec<&u32> = map
        .iter()
        .filter(|(code, _)| !mentioned.contains(code.as_str()))
        .map(|(_, count)| count)
        .collect();
    let extras_total: u64 = extras.iter().map(|count| **count as u64).sum();
    match &pattern.wildcard {
        Wildcard::None => extras.is_empty(),
        Wildcard::Any => true,
        Wildcard::Exactly(n) => extras_total == *n as u64,
        Wildcard::Only(codes) => map
            .iter()
            .filter(|(code, _)| !mentioned.contains(code.as_str()))
            .all(|(code, _)| codes.contains(code)),
        Wildcard::OnlyExactly(n, codes) => {
            extras_total == *n as u64
                && map
                    .iter()
                    .filter(|(code, _)| !mentioned.contains(code.as_str()))
                    .all(|(code, _)| codes.contains(code))
        }
    }
}

/// Runs the full manipulation pipeline, including the ok-only pre-filter,
/// grouping, and the implicit `el` sort; never mutates the input. Extract
/// queries are parsed here (composition grouping) and an unparsable query
/// is reported instead of silently matching nothing. The memo carries the
/// run's thermochemistry conditions (`rqm/thermo.md`) and derives each
/// record at most once across every Gibbs-valued stage: the `g` cutoff,
/// the `g` sort key, the `g` uniqueness metric, and `--glob`
/// (`rqm/thermo.md` Performance).
///
/// (rq-bc3e1482 rq-0137b5f9 rq-8b44ad8d)
pub fn apply_filters(
    records: &[Record],
    spec: &FilterSpec,
    memo: &ThermoMemo,
) -> Result<Vec<Record>, ExtractError> {
    let mut current: Vec<Record> = records
        .iter()
        .filter(|record| record.status == RecordStatus::Ok)
        .cloned()
        .collect();

    if let Some((queries, mode)) = &spec.extract {
        apply_extract(&mut current, queries, *mode, spec.grouping)?;
    }
    for cutoff in &spec.cutoffs {
        apply_cutoff(&mut current, cutoff, spec.grouping, memo);
    }

    let implicit_sort = spec.sort.is_none()
        && (matches!(spec.uniq, Some(UniqMode::Keys(_))) || spec.select.is_some());
    if implicit_sort {
        sort_records(&mut current, SortKey::El, spec.reverse, memo);
    }
    if let Some(uniq) = &spec.uniq {
        apply_uniq(&mut current, uniq, spec.grouping, memo);
    }
    if let Some(reacted) = &spec.reacted {
        apply_reacted(&mut current, *reacted, spec.grouping);
    }
    if let Some(key) = spec.sort {
        sort_records(&mut current, key, spec.reverse, memo);
    }
    if let Some(count) = spec.select {
        apply_select(&mut current, count, spec.grouping);
    }
    if spec.glob {
        apply_glob(&mut current, spec.grouping, memo);
    }
    if spec.shuffle {
        shuffle(&mut current, spec.seed);
    }
    Ok(current)
}

/// A text table: a header row of column names, then one row per record in
/// order, single-space separated; undefined values print `-`. The energy
/// columns resolve through the derived-property registry on the run's
/// memo, which derives each record's thermochemistry once across all
/// thermo columns (`rqm/thermo.md` Performance); `unit` converts every
/// energy column (`el`, `zpe`, `h`, `s`, `g`, `elcorr`, `hcorr`, `gcorr`)
/// to kcal/mol.
///
/// (rq-8420b870 rq-4ea8ca35 rq-8b44ad8d)
pub fn print_table(
    records: &[Record],
    columns: &[Column],
    memo: &ThermoMemo,
    unit: Unit,
) -> String {
    let mut lines = Vec::with_capacity(records.len() + 1);
    lines.push(
        columns
            .iter()
            .map(|column| column.label())
            .collect::<Vec<_>>()
            .join(" "),
    );
    for record in records {
        lines.push(
            columns
                .iter()
                .map(|column| format_cell(record, *column, memo, unit))
                .collect::<Vec<_>>()
                .join(" "),
        );
    }
    lines.join("\n")
}

/// One energy cell: hartree with 10 decimals, or kcal/mol with 4 under
/// `--unit` (rqm/filtering.md Printing). Shared with the formation
/// section so the table and the section cannot drift on precision.
pub(crate) fn format_energy(value: f64, unit: Unit) -> String {
    match unit {
        Unit::Au => format!("{value:.10}"),
        Unit::Kcal => format!("{:.4}", value * KCAL_MOL_PER_HARTREE),
    }
}

fn format_cell(record: &Record, column: Column, memo: &ThermoMemo, unit: Unit) -> String {
    let undefined = || "-".to_string();
    match column {
        // The energy columns share one registry dispatch (rq-8f6da1c2);
        // the corrected columns resolve the corrected keys through it
        // (rqm/correction.md).
        // rq-8f6da1c2 rq-b5e3b553
        Column::El
        | Column::Zpe
        | Column::H
        | Column::S
        | Column::G
        | Column::ElCorr
        | Column::HCorr
        | Column::GCorr => column
            .registry_key()
            .and_then(|key| memo.value(key, record))
            .map_or_else(undefined, |value| format_energy(value, unit)),
        Column::Name => record.name.clone().unwrap_or_else(undefined),
        Column::Status => match record.status {
            RecordStatus::Ok => "ok",
            RecordStatus::Failed => "failed",
            RecordStatus::Truncated => "truncated",
        }
        .to_string(),
        // The program cell joins token and version with one space; the
        // command cell quotes its value because the table separates
        // values with single spaces.
        // rq-f9ccc35c rq-6decd555 rq-777c8582 rq-804a53e2
        Column::Program => match &record.program {
            Some(program) => match &record.program_version {
                Some(version) => format!("{program} {version}"),
                None => program.clone(),
            },
            None => undefined(),
        },
        Column::Command => match &record.command {
            Some(command) => format!("\"{command}\""),
            None => undefined(),
        },
        // The correction provenance cell: the kind, then the program and
        // version the way the program column prints them (absent pieces
        // dropped), then each command double-quoted like the command
        // column (`rqm/correction.md` Printing).
        // rq-21414dd3 rq-be59d52a
        Column::Correction => match &record.correction {
            None => undefined(),
            Some(correction) => {
                let mut parts = vec![correction.kind.clone()];
                if let Some(program) = &correction.program {
                    match &correction.program_version {
                        Some(version) => parts.push(format!("{program} {version}")),
                        None => parts.push(program.clone()),
                    }
                }
                for command in correction.commands.iter().flatten() {
                    parts.push(format!("\"{command}\""));
                }
                parts.join(" ")
            }
        },
        Column::Rg => match metric_radius_of_gyration(record) {
            Some(value) => format!("{value:.3}"),
            None => undefined(),
        },
        Column::Mass => match metric_total_mass(record) {
            Some(value) => format!("{value:.3}"),
            None => undefined(),
        },
        Column::Dip => match metric_dipole_magnitude(record) {
            Some(value) => format!("{value:.3}"),
            None => undefined(),
        },
        Column::Charge => match record.charge {
            Some(value) => value.to_string(),
            None => undefined(),
        },
        Column::Multiplicity => match record.multiplicity {
            Some(value) => value.to_string(),
            None => undefined(),
        },
        Column::Natoms => match &record.geometry {
            Some(geometry) => geometry.symbols.len().to_string(),
            None => undefined(),
        },
        // The lowest raw frequency, shared with the `lf` cutoff property
        // (rq-4ea8ca35).
        Column::Lf => property_value(record, Property::Lf, memo)
            .map_or_else(undefined, |value| format!("{value:.1}")),
        Column::Nfreq => match &record.vibrational_frequencies {
            Some(values) => values.len().to_string(),
            None => undefined(),
        },
        Column::Nimag => match &record.vibrational_frequencies {
            Some(values) => values
                .iter()
                .filter(|value| **value < 0.0)
                .count()
                .to_string(),
            None => undefined(),
        },
        Column::Components => match &record.components {
            Some(components) if !components.is_empty() => components
                .iter()
                .map(|(code, count)| format!("{count}{code}"))
                .collect::<Vec<_>>()
                .join("+"),
            _ => undefined(),
        },
        Column::Ct => composition_token(record.components.as_ref()).unwrap_or_else(undefined),
    }
}

/// The composition token: the components map as alphabetical
/// `{count}{code}` pairs with the separators dropped (`2am3sa`,
/// `rqm/filtering.md` Printing); `None` without components or for an
/// empty map. Shared with the formation section (`rqm/formation.md`).
///
/// (rq-4ea8ca35 rq-8dee4f4e)
pub(crate) fn composition_token(components: Option<&Components>) -> Option<String> {
    let components = components?;
    if components.is_empty() {
        return None;
    }
    Some(
        components
            .iter()
            .map(|(code, count)| format!("{count}{code}"))
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// Stage implementations.
// ---------------------------------------------------------------------------

fn apply_extract(
    records: &mut Vec<Record>,
    queries: &[String],
    mode: ExtractMode,
    grouping: Grouping,
) -> Result<(), ExtractError> {
    let keep: Vec<bool> = match grouping {
        Grouping::Unit => {
            // Verbatim name comparison: no composition grammar to parse
            // (`rqm/filtering.md`), so this arm cannot fail.
            let names: Vec<&str> = queries
                .iter()
                .flat_map(|query| query.split(','))
                .map(str::trim)
                .filter(|term| !term.is_empty())
                .collect();
            records
                .iter()
                .map(|record| {
                    names
                        .iter()
                        .any(|name| record.name.as_deref() == Some(name))
                })
                .collect()
        }
        Grouping::Composition => {
            let mut patterns: Vec<ExtractPattern> = Vec::new();
            for query in queries {
                patterns.extend(parse_extract_query(query)?);
            }
            records
                .iter()
                .map(|record| {
                    patterns
                        .iter()
                        .any(|pattern| extract_matches(pattern, record.components.as_ref()))
                })
                .collect()
        }
    };
    let keep = match mode {
        ExtractMode::Keep => keep,
        ExtractMode::Drop => keep.iter().map(|flag| !flag).collect(),
    };
    retain(records, &keep);
    Ok(())
}

fn apply_cutoff(records: &mut Vec<Record>, cutoff: &Cutoff, grouping: Grouping, memo: &ThermoMemo) {
    let mut keep = vec![false; records.len()];
    for members in group_indices(records, grouping) {
        let values: Vec<Option<f64>> = members
            .iter()
            .map(|&index| property_value(&records[index], cutoff.property, memo))
            .collect();
        let minimum: Option<f64> = match cutoff.mode {
            Mode::Absolute => Some(0.0),
            Mode::Relative => values
                .iter()
                .flatten()
                .map(|value| scale_relative(*value, cutoff))
                .reduce(f64::min),
        };
        for (position, &index) in members.iter().enumerate() {
            keep[index] = match (values[position], minimum) {
                // Undefined values fail every comparison except `!=`.
                (None, _) | (Some(_), None) => cutoff.op == CmpOp::Ne,
                (Some(value), Some(minimum)) => cutoff
                    .op
                    .holds(scale_relative(value, cutoff) - minimum, cutoff.value),
            };
        }
    }
    retain(records, &keep);
}

/// Relative `el`, `g`, `elcorr`, and `gcorr` comparisons run in kcal/mol;
/// every other property keeps its native units.
fn scale_relative(value: f64, cutoff: &Cutoff) -> f64 {
    if cutoff.mode == Mode::Relative
        && matches!(
            cutoff.property,
            Property::El | Property::G | Property::ElCorr | Property::GCorr
        )
    {
        value * KCAL_MOL_PER_HARTREE
    } else {
        value
    }
}

fn apply_uniq(records: &mut Vec<Record>, mode: &UniqMode, grouping: Grouping, memo: &ThermoMemo) {
    let mut keep = vec![false; records.len()];
    for members in group_indices(records, grouping) {
        let group: Vec<&Record> = members.iter().map(|&index| &records[index]).collect();
        let mask = match mode {
            UniqMode::Dup => dedup_names_mask(&group),
            UniqMode::Keys(keys) => uniqueness_mask(&group, keys, memo),
        };
        for (position, &index) in members.iter().enumerate() {
            keep[index] = mask[position];
        }
    }
    retain(records, &keep);
}

fn apply_reacted(records: &mut Vec<Record>, mode: ReactedMode, grouping: Grouping) {
    let mut keep = vec![false; records.len()];
    for members in group_indices(records, grouping) {
        let group: Vec<&Record> = members.iter().map(|&index| &records[index]).collect();
        let mask = reacted_mask(&group, mode);
        for (position, &index) in members.iter().enumerate() {
            keep[index] = mask[position];
        }
    }
    retain(records, &keep);
}

fn apply_select(records: &mut Vec<Record>, count: usize, grouping: Grouping) {
    let mut keep = vec![false; records.len()];
    for members in group_indices(records, grouping) {
        for &index in members.iter().take(count) {
            keep[index] = true;
        }
    }
    retain(records, &keep);
}

/// `--glob`: per group keep the record with the lowest Gibbs free energy
/// at the run's conditions; ties keep the first in order, and records with
/// undefined Gibbs drop (`rqm/filtering.md`). The Gibbs values come from
/// the run's memo (`rqm/thermo.md` Performance).
// rq-17ea1903 rq-8b44ad8d
fn apply_glob(records: &mut Vec<Record>, grouping: Grouping, memo: &ThermoMemo) {
    let mut keep = vec![false; records.len()];
    for members in group_indices(records, grouping) {
        let mut lowest: Option<(usize, f64)> = None;
        for &index in &members {
            if let Some(gibbs) = memo.value("g", &records[index]) {
                if lowest.is_none_or(|(_, best)| gibbs < best) {
                    lowest = Some((index, gibbs));
                }
            }
        }
        if let Some((index, _)) = lowest {
            keep[index] = true;
        }
    }
    retain(records, &keep);
}

/// Ascending by default; undefined keys sort last ascending and first
/// descending; nameless records sort first ascending under `name`.
/// Numeric keys are decorated (the sort key is resolved once per record
/// through the memo), sorted, and undecorated — one derivation per record
/// for the `g` key, not one per comparison (`rqm/thermo.md` Performance).
/// The sort is stable, so equal keys keep their relative order.
// rq-8b44ad8d
fn sort_records(records: &mut Vec<Record>, key: SortKey, reverse: bool, memo: &ThermoMemo) {
    if key == SortKey::Name {
        records.sort_by(|a, b| {
            let ordering = match (a.name.as_deref(), b.name.as_deref()) {
                (Some(x), Some(y)) => x.cmp(y),
                (None, Some(_)) => Ordering::Less,
                (Some(_), None) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            };
            if reverse {
                ordering.reverse()
            } else {
                ordering
            }
        });
        return;
    }
    let mut decorated: Vec<(Option<f64>, Record)> = records
        .drain(..)
        .map(|record| (sort_value_for(&record, key, memo), record))
        .collect();
    decorated.sort_by(|(left, _), (right, _)| {
        let ordering = match (left, right) {
            (Some(x), Some(y)) => x.total_cmp(y),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        };
        if reverse {
            ordering.reverse()
        } else {
            ordering
        }
    });
    records.extend(decorated.into_iter().map(|(_, record)| record));
}

/// The numeric value of a record's sort key; `None` marks it undefined.
/// `el`, `g`, `elcorr`, and `gcorr` resolve through the derived-property
/// registry on the run's memo.
pub(crate) fn sort_value_for(record: &Record, key: SortKey, memo: &ThermoMemo) -> Option<f64> {
    match key {
        SortKey::El => property_value(record, Property::El, memo),
        SortKey::G => property_value(record, Property::G, memo),
        SortKey::ElCorr => property_value(record, Property::ElCorr, memo),
        SortKey::GCorr => property_value(record, Property::GCorr, memo),
        SortKey::Rg => property_value(record, Property::Rg, memo),
        SortKey::Mass => property_value(record, Property::Mass, memo),
        SortKey::Name => None,
    }
}

/// Fisher–Yates driven by splitmix64 seeded from `--seed`.
///
/// (rq-17ea1903)
fn shuffle(records: &mut [Record], seed: u64) {
    let mut state = seed;
    for i in (1..records.len()).rev() {
        let roll = splitmix64(&mut state);
        let j = (roll % (i as u64 + 1)) as usize;
        records.swap(i, j);
    }
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Group members in order of first appearance; each inner vector holds
/// record indices in current order.
fn group_indices(records: &[Record], grouping: Grouping) -> Vec<Vec<usize>> {
    match grouping {
        Grouping::Unit => vec![(0..records.len()).collect()],
        Grouping::Composition => {
            let mut groups: Vec<Vec<usize>> = Vec::new();
            let mut ids: HashMap<Option<&Components>, usize> = HashMap::new();
            for (index, record) in records.iter().enumerate() {
                let key = record.components.as_ref();
                let id = match ids.get(&key) {
                    Some(&id) => id,
                    None => {
                        let id = groups.len();
                        ids.insert(key, id);
                        groups.push(Vec::new());
                        id
                    }
                };
                groups[id].push(index);
            }
            groups
        }
    }
}

fn retain(records: &mut Vec<Record>, keep: &[bool]) {
    let mut flags = keep.iter().copied();
    records.retain(|_| flags.next().unwrap_or(false));
}

// ---------------------------------------------------------------------------
// Extract query parsing.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Count(u32),
    Code(String),
    Star,
    Dash,
    LParen,
    RParen,
    Comma,
}

/// Splits on commas outside parentheses (multi-choice commas stay inside).
fn split_commas(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    for ch in query.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => terms.push(std::mem::take(&mut current)),
            _ => current.push(ch),
        }
    }
    terms.push(current);
    terms
}

fn tokenize(term: &str) -> Result<Vec<Tok>, ExtractError> {
    let chars: Vec<char> = term.trim().chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if ch.is_ascii_whitespace() {
            return Err(ExtractError(format!(
                "`{term}`: whitespace inside a pattern"
            )));
        }
        if ch.is_ascii_digit() {
            let start = index;
            while index < chars.len() && chars[index].is_ascii_digit() {
                index += 1;
            }
            let digits: String = chars[start..index].iter().collect();
            let count: u32 = digits.parse().map_err(|_| {
                ExtractError(format!("`{term}`: count `{digits}` exceeds u32::MAX"))
            })?;
            tokens.push(Tok::Count(count));
            continue;
        }
        if ch.is_ascii_alphabetic() {
            let start = index;
            while index < chars.len() && chars[index].is_ascii_alphabetic() {
                index += 1;
            }
            tokens.push(Tok::Code(chars[start..index].iter().collect()));
            continue;
        }
        tokens.push(match ch {
            '*' => Tok::Star,
            '-' => Tok::Dash,
            '(' => Tok::LParen,
            ')' => Tok::RParen,
            ',' => Tok::Comma,
            other => {
                return Err(ExtractError(format!(
                    "`{term}`: unexpected character `{other}`"
                )))
            }
        });
        index += 1;
    }
    Ok(tokens)
}

/// Term count cap keeping pathological expansions (`1am1-4294967295w`) from
/// allocating unbounded patterns (`rqm/filtering.md`).
const MAX_EXTRACT_PATTERNS: usize = 10_000;
/// Most count values a single range may expand to (`1am1-1000w` is the
/// widest accepted range).
const MAX_EXTRACT_RANGE_VALUES: u32 = 1_000;

/// Expands `a-b` count ranges inclusively: only a dash between two counts,
/// capped in width and cumulative pattern count.
fn expand_ranges(tokens: Vec<Tok>) -> Result<Vec<Vec<Tok>>, ExtractError> {
    let dash = tokens.iter().position(|token| *token == Tok::Dash);
    if let Some(index) = dash {
        if index >= 1 {
            if let (Some(Tok::Count(low)), Some(Tok::Count(high))) =
                (tokens.get(index - 1), tokens.get(index + 1))
            {
                let (low, high) = (*low, *high);
                // A reversed range is an error, not an underflowing width.
                let span = high
                    .checked_sub(low)
                    .ok_or_else(|| ExtractError(format!("count range {low}-{high} is reversed")))?;
                if span >= MAX_EXTRACT_RANGE_VALUES {
                    return Err(ExtractError(format!(
                        "count range {low}-{high} expands beyond \
                         {MAX_EXTRACT_RANGE_VALUES} count values"
                    )));
                }
                let prefix = tokens[..index - 1].to_vec();
                let suffix = tokens[index + 2..].to_vec();
                let mut expanded = Vec::new();
                for value in low..=high {
                    let mut term = prefix.clone();
                    term.push(Tok::Count(value));
                    term.extend_from_slice(&suffix);
                    expanded.extend(expand_ranges(term)?);
                    if expanded.len() > MAX_EXTRACT_PATTERNS {
                        return Err(ExtractError(format!(
                            "query expands beyond {MAX_EXTRACT_PATTERNS} patterns"
                        )));
                    }
                }
                return Ok(expanded);
            }
        }
        return Ok(vec![strip_comment(tokens)]);
    }
    Ok(vec![tokens])
}

/// Everything from an unconsumed `-` on is a label comment.
fn strip_comment(tokens: Vec<Tok>) -> Vec<Tok> {
    match tokens.iter().position(|token| *token == Tok::Dash) {
        Some(index) => tokens[..index].to_vec(),
        None => tokens,
    }
}

/// Expands `(a,b)` choices into one term per count.
fn expand_choices(tokens: Vec<Tok>) -> Result<Vec<Vec<Tok>>, ExtractError> {
    let Some(open) = tokens.iter().position(|token| *token == Tok::LParen) else {
        return Ok(vec![tokens]);
    };
    let close = open
        + tokens[open..]
            .iter()
            .position(|token| *token == Tok::RParen)
            .ok_or_else(|| ExtractError("unbalanced `(` in extract query".to_string()))?;
    let prefix = tokens[..open].to_vec();
    let suffix = tokens[close + 1..].to_vec();
    let mut expanded = Vec::new();
    for piece in split_comma_tokens(&tokens[open + 1..close]) {
        let count = match piece.as_slice() {
            [Tok::Count(count)] => *count,
            _ => {
                return Err(ExtractError(
                    "choices must be comma-separated counts".to_string(),
                ))
            }
        };
        let mut term = prefix.clone();
        term.push(Tok::Count(count));
        term.extend_from_slice(&suffix);
        expanded.extend(expand_choices(term)?);
    }
    Ok(expanded)
}

fn split_comma_tokens(tokens: &[Tok]) -> Vec<Vec<Tok>> {
    let mut pieces = Vec::new();
    let mut current = Vec::new();
    for token in tokens {
        if *token == Tok::Comma {
            pieces.push(std::mem::take(&mut current));
        } else {
            current.push(token.clone());
        }
    }
    pieces.push(current);
    pieces
}

/// Turns a fully expanded token stream into pairs plus a wildcard.
fn pattern_from_tokens(tokens: &[Tok]) -> Result<ExtractPattern, ExtractError> {
    let mut pairs: Vec<(u32, String)> = Vec::new();
    let mut exactly: Option<u32> = None;
    let mut only: Vec<String> = Vec::new();
    let mut any = false;
    let mut index = 0;
    while index < tokens.len() {
        match &tokens[index] {
            Tok::Count(count) => {
                if matches!(tokens.get(index + 1), Some(Tok::Star)) {
                    exactly = Some(*count);
                    index += 2;
                    // `N*code`: the named codes restrict the extras.
                    if let Some(Tok::Code(code)) = tokens.get(index) {
                        if !only.contains(code) {
                            only.push(code.clone());
                        }
                        index += 1;
                    }
                } else if let Some(Tok::Code(new_code)) = tokens.get(index + 1) {
                    match pairs.iter().position(|(_, code)| code == new_code) {
                        Some(position) => pairs[position].0 += count,
                        None => pairs.push((*count, new_code.clone())),
                    }
                    index += 2;
                } else {
                    return Err(ExtractError("count without a component code".to_string()));
                }
            }
            Tok::Star => {
                if let Some(Tok::Code(code)) = tokens.get(index + 1) {
                    if !only.contains(code) {
                        only.push(code.clone());
                    }
                    index += 2;
                } else {
                    any = true;
                    index += 1;
                }
            }
            _ => {
                return Err(ExtractError(
                    "unexpected token in extract pattern".to_string(),
                ))
            }
        }
    }
    let wildcard = if any {
        Wildcard::Any
    } else {
        match (exactly, only.is_empty()) {
            (Some(count), false) => Wildcard::OnlyExactly(count, only),
            (Some(count), true) => Wildcard::Exactly(count),
            (None, false) => Wildcard::Only(only),
            (None, true) => Wildcard::None,
        }
    };
    pairs.sort_by(|left, right| left.1.cmp(&right.1));
    Ok(ExtractPattern { pairs, wildcard })
}
