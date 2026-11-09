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
     \x20  3:       0.00 cm**-1\n\
     \x20  4:       0.00 cm**-1\n\
     \x20  5:       0.00 cm**-1\n\
     \x20  6:    1595.77 cm**-1\n\
     \x20  7:    1650.33 cm**-1\n\
     \x20  8:    3831.60 cm**-1\n\
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
    // The banner pattern matches only the second line, so the offset the
    // parser reports is that line's byte start, not the head's start.
    let head = "                                        ****ORCA*****\n   * O   R   C   A *\n";
    let banner_offset = "                                        ****ORCA*****\n".len();
    assert_eq!(OrcaParser.detect(head), Some(banner_offset));
}

// rq-5f31c98f
#[test]
fn unrelated_head_is_not_claimed() {
    assert_eq!(
        OrcaParser.detect("Some other program's fine output\nnothing here\n"),
        None
    );
}

// rq-eb7784d8
#[test]
fn terminated_opt_freq_run_yields_full_observable_set() {
    let record = parse(&completed_log());
    assert_eq!(record.status, RecordStatus::Ok);
    assert_eq!(record.program_version.as_deref(), Some("6.0.0"));
    assert_eq!(record.command.as_deref(), Some("wB97x-3c Opt Freq"));
    assert_eq!(record.charge, Some(0));
    assert_eq!(record.multiplicity, Some(1));
    assert_eq!(record.electronic_energy, Some(-76.402725158737));
    let geometry = record.geometry.expect("geometry");
    assert_eq!(geometry.symbols, ["O", "H", "H"]);
    assert_eq!(geometry.coordinates[2], 0.11779);
    assert_eq!(
        record.vibrational_frequencies,
        Some(vec![1595.77, 1650.33, 3831.6])
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
fn command_requires_an_echoed_input_line() {
    // Remove the real echo (`|  1> ! …`) and drop the `!` keyword onto a
    // non-echo line: with no echoed input block the command must be
    // absent, not scraped from the stray `!`.
    let with_stray =
        completed_log().replace("|  1> ! wB97x-3c Opt Freq\n", "! wB97x-3c Opt Freq\n");
    assert_eq!(
        parse(&with_stray).command,
        None,
        "a `!` outside the input echo must not yield a command"
    );
    // Control: the same keyword line on an echo line is recognized whole.
    assert_eq!(
        parse(&completed_log()).command.as_deref(),
        Some("wB97x-3c Opt Freq")
    );
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
    let content = completed_log().replace("   6:    1595.77 cm**-1", "   6:     -42.13 cm**-1");
    let record = parse(&content);
    let frequencies = record.vibrational_frequencies.expect("frequencies");
    assert!(frequencies.contains(&-42.13), "{frequencies:?}");
}

// rq-f11e09b1
#[test]
fn trivial_modes_are_dropped_and_a_genuine_imaginary_mode_is_kept() {
    let section = "VIBRATIONAL FREQUENCIES\n\
         -----------------------\n\
         \x20  0:       0.00 cm**-1\n\
         \x20  1:       0.00 cm**-1\n\
         \x20  2:       0.00 cm**-1\n\
         \x20  3:       0.00 cm**-1\n\
         \x20  4:       0.00 cm**-1\n\
         \x20  5:       0.00 cm**-1\n\
         \x20  6:     -42.13 cm**-1\n\
         \x20  7:      38.55 cm**-1\n\
         \x20  8:    3641.12 cm**-1\n\
         -----------------------\n\
         IR SPECTRUM\n\n";
    let content = completed_log().replace(FREQUENCIES, section);
    let record = parse(&content);
    assert_eq!(
        record.vibrational_frequencies,
        Some(vec![-42.13, 38.55, 3641.12])
    );
}

// rq-d3ed1e6c
#[test]
fn mulliken_rows_of_both_symbol_spellings_yield_every_charge() {
    // ORCA prints the symbol in a two-character field ending at the
    // colon: a two-character symbol carries the colon glued to it, a
    // one-character symbol leaves it spaced. The summary line ends the
    // table.
    let section = "MULLIKEN ATOMIC CHARGES\n\
         -----------------------\n\
         \x20  0 Na:    0.861759\n\
         \x20  1 I :   -0.797525\n\
         \x20  2 H :    0.349900\n\
         Sum of atomic charges:    0.414134\n\n";
    let content = completed_log().replace(MULLIKEN, section);
    let record = parse(&content);
    assert_eq!(
        record.mulliken_charges,
        Some(vec![0.861759, -0.797525, 0.349900])
    );
}

// rq-48469bb2
#[test]
fn a_frequency_section_of_only_trivial_modes_leaves_the_field_absent() {
    let section = "VIBRATIONAL FREQUENCIES\n\
         -----------------------\n\
         \x20  0:       0.00 cm**-1\n\
         \x20  1:       0.00 cm**-1\n\
         \x20  2:       0.00 cm**-1\n\
         \x20  3:       0.00 cm**-1\n\
         \x20  4:       0.00 cm**-1\n\
         \x20  5:       0.00 cm**-1\n\
         -----------------------\n\
         IR SPECTRUM\n\n";
    let content = completed_log().replace(FREQUENCIES, section);
    let record = parse(&content);
    assert_eq!(record.vibrational_frequencies, None);
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
    assert_eq!(record.command.as_deref(), Some("wB97x-3c Opt Freq"));
    assert_eq!(record.electronic_energy, Some(-199.642065483666));
    assert_eq!(record.geometry.expect("geometry").symbols.len(), 34);
    assert_eq!(
        record.vibrational_frequencies.expect("frequencies").len(),
        96
    );
    assert_eq!(record.rotational_symmetry_number, Some(1));
}

// rq-82f9c544
#[test]
fn elided_frequency_header_falls_back_to_visible_rows() {
    // Three-atom run whose last VIBRATIONAL FREQUENCIES header sits in
    // the head window with six rows visible below it; the section's
    // remaining rows open the tail window.
    let head = format!(
        "{}{ECHO}{SUMMARY}{GEOMETRY}{ENERGY}\
         VIBRATIONAL FREQUENCIES\n\
         \x20-----------------------\n\
         \x20Scaling factor for frequencies =  1.000000000  (already applied!)\n\
         \x20\n\
         \x20  0:       0.00 cm**-1\n\
         \x20  1:       0.00 cm**-1\n\
         \x20  2:       0.00 cm**-1\n\
         \x20  3:    1601.00 cm**-1\n\
         \x20  4:    1602.00 cm**-1\n\
         \x20  5:    1603.00 cm**-1\n",
        banner()
    );
    let middle = " filler line\n".repeat(200);
    let tail = format!(
        "\x20  6:    1604.00 cm**-1\n\
         \x20  7:    1605.00 cm**-1\n\
         \x20  8:    1606.00 cm**-1\n\
         \x20-----------------------\n\
         IR SPECTRUM\n\
         {}{}\n",
        " filler line\n".repeat(50),
        "                    ****ORCA TERMINATED NORMALLY****"
    );
    let content = format!("{head}{middle}{tail}");
    let mut cursor = Cursor::new(content.as_bytes());
    let windows =
        FileWindows::from_seekable(&mut cursor, head.len(), tail.len()).expect("uncovered");
    let record = OrcaParser.parse(&windows);
    assert_eq!(record.status, RecordStatus::Ok);
    assert_eq!(record.geometry.expect("geometry").symbols.len(), 3);
    assert_eq!(
        record.vibrational_frequencies,
        Some(vec![1601.0, 1602.0, 1603.0, 1604.0, 1605.0, 1606.0])
    );
}

// rq-7ab0f27f
#[test]
fn seam_cut_frequency_section_is_stored_truncated() {
    // Three-atom run: only trivial rows are visible in the head below
    // the header and one genuine row opens the tail — a post-filter
    // count of one, short of the 3N-6 = 3 the section must print.
    let head = format!(
        "{}{ECHO}{SUMMARY}{GEOMETRY}{ENERGY}\
         VIBRATIONAL FREQUENCIES\n\
         \x20-----------------------\n\
         \x20Scaling factor for frequencies =  1.000000000  (already applied!)\n\
         \x20\n\
         \x20  0:       0.00 cm**-1\n\
         \x20  1:       0.00 cm**-1\n\
         \x20  2:       0.00 cm**-1\n",
        banner()
    );
    let middle = " filler line\n".repeat(200);
    let tail = format!(
        "\x20  3:    1601.00 cm**-1\n\
         \x20-----------------------\n\
         {}{}\n",
        " filler line\n".repeat(50),
        "                    ****ORCA TERMINATED NORMALLY****"
    );
    let content = format!("{head}{middle}{tail}");
    let mut cursor = Cursor::new(content.as_bytes());
    let windows =
        FileWindows::from_seekable(&mut cursor, head.len(), tail.len()).expect("uncovered");
    let record = OrcaParser.parse(&windows);
    assert_eq!(record.status, RecordStatus::Truncated);
    assert!(record
        .error
        .as_deref()
        .is_some_and(|error| error.contains("frequency table may be incomplete")));
    assert_eq!(record.vibrational_frequencies, Some(vec![1601.0]));
}

// rq-7bca16e5
#[test]
fn scf_energy_is_the_scf_total_distinct_from_the_final_single_point_energy() {
    // The summary box of newer printings.
    let content = completed_log().replace(
        ENERGY.trim_end(),
        "| TOTAL ENERGY              -28.322836452494 Eh   |\n\
         FINAL SINGLE POINT ENERGY       -28.322836452490",
    );
    let record = parse(&content);
    assert_eq!(record.scf_energy, Some(-28.322836452494));
    assert_eq!(record.electronic_energy, Some(-28.322836452490));
    // The section-end line of older ones; the last section wins.
    let content = completed_log().replace(
        ENERGY.trim_end(),
        "----------------\n\
         TOTAL SCF ENERGY\n\
         ----------------\n\
         \n\
         Total Energy       :       -199.58434234097697 Eh           -5430.96606 eV\n\
         \n\
         FINAL SINGLE POINT ENERGY      -199.634522052117\n\
         \n\
         ----------------\n\
         TOTAL SCF ENERGY\n\
         ----------------\n\
         \n\
         Total Energy       :       -199.59380288816905 Eh           -5431.22349 eV\n\
         \n\
         FINAL SINGLE POINT ENERGY      -199.642065483666",
    );
    let record = parse(&content);
    assert_eq!(record.scf_energy, Some(-199.59380288816905));
    assert_eq!(record.electronic_energy, Some(-199.642065483666));
}

// rq-97f5385b
#[test]
fn a_job_without_an_scf_section_carries_no_scf_energy() {
    let record = parse(&completed_log());
    assert_eq!(record.scf_energy, None);
    // The per-iteration echo is not a SCF total in any spelling.
    let content = completed_log().replace(
        ENERGY.trim_end(),
        "Total energy after final integration           :    -76.402725158 Eh\n\
         FINAL SINGLE POINT ENERGY       -76.402725158737",
    );
    let record = parse(&content);
    assert_eq!(record.scf_energy, None);
    assert_eq!(record.electronic_energy, Some(-76.402725158737));
}

// rq-23b2c5e0
#[test]
fn the_scf_total_line_test_admits_exactly_the_two_spellings() {
    // Both genuine spellings print among near-miss lines — an
    // un-finalized box row (no Eh), an unboxed TOTAL row, and a colonless
    // Total row — and the later genuine spelling's value wins; the
    // near-misses contribute nothing.
    let content = completed_log().replace(
        ENERGY.trim_end(),
        "| TOTAL ENERGY              -28.322836452494 Eh   |\n\
         | TOTAL ENERGY              -28.500000000000      |\n\
         TOTAL ENERGY               -28.6 Eh\n\
         Total Energy              -28.7 Eh\n\
         Total Energy    :     -28.422836452494 Eh\n\
         FINAL SINGLE POINT ENERGY       -28.422836452490",
    );
    let record = parse(&content);
    assert_eq!(record.scf_energy, Some(-28.422836452494));
    assert_eq!(record.electronic_energy, Some(-28.422836452490));
}

// rq-82f9c544 rq-15b1763d
#[test]
fn an_elided_header_fallback_stops_at_the_first_non_row_gap() {
    // The whole-tail gather runs by literal scan, but its answer stays
    // the line walk's own: rows stop at the first non-row line, so a
    // later run of rows in the same window — a thermo table's cm**-1
    // column — contributes nothing.
    let head = format!(
        "{}{ECHO}{SUMMARY}{GEOMETRY}{ENERGY}\
         VIBRATIONAL FREQUENCIES\n\
         \x20-----------------------\n\
         \x20\n\
         \x20  0:       0.00 cm**-1\n",
        banner()
    );
    let middle = " filler line\n".repeat(200);
    let tail = format!(
        "\x20  1:     1601.00 cm**-1\n\
         \x20  2:     1602.00 cm**-1\n\
         \x20 an ordinary line between the two row runs\n\
         \x20  3:     1603.00 cm**-1\n\
         \x20  4:     1604.00 cm**-1\n\
         {}{}\n",
        " filler line\n".repeat(50),
        "                    ****ORCA TERMINATED NORMALLY****"
    );
    let content = format!("{head}{middle}{tail}");
    let mut cursor = std::io::Cursor::new(content.as_bytes());
    let windows =
        FileWindows::from_seekable(&mut cursor, head.len(), tail.len()).expect("uncovered");
    let record = OrcaParser.parse(&windows);
    assert_eq!(record.vibrational_frequencies, Some(vec![1601.0, 1602.0]));
}
