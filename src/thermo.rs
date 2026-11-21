//! Thermochemistry on demand (see `rqm/thermo.md`): zero-point energy,
//! enthalpy, entropy, and Gibbs free energy are derived properties — pure
//! functions of the stored observables and the run's `ThermoConditions`,
//! computed at request time and never stored. The model reproduces ORCA 6's
//! quasi-RRHO thermochemistry: the ideal-gas rigid rotor with the Grimme
//! quasi-harmonic vibrational entropy (Chai–Head–Gordon damping, free-rotor
//! moment reduced by the molecule's average principal moment).

use std::cell::RefCell;
use std::collections::HashMap;

use crate::metrics::atomic_mass;
use crate::parser::extract::BOHR_TO_ANGSTROM;
use crate::record::Record;

// rq-361306ea — CODATA 2018, the single constant set behind every
// conversion in the crate.
const H_PLANCK: f64 = 6.62607015e-34; // J s
const C_LIGHT_CM_PER_S: f64 = 2.99792458e10; // cm/s
pub const K_B: f64 = 1.380649e-23; // J/K
const N_A: f64 = 6.02214076e23; // 1/mol
const R: f64 = N_A * K_B; // J mol^-1 K^-1
const U_ATOMIC_MASS: f64 = 1.66053906660e-27; // kg
pub const P_STANDARD: f64 = 101325.0; // Pa (1 atm)
pub const E_HARTREE: f64 = 4.3597447222071e-18; // J

/// Hartree to kcal/mol (`rqm/thermo.md` Constants).
///
/// (rq-361306ea)
pub const KCAL_MOL_PER_HARTREE: f64 = 627.5094740631;

/// Elementary-charge bohr to Debye: the print conversion for the dipole
/// columns and the inverse of Gaussian's Debye-to-atomic-unit extraction
/// (`rqm/thermo.md` Constants).
///
/// (rq-6da80a21)
pub const DEBYE_PER_E_A0: f64 = 2.541746473;

/// Cubic bohr to cubic ångström: the print conversion for the
/// polarizability columns, derived from the length constant so the two
/// cannot drift (`rqm/thermo.md` Constants).
///
/// (rq-6da80a21)
pub const ANGSTROM3_PER_BOHR3: f64 = BOHR_TO_ANGSTROM * BOHR_TO_ANGSTROM * BOHR_TO_ANGSTROM;

/// A frequency at or below this value (cm⁻¹) never participates in the
/// vibrational sums: programs print near-zero translational and rotational
/// contaminant modes there (ORCA's "Cut-Off Frequency 1.00 cm⁻¹"). This
/// participation cut is independent of the parser's `TRIVIAL_MODE_CUTOFF`
/// (`src/parser/extract.rs`): the record archives every mode above that
/// printed-zero recognizer, and the derivation re-filters soft modes below
/// 1.0 cm⁻¹ here.
const FREQ_PARTICIPATION_CUTOFF: f64 = 1.0;

/// The run's thermochemistry settings: temperature (`--temp`),
/// quasi-harmonic reference frequency (`--fc`), frequency scale
/// (`--fscale`), and imaginary-mode handling (`--makereal`,
/// `--ignoreimag`). The CLI rejects `makereal` and `ignoreimag`
/// together, so no reachable condition set has both true.
///
/// (rq-3315a51c rq-142ac35d)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThermoConditions {
    /// Temperature in K, > 0.
    pub temperature: f64,
    /// Reference frequency ν̃₀ in cm⁻¹, ≥ 0; 0 selects the pure harmonic
    /// oscillator.
    pub freq_cutoff: f64,
    /// Vibrational frequency scale factor, > 0.
    pub freq_scale: f64,
    /// Flip imaginary (negative) frequencies to their absolute values
    /// inside the derivation.
    pub makereal: bool,
    /// Skip imaginary (negative) frequencies: they contribute to no
    /// vibrational term.
    pub ignoreimag: bool,
}

impl Default for ThermoConditions {
    fn default() -> Self {
        ThermoConditions {
            temperature: 298.15,
            freq_cutoff: 100.0,
            freq_scale: 1.0,
            makereal: false,
            ignoreimag: false,
        }
    }
}

/// The rigid-rotor species class of a geometry.
///
/// (rq-fb4ef102)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeciesClass {
    /// One atom: no rotational or vibrational terms.
    Monatomic,
    /// I_a ≤ 10⁻⁴ · I_c: one rotational constant and rotational term.
    Linear,
    /// Three nonzero principal moments.
    Nonlinear,
}

/// One record's derivation at one condition set, all fields in hartree
/// (the T·S fields are temperature-multiplied energy terms).
///
/// (rq-162b0bbe)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thermo {
    /// Harmonic zero-point energy.
    pub zpe: f64,
    /// Thermal vibrational correction E_vib.
    pub thermal_vibrational: f64,
    /// Thermal rotational correction E_rot.
    pub thermal_rotational: f64,
    /// Thermal translational correction E_trans.
    pub thermal_translational: f64,
    /// Enthalpy H.
    pub enthalpy: f64,
    /// T·S_trans.
    pub ts_translational: f64,
    /// T·S_rot.
    pub ts_rotational: f64,
    /// T·S_vib.
    pub ts_vibrational: f64,
    /// T·S_el.
    pub ts_electronic: f64,
    /// T·S total.
    pub entropy_term: f64,
    /// Gibbs free energy G = H − T·S.
    pub gibbs: f64,
}

// ---------------------------------------------------------------------------
// Species geometry: principal moments and rotational constants.
// ---------------------------------------------------------------------------

/// The species class, principal moments of inertia (from the geometry with
/// the standard atomic weights of `rqm/uniqueness.md`), and total mass.
struct SpeciesMoments {
    class: SpeciesClass,
    /// Ascending principal moments in kg·m².
    moments: [f64; 3],
    /// Total mass in u.
    total_mass_u: f64,
}

// rq-e1e88e58
fn species_moments(record: &Record) -> Option<SpeciesMoments> {
    let geometry = record.geometry.as_ref()?;
    let atoms = geometry.symbols.len();
    if geometry.coordinates.len() != 3 * atoms {
        return None;
    }
    let mut masses = Vec::with_capacity(atoms);
    let mut total_u = 0.0;
    for symbol in &geometry.symbols {
        let mass_u = atomic_mass(symbol)?;
        masses.push(mass_u);
        total_u += mass_u;
    }
    if atoms == 1 {
        return Some(SpeciesMoments {
            class: SpeciesClass::Monatomic,
            moments: [0.0; 3],
            total_mass_u: total_u,
        });
    }
    // Center of mass, then the inertia tensor (Å and u converted to m and
    // kg so the moments come out in kg·m²).
    const ANGSTROM: f64 = 1e-10;
    let mut center = [0.0f64; 3];
    for (atom, &mass_u) in masses.iter().enumerate() {
        for (axis, slot) in center.iter_mut().enumerate() {
            *slot += mass_u * geometry.coordinates[3 * atom + axis];
        }
    }
    for slot in &mut center {
        *slot /= total_u;
    }
    let mut tensor = [[0.0f64; 3]; 3];
    for (atom, &mass_u) in masses.iter().enumerate() {
        let mass_kg = mass_u * U_ATOMIC_MASS;
        let position = [
            (geometry.coordinates[3 * atom] - center[0]) * ANGSTROM,
            (geometry.coordinates[3 * atom + 1] - center[1]) * ANGSTROM,
            (geometry.coordinates[3 * atom + 2] - center[2]) * ANGSTROM,
        ];
        let norm_squared =
            position[0] * position[0] + position[1] * position[1] + position[2] * position[2];
        // I_ij = Σ m (δ_ij |r|² − r_i r_j): the diagonal keeps the axis
        // components out (I_xx = Σ m(y²+z²)), the off-diagonal is −Σ m r_i r_j.
        for (row, row_slot) in tensor.iter_mut().enumerate() {
            for (column, slot) in row_slot.iter_mut().enumerate() {
                let delta = if row == column {
                    norm_squared - position[row] * position[row]
                } else {
                    -position[row] * position[column]
                };
                *slot += mass_kg * delta;
            }
        }
    }
    let mut moments = jacobi_eigenvalues(tensor);
    moments.sort_by(|left, right| left.total_cmp(right));
    // rq-ac683d6d — A degenerate geometry (every atom coinciding at the
    // center of mass) has vanishing moments about every axis; the middle
    // moment ≤ 0 means no rotational axis exists and every rotational
    // term would divide by zero, so the derivation is undefined.
    if moments[1] <= 0.0 {
        return None;
    }
    let class = if moments[0] <= 1e-4 * moments[2] {
        SpeciesClass::Linear
    } else {
        SpeciesClass::Nonlinear
    };
    Some(SpeciesMoments {
        class,
        moments,
        total_mass_u: total_u,
    })
}

/// Eigenvalues of a symmetric 3×3 matrix by Jacobi rotations: deterministic
/// in iteration order and exact for degenerate spectra.
fn jacobi_eigenvalues(mut a: [[f64; 3]; 3]) -> [f64; 3] {
    for _ in 0..100 {
        let mut largest = 0.0f64;
        let mut pivot = (0usize, 1usize);
        for (row, row_slot) in a.iter().enumerate() {
            for (column, value) in row_slot.iter().enumerate().skip(row + 1) {
                if value.abs() > largest {
                    largest = value.abs();
                    pivot = (row, column);
                }
            }
        }
        let scale: f64 = a
            .iter()
            .enumerate()
            .map(|(index, row)| row[index].abs())
            .sum();
        if largest == 0.0 || largest <= 1e-18 * scale {
            break;
        }
        let (p, q) = pivot;
        let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
        let tangent = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
        let cosine = 1.0 / (tangent * tangent + 1.0).sqrt();
        let sine = tangent * cosine;
        let pivot_value = a[p][q];
        a[p][p] -= tangent * pivot_value;
        a[q][q] += tangent * pivot_value;
        a[p][q] = 0.0;
        a[q][p] = 0.0;
        // The one remaining index of a 3×3 rotation plane.
        let third = 3 - p - q;
        let (a_ip, a_iq) = (a[third][p], a[third][q]);
        a[third][p] = cosine * a_ip - sine * a_iq;
        a[p][third] = a[third][p];
        a[third][q] = sine * a_ip + cosine * a_iq;
        a[q][third] = a[third][q];
    }
    [a[0][0], a[1][1], a[2][2]]
}

/// The species class of a record's geometry: `Monatomic` for one atom,
/// `Linear` when the smallest principal moment is at most 10⁻⁴ of the
/// largest, `Nonlinear` otherwise; `None` without a geometry, for a
/// malformed or degenerate (all-atoms-coincident) geometry, or with an
/// element symbol outside the mass table.
///
/// (rq-e1e88e58 rq-ac683d6d)
pub fn classify_species(record: &Record) -> Option<SpeciesClass> {
    species_moments(record).map(|species| species.class)
}

/// B̃ (cm⁻¹) from a principal moment in kg·m²: h / (8π² c I).
fn rotational_constant(moment: f64) -> f64 {
    H_PLANCK / (8.0 * std::f64::consts::PI * std::f64::consts::PI * C_LIGHT_CM_PER_S * moment)
}

/// The rotational constants B̃ (cm⁻¹) of the geometry's nonzero principal
/// moments, listed descending (B̃ decreases as the moment I increases):
/// three for a `Nonlinear` species, one for a `Linear` species, none for
/// a `Monatomic` record. `None` without a geometry, with an unknown
/// element symbol, or for a degenerate geometry.
///
/// (rq-3e2a7cc5 rq-41528ecc)
pub fn rotational_constants(record: &Record) -> Option<Vec<f64>> {
    let species = species_moments(record)?;
    match species.class {
        SpeciesClass::Monatomic => Some(Vec::new()),
        SpeciesClass::Linear => Some(vec![rotational_constant(species.moments[1])]),
        SpeciesClass::Nonlinear => Some(vec![
            rotational_constant(species.moments[0]),
            rotational_constant(species.moments[1]),
            rotational_constant(species.moments[2]),
        ]),
    }
}

// ---------------------------------------------------------------------------
// The model (rqm/thermo.md Model and Terms).
// ---------------------------------------------------------------------------

/// The full thermochemistry of one record at one condition set: `None`
/// exactly when the derivation is undefined — no electronic energy, no
/// geometry, a degenerate geometry, an unknown element symbol, an
/// imaginary frequency under neither `makereal` nor `ignoreimag`, a
/// non-monatomic record without participating modes, or a non-finite
/// derived term (possible only for conditions beyond the `rqm/thermo.md`
/// Numerics range).
/// Pure: no caching, no mutation, deterministic in the inputs.
///
/// (rq-bdd59f53 rq-5b33312a rq-ac683d6d)
pub fn thermochemistry(record: &Record, conditions: &ThermoConditions) -> Option<Thermo> {
    thermochemistry_with_energy(record, record.electronic_energy?, conditions)
}

/// `thermochemistry` with an explicit electronic energy: the same
/// derivation with the given energy in place of the stored one — the form
/// the corrected keys `hcorr`/`gcorr` reuse with an attached correction's
/// energy (`rqm/correction.md`). Every other input (geometry, modes,
/// multiplicity, symmetry number) and every definedness rule is the
/// record's own.
// rq-bdd59f53
pub(crate) fn thermochemistry_with_energy(
    record: &Record,
    electronic_energy: f64,
    conditions: &ThermoConditions,
) -> Option<Thermo> {
    let species = species_moments(record)?;
    let temperature = conditions.temperature;

    // Mode selection: an imaginary frequency with neither imaginary-mode
    // flag leaves the whole derivation undefined; `--makereal` flips it to
    // its absolute value before the participation test and `--ignoreimag`
    // drops it outright, so a negative value reaches the flip only under
    // `makereal` (rqm/thermo.md).
    debug_assert!(
        !(conditions.makereal && conditions.ignoreimag),
        "the CLI rejects --makereal together with --ignoreimag"
    );
    let raw = record.vibrational_frequencies.as_deref().unwrap_or(&[]);
    if raw.iter().any(|value| *value < 0.0) && !conditions.makereal && !conditions.ignoreimag {
        return None;
    }
    let participating: Vec<f64> = raw
        .iter()
        .filter(|value| **value >= 0.0 || conditions.makereal)
        .map(|value| value.abs())
        .filter(|value| *value > FREQ_PARTICIPATION_CUTOFF)
        .collect();
    let modes: &[f64] = match species.class {
        SpeciesClass::Monatomic => &[],
        _ => {
            if participating.is_empty() {
                return None;
            }
            &participating
        }
    };

    let sigma = f64::from(record.rotational_symmetry_number.unwrap_or(1));
    let multiplicity = f64::from(record.multiplicity.unwrap_or(1));
    let translational_kbt = K_B * temperature / E_HARTREE;

    // Vibrational sums over the scaled frequencies. The e^{−x} forms keep
    // stiff modes and low temperatures finite (rqm/thermo.md Numerics).
    let average_moment = match species.class {
        SpeciesClass::Monatomic => 0.0,
        SpeciesClass::Linear => 0.5 * (species.moments[1] + species.moments[2]),
        SpeciesClass::Nonlinear => {
            (species.moments[0] + species.moments[1] + species.moments[2]) / 3.0
        }
    };
    let mut zpe = 0.0;
    let mut e_vib = 0.0;
    let mut s_vib = 0.0;
    for mode in modes {
        let scaled = conditions.freq_scale * mode;
        let x = H_PLANCK * C_LIGHT_CM_PER_S * scaled / (K_B * temperature);
        let boltzmann = (-x).exp();
        let occupation = boltzmann / (1.0 - boltzmann);
        let mode_energy = H_PLANCK * C_LIGHT_CM_PER_S * scaled;
        zpe += 0.5 * mode_energy / E_HARTREE;
        e_vib += mode_energy * occupation / E_HARTREE;
        let harmonic = x * boltzmann / (1.0 - boltzmann) - (1.0 - boltzmann).ln();
        let weight = if conditions.freq_cutoff > 0.0 {
            1.0 / (1.0 + (scaled / conditions.freq_cutoff).powi(4))
        } else {
            0.0
        };
        let rotor_moment = H_PLANCK
            / (8.0 * std::f64::consts::PI * std::f64::consts::PI * C_LIGHT_CM_PER_S * scaled);
        let reduced_moment = rotor_moment * average_moment / (rotor_moment + average_moment);
        let free_rotor = 0.5
            + ((8.0 * std::f64::consts::PI.powi(3) * reduced_moment * K_B * temperature).sqrt()
                / H_PLANCK)
                .ln();
        s_vib += (1.0 - weight) * R * harmonic + weight * R * free_rotor;
    }

    // Rotational and translational terms.
    let (e_rot, s_rot) = match species.class {
        SpeciesClass::Monatomic => (0.0, 0.0),
        SpeciesClass::Linear => {
            let b = rotational_constant(species.moments[1]);
            (
                translational_kbt,
                R * ((K_B * temperature / (sigma * H_PLANCK * C_LIGHT_CM_PER_S * b)).ln() + 1.0),
            )
        }
        SpeciesClass::Nonlinear => {
            let (b_a, b_b, b_c) = (
                rotational_constant(species.moments[0]),
                rotational_constant(species.moments[1]),
                rotational_constant(species.moments[2]),
            );
            (
                1.5 * translational_kbt,
                R * ((std::f64::consts::PI.sqrt() / sigma * (K_B * temperature).powf(1.5)
                    / ((H_PLANCK * C_LIGHT_CM_PER_S * b_a)
                        * (H_PLANCK * C_LIGHT_CM_PER_S * b_b)
                        * (H_PLANCK * C_LIGHT_CM_PER_S * b_c))
                        .sqrt())
                .ln()
                    + 1.5),
            )
        }
    };
    let e_trans = 1.5 * translational_kbt;
    let p_v = translational_kbt;

    let enthalpy = electronic_energy + zpe + e_vib + e_rot + e_trans + p_v;

    // Entropies in J mol^-1 K^-1; the T·S fields carry the molar-to-Eh
    // conversion factor.
    let molar_eh = N_A * E_HARTREE;
    let to_ts = |entropy: f64| temperature * entropy / molar_eh;
    let mass_kg = species.total_mass_u * U_ATOMIC_MASS;
    let s_trans = R
        * (2.5
            + ((2.0 * std::f64::consts::PI * mass_kg * K_B * temperature / (H_PLANCK * H_PLANCK))
                .powf(1.5)
                * (K_B * temperature / P_STANDARD))
                .ln());
    let s_el = R * multiplicity.ln();
    let ts_translational = to_ts(s_trans);
    let ts_rotational = to_ts(s_rot);
    let ts_vibrational = to_ts(s_vib);
    let ts_electronic = to_ts(s_el);
    let entropy_term = ts_translational + ts_rotational + ts_vibrational + ts_electronic;
    let gibbs = enthalpy - entropy_term;

    let thermo = Thermo {
        zpe,
        thermal_vibrational: e_vib,
        thermal_rotational: e_rot,
        thermal_translational: e_trans,
        enthalpy,
        ts_translational,
        ts_rotational,
        ts_vibrational,
        ts_electronic,
        entropy_term,
        gibbs,
    };
    // rq-ac683d6d — A derivation with any non-finite term is undefined,
    // never served as an infinity or NaN. Inside the Numerics range the
    // check always passes; beyond it (an accepted-but-absurd --temp)
    // e^{−x} can round to exactly 1 and diverge the vibrational sums.
    let terms = [
        thermo.zpe,
        thermo.thermal_vibrational,
        thermo.thermal_rotational,
        thermo.thermal_translational,
        thermo.enthalpy,
        thermo.ts_translational,
        thermo.ts_rotational,
        thermo.ts_vibrational,
        thermo.ts_electronic,
        thermo.entropy_term,
        thermo.gibbs,
    ];
    if terms.iter().all(|value| value.is_finite()) {
        Some(thermo)
    } else {
        None
    }
}

/// The single registry dispatch (`rqm/ARCHITECTURE.md` extensibility,
/// `rqm/thermo.md` Feature API): every consumer of a derived energy key
/// resolves it here — printing, cutoffs, sorting, uniqueness, and
/// `--glob` never hand-wire their own match arm to a derivation; the
/// formation values instead read the memo's whole-record
/// thermochemistry derivation in one call, which keeps their three
/// thermal levels defined together (`rqm/formation.md`). The
/// thermochemistry-dependent keys evaluate their derivation lazily, so
/// `"el"`, `"elcorr"`, and unknown keys never derive; the corrected keys
/// `hcorr`/`gcorr` evaluate the corrected derivation
/// (`rqm/correction.md`).
///
/// (rq-8f6da1c2)
fn resolve(
    key: &str,
    record: &Record,
    thermo: impl FnOnce() -> Option<Thermo>,
    corrected: impl FnOnce() -> Option<Thermo>,
) -> Option<f64> {
    match key {
        "el" => record.electronic_energy,
        "elcorr" => crate::correction::corrected_energy(record),
        "zpe" => thermo().map(|thermo| thermo.zpe),
        "h" => thermo().map(|thermo| thermo.enthalpy),
        "s" => thermo().map(|thermo| thermo.entropy_term),
        "g" => thermo().map(|thermo| thermo.gibbs),
        "hcorr" => corrected().map(|thermo| thermo.enthalpy),
        "gcorr" => corrected().map(|thermo| thermo.gibbs),
        _ => None,
    }
}

/// The derived-property registry lookup (`rqm/ARCHITECTURE.md`
/// extensibility): `key` maps to a derivation function returning the
/// record's value in hartree at the given conditions. The registry holds
/// `"el"` (the passthrough electronic energy), `"zpe"`, `"h"`, `"s"` (the
/// T·S entropy term), `"g"`, and the corrected keys `"elcorr"`, `"hcorr"`,
/// and `"gcorr"` (`rqm/correction.md`); unknown keys return `None`.
///
/// (rq-8f6da1c2)
pub fn derived_value(key: &str, record: &Record, conditions: &ThermoConditions) -> Option<f64> {
    resolve(
        key,
        record,
        || thermochemistry(record, conditions),
        || crate::correction::corrected_thermo(record, conditions),
    )
}

/// The per-run derivation memo (`rqm/thermo.md` Performance): carries the
/// run's `ThermoConditions` and derives each record's `Thermo` at most
/// once, so printing, cutoffs, sorting, uniqueness, `--glob`, and the
/// formation section share one derivation per record. The corrected
/// derivation (`rqm/correction.md`) is memoized the same way in its own
/// cache. Cache hits require full record equality (`Eq` over every
/// field): the memo shares derivations across the consumers of a run,
/// never across different records. The memo is a call-site convenience
/// only: `thermochemistry` itself stays pure, uncached, and deterministic.
///
/// (rq-8b44ad8d rq-f9178b9d)
#[derive(Debug, Default)]
pub struct ThermoMemo {
    conditions: ThermoConditions,
    cache: RefCell<HashMap<Record, Option<Thermo>>>,
    corrected_cache: RefCell<HashMap<Record, Option<Thermo>>>,
}

impl ThermoMemo {
    /// An empty memo for a run at the given conditions.
    pub fn new(conditions: ThermoConditions) -> Self {
        ThermoMemo {
            conditions,
            cache: RefCell::new(HashMap::new()),
            corrected_cache: RefCell::new(HashMap::new()),
        }
    }

    /// The run's temperature in K as configured at construction: the
    /// formation sections' concentration correction resolves the `--conc`
    /// pressures and the k_B T term through it (`rqm/formation.md`).
    ///
    /// (rq-10341821)
    pub fn temperature(&self) -> f64 {
        self.conditions.temperature
    }

    /// The record's full derivation, computed at most once per record
    /// content for the lifetime of this memo.
    pub fn thermo(&self, record: &Record) -> Option<Thermo> {
        if let Some(hit) = self.cache.borrow().get(record) {
            return *hit;
        }
        let value = thermochemistry(record, &self.conditions);
        self.cache.borrow_mut().insert(record.clone(), value);
        value
    }

    /// The record's corrected derivation (`rqm/correction.md`): the
    /// standard thermochemistry with the attached correction's energy in
    /// place of the stored one, computed at most once per record content
    /// for the lifetime of this memo. `None` without a correction or when
    /// the derivation is undefined under the usual rules. The corrected
    /// formation section reads the corrected enthalpy and the
    /// correction-invariant T·S entropy term through this one derivation
    /// (`rqm/formation.md`).
    // rq-8b44ad8d rq-789635d3
    pub(crate) fn corrected_thermo(&self, record: &Record) -> Option<Thermo> {
        if let Some(hit) = self.corrected_cache.borrow().get(record) {
            return *hit;
        }
        let value = crate::correction::corrected_thermo(record, &self.conditions);
        self.corrected_cache
            .borrow_mut()
            .insert(record.clone(), value);
        value
    }

    /// The registry lookup of `key` served through this memo's
    /// derivations (`rq-8f6da1c2`).
    pub fn value(&self, key: &str, record: &Record) -> Option<f64> {
        resolve(
            key,
            record,
            || self.thermo(record),
            || self.corrected_thermo(record),
        )
    }
}
