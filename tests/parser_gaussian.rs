//! Gaussian parser scenarios (`rqm/parsers/gaussian.md`), one test per
//! Gherkin scenario: synthetic logs exercising the extraction rules, plus
//! the corpus fixture through the standard registry.

use std::io::Cursor;

use complied_jkqc::{FileWindows, GaussianParser, Parser, ParserRegistry, Record, RecordStatus};

fn covered(content: &str) -> FileWindows {
    let mut cursor = Cursor::new(content.as_bytes());
    FileWindows::from_seekable(&mut cursor, 1 << 20, 1 << 20).expect("windows")
}

fn parse(content: &str) -> Record {
    GaussianParser.parse(&covered(content))
}

const BANNER: &str = " Entering Gaussian System\n Gaussian 16, Revision B.01,\n";

const ROUTE: &str = " # wb97xd 6-31++g** opt freq\n ----------------------------------------\n";

const CHARGE: &str = " Charge =  0 Multiplicity = 1\n";

const GEOMETRY: &str = "                         Standard orientation:\n ---------------------------------------------------------------------\n Center     Atomic      Atomic             Coordinates (Angstroms)\n Number     Number       Type             X           Y           Z\n ---------------------------------------------------------------------\n      1          6           0       -4.061711   -1.017734   -1.010761\n      2          1           0       -3.013527   -1.032997   -1.041370\n      3          1           0       -4.429744   -1.679218   -0.264714\n\n";

const NATOMS: &str = " NAtoms=    3 NActive=    3 NUniq=    3 SFac= 1.00D+00\n";

const DIPOLE: &str = " Dipole moment (field-independent basis, Debye):\n    X=             -0.0957    Y=              0.3090    Z=              0.3231  Tot=              0.4572\n";

const MULLIKEN: &str = " Mulliken charges:\n               1\n     1  C   -0.192385\n     2  H    0.154674\n     3  H    0.186246\n Sum of Mulliken charges =   0.148535\n";

const FREQUENCIES: &str = " Frequencies --     30.7923                46.2953                54.2757\n Frequencies --   1650.3300              3831.6000              4000.0100\n";

const SYMMETRY: &str = " Rotational symmetry number  1.\n";

const NORMAL_1: &str = " Normal termination of Gaussian 16 at Tue Nov 14 12:53:45 2023.\n\n";

const SCF_1: &str = " SCF Done:  E(RwB97XD) =  -1526.66238486     A.U. after   14 cycles\n";

const SCF_2: &str = " SCF Done:  E(RwB97XD) =  -1526.66937796     A.U. after   13 cycles\n";

const NORMAL_2: &str = " Normal termination of Gaussian 16 at Tue Nov 14 15:58:48 2023.\n";

/// A chained opt+freq log: a finished first job plus a complete final
/// frequency job.
fn completed_log() -> String {
    format!(
        "{BANNER}{ROUTE}{CHARGE}{SCF_1}\n{NORMAL_1}{CHARGE}{SCF_2}{NATOMS}{GEOMETRY}{FREQUENCIES}{DIPOLE}{MULLIKEN}{SYMMETRY}{NORMAL_2}"
    )
}

// rq-799f6155
#[test]
fn version_banner_is_detected() {
    assert!(GaussianParser
        .detect(" Entering Gaussian System\n Gaussian 16, Revision B.01,\n")
        .is_some());
}

// rq-9f5aa509
#[test]
fn unrelated_head_is_not_claimed() {
    assert!(GaussianParser
        .detect("Some other program's fine output\nnothing here\n")
        .is_none());
}

// rq-d8a0968b
#[test]
fn opt_freq_log_yields_final_job_observables() {
    let record = parse(&completed_log());
    assert_eq!(record.status, RecordStatus::Ok);
    assert_eq!(
        record.program_version.as_deref(),
        Some("Gaussian 16 Rev. B.01")
    );
    assert_eq!(record.command.as_deref(), Some("wb97xd 6-31++g** opt freq"));
    assert_eq!(record.charge, Some(0));
    assert_eq!(record.multiplicity, Some(1));
    assert_eq!(record.electronic_energy, Some(-1526.66937796));
    let geometry = record.geometry.expect("geometry");
    assert_eq!(geometry.symbols, ["C", "H", "H"]);
    assert_eq!(geometry.coordinates[0], -4.061711);
    assert_eq!(
        record.vibrational_frequencies,
        Some(vec![30.7923, 46.2953, 54.2757, 1650.33, 3831.6, 4000.01])
    );
    assert_eq!(
        record.mulliken_charges,
        Some(vec![-0.192385, 0.154674, 0.186246])
    );
    assert_eq!(record.rotational_symmetry_number, Some(1));
    let dipole = record.dipole_moment.expect("dipole");
    for (component, debye) in dipole.iter().zip([-0.0957, 0.3090, 0.3231]) {
        assert!(
            (component - debye / 2.541746473).abs() < 1e-12,
            "{component} vs {debye}"
        );
    }
}

// rq-bf318b99
#[test]
fn last_scf_done_wins() {
    let record = parse(&completed_log());
    assert_eq!(record.electronic_energy, Some(-1526.66937796));
}

// rq-c7886a80
#[test]
fn imaginary_frequency_keeps_its_sign() {
    let content = completed_log().replace(
        FREQUENCIES.trim_end(),
        "Frequencies --   -105.2345               46.2953                54.2757",
    );
    let record = parse(&content);
    let frequencies = record.vibrational_frequencies.expect("frequencies");
    assert_eq!(&frequencies[..3], &[-105.2345, 46.2953, 54.2757]);
}

// rq-363a8a65 rq-c37111b1
#[test]
fn printed_trivial_modes_are_dropped_from_the_table() {
    let content = completed_log().replace(
        "Frequencies --     30.7923                46.2953                54.2757",
        "Frequencies --       0.0000                -0.0000               54.2757",
    );
    let record = parse(&content);
    assert_eq!(
        record.vibrational_frequencies,
        Some(vec![54.2757, 1650.33, 3831.6, 4000.01])
    );
}

// rq-363a8a65 rq-c37111b1
#[test]
fn a_frequency_table_of_only_trivial_modes_leaves_the_field_absent() {
    let content = completed_log().replace(
        FREQUENCIES.trim_end(),
        "Frequencies --       0.0000                -0.0000                0.0000",
    );
    let record = parse(&content);
    assert_eq!(record.vibrational_frequencies, None);
}

// rq-363a8a65
#[test]
fn only_the_last_frequency_job_is_stored() {
    let earlier = " Frequencies --    100.0000               200.0000\n";
    let final_job = " Frequencies --    300.0000               400.0000\n";
    let content = format!(
        "{BANNER}{ROUTE}{CHARGE}{SCF_1}\n{NORMAL_1}\
         {CHARGE}{SCF_2}{NATOMS}{GEOMETRY}{earlier}{NORMAL_1}\
         {CHARGE}{SCF_2}{NATOMS}{GEOMETRY}{final_job}{SYMMETRY}{NORMAL_2}"
    );
    let record = parse(&content);
    assert_eq!(
        record.vibrational_frequencies,
        Some(vec![300.0, 400.0]),
        "an earlier frequency job must not concatenate into the tail"
    );
}

// rq-e6338052
#[test]
fn error_termination_is_failed() {
    let content = completed_log().replace(
        NORMAL_2.trim_end(),
        "Error termination via Lnk1e in /opt/g16/l999.exe at Tue Nov 14 16:00:01 2023.",
    );
    let record = parse(&content);
    assert_eq!(record.status, RecordStatus::Failed);
    assert!(record
        .error
        .as_deref()
        .is_some_and(|e| e.contains("Error termination via Lnk1e")));
    assert_eq!(record.electronic_energy, Some(-1526.66937796));
    assert!(record.geometry.is_some());
}

// rq-b721514a
#[test]
fn cut_off_final_job_is_truncated() {
    let content = completed_log().replace(
        &format!("{SYMMETRY}{NORMAL_2}"),
        " Intensivities and depolarization ratios for compound\n",
    );
    let record = parse(&content);
    assert_eq!(record.status, RecordStatus::Truncated);
    assert!(record
        .error
        .as_deref()
        .is_some_and(|e| e.contains("termination line")));
}

// rq-2d633569
#[test]
fn single_point_log_reports_no_frequencies() {
    let content = format!("{BANNER}{ROUTE}{CHARGE}{SCF_1}{NATOMS}{GEOMETRY}{NORMAL_1}");
    let record = parse(&content);
    assert_eq!(record.status, RecordStatus::Ok);
    assert_eq!(record.electronic_energy, Some(-1526.66238486));
    assert_eq!(record.vibrational_frequencies, None);
}

// rq-58995f44
#[test]
fn hidden_dipole_section_leaves_dipole_absent() {
    let full = completed_log();
    let dipole_at = full
        .find(" Dipole moment (field-independent basis, Debye):")
        .expect("anchor");
    let mulliken_at = full.find(" Mulliken charges:").expect("anchor");
    let (head, rest) = full.split_at(dipole_at);
    let (hidden, tail) = rest.split_at(mulliken_at - dipole_at);
    let content = format!("{head}{hidden}{tail}");
    let mut cursor = Cursor::new(content.as_bytes());
    let windows = FileWindows::from_seekable(&mut cursor, head.len(), tail.len()).expect("windows");
    assert!(!windows.covered(), "the dipole section must be elided");
    let record = GaussianParser.parse(&windows);
    assert_eq!(record.dipole_moment, None);
    assert_eq!(record.electronic_energy, Some(-1526.66937796));
    assert!(record.mulliken_charges.is_some());
}

// rq-5e5adfe9
#[test]
fn corpus_fixture_conforms() {
    let record = ParserRegistry::standard()
        .parse_file(std::path::Path::new(
            "tests/fixtures/gaussian/2cha-020_1_0-str3.log",
        ))
        .expect("parse");
    assert_eq!(record.status, RecordStatus::Ok);
    assert_eq!(record.program.as_deref(), Some("gaussian"));
    assert_eq!(
        record.program_version.as_deref(),
        Some("Gaussian 16 Rev. B.01")
    );
    assert_eq!(
        record.command.as_deref(),
        Some("wb97xd 6-31++g** opt freq gfinput IOP(6/7=1)")
    );
    assert_eq!(record.electronic_energy, Some(-1526.66937796));
    assert_eq!(record.geometry.expect("geometry").symbols.len(), 52);
    assert_eq!(
        record.vibrational_frequencies.expect("frequencies").len(),
        150
    );
    assert_eq!(record.rotational_symmetry_number, Some(1));
    let dipole = record.dipole_moment.expect("dipole");
    for (component, expected) in dipole.iter().zip([
        -0.02333041498431146,
        0.1753125281114534,
        0.11318988854951782,
    ]) {
        assert!(
            (component - expected).abs() < 1e-8,
            "{component} vs {expected}"
        );
    }
}
