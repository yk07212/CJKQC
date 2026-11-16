//! CJKQC — a spiritual successor to JKQC from JKCS2.1: a standalone tool for
//! collecting, organizing, filtering, and post-processing quantum-chemistry
//! output for molecular clusters (see `rqm/ARCHITECTURE.md`).
//!
//! Library target for unit and integration tests. Domain modules arrive with
//! the milestones that implement them.

pub mod arbalign;
pub mod collect;
pub mod composition;
pub mod correction;
pub mod db;
pub mod extrapolate;
pub mod filter;
pub mod formation;
pub mod hydration;
pub mod metrics;
pub mod movie;
pub mod overwrite;
pub mod parser;
pub mod population;
pub mod reacted;
pub mod record;
pub mod thermo;

pub(crate) mod atomic;

// rq-8c5e4002
pub use record::{Components, Correction, Geometry, Record, RecordInvalidity, RecordStatus};

// rq-7330b65b rq-76495264 rq-4ddb030f rq-a637fc31 rq-bb73fa3f rq-8b4b0278 rq-1967b16e
// arbalign surface (rqm/arbalign.md)
pub use arbalign::{
    apply_arbalign, arbalign_mask, arbalign_rmsd, parse_arbalign_threshold, prepare_arbalign,
    ArbAlignReport, ArbAlignStructure,
};

// rq-f8ab9d1b single-point correction surface (rqm/correction.md)
pub use correction::{attach_corrections, corrected_energy, Attachment, CorrectionError};

// rq-b0a418c4 extrapolation correction surface (rqm/extrapolation.md)
pub use extrapolate::{
    blend_energy, extrapolate_corrections, parse_extrapolation, two_point_energy, ExtrapMode,
    Extrapolation, ExtrapolationError, ExtrapolationSpec,
};

// rq-fe2700dc
pub use composition::{
    parse_jkqc_name, rename_component, resolve_components, RenameError, RenameSpec,
};

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
    ThermoConditions, ThermoMemo, E_HARTREE, KCAL_MOL_PER_HARTREE, K_B, P_STANDARD,
};

// rq-f8ebbc84 formation surface (rqm/formation.md)
pub use formation::{
    collapse_ensembles, collect_monomers, concentration_correction, corrected_collapse_ensembles,
    corrected_ensemble_formation_values, corrected_ensemble_references, corrected_formation_values,
    corrected_monomer_references, ensemble_formation_values, ensemble_references, formation_values,
    parse_conc_value, print_formation_corr_ensembles, print_formation_corr_section,
    print_formation_ensembles, print_formation_section, ConcConditions, ConcSpec, ConcUnit,
    CorrectedMonomerReferences, Ensemble, Formation, MonomerReferences,
};

// rq-9e28654c population surface (rqm/population.md)
pub use population::{parse_pop_level, pop_level_value, populations, PopLevel};

// rq-84386009 hydration + solvation distribution surface (rqm/hydration.md)
pub use hydration::{
    corrected_hydration_states, corrected_hydration_states_from_ensembles, hydration_percentages,
    hydration_states, hydration_states_from_ensembles, print_hydration_section,
    saturation_pressure_water, HydrationGroup, HydrationState, SolventConditions,
};

// rq-bf062c8f filtering surface (rqm/filtering.md)
pub use filter::{
    apply_filters, extract_matches, parse_extract_query, print_table, property_value, CmpOp,
    Column, Cutoff, ExtractError, ExtractMode, ExtractPattern, FilterError, FilterOutcome,
    FilterSpec, Grouping, Mode, Property, SortKey, Unit, Wildcard,
};

// rq-eff8dce8 overwrite confirmation surface (rqm/collection.md)
pub use overwrite::{
    confirm_overwrite, overwrite_error, parse_overwrite_answer, OverwriteDecision,
};

// rq-6fb174a0 movie export surface (rqm/movie.md)
pub use movie::{frame_text, movie_frames, write_movie_xyz, MovieError};

// rq-570d2b80 per-structure xyz export surface (rqm/movie.md)
pub use movie::{select_xyz_records, write_xyz_files, xyz_file_name, XyzError};

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
