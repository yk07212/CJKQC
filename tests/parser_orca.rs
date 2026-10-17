//! ORCA parser scenarios (`rqm/parsers/orca.md`), one test per Gherkin
//! scenario: synthetic outputs exercising the extraction rules, plus the
//! corpus fixture through the standard registry.

use std::io::Cursor;

use complied_jkqc::{FileWindows, OrcaParser, Parser, ParserRegistry, Record, RecordStatus};

fn covered(content: &str) -> FileWindows {
    let mut cursor = Cursor::new(content.as_bytes());
    FileWindows::from_seekable(&mut cursor, 1 << 20, 1 << 20).expect("windows")
}

fn parse(content: &str) -> Record {
    OrcaParser.parse(&covered(content))
}

fn banner() -> String {
    "                                        ****ORCA*****\n\
     \x20                                * O   R   C   A *\n\
                             Program Version 6.0.0  -   RELEASE  -\n"
        .to_string()
}

const ECHO: &str = "INPUT FILE\n\
     -----------------------\n\
     |  1> ! wB97x-3c Opt Freq\n\
     |  2>\n";

const SUMMARY: &str = " Total Charge           Charge          ....    0\n\
     \x20Multiplicity           Mult            ....    1\n";

const GEOMETRY: &str = "CARTESIAN COORDINATES (ANGSTROEM)\n\
     ---------------------------------\n\
     \x20 O      0.000000    0.000000    0.117790\n\
     \x20 H      0.000000    0.755453   -0.471161\n\
     \x20 H      0.000000   -0.755453   -0.471161\n\n";

const ENERGY: &str = "FINAL SINGLE POINT ENERGY       -76.402725158737\n\n";

const FREQUENCIES: &str = "VIBRATIONAL FREQUENCIES\n\
     -----------------------\n\
     Scaling factor for frequencies =  1.000000000  (already applied!)\n\
     \n\
     \x20  0:       0.00 cm**-1\n\
     \x20  1:       0.00 cm**-1\n\
     \x20  2:       0.00 cm**-1\n\
     \x20  3:    1595.77 cm**-1\n\
     \x20  4:    1650.33 cm**-1\n\
     \x20  5:    3831.60 cm**-1\n\
     -----------------------\n\
     IR SPECTRUM\n\n";

const DIPOLE: &str = "DIPOLE MOMENT\n\
     ----------------------\n\
                     X           Y           Z\n\
     Total Dipole Moment    :      0.000000     0.000000     0.733977\n\
     ----------------------\n\n";

const MULLIKEN: &str = "MULLIKEN ATOMIC CHARGES\n\
     -----------------------\n\
     \x20  0 O :    -0.784759\n\
     \x20  1 H :     0.392379\n\
     \x20  2 H :     0.392379\n\
     Sum of formal charges Nonzero. Molecule is charged\n\n";

const POINT_GROUP: &str = "Point Group:  C1, Symmetry Number:   1\n\n";

/// A minimal but structurally faithful opt+freq output: everything
/// except the closing termination banner.
fn log_body() -> String {
    format!("{ECHO}{SUMMARY}{GEOMETRY}{ENERGY}{FREQUENCIES}{DIPOLE}{MULLIKEN}{POINT_GROUP}")
}

fn completed_log() -> String {
    format!(
        "{}{}                    ****ORCA TERMINATED NORMALLY****\n",
        banner(),
        log_body()
    )
}

// rq-5b9c015a
#[test]
fn ascii_art_banner_is_detected() {
    assert!(OrcaParser
        .detect("                                        ****ORCA*****\n   * O   R   C   A *\n"));
}

// rq-5f31c98f
#[test]
fn unrelated_head_is_not_claimed() {
    assert!(!OrcaParser.detect("Some other program's fine output\nnothing here\n"));
}

// rq-eb7784d8
#[test]
fn terminated_opt_freq_run_yields_full_observable_set() {
    let record = parse(&completed_log());
    assert_eq!(record.status, RecordStatus::Ok);
    assert_eq!(record.program_version.as_deref(), Some("6.0.0"));
    assert_eq!(record.method.as_deref(), Some("wB97x-3c"));
    assert_eq!(record.charge, Some(0));
    assert_eq!(record.multiplicity, Some(1));
    assert_eq!(record.electronic_energy, Some(-76.402725158737));
    let geometry = record.geometry.expect("geometry");
    assert_eq!(geometry.symbols, ["O", "H", "H"]);
    assert_eq!(geometry.coordinates[2], 0.11779);
    assert_eq!(
        record.vibrational_frequencies,
        Some(vec![0.0, 0.0, 0.0, 1595.77, 1650.33, 3831.6])
    );
    assert_eq!(record.dipole_moment, Some([0.0, 0.0, 0.733977]));
    assert_eq!(
        record.mulliken_charges,
        Some(vec![-0.784759, 0.392379, 0.392379])
    );
    assert_eq!(record.rotational_symmetry_number, Some(1));
}

// rq-527a66db
#[test]
fn method_requires_an_echoed_input_line() {
    // Remove the real echo (`|  1> ! …`) and drop the `!` keyword onto a
    // non-echo line: with no echoed input block the method must be absent,
    // not scraped from the stray `!`.
    let with_stray =
        completed_log().replace("|  1> ! wB97x-3c Opt Freq\n", "! wB97x-3c Opt Freq\n");
    assert_eq!(
        parse(&with_stray).method,
        None,
        "a `!` outside the input echo must not yield a method"
    );
    // Control: the same keyword on an echo line is recognized.
    assert_eq!(parse(&completed_log()).method.as_deref(), Some("wB97x-3c"));
}

// rq-b7387503
#[test]
fn last_single_point_energy_line_wins() {
    let content = completed_log().replace(
        ENERGY.trim_end(),
        "FINAL SINGLE POINT ENERGY      -199.634522052117\nFINAL SINGLE POINT ENERGY      -199.642065483666",
    );
    let record = parse(&content);
    assert_eq!(record.electronic_energy, Some(-199.642065483666));
}

// rq-9d9e798e
#[test]
fn imaginary_modes_keep_their_sign() {
    let content = completed_log().replace("   3:    1595.77 cm**-1", "   6:     -42.13 cm**-1");
    let record = parse(&content);
    let frequencies = record.vibrational_frequencies.expect("frequencies");
    assert!(frequencies.contains(&-42.13), "{frequencies:?}");
}

// rq-a5fee06d
#[test]
fn run_without_termination_banner_is_truncated() {
    let record = parse(&format!("{}{}", banner(), log_body()));
    assert_eq!(record.status, RecordStatus::Truncated);
    assert!(record.error.expect("error").contains("termination banner"));
    assert_eq!(record.electronic_energy, Some(-76.402725158737));
    assert!(record.vibrational_frequencies.is_some());
}

// rq-0f8ebb73
#[test]
#[ignore = "awaiting the genuine ORCA failed-run fixture"]
fn recorded_failed_run_is_classified_failed() {
    let record = ParserRegistry::standard()
        .parse_file(std::path::Path::new("tests/fixtures/orca/1dma-failed.out"))
        .expect("parse failed fixture");
    assert_eq!(record.status, RecordStatus::Failed);
    assert!(record.error.as_deref().is_some_and(|e| !e.is_empty()));
}

// The provisional error signal (`ORCA TERMINATED DUE TO`): verified
// against synthetic content until the genuine fixture pins the wording.
#[test]
fn provisional_error_signal_is_failed() {
    let content = format!(
        "{}{}\nORCA TERMINATED DUE TO AN ERROR\n",
        banner(),
        log_body()
    );
    let record = parse(&content);
    assert_eq!(record.status, RecordStatus::Failed);
    assert_eq!(record.electronic_energy, Some(-76.402725158737));
}

// rq-69c48453
#[test]
fn single_point_run_reports_no_frequencies_or_symmetry() {
    let content = completed_log()
        .replace(FREQUENCIES, "")
        .replace(POINT_GROUP, "");
    let record = parse(&content);
    assert_eq!(record.status, RecordStatus::Ok);
    assert_eq!(record.electronic_energy, Some(-76.402725158737));
    assert_eq!(record.vibrational_frequencies, None);
    assert_eq!(record.rotational_symmetry_number, None);
}

// rq-ec125e27
#[test]
fn hidden_dipole_block_leaves_dipole_absent() {
    let full = completed_log();
    let dipole_at = full.find("DIPOLE MOMENT").expect("anchor");
    let mulliken_at = full.find("MULLIKEN ATOMIC CHARGES").expect("anchor");
    let (head, rest) = full.split_at(dipole_at);
    let (hidden, tail) = rest.split_at(mulliken_at - dipole_at);
    let content = format!("{head}{hidden}{tail}");
    let mut cursor = Cursor::new(content.as_bytes());
    let windows = FileWindows::from_seekable(&mut cursor, head.len(), tail.len()).expect("windows");
    assert!(!windows.covered(), "the dipole block must be elided");
    let record = OrcaParser.parse(&windows);
    assert_eq!(record.dipole_moment, None);
    assert_eq!(record.electronic_energy, Some(-76.402725158737));
    assert!(record.mulliken_charges.is_some());
}

// rq-e770b7b2
#[test]
fn corpus_fixture_conforms() {
    let record = ParserRegistry::standard()
        .parse_file(std::path::Path::new(
            "tests/fixtures/orca/1pinic1sa-110_1_9-str9.out",
        ))
        .expect("parse");
    assert_eq!(record.status, RecordStatus::Ok);
    assert_eq!(record.program.as_deref(), Some("orca"));
    assert_eq!(record.program_version.as_deref(), Some("6.0.0"));
    assert_eq!(record.method.as_deref(), Some("wB97x-3c"));
    assert_eq!(record.electronic_energy, Some(-199.642065483666));
    assert_eq!(record.geometry.expect("geometry").symbols.len(), 34);
    assert_eq!(
        record.vibrational_frequencies.expect("frequencies").len(),
        102
    );
    assert_eq!(record.rotational_symmetry_number, Some(1));
}
