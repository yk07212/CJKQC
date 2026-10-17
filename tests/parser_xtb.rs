//! XTB parser scenarios (`rqm/parsers/xtb.md`), one test per Gherkin
//! scenario: synthetic logs exercising the extraction rules, plus the
//! corpus fixture through the standard registry.

use std::io::Cursor;

use complied_jkqc::{FileWindows, Parser, ParserRegistry, Record, RecordStatus, XtbParser};

fn covered(content: &str) -> FileWindows {
    let mut cursor = Cursor::new(content.as_bytes());
    FileWindows::from_seekable(&mut cursor, 1 << 20, 1 << 20).expect("windows")
}

fn parse(content: &str) -> Record {
    XtbParser.parse(&covered(content))
}

/// A minimal but structurally faithful xtb log: everything except the
/// trailing completion marker.
fn log_body() -> String {
    "-----------------------------------------------------------
     * xtb version 6.7.1 (26dd68d) compiled by 'x' on 2026-05-14

          program call               : /opt/xtb mol.xyz --gxtb --acc 0.01
molecular charge               0.0000000000000E+00 e
number of unpaired electrons   0.0000000000000E+00 e
================
 final structure:
================
3
 xtb: 6.7.1 (26dd68d)
O            0.10000000000000       0.20000000000000       0.30000000000000
H            1.10000000000000       0.20000000000000       0.30000000000000
H           -0.90000000000000       0.20000000000000       0.30000000000000

projected vibrational frequencies (cm-1)
eigval :       -0.00    -0.00     0.00     0.00     0.00     0.00
eigval :      232.39   273.07   -57.30
 reduced masses (amu)
    1:      3.73

Atomic charges and shell populations (in atomic units):
------------------------------------------------------
     #    Z                q          p(s)       p(p)
------------------------------------------------------
     1    8 O      -0.500000      1.000000   3.000000
     2    1 H       0.250000      0.750000
     3    1 H       0.250000      0.750000
------------------------------------------------------

Atomic dipole moments (in atomic units):
---------------------------------------------------------
     #    Z                 x            y            z
---------------------------------------------------------
           total    -0.546489    -0.297958    -0.000000
---------------------------------------------------------

           -------------------------------------------------
          | TOTAL ENERGY             -10.500000000000 Eh    |
           -------------------------------------------------
"
    .to_string()
}

fn completed_log() -> String {
    format!(
        "{} * finished run on 2026-10-17 at 17:33:26.897\n",
        log_body()
    )
}

// rq-35c8e2c9
#[test]
fn version_banner_is_detected() {
    assert!(XtbParser.detect("* xtb version 6.7.1 (26dd68d) compiled by 'x' on 2026-05-14"));
}

// rq-0bb4b64f
#[test]
fn unrelated_head_is_not_claimed() {
    assert!(!XtbParser.detect("Some other program's fine output\nnothing here\n"));
}

// rq-68850338
#[test]
fn completed_run_yields_full_observable_set() {
    let record = parse(&completed_log());
    assert_eq!(record.status, RecordStatus::Ok);
    assert_eq!(record.program_version.as_deref(), Some("6.7.1"));
    assert_eq!(record.method.as_deref(), Some("g-xTB"));
    assert_eq!(record.charge, Some(0));
    assert_eq!(record.multiplicity, Some(1));
    assert_eq!(record.electronic_energy, Some(-10.5));
    let geometry = record.geometry.expect("geometry");
    assert_eq!(geometry.symbols, ["O", "H", "H"]);
    assert_eq!(geometry.coordinates[0], 0.1);
    let frequencies = record.vibrational_frequencies.expect("frequencies");
    assert_eq!(frequencies.len(), 9);
    assert_eq!(frequencies[6], 232.39);
    assert_eq!(record.dipole_moment, Some([-0.546489, -0.297958, -0.0]));
    assert_eq!(record.mulliken_charges, Some(vec![-0.5, 0.25, 0.25]));
    assert_eq!(record.rotational_symmetry_number, None);
}

// rq-20204b94
#[test]
fn last_total_energy_line_wins() {
    let boxed = "          | TOTAL ENERGY             -10.500000000000 Eh    |";
    let earlier = "          | TOTAL ENERGY              -9.900000000000 Eh    |";
    let content = format!(
        "{}\n * finished run\n",
        log_body().replace(boxed, &format!("{earlier}\n{boxed}"))
    );
    let record = parse(&content);
    assert_eq!(record.electronic_energy, Some(-10.5));
}

// rq-1fb0a352
#[test]
fn method_flags_map_to_method_names() {
    for (flag, expected) in [
        ("--gfnn", "GFN2-xTB"),
        ("--gfno", "GFN0-xTB"),
        ("--gxtb", "g-xTB"),
        ("", "GFN2-xTB"),
    ] {
        let content = completed_log().replace(
            "/opt/xtb mol.xyz --gxtb --acc 0.01",
            format!("/opt/xtb mol.xyz {flag}").trim(),
        );
        let record = parse(&content);
        assert_eq!(record.method.as_deref(), Some(expected), "flag {flag:?}");
    }
}

// rq-5882d984
#[test]
fn negative_frequencies_keep_their_sign() {
    let record = parse(&completed_log());
    let frequencies = record.vibrational_frequencies.expect("frequencies");
    assert!(frequencies.contains(&-57.3));
}

// rq-ce1b9305
#[test]
fn run_without_end_marker_is_truncated() {
    let record = parse(&log_body());
    assert_eq!(record.status, RecordStatus::Truncated);
    assert!(record.error.expect("error").contains("finished run"));
    assert_eq!(record.electronic_energy, Some(-10.5));
    assert!(record.geometry.is_some());
}

// rq-62e1382e
#[test]
#[ignore = "awaiting the genuine xtb failed-run fixture"]
fn recorded_failed_run_is_classified_failed() {
    let path = std::path::Path::new("tests/fixtures/xtb/1dma-failed.log");
    let record = ParserRegistry::standard()
        .parse_file(path)
        .expect("parse failed fixture");
    assert_eq!(record.status, RecordStatus::Failed);
    assert!(record.error.as_deref().is_some_and(|e| !e.is_empty()));
}

// The provisional error signal (xtb prints `ERROR:` lines): verified
// against synthetic content until the genuine fixture pins the wording.
#[test]
fn provisional_error_signal_is_failed() {
    let content = format!("{}\nERROR: could not read coordinates\n", log_body());
    let record = parse(&content);
    assert_eq!(record.status, RecordStatus::Failed);
    assert!(record.electronic_energy.is_some(), "observables survive");
}

// rq-afe58819
#[test]
fn hidden_final_structure_leaves_geometry_absent() {
    let head = "* xtb version 6.7.1 (26dd68d) compiled by 'x'\n";
    let hidden =
        "================\n final structure:\n================\n1\ncomment\nHe 0.0 0.0 0.0\n";
    let tail =
        "molecular charge               0.0E+00 e\n| TOTAL ENERGY -10.5 Eh |\n * finished run\n";
    let content = format!("{head}{hidden}{tail}");
    let mut cursor = Cursor::new(content.as_bytes());
    let windows = FileWindows::from_seekable(&mut cursor, head.len(), tail.len()).expect("windows");
    assert!(!windows.covered(), "the structure must be elided");
    let record = XtbParser.parse(&windows);
    assert!(record.geometry.is_none());
    assert_eq!(record.electronic_energy, Some(-10.5));
}

// rq-ebeceeae
#[test]
fn corpus_fixture_conforms() {
    let record = ParserRegistry::standard()
        .parse_file(std::path::Path::new("tests/fixtures/xtb/1dma.log"))
        .expect("parse");
    assert_eq!(record.status, RecordStatus::Ok);
    assert_eq!(record.electronic_energy, Some(-135.137949826927));
    assert_eq!(record.method.as_deref(), Some("g-xTB"));
    assert_eq!(record.program.as_deref(), Some("xtb"));
    assert_eq!(record.geometry.expect("geometry").symbols.len(), 10);
}
