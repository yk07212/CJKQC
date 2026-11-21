//! The manipulation pipeline behind the filtering command line (see
//! `rqm/filtering.md`): extract, cutoffs, uniqueness, reacted detection,
//! sort, select, glob, and shuffle, all pure functions over records, plus
//! the JKQC-style per-column printing.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::arbalign::{apply_arbalign, ArbAlignReport};
use crate::composition::{rename_component, RenameError, RenameSpec};
use crate::metrics::{
    dedup_names_mask, metric_dipole_magnitude, metric_radius_of_gyration, metric_total_mass,
    uniqueness_mask, UniqMode,
};
use crate::population::{populations, PopLevel};
use crate::reacted::{reacted_mask, ReactedMode};
use crate::record::{Components, Record, RecordStatus};
use crate::thermo::{
    rotational_constants, ThermoMemo, ANGSTROM3_PER_BOHR3, DEBYE_PER_E_A0, KCAL_MOL_PER_HARTREE,
};

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

/// A sort key: `el`, `g`, `elcorr`, `gcorr`, `rg`, `mass`, `filename`.
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
    /// The record's `name` — the file stem, the `filename` column.
    Filename,
}

impl SortKey {
    /// Parses a sort key name; `None` for unknown names.
    // rq-2371c178
    pub fn parse(name: &str) -> Option<SortKey> {
        match name {
            "el" => Some(SortKey::El),
            "g" => Some(SortKey::G),
            "elcorr" => Some(SortKey::ElCorr),
            "gcorr" => Some(SortKey::GCorr),
            "rg" => Some(SortKey::Rg),
            "mass" => Some(SortKey::Mass),
            "filename" => Some(SortKey::Filename),
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

/// Why a pipeline run stopped: an extract query that fails to parse (a
/// usage error) or a component-count overflow while renaming (a runtime
/// error).
///
/// (rq-2c3246dc)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterError {
    /// An extract query failed to parse.
    Extract(ExtractError),
    /// A `--rename` count merge would exceed `u32::MAX`; `record` is the
    /// name of the record that could not be renamed
    /// (`rqm/composition.md`).
    Rename {
        /// The offending record's `name`, when it has one.
        record: Option<String>,
        /// The rename failure.
        error: RenameError,
    },
}

impl fmt::Display for FilterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FilterError::Extract(error) => write!(f, "{error}"),
            FilterError::Rename { error, .. } => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for FilterError {}

impl From<ExtractError> for FilterError {
    fn from(error: ExtractError) -> Self {
        FilterError::Extract(error)
    }
}

/// The whole manipulation: the pipeline stages in `rqm/filtering.md`.
///
/// (rq-773db11b)
#[derive(Debug, Clone, Default)]
pub struct FilterSpec {
    /// `--extract`/`--nextract`: raw query strings and direction.
    pub extract: Option<(Vec<String>, ExtractMode)>,
    /// `--rename` component renames, chained in command order
    /// (`rqm/composition.md`).
    pub renames: Vec<RenameSpec>,
    /// Cutoffs, chained in command order.
    pub cutoffs: Vec<Cutoff>,
    /// `--uniq` mode.
    pub uniq: Option<UniqMode>,
    // rq-7c5578c0
    /// `--arbalign` RMSD threshold in Å (`rqm/arbalign.md`).
    pub arbalign: Option<f64>,
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
    // rq-7c5578c0
    /// The run's `--parallel` worker count; only arbalign reads it.
    pub parallel: usize,
}

/// The pipeline result: the surviving records and the arbalign stage's
/// diagnostics (`rqm/arbalign.md`).
///
/// (rq-87ba569d)
#[derive(Debug, Clone)]
pub struct FilterOutcome {
    /// Records that survived every pipeline stage.
    pub records: Vec<Record>,
    /// The arbalign stage's report; `skipped` is zero when arbalign ran
    /// not at all or found every geometry usable.
    pub arbalign: ArbAlignReport,
}

/// The print-boundary unit mode: `Kcal` when the `--unit` flag is
/// present, `Au` otherwise. `Kcal` prints energies in kcal/mol, dipoles
/// in Debye, and polarizabilities in Å³; `Au` prints energies in
/// hartree and dipoles and polarizabilities in atomic units. The variant
/// name `Kcal` is historical: it selects every practical print unit, not
/// only kcal/mol, and is retained for API stability.
///
/// (rq-8718aee5)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Unit {
    /// Atomic units (the default): hartree energies.
    #[default]
    Au,
    /// Practical units (`--unit`): kcal/mol energies, Debye dipoles,
    /// Å³ polarizabilities. The name is historical and retained for API
    /// stability.
    Kcal,
}

/// A printable column; the canonical print order is the order of
/// [`Column::ALL`], regardless of flag order on the command line.
///
/// (rq-38b619ff rq-4ea8ca35)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    /// The record's `name`: the stem of the source file name
    /// (`rqm/record.md`).
    ///
    /// (rq-38b619ff)
    Filename,
    /// Composition token: alphabetical pairs without separators
    /// (`2am3sa`).
    ///
    /// (rq-38b619ff)
    Name,
    /// The record's `source_path` (`rqm/record.md`), verbatim.
    ///
    /// (rq-4ea8ca35)
    Path,
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
    /// Boltzmann population from the Gibbs free energy
    /// (`rqm/population.md`), a dimensionless probability, 4 decimals.
    ///
    /// (rq-38b619ff)
    Pop,
    /// Boltzmann population from the electronic energy
    /// (`rqm/population.md`), like `pop`.
    ///
    /// (rq-38b619ff)
    PopEl,
    /// Boltzmann population from the attached correction energy
    /// (`rqm/population.md`), like `pop`.
    ///
    /// (rq-38b619ff)
    PopElCorr,
    /// Boltzmann population from the corrected Gibbs free energy
    /// (`rqm/population.md`), like `pop`.
    ///
    /// (rq-38b619ff)
    PopGCorr,
    /// Radius of gyration, 3 decimals.
    Rg,
    /// Total mass, 3 decimals.
    Mass,
    /// Dipole magnitude, 3 decimals.
    Dip,
    /// Dipole moment vector components, 3 decimals each, comma-separated.
    ///
    /// (rq-4ea8ca35)
    Dips,
    /// Isotropic polarizability, 3 decimals.
    ///
    /// (rq-1b7b3af3 rq-b6bcec13)
    Pol,
    /// Polarizability tensor entries, row-major, 3 decimals each,
    /// comma-separated.
    ///
    /// (rq-77dd2f68 rq-31d1c63f)
    Pols,
    /// Net charge.
    Charge,
    /// Spin multiplicity.
    Multiplicity,
    /// Rotational symmetry number.
    ///
    /// (rq-4ea8ca35)
    Rsn,
    /// Atom count.
    Natoms,
    /// Lowest stored vibrational frequency in cm⁻¹, raw values, 1 decimal
    /// (`rqm/filtering.md` Printing).
    ///
    /// (rq-38b619ff)
    Lf,
    /// All stored vibrational frequencies in cm⁻¹, raw values, 1 decimal
    /// each, comma-separated.
    ///
    /// (rq-4ea8ca35)
    Freqs,
    /// Frequency count.
    Nfreq,
    /// Negative frequency count.
    Nimag,
    /// Rotational constants in cm⁻¹, descending, 6 decimals each,
    /// comma-separated (`rqm/thermo.md`).
    ///
    /// (rq-4ea8ca35)
    RotS,
    /// Components as `2am+3sa`.
    Components,
}

impl Column {
    /// All columns in canonical print order.
    // rq-6fb14f23
    pub const ALL: [Column; 35] = [
        Column::Filename,
        Column::Name,
        Column::Path,
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
        Column::Pop,
        Column::PopEl,
        Column::PopElCorr,
        Column::PopGCorr,
        Column::Rg,
        Column::Mass,
        Column::Dip,
        Column::Dips,
        Column::Pol,
        Column::Pols,
        Column::Charge,
        Column::Multiplicity,
        Column::Rsn,
        Column::Natoms,
        Column::Lf,
        Column::Freqs,
        Column::Nfreq,
        Column::Nimag,
        Column::RotS,
        Column::Components,
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

    /// The population level behind a population column, `None` for every
    /// other column (`rqm/population.md` Bases and Flags): `pop` the
    /// Gibbs basis, `popel` the electronic basis, `popelcorr`/`popgcorr`
    /// the corrected bases.
    ///
    /// (rq-38b619ff)
    pub fn pop_level(&self) -> Option<PopLevel> {
        match self {
            Column::Pop => Some(PopLevel::G),
            Column::PopEl => Some(PopLevel::El),
            Column::PopElCorr => Some(PopLevel::ElCorr),
            Column::PopGCorr => Some(PopLevel::GCorr),
            _ => None,
        }
    }

    /// Header label (also the print flag's long name).
    pub fn label(&self) -> &'static str {
        match self {
            Column::Filename => "filename",
            Column::Name => "name",
            Column::Path => "path",
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
            Column::Pop => "pop",
            Column::PopEl => "popel",
            Column::PopElCorr => "popelcorr",
            Column::PopGCorr => "popgcorr",
            Column::Rg => "rg",
            Column::Mass => "mass",
            Column::Dip => "dip",
            Column::Dips => "dips",
            Column::Pol => "pol",
            Column::Pols => "pols",
            Column::Charge => "charge",
            Column::Multiplicity => "multiplicity",
            Column::Rsn => "rsn",
            Column::Natoms => "natoms",
            Column::Lf => "lf",
            Column::Freqs => "freqs",
            Column::Nfreq => "nfreq",
            Column::Nimag => "nimag",
            Column::RotS => "rots",
            Column::Components => "components",
        }
    }

    /// The column's alignment in the table: the text columns
    /// (`filename`, `name`, `path`, `status`, `program`, `command`,
    /// `correction`, `components`) align left, every numeric column
    /// aligns right (`rqm/filtering.md` Printing).
    ///
    /// (rq-4ea8ca35)
    pub(crate) fn alignment(&self) -> Alignment {
        match self {
            Column::Filename
            | Column::Name
            | Column::Path
            | Column::Status
            | Column::Program
            | Column::Command
            | Column::Correction
            | Column::Components => Alignment::Left,
            _ => Alignment::Right,
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
/// `--rename`, grouping, and the implicit `el` sort; never mutates the
/// input. Renames apply to the survivors in command order before every
/// composition-dependent stage (`rqm/composition.md`). Extract queries are
/// parsed here (composition grouping) and an unparsable query is reported
/// instead of silently matching nothing. The memo carries the run's
/// thermochemistry conditions (`rqm/thermo.md`) and derives each record at
/// most once across every Gibbs-valued stage: the `g` cutoff, the `g` sort
/// key, the `g` uniqueness metric, and `--glob` (`rqm/thermo.md`
/// Performance).
///
/// (rq-bc3e1482 rq-0137b5f9 rq-8b44ad8d rq-958da001 rq-2c3246dc rq-ae71d266)
pub fn apply_filters(
    records: &[Record],
    spec: &FilterSpec,
    memo: &ThermoMemo,
) -> Result<FilterOutcome, FilterError> {
    let mut current: Vec<Record> = records
        .iter()
        .filter(|record| record.status == RecordStatus::Ok)
        .cloned()
        .collect();

    // Renaming precedes every composition-dependent stage
    // (`rqm/composition.md`, Renaming Component Codes).
    for rename in &spec.renames {
        for record in &mut current {
            let outcome = record
                .components
                .as_mut()
                .map(|components| rename_component(components, &rename.old, &rename.new));
            if let Some(Err(error)) = outcome {
                return Err(FilterError::Rename {
                    record: record.name.clone(),
                    error,
                });
            }
        }
    }

    if let Some((queries, mode)) = &spec.extract {
        apply_extract(&mut current, queries, *mode, spec.grouping)?;
    }
    for cutoff in &spec.cutoffs {
        apply_cutoff(&mut current, cutoff, spec.grouping, memo);
    }

    let implicit_sort = spec.sort.is_none()
        && (matches!(spec.uniq, Some(UniqMode::Keys(_)))
            || spec.select.is_some()
            || spec.arbalign.is_some());
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
    let mut arbalign_report = ArbAlignReport::default();
    if let Some(threshold) = spec.arbalign {
        arbalign_report = apply_arbalign(&mut current, threshold, spec.parallel);
    }
    if spec.glob {
        apply_glob(&mut current, spec.grouping, memo);
    }
    if spec.shuffle {
        shuffle(&mut current, spec.seed);
    }
    Ok(FilterOutcome {
        records: current,
        arbalign: arbalign_report,
    })
}

/// A column's cell alignment in the aligned tables: text columns
/// (`filename`, `name`, `path`, `status`, `program`, `command`,
/// `correction`, `components`) align left, numeric columns align right
/// (`rqm/filtering.md` Printing).
///
/// (rq-4ea8ca35)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Alignment {
    /// Pad the cell on the right.
    Left,
    /// Pad the cell on the left.
    Right,
}

/// Join a header row and data rows into an aligned table: every column
/// as wide as its widest cell (header included; a cell's width is its
/// whole printed string, internal spaces included), single space between
/// columns, each cell padded per its column's alignment, header cells
/// aligning with their column, and no trailing whitespace
/// (`rqm/filtering.md` Printing). `rows[0]` is the header row.
///
/// (rq-4ea8ca35 rq-8dee4f4e)
pub(crate) fn align_table(rows: &[Vec<String>], aligns: &[Alignment]) -> String {
    let mut widths = vec![0usize; aligns.len()];
    for row in rows {
        debug_assert_eq!(
            row.len(),
            aligns.len(),
            "table row has {} cells for {} alignments",
            row.len(),
            aligns.len()
        );
        for (cell, width) in row.iter().zip(widths.iter_mut()) {
            *width = (*width).max(cell.chars().count());
        }
    }
    rows.iter()
        .map(|row| {
            let line = row
                .iter()
                .zip(&widths)
                .zip(aligns)
                .map(|((cell, width), align)| match align {
                    Alignment::Left => format!("{cell:<width$}"),
                    Alignment::Right => format!("{cell:>width$}"),
                })
                .collect::<Vec<_>>()
                .join(" ");
            line.trim_end().to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A text table: a header row of column names, then one row per record in
/// order, laid out `column -t` style (`rqm/filtering.md` Printing):
/// every column as wide as its widest cell with a single space between
/// columns, text columns left-aligned, numeric columns right-aligned,
/// header cells aligning with their column. Undefined values print `-`.
/// The energy
/// columns resolve through the derived-property registry on the run's
/// memo, which derives each record's thermochemistry once across all
/// thermo columns (`rqm/thermo.md` Performance); `unit` converts every
/// energy column (`el`, `zpe`, `h`, `s`, `g`, `elcorr`, `hcorr`, `gcorr`)
/// to kcal/mol, the dipole columns (`dip`, `dips`) to Debye, and the
/// polarizability columns (`pol`, `pols`) to Å³. `grouping` is the run's
/// grouping (`rqm/filtering.md`
/// Grouping): the population columns (`rqm/population.md`) pool per
/// composition group, or into one global pool under `--noname`.
///
/// (rq-8420b870 rq-4ea8ca35 rq-8b44ad8d)
pub fn print_table(
    records: &[Record],
    columns: &[Column],
    memo: &ThermoMemo,
    unit: Unit,
    grouping: Grouping,
) -> String {
    // Population columns are ensemble properties: each requested column's
    // populations are computed once over the printed record set, grouped
    // per the run's grouping (`rqm/population.md` Computation).
    // rq-540b4c18
    let population_columns: Vec<(Column, Vec<Option<f64>>)> = columns
        .iter()
        .filter_map(|column| {
            column
                .pop_level()
                .map(|level| (*column, populations(records, level, memo, grouping)))
        })
        .collect();
    let mut rows = Vec::with_capacity(records.len() + 1);
    rows.push(
        columns
            .iter()
            .map(|column| column.label().to_string())
            .collect(),
    );
    for (row, record) in records.iter().enumerate() {
        let mut cells = Vec::with_capacity(columns.len());
        for column in columns {
            match column.pop_level().and_then(|_| {
                population_columns
                    .iter()
                    .find(|(population, _)| population == column)
                    .map(|(_, values)| values[row])
            }) {
                // Dimensionless probabilities print with 4 decimals
                // (`rqm/population.md` Computation).
                // rq-b7d07857
                Some(Some(value)) => cells.push(format!("{value:.4}")),
                Some(None) => cells.push("-".to_string()),
                None => cells.push(format_cell(record, *column, memo, unit)),
            }
        }
        rows.push(cells);
    }
    let aligns: Vec<Alignment> = columns.iter().map(|column| column.alignment()).collect();
    align_table(&rows, &aligns)
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

/// One dipole cell: atomic units or Debye with 3 decimals under `--unit`
/// (`rqm/filtering.md` Printing; `rqm/thermo.md` Constants).
///
/// (rq-4ea8ca35 rq-6da80a21 rq-ee2f64a3)
fn format_dipole(value: f64, unit: Unit) -> String {
    match unit {
        Unit::Au => format!("{value:.3}"),
        Unit::Kcal => format!("{:.3}", value * DEBYE_PER_E_A0),
    }
}

/// One polarizability cell: atomic units or Å³ with 3 decimals under
/// `--unit` (`rqm/filtering.md` Printing; `rqm/thermo.md` Constants).
///
/// (rq-4ea8ca35 rq-6da80a21 rq-ac4624f4)
fn format_polarizability(value: f64, unit: Unit) -> String {
    match unit {
        Unit::Au => format!("{value:.3}"),
        Unit::Kcal => format!("{:.3}", value * ANGSTROM3_PER_BOHR3),
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
        // The filename cell prints the record's `name` (the file stem),
        // the name cell the composition token, the path cell the
        // `source_path` verbatim (`rqm/record.md`).
        // rq-c0c700b2 rq-9bea312b rq-1f63ac38 rq-7b0d8978 rq-5ad1c077 rq-3f2648cf
        Column::Filename => record.name.clone().unwrap_or_else(undefined),
        Column::Path => record.source_path.clone().unwrap_or_else(undefined),
        Column::Name => composition_token(record.components.as_ref()).unwrap_or_else(undefined),
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
        // rq-ee2f64a3 rq-79642963
        Column::Dip => match metric_dipole_magnitude(record) {
            Some(value) => format_dipole(value, unit),
            None => undefined(),
        },
        Column::Dips => match record.dipole_moment {
            Some([x, y, z]) => format!(
                "{},{},{}",
                format_dipole(x, unit),
                format_dipole(y, unit),
                format_dipole(z, unit)
            ),
            None => undefined(),
        },
        // The isotropic value is one third of the tensor trace; the
        // tensor cell lists all nine entries row-major.
        // rq-1b7b3af3 rq-77dd2f68 rq-ac4624f4 rq-79642963
        Column::Pol => match &record.polarizability {
            Some(tensor) => format_polarizability((tensor[0] + tensor[4] + tensor[8]) / 3.0, unit),
            None => undefined(),
        },
        Column::Pols => match &record.polarizability {
            Some(tensor) => tensor
                .iter()
                .map(|value| format_polarizability(*value, unit))
                .collect::<Vec<_>>()
                .join(","),
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
        Column::Rsn => match record.rotational_symmetry_number {
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
        Column::Freqs => match &record.vibrational_frequencies {
            Some(values) if !values.is_empty() => values
                .iter()
                .map(|value| format!("{value:.1}"))
                .collect::<Vec<_>>()
                .join(","),
            _ => undefined(),
        },
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
        Column::RotS => match rotational_constants(record) {
            Some(values) if !values.is_empty() => values
                .iter()
                .map(|value| format!("{value:.6}"))
                .collect::<Vec<_>>()
                .join(","),
            _ => undefined(),
        },
        Column::Components => match &record.components {
            Some(components) if !components.is_empty() => components
                .iter()
                .map(|(code, count)| format!("{count}{code}"))
                .collect::<Vec<_>>()
                .join("+"),
            _ => undefined(),
        },
        // Population cells never reach `format_cell`: print_table renders
        // them from the precomputed ensemble vectors above
        // (`rqm/population.md` Computation). Fail loudly if a future print
        // path bypasses the precompute instead of printing a silent dash.
        Column::Pop | Column::PopEl | Column::PopElCorr | Column::PopGCorr => {
            unreachable!("population column {} reached format_cell", column.label())
        }
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
/// descending; nameless records sort first ascending under `filename`.
/// Numeric keys are decorated (the sort key is resolved once per record
/// through the memo), sorted, and undecorated — one derivation per record
/// for the `g` key, not one per comparison (`rqm/thermo.md` Performance).
/// The sort is stable, so equal keys keep their relative order.
// rq-8b44ad8d
fn sort_records(records: &mut Vec<Record>, key: SortKey, reverse: bool, memo: &ThermoMemo) {
    if key == SortKey::Filename {
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
        SortKey::Filename => None,
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
/// record indices in current order. Shared with the population columns
/// (`rqm/population.md` Computation).
pub(crate) fn group_indices(records: &[Record], grouping: Grouping) -> Vec<Vec<usize>> {
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
