//! Primary-observable records — the unit of storage in JSON Lines databases
//! (see the `db` module). One record holds one quantum-chemistry job's
//! observables; derived quantities are computed on demand and never stored.
//! Canonical units: energies in hartree, coordinates in Ångström, gradients
//! in hartree/Å, frequencies in cm⁻¹, dipole moments in e·a₀, charges in
//! elementary charges.
//!
//! (rq-0ae95856 rq-72f50fe8)

use std::collections::BTreeMap;
use std::fmt;
use std::hash::{Hash, Hasher};

/// A resolved composition: component code to count. Keys are non-empty ASCII
/// letter strings, matched case-sensitively (`Am` and `am` are distinct
/// codes); counts are at least 1; iteration (and therefore JSON) order is
/// alphabetical.
///
/// (rq-ffc1bde0)
pub type Components = BTreeMap<String, u32>;

/// A structure as parallel arrays: element symbols and their positions.
///
/// (rq-e17eea18)
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Geometry {
    /// Element symbols in title case (`"N"`, `"Cl"`); at least one atom.
    pub symbols: Vec<String>,
    /// Positions in Å, row-major: exactly three values per atom, in
    /// `symbols` order.
    pub coordinates: Vec<f64>,
}

// ---------------------------------------------------------------------------
// Hashing support for memo keys (`rqm/thermo.md` Performance).
//
// `Eq` on the float fields is unsound only for NaN, which record
// validation rejects (`RecordInvalidity::NonFiniteFloat`). Floats hash by
// canonical bit pattern — ±0.0 agree because `Eq` holds them equal — so
// equal records always hash alike.
// ---------------------------------------------------------------------------

impl Eq for Geometry {}

fn hash_f64<H: Hasher>(value: f64, state: &mut H) {
    if value == 0.0 {
        0u64.hash(state);
    } else {
        value.to_bits().hash(state);
    }
}

fn hash_f64_slice<H: Hasher>(values: &[f64], state: &mut H) {
    state.write_usize(values.len());
    for value in values {
        hash_f64(*value, state);
    }
}

impl Hash for Geometry {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.symbols.hash(state);
        hash_f64_slice(&self.coordinates, state);
    }
}

/// The outcome of a quantum-chemistry job, serialized as `"ok"`, `"failed"`,
/// or `"truncated"`.
///
/// (rq-f8505d7b)
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum RecordStatus {
    /// The job converged; its observables are final.
    #[default]
    Ok,
    /// The job did not finish; observables may be partially present.
    Failed,
    /// The output file ends mid-run; observables may be partially present.
    Truncated,
}

/// The reason a record fails validation.
///
/// (rq-4eefe504)
#[derive(Debug, Clone, PartialEq)]
pub enum RecordInvalidity {
    /// A floating-point field holds NaN or ±∞; JSON cannot represent it.
    NonFiniteFloat { field: &'static str },
    /// Zero symbols, or a coordinates length that is not exactly three times
    /// the symbols length.
    MalformedGeometry { symbols: usize, coordinates: usize },
    /// A per-atom array whose length disagrees with the geometry's atom
    /// count.
    LengthMismatch {
        field: &'static str,
        expected: usize,
        actual: usize,
    },
    /// Status is `failed` without an error message.
    MissingError,
    /// `multiplicity` or `rotational_symmetry_number` below 1.
    InvalidScalar { field: &'static str, reason: String },
    /// The components map is empty, a component code is empty or holds
    /// anything but ASCII letters, or a count is below 1.
    InvalidComponents,
}

impl fmt::Display for RecordInvalidity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecordInvalidity::NonFiniteFloat { field } => {
                write!(f, "field `{field}` holds a non-finite value")
            }
            RecordInvalidity::MalformedGeometry {
                symbols,
                coordinates,
            } => write!(
                f,
                "geometry has {symbols} symbols and {coordinates} coordinates, \
                 expected at least one atom and exactly three coordinates per atom"
            ),
            RecordInvalidity::LengthMismatch {
                field,
                expected,
                actual,
            } => write!(
                f,
                "field `{field}` has {actual} values, expected {expected}"
            ),
            RecordInvalidity::MissingError => {
                write!(f, "failed record carries no error message")
            }
            RecordInvalidity::InvalidScalar { field, reason } => {
                write!(f, "field `{field}` is invalid: {reason}")
            }
            RecordInvalidity::InvalidComponents => {
                write!(
                    f,
                    "components must be non-empty with letter-only codes and counts of at least 1"
                )
            }
        }
    }
}

impl std::error::Error for RecordInvalidity {}

/// One attached single-point correction with its provenance: the
/// alternative electronic energy a `--add-correction` run matched to the
/// record's `name`, plus where that energy came from
/// (`rqm/correction.md`). The stored `electronic_energy` is never altered.
///
/// (rq-95e1c918)
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Correction {
    /// The single-point electronic energy in hartree.
    pub energy: f64,
    /// The correction job's program token, copied from the correction
    /// record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    /// The correction job's version string, copied from the correction
    /// record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program_version: Option<String>,
    /// The command line(s) that produced the energy: the correction
    /// record's `command` as a one-element list, omitted when unknown; an
    /// extrapolation correction lists one entry per contributing
    /// calculation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commands: Option<Vec<String>>,
    /// Non-empty token describing how the energy was obtained; `sp` when
    /// attached from a parsed single-point record. Open vocabulary: tokens
    /// are stored verbatim so later versions' databases stay loadable.
    pub kind: String,
}

/// One quantum-chemistry job's primary observables. Serializable to one
/// line of compact JSON; absent observables are omitted from the line and
/// read back as `None`. Unknown keys are ignored on read, and a missing
/// `status` key reads as `Ok`.
///
/// (rq-43be8f79 rq-5e237eee)
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Record {
    /// The job's outcome.
    #[serde(default)]
    pub status: RecordStatus,
    /// Human-readable explanation of a failed or truncated job.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The structure's identity: the stem of the source file name, carried
    /// verbatim; matches the same structure across databases.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Path of the QC output file the record was parsed from, verbatim as
    /// given to the tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    /// Lowercase program token (`"xtb"`, `"gaussian"`, `"orca"`, `"mrcc"`,
    /// `"xyz"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    /// Version string as printed by the program.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program_version: Option<String>,
    /// The run's command line as echoed by the program; each program's
    /// extraction rule lives in `rqm/parsers/`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Total electronic energy of the final structure, in hartree.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub electronic_energy: Option<f64>,
    /// The converged self-consistent-field energy of the final structure,
    /// in hartree: the Hartree–Fock total for wavefunction methods, the
    /// final SCF-cycle total for density-functional ones. Extracted where
    /// the program prints one (`rqm/parsers/`); an input to basis-set
    /// extrapolation (`rqm/extrapolation.md`) and to no other derivation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scf_energy: Option<f64>,
    /// Net charge in elementary charges.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub charge: Option<i32>,
    /// Spin multiplicity, at least 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiplicity: Option<u32>,
    /// The structure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry: Option<Geometry>,
    /// Harmonic frequencies in cm⁻¹, sign preserved (negative marks an
    /// imaginary mode).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vibrational_frequencies: Option<Vec<f64>>,
    /// Cartesian energy gradient in hartree/Å, row-major, one atom per
    /// triple, in geometry atom order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gradient: Option<Vec<f64>>,
    /// Dipole moment vector in atomic units (e·a₀).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dipole_moment: Option<[f64; 3]>,
    /// Per-atom Mulliken charges in elementary charges, in geometry atom
    /// order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mulliken_charges: Option<Vec<f64>>,
    /// Point-group rotational symmetry number, at least 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotational_symmetry_number: Option<u32>,
    /// The resolved composition: component code to count. Not an observable:
    /// the parser never fills it — composition resolution and CLI overrides
    /// populate it.
    ///
    /// (rq-43be8f79 rq-ffc1bde0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Components>,
    /// An attached single-point correction with its provenance
    /// (`rqm/correction.md`). Not an observable of this job: attached by
    /// the correction mechanism or loaded from a previously corrected
    /// database; the raw `electronic_energy` is never altered.
    ///
    /// (rq-43be8f79)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correction: Option<Correction>,
}

// See the `Geometry` note: validation rejects the only reflexivity gap
// (NaN), making the `Hash`/`Eq` pair sound for memo keys
// (`rqm/thermo.md` Performance).
impl Eq for Record {}

impl Hash for Record {
    /// A Hash over a subset of `Eq`'s fields: equal records hash alike,
    /// and hashing the fewest fields keeps memo probes cheap. The subset
    /// is hash-only — a cache hit is still confirmed by `Eq` over every
    /// field (`rqm/thermo.md` Performance).
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self.electronic_energy {
            Some(value) => {
                1u8.hash(state);
                hash_f64(value, state);
            }
            None => 0u8.hash(state),
        }
        match &self.geometry {
            Some(geometry) => {
                1u8.hash(state);
                Hash::hash(geometry, state);
            }
            None => 0u8.hash(state),
        }
        match &self.vibrational_frequencies {
            Some(values) => {
                1u8.hash(state);
                hash_f64_slice(values, state);
            }
            None => 0u8.hash(state),
        }
        self.multiplicity.hash(state);
        self.rotational_symmetry_number.hash(state);
    }
}

impl Record {
    /// Checks the record's invariants, reporting the first violation found:
    /// every floating-point field is finite; the geometry shape is well
    /// formed; per-atom arrays agree with the geometry's atom count when a
    /// geometry is present; a `failed` record carries a non-empty error
    /// message; `multiplicity` and `rotational_symmetry_number` are at
    /// least 1; a present components map is non-empty with valid codes and
    /// counts; a present correction carries a finite energy and a non-empty
    /// kind.
    ///
    /// (rq-1d31610c)
    pub fn validate(&self) -> Result<(), RecordInvalidity> {
        let finite = |field: &'static str, value: f64| -> Result<(), RecordInvalidity> {
            if value.is_finite() {
                Ok(())
            } else {
                Err(RecordInvalidity::NonFiniteFloat { field })
            }
        };

        if let Some(value) = self.electronic_energy {
            finite("electronic_energy", value)?;
        }
        if let Some(value) = self.scf_energy {
            finite("scf_energy", value)?;
        }
        if let Some(geometry) = &self.geometry {
            for &value in &geometry.coordinates {
                finite("geometry.coordinates", value)?;
            }
        }
        if let Some(values) = &self.vibrational_frequencies {
            for &value in values {
                finite("vibrational_frequencies", value)?;
            }
        }
        if let Some(values) = &self.gradient {
            for &value in values {
                finite("gradient", value)?;
            }
        }
        if let Some(vector) = self.dipole_moment {
            for &value in &vector {
                finite("dipole_moment", value)?;
            }
        }
        if let Some(values) = &self.mulliken_charges {
            for &value in values {
                finite("mulliken_charges", value)?;
            }
        }
        // A present correction carries a finite energy and a non-empty
        // kind (rqm/correction.md).
        // rq-f03405ec rq-0b4ab1a5
        if let Some(correction) = &self.correction {
            finite("correction.energy", correction.energy)?;
            if correction.kind.is_empty() {
                return Err(RecordInvalidity::InvalidScalar {
                    field: "correction.kind",
                    reason: "must not be empty".to_string(),
                });
            }
        }

        let atom_count = match &self.geometry {
            Some(geometry)
                if geometry.symbols.is_empty()
                    || geometry.coordinates.len() != 3 * geometry.symbols.len() =>
            {
                return Err(RecordInvalidity::MalformedGeometry {
                    symbols: geometry.symbols.len(),
                    coordinates: geometry.coordinates.len(),
                });
            }
            Some(geometry) => Some(geometry.symbols.len()),
            None => None,
        };
        if let Some(atoms) = atom_count {
            if let Some(values) = &self.gradient {
                if values.len() != 3 * atoms {
                    return Err(RecordInvalidity::LengthMismatch {
                        field: "gradient",
                        expected: 3 * atoms,
                        actual: values.len(),
                    });
                }
            }
            if let Some(values) = &self.mulliken_charges {
                if values.len() != atoms {
                    return Err(RecordInvalidity::LengthMismatch {
                        field: "mulliken_charges",
                        expected: atoms,
                        actual: values.len(),
                    });
                }
            }
        }

        if matches!(self.status, RecordStatus::Failed)
            && self.error.as_deref().is_none_or(str::is_empty)
        {
            return Err(RecordInvalidity::MissingError);
        }

        if let Some(value) = self.multiplicity {
            if value < 1 {
                return Err(RecordInvalidity::InvalidScalar {
                    field: "multiplicity",
                    reason: "must be at least 1".to_string(),
                });
            }
        }
        if let Some(value) = self.rotational_symmetry_number {
            if value < 1 {
                return Err(RecordInvalidity::InvalidScalar {
                    field: "rotational_symmetry_number",
                    reason: "must be at least 1".to_string(),
                });
            }
        }
        if let Some(components) = &self.components {
            if components.is_empty() {
                return Err(RecordInvalidity::InvalidComponents);
            }
            for (code, count) in components {
                let valid_code =
                    !code.is_empty() && code.bytes().all(|byte| byte.is_ascii_alphabetic());
                if !valid_code || *count < 1 {
                    return Err(RecordInvalidity::InvalidComponents);
                }
            }
        }
        Ok(())
    }

    /// True when status is `failed` and `program` is absent — the parser
    /// framework's signature for a file that matched no parser or was
    /// empty. Collection aborts on such records instead of storing them.
    ///
    /// (rq-8ebc6467)
    pub fn is_unrecognized(&self) -> bool {
        matches!(self.status, RecordStatus::Failed) && self.program.is_none()
    }
}
