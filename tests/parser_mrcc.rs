//! MRCC parser scenarios (`rqm/parsers/mrcc.md`), one test per Gherkin
//! scenario: synthetic outputs exercising the extraction rules, plus the
//! corpus fixture through the standard registry.

use std::io::Cursor;

use complied_jkqc::{
    FileWindows, MrccParser, Parser, ParserRegistry, Record, RecordStatus, BOHR_TO_ANGSTROM,
};

fn covered(content: &str) -> FileWindows {
    let mut cursor = Cursor::new(content.as_bytes());
    FileWindows::from_seekable(&mut cursor, 1 << 20, 1 << 20).expect("windows")
}

fn parse(content: &str) -> Record {
    MrccParser.parse(&covered(content))
}

const HEADER: &str = "                          ***********************\n\
     \x20                        MRCC program system\n\
     \x20                        ***********************\n\
     \x20                          Version: 25.1.2\n";

const ECHO: &str = " Input file:\n\
     \x20calc=LCCSD(T)\n\
     \x20basis=aug'-cc-pVTZ\n\
     \x20scfalg=direct\n\
     \x20mem=86GB\n\
     \x20charge=0\n\
     \x20Keywords:\n";

const PROPERTIES: &str = " Charge:                           0\n\
     \x20Spin multiplicity:                1\n\
     \x20Number of atoms:                  3\n";

const GEOMETRY: &str = "                 Cartesian coordinates [bohr]      Atomic mass [AMU]  Z\n\
     \x20  1  H    -1.44826169    2.26547449    0.00000000     1.007825       1\n\
     \x20  2  H     1.80928208   -1.48067292    2.41396594     1.007825       1\n\
     \x20  3  N    -0.05688137    0.80666617    3.93265396    14.0030740       7\n\n";

const ENERGY: &str = " Energy summary:\n\
     \x20Total CCSD energy [au]:                               -134.911659201770\n\
     \x20CCSD(T) correlation energy [au]:                       -0.639624171685\n\
     \x20Total CCSD(T) energy [au]:                            -134.934017249766\n\
     \x20CCSD(T) correlation energy + MP2 corrections [au]:     -0.641901761240\n\
     \x20Total LNO-CCSD(T) energy with MP2 corrections [au]:  -134.936294839321\n\n";

const TERMINATION: &str = "                      Normal termination of mrcc.\n";

/// A minimal but structurally faithful run: everything except the
/// energy summary and the termination banner.
fn run_body() -> String {
    format!("{HEADER}{ECHO}{PROPERTIES}{GEOMETRY}")
}

fn completed_run() -> String {
    format!("{}{}{}", run_body(), ENERGY, TERMINATION)
}

// rq-33ab6159
#[test]
fn banner_is_detected() {
    assert!(MrccParser
        .detect("                          MRCC program system\n")
        .is_some());
}

// rq-8890cc1d
#[test]
fn unrelated_head_is_not_claimed() {
    assert!(MrccParser
        .detect("Some other program's fine output\nnothing here\n")
        .is_none());
}

// rq-de12aef8
#[test]
fn terminated_run_yields_observable_set() {
    let record = parse(&completed_run());
    assert_eq!(record.status, RecordStatus::Ok);
    assert_eq!(record.program_version.as_deref(), Some("25.1.2"));
    assert_eq!(
        record.command.as_deref(),
        Some("calc=LCCSD(T) basis=aug'-cc-pVTZ scfalg=direct")
    );
    assert_eq!(record.charge, Some(0));
    assert_eq!(record.multiplicity, Some(1));
    let geometry = record.geometry.expect("geometry");
    assert_eq!(geometry.symbols, ["H", "H", "N"]);
    assert!(
        (geometry.coordinates[0] - (-1.44826169 * BOHR_TO_ANGSTROM)).abs() < 1e-12,
        "{}",
        geometry.coordinates[0]
    );
}

// rq-e719ddf4
#[test]
fn corrected_total_energy_wins() {
    let record = parse(&completed_run());
    assert_eq!(record.electronic_energy, Some(-134.936294839321));
}

// rq-a60f442f
#[test]
fn run_without_local_corrections_reports_plain_total() {
    let content = completed_run().replace(
        " CCSD(T) correlation energy + MP2 corrections [au]:     -0.641901761240\n Total LNO-CCSD(T) energy with MP2 corrections [au]:  -134.936294839321\n\n",
        "\n",
    );
    let record = parse(&content);
    assert_eq!(record.electronic_energy, Some(-134.934017249766));
}

// rq-b8a853f8
#[test]
fn command_block_stops_at_the_first_resource_or_geometry_key() {
    let content = completed_run().replace(
        " calc=LCCSD(T)\n basis=aug'-cc-pVTZ\n scfalg=direct\n mem=86GB\n charge=0\n",
        " calc=MP2\n scfalg=direct\n charge=0\n",
    );
    let record = parse(&content);
    assert_eq!(record.command.as_deref(), Some("calc=MP2 scfalg=direct"));
}

// rq-91435a73
#[test]
#[ignore = "awaiting the genuine MRCC failed-run fixture"]
fn recorded_failed_run_is_classified_failed() {
    let record = ParserRegistry::standard()
        .parse_file(std::path::Path::new("tests/fixtures/mrcc/1dma-failed.out"))
        .expect("parse failed fixture");
    assert_eq!(record.status, RecordStatus::Failed);
    assert!(record.error.as_deref().is_some_and(|e| !e.is_empty()));
    assert!(record.geometry.is_some());
}

// The provisional abort signal (`Aborting …`): verified against
// synthetic content until the genuine fixture pins the wording.
#[test]
fn provisional_abort_signal_is_failed() {
    let content = format!(
        "{} Aborting program mrcc due to unconverged CC iterations.\n",
        run_body()
    );
    let record = parse(&content);
    assert_eq!(record.status, RecordStatus::Failed);
    assert!(record.geometry.is_some(), "head observables survive");
}

// rq-be38168f
#[test]
fn run_cut_mid_flight_is_truncated() {
    let content = format!(
        "{} CCSD iterations:\n     Iter    Residual       DeltaE   ...\n      1   1.2D-03      -0.0021\n",
        run_body()
    );
    let record = parse(&content);
    assert_eq!(record.status, RecordStatus::Truncated);
    assert_eq!(record.electronic_energy, None);
    assert!(record.geometry.is_some());
}

// rq-fd767996
#[test]
fn hidden_geometry_table_leaves_geometry_absent() {
    let full = completed_run();
    let table_at = full.find("Cartesian coordinates [bohr]").expect("anchor");
    let summary_at = full.find(" Energy summary:").expect("anchor");
    let (head, rest) = full.split_at(table_at);
    let (hidden, tail) = rest.split_at(summary_at - table_at);
    let content = format!("{head}{hidden}{tail}");
    let mut cursor = Cursor::new(content.as_bytes());
    let windows = FileWindows::from_seekable(&mut cursor, head.len(), tail.len()).expect("windows");
    assert!(!windows.covered(), "the geometry table must be elided");
    let record = MrccParser.parse(&windows);
    assert_eq!(record.geometry, None);
    assert_eq!(record.electronic_energy, Some(-134.936294839321));
}

// rq-465a02d6
#[test]
fn corpus_fixture_conforms() {
    let record = ParserRegistry::standard()
        .parse_file(std::path::Path::new("tests/fixtures/mrcc/1dma.out"))
        .expect("parse");
    assert_eq!(record.status, RecordStatus::Ok);
    assert_eq!(record.program.as_deref(), Some("mrcc"));
    assert_eq!(record.program_version.as_deref(), Some("25.1.2"));
    assert_eq!(
        record.command.as_deref(),
        Some("calc=LCCSD(T) basis=aug'-cc-pVTZ scfalg=direct")
    );
    assert_eq!(record.electronic_energy, Some(-134.936294839321));
    let geometry = record.geometry.expect("geometry");
    assert_eq!(geometry.symbols.len(), 10);
    assert!(
        (geometry.coordinates[0] - (-0.76638707945281)).abs() < 1e-8,
        "{}",
        geometry.coordinates[0]
    );
}

// rq-d82e314a
#[test]
fn scf_energy_is_the_final_hartree_fock_energy() {
    let hf = " ***FINAL HARTREE-FOCK ENERGY:       -134.2943930780812707 [AU]\n\n";
    let content = format!("{}{}{}{}", run_body(), hf, ENERGY, TERMINATION);
    let record = parse(&content);
    assert_eq!(record.scf_energy, Some(-134.294_393_078_081_27));
    assert_eq!(record.electronic_energy, Some(-134.936294839321));
}

// rq-16ad220b
#[test]
fn a_run_without_the_final_hf_line_carries_no_scf_energy() {
    // The per-step energy echo is not the final Hartree-Fock line.
    let step = " ***HARTREE-FOCK ENERGY IN STEP  11 IS       -134.2943930780812707 [AU]\n\n";
    let content = format!("{}{}{}{}", run_body(), step, ENERGY, TERMINATION);
    let record = parse(&content);
    assert_eq!(record.scf_energy, None);
    // No HF line at all behaves the same.
    let content = format!("{}{}{}", run_body(), ENERGY, TERMINATION);
    let record = parse(&content);
    assert_eq!(record.scf_energy, None);
}

// rq-b9d141f4
#[test]
fn the_hf_energy_line_test_admits_only_the_starred_spelling() {
    // The starred spelling prints among near-miss lines — the same line
    // without its stars and the starred prefix without the ENERGY word —
    // and only the starred spelling contributes.
    let record = parse(&format!(
        "{}{}***FINAL HARTREE-FOCK ENERGY:      -134.2900000000000 Eh\n\
         FINAL HARTREE-FOCK ENERGY:        -134.3000000000000 Eh\n\
         ***FINAL HARTREE-FOCK:             -134.3100000000000\n\n{}",
        run_body(), ENERGY, TERMINATION
    ));
    assert_eq!(record.scf_energy, Some(-134.29));
}
