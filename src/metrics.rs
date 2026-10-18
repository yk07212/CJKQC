//! Derived geometric metrics and property-based uniqueness (see
//! `rqm/uniqueness.md`). Total mass, mass-weighted radius of gyration, and
//! dipole magnitude are computed on demand from the stored observables and
//! never stored. Uniqueness drops records whose name repeats (`Dup`) or
//! whose rounded metric signature was already seen in the current order
//! (`Keys`).

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashSet};

use crate::record::Record;

/// Standard atomic weights in unified atomic mass units (u) for element
/// symbols H through Og (Z = 1–118), title-case keys; radioactive elements
/// use the mass number of their most stable isotope.
///
/// (rq-12d8fdf2)
const ATOMIC_MASSES: &[(&str, f64)] = &[
    ("H", 1.008),
    ("He", 4.002602),
    ("Li", 6.94),
    ("Be", 9.0121831),
    ("B", 10.81),
    ("C", 12.011),
    ("N", 14.007),
    ("O", 15.999),
    ("F", 18.998403162),
    ("Ne", 20.1797),
    ("Na", 22.98976928),
    ("Mg", 24.305),
    ("Al", 26.9815385),
    ("Si", 28.085),
    ("P", 30.973761998),
    ("S", 32.06),
    ("Cl", 35.45),
    ("Ar", 39.948),
    ("K", 39.0983),
    ("Ca", 40.078),
    ("Sc", 44.955908),
    ("Ti", 47.867),
    ("V", 50.9415),
    ("Cr", 51.9961),
    ("Mn", 54.938044),
    ("Fe", 55.845),
    ("Co", 58.933194),
    ("Ni", 58.6934),
    ("Cu", 63.546),
    ("Zn", 65.38),
    ("Ga", 69.723),
    ("Ge", 72.63),
    ("As", 74.921595),
    ("Se", 78.971),
    ("Br", 79.904),
    ("Kr", 83.798),
    ("Rb", 85.4678),
    ("Sr", 87.62),
    ("Y", 88.90584),
    ("Zr", 91.224),
    ("Nb", 92.90637),
    ("Mo", 95.95),
    ("Tc", 98.0),
    ("Ru", 101.07),
    ("Rh", 102.9055),
    ("Pd", 106.42),
    ("Ag", 107.8682),
    ("Cd", 112.414),
    ("In", 114.818),
    ("Sn", 118.71),
    ("Sb", 121.76),
    ("Te", 127.6),
    ("I", 126.90447),
    ("Xe", 131.293),
    ("Cs", 132.90545196),
    ("Ba", 137.327),
    ("La", 138.90547),
    ("Ce", 140.116),
    ("Pr", 140.90766),
    ("Nd", 144.242),
    ("Pm", 145.0),
    ("Sm", 150.36),
    ("Eu", 151.964),
    ("Gd", 157.25),
    ("Tb", 158.92535),
    ("Dy", 162.5),
    ("Ho", 164.93033),
    ("Er", 167.259),
    ("Tm", 168.93422),
    ("Yb", 173.054),
    ("Lu", 174.9668),
    ("Hf", 178.49),
    ("Ta", 180.94788),
    ("W", 183.84),
    ("Re", 186.207),
    ("Os", 190.23),
    ("Ir", 192.217),
    ("Pt", 195.084),
    ("Au", 196.96657),
    ("Hg", 200.59),
    ("Tl", 204.38),
    ("Pb", 207.2),
    ("Bi", 208.9804),
    ("Po", 209.0),
    ("At", 210.0),
    ("Rn", 222.0),
    ("Fr", 223.0),
    ("Ra", 226.0),
    ("Ac", 227.0),
    ("Th", 232.038),
    ("Pa", 231.036),
    ("U", 238.029),
    ("Np", 237.0),
    ("Pu", 244.0),
    ("Am", 243.0),
    ("Cm", 247.0),
    ("Bk", 247.0),
    ("Cf", 251.0),
    ("Es", 252.0),
    ("Fm", 257.0),
    ("Md", 258.0),
    ("No", 259.0),
    ("Lr", 266.0),
    ("Rf", 267.0),
    ("Db", 268.0),
    ("Sg", 269.0),
    ("Bh", 270.0),
    ("Hs", 269.0),
    ("Mt", 278.0),
    ("Ds", 281.0),
    ("Rg", 282.0),
    ("Cn", 285.0),
    ("Nh", 286.0),
    ("Fl", 289.0),
    ("Mc", 288.0),
    ("Lv", 293.0),
    ("Ts", 294.0),
    ("Og", 294.0),
];

/// Standard atomic weight in u; exact title-case match, `None` otherwise.
///
/// (rq-3e2c966b)
pub fn atomic_mass(symbol: &str) -> Option<f64> {
    ATOMIC_MASSES
        .iter()
        .find(|(name, _)| *name == symbol)
        .map(|(_, weight)| *weight)
}

/// `Σ m(symbol_i)` over the geometry's atoms, in u; `None` without a
/// geometry or with a symbol outside the mass table.
///
/// (rq-9f4db37a)
pub fn metric_total_mass(record: &Record) -> Option<f64> {
    let geometry = record.geometry.as_ref()?;
    let mut total = 0.0;
    for symbol in &geometry.symbols {
        total += atomic_mass(symbol)?;
    }
    Some(total)
}

/// Mass-weighted radius of gyration about the center of mass, in Å:
/// `sqrt(Σ m_i |r_i − R_cm|² / Σ m_i)`. `None` when any input is absent or
/// any symbol is outside the mass table.
///
/// (rq-b7959747)
pub fn metric_radius_of_gyration(record: &Record) -> Option<f64> {
    let geometry = record.geometry.as_ref()?;
    let atoms = geometry.symbols.len();
    if geometry.coordinates.len() != 3 * atoms {
        return None;
    }
    let mut masses = Vec::with_capacity(atoms);
    for symbol in &geometry.symbols {
        masses.push(atomic_mass(symbol)?);
    }
    let total: f64 = masses.iter().sum();
    let mut center = [0.0; 3];
    for (atom, &mass) in masses.iter().enumerate() {
        for (axis, value) in center.iter_mut().enumerate() {
            *value += mass * geometry.coordinates[3 * atom + axis];
        }
    }
    for value in &mut center {
        *value /= total;
    }
    let mut weighted = 0.0;
    for (atom, &mass) in masses.iter().enumerate() {
        for (axis, &coordinate) in geometry.coordinates[3 * atom..3 * atom + 3]
            .iter()
            .enumerate()
        {
            let delta = coordinate - center[axis];
            weighted += mass * delta * delta;
        }
    }
    Some((weighted / total).sqrt())
}

/// `sqrt(x² + y² + z²)` of the record's dipole moment vector, in atomic
/// units; `None` when `dipole_moment` is absent.
///
/// (rq-dab135ee)
pub fn metric_dipole_magnitude(record: &Record) -> Option<f64> {
    let [x, y, z] = record.dipole_moment?;
    Some((x * x + y * y + z * z).sqrt())
}

/// The metric a uniqueness key rounds: `el` the electronic energy, `rg` the
/// radius of gyration, `mass` the total mass, `dip` the dipole magnitude.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UniqMetric {
    /// `electronic_energy` (hartree).
    El,
    /// Radius of gyration (Å).
    Rg,
    /// Total mass (u).
    Mass,
    /// Dipole magnitude (atomic units).
    Dip,
}

impl UniqMetric {
    /// The default decimal count when the key string omits one: `el` 3,
    /// `rg` 2, `mass` 2, `dip` 1.
    fn default_decimals(self) -> f64 {
        match self {
            UniqMetric::El => 3.0,
            UniqMetric::Rg => 2.0,
            UniqMetric::Mass => 2.0,
            UniqMetric::Dip => 1.0,
        }
    }

    fn value(self, record: &Record) -> Option<f64> {
        match self {
            UniqMetric::El => record.electronic_energy,
            UniqMetric::Rg => metric_radius_of_gyration(record),
            UniqMetric::Mass => metric_total_mass(record),
            UniqMetric::Dip => metric_dipole_magnitude(record),
        }
    }
}

/// A uniqueness key: a metric with an explicit decimal count (at least 0);
/// the metric's default applies when the CLI key string omits one.
///
/// (rq-159da4d1)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UniqKey {
    /// The rounded metric.
    pub metric: UniqMetric,
    /// Decimal places for the rounding factor `10^decimals`.
    pub decimals: f64,
}

impl UniqKey {
    /// The rounded signature component of a record: `floor(v · 10^d)`
    /// toward −∞, NaN for an undefined metric (NaN-like components compare
    /// equal to each other).
    fn component(&self, record: &Record) -> OrderedF64 {
        let factor = 10f64.powf(self.decimals);
        match self.metric.value(record) {
            Some(value) => OrderedF64(norm_zero((value * factor).floor())),
            None => OrderedF64(f64::NAN),
        }
    }
}

/// Uniqueness mode: `Dup` drops records whose `name` repeats; `Keys` keeps
/// one record per rounded metric signature.
///
/// (rq-990dde7d)
#[derive(Debug, Clone, PartialEq)]
pub enum UniqMode {
    /// Drop duplicate-basename records.
    Dup,
    /// Drop records with an already-seen rounded signature.
    Keys(Vec<UniqKey>),
}

/// Parses a `--uniq` value: the literal `dup` yields `UniqMode::Dup`;
/// anything else is parsed as a key list.
///
/// (rq-07f5aebd)
pub fn parse_uniq_mode(value: &str) -> Result<UniqMode, String> {
    if value.trim() == "dup" {
        return Ok(UniqMode::Dup);
    }
    Ok(UniqMode::Keys(parse_uniq_keys(value)?))
}

/// Parses a comma list of `metric[decimals]` tokens (`el2.4,rg`); unknown
/// metrics, negative or malformed decimal counts, and empty lists yield the
/// `Err` text for CLI usage exit 2. Identical metric+decimals entries
/// collapse to one.
///
/// (rq-0f6147ac)
pub fn parse_uniq_keys(list: &str) -> Result<Vec<UniqKey>, String> {
    if list.trim().is_empty() {
        return Err("`--uniq` needs `dup` or a non-empty key list".to_string());
    }
    let mut keys: Vec<UniqKey> = Vec::new();
    for token in list.split(',') {
        let token = token.trim();
        if token.is_empty() {
            return Err(format!("`--uniq {list}`: empty key"));
        }
        let split = token
            .find(|c: char| c.is_ascii_digit() || c == '.' || c == '-')
            .unwrap_or(token.len());
        let (name, decimals) = token.split_at(split);
        let metric = match name {
            "el" => UniqMetric::El,
            "rg" => UniqMetric::Rg,
            "mass" => UniqMetric::Mass,
            "dip" => UniqMetric::Dip,
            other => return Err(format!("`--uniq {list}`: unknown metric `{other}`")),
        };
        let decimals = if decimals.is_empty() {
            metric.default_decimals()
        } else {
            let value: f64 = decimals
                .parse()
                .map_err(|_| format!("`--uniq {list}`: bad decimal count `{decimals}`"))?;
            if !value.is_finite() || value < 0.0 {
                return Err(format!("`--uniq {list}`: bad decimal count `{decimals}`"));
            }
            value
        };
        if !keys
            .iter()
            .any(|key| key.metric == metric && key.decimals == decimals)
        {
            keys.push(UniqKey { metric, decimals });
        }
    }
    Ok(keys)
}

/// Keeps the first record for each distinct present `name`; nameless
/// records are always kept; input order preserved.
///
/// (rq-a7757bca)
pub fn dedup_names(records: &[Record]) -> Vec<Record> {
    let refs: Vec<&Record> = records.iter().collect();
    records
        .iter()
        .zip(dedup_names_mask(&refs))
        .filter(|(_, keep)| *keep)
        .map(|(record, _)| record.clone())
        .collect()
}

/// First-wins name-duplication flags over the slice order.
pub(crate) fn dedup_names_mask(records: &[&Record]) -> Vec<bool> {
    let mut seen: HashSet<&str> = HashSet::new();
    records
        .iter()
        .map(|record| match &record.name {
            Some(name) => seen.insert(name.as_str()),
            None => true,
        })
        .collect()
}

/// Stable first-wins selection on rounded signature tuples over the whole
/// slice; signatures with undefined metrics are mutually equal in those
/// components (grouping is the caller's concern, `rqm/filtering.md`).
///
/// (rq-7650a23d)
pub fn unique_by_properties(records: &[Record], keys: &[UniqKey]) -> Vec<Record> {
    let refs: Vec<&Record> = records.iter().collect();
    records
        .iter()
        .zip(uniqueness_mask(&refs, keys))
        .filter(|(_, keep)| *keep)
        .map(|(record, _)| record.clone())
        .collect()
}

/// First-wins rounded-signature flags over the slice order.
pub(crate) fn uniqueness_mask(records: &[&Record], keys: &[UniqKey]) -> Vec<bool> {
    let mut seen: BTreeSet<Vec<OrderedF64>> = BTreeSet::new();
    records
        .iter()
        .map(|record| {
            let signature: Vec<OrderedF64> = keys.iter().map(|key| key.component(record)).collect();
            seen.insert(signature)
        })
        .collect()
}
/// −0.0 and 0.0 are the same rounded value.
fn norm_zero(value: f64) -> f64 {
    if value == 0.0 {
        0.0
    } else {
        value
    }
}

/// A float totally ordered by `total_cmp` so signatures containing NaN are
/// usable in a `BTreeSet`; NaN compares equal to NaN.
#[derive(Debug, Clone, Copy)]
struct OrderedF64(f64);

impl PartialEq for OrderedF64 {
    fn eq(&self, other: &Self) -> bool {
        self.0.total_cmp(&other.0) == Ordering::Equal
    }
}

impl Eq for OrderedF64 {}

impl PartialOrd for OrderedF64 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderedF64 {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}
