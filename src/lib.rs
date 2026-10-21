//! CJKQC — a spiritual successor to JKQC from JKCS2.1: a standalone tool for
//! collecting, organizing, filtering, and post-processing quantum-chemistry
//! output for molecular clusters (see `rqm/ARCHITECTURE.md`).
//!
//! Library target for unit and integration tests. Domain modules arrive with
//! the milestones that implement them.

pub mod collect;
pub mod composition;
pub mod correction;
pub mod db;
pub mod filter;
pub mod formation;
pub mod metrics;
pub mod parser;
pub mod reacted;
pub mod record;
pub mod thermo;

// rq-8c5e4002
pub use record::{Components, Correction, Geometry, Record, RecordInvalidity, RecordStatus};

// rq-f8ab9d1b single-point correction surface (rqm/correction.md)
pub use correction::{attach_corrections, corrected_energy, Attachment, CorrectionError};

// rq-fe2700dc
pub use composition::{parse_jkqc_name, resolve_components};

// rq-fcb0174f
pub use collect::{
    classify_inputs, collect_inputs, collect_inputs_with, gather_inputs, gather_inputs_with,
    CollectError, CollectOutcome, CollectionMode, Gathered,
};

// rq-88cb3513 metrics + uniqueness surface (rqm/uniqueness.md)
pub use metrics::{
    atomic_mass, dedup_names, metric_dipole_magnitude, metric_radius_of_gyration,
    metric_total_mass, parse_uniq_keys, parse_uniq_mode, unique_by_properties, UniqKey, UniqMetric,
    UniqMode,
};

// rq-83c62ce0 detection surface (rqm/detection.md)
pub use reacted::{are_bonded, filter_reacted, fragment_signature, ReactedMode};

// rq-3345a07b thermochemistry surface (rqm/thermo.md)
pub use thermo::{
    classify_species, derived_value, rotational_constants, thermochemistry, SpeciesClass, Thermo,
    ThermoConditions, ThermoMemo, KCAL_MOL_PER_HARTREE,
};

// rq-f8ebbc84 formation surface (rqm/formation.md)
pub use formation::{
    collect_monomers, corrected_formation_values, corrected_monomer_references, formation_values,
    print_formation_corr_section, print_formation_section, CorrectedMonomerReferences, Formation,
    MonomerReferences,
};

// rq-bf062c8f filtering surface (rqm/filtering.md)
pub use filter::{
    apply_filters, extract_matches, parse_extract_query, print_table, property_value, CmpOp,
    Column, Cutoff, ExtractError, ExtractMode, ExtractPattern, FilterSpec, Grouping, Mode,
    Property, SortKey, Unit, Wildcard,
};

// rq-15f15563
pub use parser::{
    first_line_offset, FileWindows, GaussianParser, LineMatches, LinePattern, MrccParser,
    OrcaParser, ParseError, Parser, ParserRegistry, Program, TailLine, WindowBudget, XtbParser,
    XyzParser, DEFAULT_HEAD_BYTES, DEFAULT_TAIL_BYTES,
};
// rq-15f15563
pub use parser::extract::{
    element_symbol, parse_fortran_f64, read_geometry_table, vibrational_modes, LengthUnits,
    SymbolColumn, TableSpec, BOHR_TO_ANGSTROM, TRIVIAL_MODE_CUTOFF,
};
