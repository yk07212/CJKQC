//! CJKQC — a spiritual successor to JKQC from JKCS2.1: a standalone tool for
//! collecting, organizing, filtering, and post-processing quantum-chemistry
//! output for molecular clusters (see `rqm/ARCHITECTURE.md`).
//!
//! Library target for unit and integration tests. Domain modules arrive with
//! the milestones that implement them.

pub mod db;
pub mod parser;
pub mod record;

// rq-8c5e4002
pub use record::{Geometry, Record, RecordInvalidity, RecordStatus};

// rq-15f15563
pub use parser::{
    FileWindows, GaussianParser, MrccParser, OrcaParser, ParseError, Parser, ParserRegistry,
    Program, XtbParser, XyzParser, DEFAULT_HEAD_BYTES, DEFAULT_TAIL_BYTES,
};
// rq-15f15563
pub use parser::extract::{
    element_symbol, parse_fortran_f64, read_geometry_table, LengthUnits, SymbolColumn, TableSpec,
    BOHR_TO_ANGSTROM,
};
