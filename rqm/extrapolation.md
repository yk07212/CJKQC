# Feature: Extrapolation Corrections (`--extrapolate`) <!-- rq-b0a418c4 -->

An electronic structure calculation is never the final word on an energy:
the complete-basis-set limit and the tighter calculation settings are both
reachable from the jobs a JKCS workflow already ran, by combining
same-named single points instead of recomputing anything. CJKQC computes
such energies with `--extrapolate`, mirroring JKCS2.1's JKextrapolate: the
flag takes a mode token, the source databases, and the mode's parameters;
every source record matching a run record's `name` contributes to that
record's extrapolated energy, which attaches as an ordinary correction
object (`rqm/correction.md`). From there the corrected machinery applies
unchanged: the `elcorr`/`hcorr`/`gcorr` columns and cutoffs, the corrected
formation and hydration sections, the population levels, and the
completion lines' corrected suffix.

The record schema carries what extrapolation consumes: `electronic_energy`
everywhere, and `scf_energy` where the program prints one
(`rqm/record.md`, `rqm/parsers/`). The correlation-like residual a record
contributes is `electronic_energy − scf_energy` — the method's correlation
energy as the program books it, including any additive corrections folded
into the final energy. No correlation energy is stored or read.

## The Invocation <!-- rq-24294fd5 -->

`--extrapolate` takes the mode token, the source database paths, and the
mode's numeric parameters as space-separated values, in JKextrapolate's
order. Like `--pop`, the flag eats the values that follow it until the
next `--`-flag, so positional inputs given after it unquoted are eaten
too — place `--extrapolate` after the positional inputs. One
`--extrapolate` per run; repeating it, or giving it alongside
`--add-correction`, is a usage error (exit 2) — a record carries one
correction.

- `--extrapolate 2p LOW HIGH N ALPHA BETA` — two-point basis-set
  extrapolation to the complete-basis-set limit over two databases: the
  lower cardinal number's single points in LOW, the next cardinal
  number's in HIGH. N is the lower cardinal number of the pair (an
  integer ≥ 1), ALPHA the SCF exponent (α > 0), and BETA the correlation
  exponent (β > 0); the original tool cites
  https://doi.org/10.1021/ct100396y for their determination. Produces
  `kind` `cbs`.
- `--extrapolate 2s LOW HIGH F` — the CPS/LAF settings blend over two
  databases that differ in calculation settings at fixed basis, with F
  the blend fraction (a finite number; JKCS's calibrated values are
  typically near 1.5). Produces `kind` `cps`.
- `--extrapolate 2p LOWPAIR HIGHPAIR N ALPHA BETA F` — the combined form:
  LOWPAIR and HIGHPAIR are comma-joined path pairs, LOWPAIR holding the
  low basis's records at the low and then the high settings, HIGHPAIR the
  high basis's records in the same order. One two-point extrapolation
  runs per settings level and the two results blend (The Combined Form),
  so F is required. Produces `kind` `cbs-cps`.

Every value must parse, and the shapes are strict: an unknown mode, a
wrong value count for the mode, a non-numeric or non-integer parameter, N
below 1, ALPHA or BETA at or below zero, a non-finite F, a comma-joined
group with more than two entries, a comma-joined group where a single
path is required, or a trailing F without two comma-joined pairs is a
usage error (exit 2), reported with a message naming the mode and the
shape it expects. Usage errors happen before any input is read.

The `2pf`/`2sf` spellings of the original tool have no counterpart:
writing the result is `--out`'s job. A path containing a comma cannot be
expressed in the combined form.

## The Formulas <!-- rq-bb43d915 -->

All energies are hartree; the results are exact double-precision
evaluations of:

- **Two-point extrapolation** over points X (LOW, cardinal N) and Y
  (HIGH, cardinal N+1), with e_X = e^(−α√N) and e_Y = e^(−α√(N+1)):

      E_CBS = (E_scf_X·e_Y − E_scf_Y·e_X)/(e_Y − e_X)
            + (N^β·E_corr_X − (N+1)^β·E_corr_Y)/(N^β − (N+1)^β)

  The SCF term extrapolates the records' `scf_energy`; the correlation
  term extrapolates the residual `electronic_energy − scf_energy`. For a
  DFT record the residual is 0 and the SCF term carries the whole energy
  — the correct degenerate case. A record whose stored
  `electronic_energy` is itself the SCF value has the same zero residual
  and extrapolates through the SCF term alone: a post-HF Gaussian record,
  whose parser reads only the `SCF Done` line
  (`rqm/parsers/gaussian.md`), carries the SCF energy in both fields, so
  its extrapolation is an SCF-limit energy, not a correlated one. The
  extrapolation never recovers a correlation energy the record does not
  store.
- **CPS/LAF blend**: `E = E_low + F·(E_high − E_low)` over the records'
  `electronic_energy`.
- **The Combined Form**: E_CBS_low = the two-point extrapolation of the
  first entries of both pairs (the low basis and the high basis, both at
  the low settings); E_CBS_high = the two-point extrapolation of the
  second entries (both at the high settings); and
  `E = E_CBS_low + F·(E_CBS_high − E_CBS_low)`. Each two-point
  extrapolation compares basis sizes at fixed settings; the blend moves
  between settings levels at the CBS limit.

A non-finite result leaves the record without a correction and is warned
like any skip — it is never written as an infinite or NaN energy
(`rqm/record.md` validation rejects one).

## Candidates and Matching <!-- rq-c68c02b0 -->

- A **source candidate** is a source-database record with status `ok`, a
  present `name`, a present `electronic_energy`, and — in every mode that
  extrapolates over bases (`2p`, both forms) — a present `scf_energy`. A
  source record failing any of these is skipped and warned individually,
  naming its name (`(unnamed)` when absent) and the reason, like a
  skipped correction record (`rqm/correction.md`).
- Two candidates with equal names within one source database are fatal
  (exit 1), naming the name and the database; nothing is written.
  Non-candidates never collide — a failed rerun collected into the same
  database as its ok replacement does not block the attachment.
- The contributors of one name must come from one program at one version:
  any two candidates that share a name present in every source database
  and whose present `program` values differ, or whose present
  `program_version` values differ, make the run fatal (exit 1), naming
  the name and the differing tokens; nothing is written. The check is
  unconditional: it applies whether or not a run record names the name.
  Programs and versions compare as exact strings (`rqm/record.md`'s
  lowercase program vocabulary, the version as the program prints it); a
  candidate without a `program` or without a `program_version` never
  violates — only present-and-different values do. Extrapolating across
  programs — or across versions of one program, whose implementations and
  defaults change between releases — mixes energies that answer to
  different Hamiltonians; the check keeps such numbers from being produced
  silently.
- The contributors of one name must be distinct calculations: any two
  candidates that share a name present in every source database and whose
  `electronic_energy` values are exactly equal
  (the same double, byte-identical in the database line) make the run
  fatal (exit 1), naming the name and the two source databases; nothing
  is written. Two genuinely different calculations do not agree to the
  last bit, so equality means the same calculation entered the sources
  twice — one file collected into two databases, or one path given for
  two arguments (which includes passing the same path for LOW and HIGH).
  The check covers the electronic energies only: the combined form's
  within-basis pairs legitimately share the SCF energy, since the settings
  levels differ in the correlation treatment and not in the SCF cycle, so
  `scf_energy` equality is never a violation.
- Source databases load through `load_database` (`rqm/database.md`),
  never classified like the positional inputs. A missing or unreadable
  source is fatal (exit 1). A line that fails to load is skipped and
  reported like a corrupt merged line — a warning naming the path and
  line number, and exit 1 — while the valid records still participate. A
  source that loads no record at all while reporting load problems is
  fatal instead: the error is reported and nothing is written.
- A run record receives the extrapolated correction iff its `name`
  appears as a candidate in **every** source database of the invocation.
  Matching is exact, case-sensitive equality of `name`; every run record
  sharing a fully covered name receives the correction, whatever its
  status (`rqm/correction.md`'s attachment rule). A run record whose name
  is not fully covered is skipped with a warning naming the name and the
  first source database missing it — a silent partial coverage in a
  four-database workflow is the failure mode worth screaming about. A run
  record that already carries a correction has it replaced on a match; an
  uncovered record keeps whatever correction it stores, so re-running an
  extrapolated database only adds and updates.
- Candidates that no run record names are reported as one aggregate
  warning with their count, like `--add-correction`'s. Warnings never
  change the exit code except the load-problem rule above.
- The run's records are enriched in memory; `--out` writes the enriched
  records, print flags show the corrected values, and without `--out`
  nothing is stored (the general rule, `rqm/filtering.md`).

## The Attached Correction <!-- rq-9e38e14b -->

The correction object follows `rqm/correction.md`'s schema:

- `energy` — the mode's result in hartree.
- `kind` — `cbs` (two-point), `cps` (blend), `cbs-cps` (combined form).
- `commands` — the contributing records' `command` values in contributor
  order — LOW then HIGH for `2p` and `2s`; the low-settings pair then the
  high-settings pair for the combined form — omitting the records that
  carry no command; the field is absent when no contributor has one.
- `program` and `program_version` — the last contributor's values (the
  HIGH database for `2p` and `2s`, the high-basis-at-high-settings record
  for the combined form), absent when it has none; the uniformity rules
  guarantee the pair agrees with every present contributor's value.

## Pipeline Semantics <!-- rq-c7255c36 -->

- `--extrapolate` is a manipulation flag (`rqm/filtering.md`): the
  enrichment precedes every pipeline stage, so cutoffs, sorts, and the
  corrected sections see the extrapolated values, and the flag
  participates in the visible-outcome rule — a run with `--extrapolate`
  but neither `--out` nor a print flag is a usage error (exit 2).
- A run whose only manipulation is `--extrapolate` and that prints
  nothing completes like a collection run — every record written, the
  collected or merged summary line — with the corrected suffix:
  `collected <n> records into <out> (<f> failed, <t> truncated, <k>
  corrected)` or `merged <n> records into <out> (<k> corrected)`, `<k>`
  counting the written records that carry a correction, stored ones
  included. Manipulation runs print the `filtered` line with the same
  suffix.
- `--extrapolate` adds no column and prints nothing of its own: like
  `--add-correction`, it only makes the corrected values defined; which of
  them surface is entirely the user's choice of print flags.
- All corrected-value plumbing of `rqm/correction.md` applies to
  extrapolated corrections unchanged: the `elcorr`/`hcorr`/`gcorr`
  registry keys and columns, the `elcorr`/`gcorr` cutoffs and sorts, the
  `--correction` provenance column, the corrected formation and hydration
  sections, and the population levels.

## Feature API <!-- rq-306e7b5d -->

### Types <!-- rq-57568392 -->

- `ExtrapolationSpec` — one parsed `--extrapolate` invocation: <!-- rq-2b4ba772 -->
  `mode: ExtrapMode`.
- `ExtrapMode` — the three forms: <!-- rq-e4c551ee -->
  - `TwoPoint { low: PathBuf, high: PathBuf, cardinal: u32, alpha: f64,
    beta: f64 }`
  - `SettingsBlend { low: PathBuf, high: PathBuf, fraction: f64 }`
  - `TwoPointBlended { low_pair: (PathBuf, PathBuf), high_pair:
    (PathBuf, PathBuf), cardinal: u32, alpha: f64, beta: f64, fraction:
    f64 }` — `low_pair` holds the low basis at (low settings, high
    settings), `high_pair` the high basis in the same order.
- `Extrapolation` — what a pass did, mirroring `Attachment` <!-- rq-60ca0795 -->
  (`rqm/correction.md`): `records: Vec<Record>` (enriched copies, input
  order; the input slice is never mutated), `attached: usize` (records
  that received an extrapolated correction), `skipped: usize` (rejected
  source candidates), `uncovered: usize` (run records whose name is not
  fully covered), `unmatched: usize` (source candidates no run record
  names).
- `ExtrapolationError` — error type of the pass: <!-- rq-ae61b293 -->
  - `SourceCount { expected: usize, actual: usize }` — the source slice
    list does not hold the number of databases the mode combines.
  - `DuplicateName { source: usize, name: String }` — two candidates
    share the name in the source at index `source`.
  - `ProgramMismatch { name: String, source_a: usize, source_b: usize,
    program_a: String, program_b: String }` — two contributors of one
    name carry different present programs.
  - `VersionMismatch { name: String, source_a: usize, source_b: usize,
    version_a: String, version_b: String }` — two contributors of one
    name carry different present program versions.
  - `DuplicateEnergy { name: String, source_a: usize, source_b: usize }`
    — two contributors of one name carry exactly equal electronic
    energies.

### Functions <!-- rq-eb93937e -->

- `parse_extrapolation(values: &[String]) -> Result<ExtrapolationSpec, String>` <!-- rq-52f7d9a4 -->
  — the invocation grammar and parameter validation; `Err` carries the
  usage message (the CLI prints it and exits 2).
- `two_point_energy(scf_low: f64, corr_low: f64, scf_high: f64, corr_high: <!-- rq-443ba191 -->
  f64, cardinal: u32, alpha: f64, beta: f64) -> f64` — the two-point
  formula, SCF term plus correlation term.
- `blend_energy(low: f64, high: f64, fraction: f64) -> f64` — the CPS/LAF <!-- rq-e8016516 -->
  blend.
- `extrapolate_corrections(records: &[Record], sources: &[&[Record]], <!-- rq-a757e0f8 -->
  spec: &ExtrapolationSpec) -> Result<Extrapolation, ExtrapolationError>`
  — pure: builds each source's candidate map in order (duplicate names
  yield `Err`), attaches per the matching rules — including the
  uniformity checks, a contributor set that mixes programs, mixes
  program versions, or repeats a calculation yielding `Err` — and returns
  the enriched records with the
  counts. `sources` supplies the record slices in the
  spec's contributor order: `[low, high]` for `TwoPoint` and
  `SettingsBlend`, `[lowB at low settings, highB at low settings, lowB at
  high settings, highB at high settings]` for `TwoPointBlended`. The list
  holds exactly the mode's number of databases — two, or four for the
  combined form — and any other length yields `Err(SourceCount)`. The CLI
  loads each source with `load_database` (reporting its problems) and
  hands the valid records over.

### Crate Layout <!-- rq-a2299e51 -->

- The grammar, formulas, and pass live in `src/extrapolate.rs`; the
  library root re-exports them. `src/main.rs` declares `--extrapolate`,
  rejects its combinations, wires the load problems, warnings, summary
  suffixes, and exit codes, and places the pass where `--add-correction`'s
  sits. No new dependencies.
- `tests/extrapolate.rs` tests the grammar (modes, arities, parameter <!-- rq-6c98665a -->
  bounds, comma groups), the two formulas against hand-computed reference
  values, the matching rules (coverage across sources, duplicates, skips,
  counts), the uniformity checks (program mismatch, version mismatch,
  duplicate energies, the SCF carve-out, absent provenance), and the
  provenance assembly; CLI coverage (flags, warnings,
  summary lines, columns, exit codes) extends `tests/cli.rs`. The parser
  docs' `scf_energy` scenarios extend the parser suites, and the corpus
  manifest gains `scf_energy` expectations hand-verified against the
  fixtures' printed values (`rqm/parsers/conformance.md`).

## Out of Scope <!-- rq-c78360ba -->

- Byte-compatibility with JKextrapolate's four-file output: the combined
  form pairs its two-point extrapolations by settings level, and every
  contributing file participates.
- Anything but energies: gradients, frequencies, and geometries are never
  extrapolated, and no derived property other than the correction energy
  is.
- Three-point or fitted extrapolations, per-structure parameter
  calibration, and scheme families beyond the three forms: the parameters
  come from the command line only.
- Combining `--extrapolate` with `--add-correction`, or more than one
  `--extrapolate` per run (one correction per record).
- Reading correlation energies from the source databases: the residual
  `electronic_energy − scf_energy` is the only correlation input.
- Extrapolating from the run's own positional records: the sources are
  the flag's databases; a positional input may double as a source by
  path, but the run's records only ever receive corrections.
- Surfacing the combined form's intermediate CBS values: the correction
  carries the final energy alone.

## Gherkin Scenarios <!-- rq-2322ad6b -->

```gherkin
Feature: Extrapolation corrections

  # --- Invocation grammar ---

  @rq-fcd67083
  Scenario: 2p attaches a cbs correction
    Given a database with an ok record named "1am"
    And a low database with an ok "1am" record with scf_energy -76.0, electronic_energy -76.0, and command "! def2-SVP Sp"
    And a high database with an ok "1am" record with scf_energy -76.1, electronic_energy -76.15, program "orca", program_version "6.0.0", and command "! def2-TZVPP Sp"
    When CJKQC is invoked with the database and "--extrapolate 2p low.jsonl high.jsonl 2 4.3 2.51 --out out.jsonl"
    Then out.jsonl loads as 1 record whose correction has energy -76.21251721253894 within 1e-9, kind "cbs", commands ["! def2-SVP Sp", "! def2-TZVPP Sp"], program "orca", and program_version "6.0.0"
    And the record's electronic_energy is still its stored value
    And the exit code is 0

  @rq-8f40cd00
  Scenario: 2s attaches a cps correction
    Given a database with an ok record named "1am"
    And a low database with an ok "1am" record with electronic_energy -76.0
    And a high database with an ok "1am" record with electronic_energy -76.1
    When CJKQC is invoked with the database and "--extrapolate 2s low.jsonl high.jsonl 1.5 --out out.jsonl"
    Then out.jsonl loads as 1 record whose correction has energy -76.15 and kind "cps"

  @rq-0f8f7fcd
  Scenario: The combined form attaches a cbs-cps correction
    Given a database with an ok record named "1am"
    And a low-basis low-settings database with an ok "1am" record with scf_energy -76.0, electronic_energy -76.0, and command "c1"
    And a high-basis low-settings database with an ok "1am" record with scf_energy -76.1, electronic_energy -76.15, and command "c2"
    And a low-basis high-settings database with an ok "1am" record with scf_energy -76.0, electronic_energy -76.02, and command "c3"
    And a high-basis high-settings database with an ok "1am" record with scf_energy -76.1, electronic_energy -76.17, program "orca", program_version "6.0.0", and command "c4"
    When CJKQC is invoked with the database and "--extrapolate 2p lbl.jsonl,lbh.jsonl hbl.jsonl,hbh.jsonl 2 4.3 2.51 1.5 --out out.jsonl"
    Then out.jsonl loads as 1 record whose correction has energy -76.24251721253893 within 1e-9, kind "cbs-cps", commands ["c1", "c2", "c3", "c4"], program "orca", and program_version "6.0.0"

  @rq-4c238015
  Scenario: A DFT record's zero residual extrapolates through the SCF term
    Given a low database with an ok "1am" record with scf_energy -76.0 and electronic_energy -76.0
    And a high database with an ok "1am" record with scf_energy -76.1 and electronic_energy -76.1
    When CJKQC is invoked with the database and "--extrapolate 2p low.jsonl high.jsonl 2 4.3 2.51 --out out.jsonl"
    Then the output record's correction energy is -76.13421861395366 within 1e-9

  @rq-6c7136f0
  Scenario: An unknown mode is a usage error
    When CJKQC is invoked with the database and "--extrapolate 3p low.jsonl high.jsonl 2 4.3 2.51"
    Then the exit code is 2

  @rq-e18bdb98
  Scenario: A wrong value count is a usage error
    When CJKQC is invoked with the database and "--extrapolate 2p low.jsonl high.jsonl 2 4.3"
    Then the exit code is 2
    And the same holds for "--extrapolate 2s low.jsonl high.jsonl"

  @rq-3981f145
  Scenario: A non-numeric parameter is a usage error
    When CJKQC is invoked with the database and "--extrapolate 2p low.jsonl high.jsonl 2 alpha 2.51"
    Then the exit code is 2

  @rq-08954513
  Scenario: Parameter bounds are enforced
    When CJKQC is invoked with the database and "--extrapolate 2p low.jsonl high.jsonl 0 4.3 2.51"
    Then the exit code is 2
    And the same holds for N "2.5", ALPHA "0", BETA "-1", and the 2s F "inf"

  @rq-7728825b
  Scenario: A trailing F without comma pairs is a usage error
    When CJKQC is invoked with the database and "--extrapolate 2p low.jsonl high.jsonl 2 4.3 2.51 1.5"
    Then the exit code is 2

  @rq-1527f9fe
  Scenario: A half-paired combined form is a usage error
    When CJKQC is invoked with the database and "--extrapolate 2p low1.jsonl,low2.jsonl high.jsonl 2 4.3 2.51 1.5"
    Then the exit code is 2

  @rq-17ef3709
  Scenario: An over-full comma group is a usage error
    When CJKQC is invoked with the database and "--extrapolate 2p a.jsonl,b.jsonl,c.jsonl d.jsonl,e.jsonl 2 4.3 2.51 1.5"
    Then the exit code is 2

  @rq-7ecf0b87
  Scenario: A comma pair where a single path is required is a usage error
    When CJKQC is invoked with the database and "--extrapolate 2s low1.jsonl,low2.jsonl high.jsonl 1.5"
    Then the exit code is 2

  @rq-50c53556
  Scenario: Repeating --extrapolate is a usage error
    When CJKQC is invoked with the database and "--extrapolate 2s low.jsonl high.jsonl 1.5 --extrapolate 2s low.jsonl high.jsonl 1.5"
    Then the exit code is 2

  @rq-d576647e
  Scenario: --extrapolate with --add-correction is a usage error
    When CJKQC is invoked with the database and "--add-correction sp.jsonl --extrapolate 2s low.jsonl high.jsonl 1.5"
    Then the exit code is 2

  @rq-c492a677
  Scenario: --extrapolate alone is not a visible outcome
    When CJKQC is invoked with the database and "--extrapolate 2s low.jsonl high.jsonl 1.5" and no --out and no print flag
    Then the exit code is 2

  # --- Candidates and matching ---

  @rq-925967be
  Scenario: A name missing from one source is uncovered
    Given a database with an ok record named "1am"
    And a low database with an ok "1am" candidate
    And a high database with an ok "2am" candidate
    When CJKQC is invoked with the database and "--extrapolate 2p low.jsonl high.jsonl 2 4.3 2.51 --out out.jsonl"
    Then the output record carries no correction
    And a stderr warning names "1am" and "high.jsonl"
    And the exit code is 0

  @rq-0ec3ddc5
  Scenario: Matching is exact and case-sensitive
    Given a database with an ok record named "1am"
    And two source databases whose candidates are all named "1AM"
    When CJKQC is invoked with the database and "--extrapolate 2s low.jsonl high.jsonl 1.5 --out out.jsonl"
    Then the output record carries no correction
    And a stderr warning names "1am"

  @rq-41bd180a
  Scenario: Every run record sharing a covered name receives the correction
    Given a database with two ok records named "1am"
    And two source databases with one "1am" candidate each
    When CJKQC is invoked with the database and "--extrapolate 2s low.jsonl high.jsonl 1.5 --out out.jsonl"
    Then both output records carry the correction

  @rq-7786c735
  Scenario: A run record without a name never matches
    Given a database with an ok record without a name
    And two source databases with an "1am" candidate each
    When CJKQC is invoked with the database and "--extrapolate 2s low.jsonl high.jsonl 1.5 --out out.jsonl"
    Then the output record carries no correction

  @rq-73d8ed95
  Scenario: Duplicate candidate names in one source are fatal
    Given a low database with two ok candidates named "1am" and a high database with an ok "1am" candidate
    When CJKQC is invoked with the database and "--extrapolate 2p low.jsonl high.jsonl 2 4.3 2.51 --out out.jsonl"
    Then the exit code is 1
    And stderr names "1am" and "low.jsonl"
    And out.jsonl does not exist

  @rq-ccc8ed3a
  Scenario: Mixed programs are fatal
    Given a low database with an ok "1am" candidate with program "orca"
    And a high database with an ok "1am" candidate with program "mrcc"
    When CJKQC is invoked with the database and "--extrapolate 2p low.jsonl high.jsonl 2 4.3 2.51 --out out.jsonl"
    Then the exit code is 1
    And stderr names "1am", "orca", and "mrcc"
    And out.jsonl does not exist

  @rq-c4c4e1d7
  Scenario: The combined form rejects one mixed contributor
    Given four source databases whose "1am" candidates all carry program "orca" but the high-basis high-settings one carries program "mrcc"
    When CJKQC is invoked with the database and "--extrapolate 2p lbl.jsonl,lbh.jsonl hbl.jsonl,hbh.jsonl 2 4.3 2.51 1.5 --out out.jsonl"
    Then the exit code is 1
    And stderr names "1am", "orca", and "mrcc"

  @rq-41d43353
  Scenario: A candidate without a program never violates
    Given a low database with an ok "1am" candidate without a program
    And a high database with an ok "1am" candidate with program "orca"
    When CJKQC is invoked with the database and "--extrapolate 2p low.jsonl high.jsonl 2 4.3 2.51 --out out.jsonl"
    Then the output record carries the correction
    And the correction's program is orca

  @rq-5f78f8a6
  Scenario: A version mismatch is fatal
    Given a low database with an ok "1am" candidate with program "orca" and program_version "6.0.0"
    And a high database with an ok "1am" candidate with program "orca" and program_version "6.1.0"
    When CJKQC is invoked with the database and "--extrapolate 2p low.jsonl high.jsonl 2 4.3 2.51 --out out.jsonl"
    Then the exit code is 1
    And stderr names "1am", "6.0.0", and "6.1.0"
    And out.jsonl does not exist

  @rq-7cb01a45
  Scenario: Matching program and version pass the uniformity check
    Given a low database with an ok "1am" candidate with program "orca" and program_version "6.1.0"
    And a high database with an ok "1am" candidate with program "orca" and program_version "6.1.0"
    When CJKQC is invoked with the database and "--extrapolate 2p low.jsonl high.jsonl 2 4.3 2.51 --out out.jsonl"
    Then the output record carries the correction
    And the correction's program_version is 6.1.0

  @rq-edffc76e
  Scenario: Identical electronic energies are fatal
    Given a low database and a high database whose "1am" candidates carry the same electronic energy bit for bit
    When CJKQC is invoked with the database and "--extrapolate 2p low.jsonl high.jsonl 2 4.3 2.51 --out out.jsonl"
    Then the exit code is 1
    And stderr names "1am", "low.jsonl", and "high.jsonl"
    And out.jsonl does not exist

  @rq-28bcea43
  Scenario: The same path given twice is caught by the energy rule
    Given a database with an ok record named "1am"
    And a database at "low.jsonl" with an ok "1am" candidate
    When CJKQC is invoked with the database and "--extrapolate 2p low.jsonl low.jsonl 2 4.3 2.51 --out out.jsonl"
    Then the exit code is 1
    And stderr names "1am"

  @rq-62f744ee
  Scenario: A shared SCF energy inside the combined form is not a violation
    Given a low-basis low-settings database with an ok "1am" candidate with scf_energy -76.0 and electronic_energy -76.05
    And a low-basis high-settings database with an ok "1am" candidate with scf_energy -76.0 and electronic_energy -76.07
    And a high-basis low-settings database with an ok "1am" candidate with scf_energy -76.1 and electronic_energy -76.15
    And a high-basis high-settings database with an ok "1am" candidate with scf_energy -76.1 and electronic_energy -76.17
    When CJKQC is invoked with the database and "--extrapolate 2p lbl.jsonl,lbh.jsonl hbl.jsonl,hbh.jsonl 2 4.3 2.51 1.5 --out out.jsonl"
    Then the output record carries the correction
    And the exit code is 0

  @rq-17e82092
  Scenario: A failed rerun does not collide with its ok replacement
    Given a low database with a failed record named "1am" and an ok record named "1am" with scf and electronic energies
    And a high database with an ok "1am" candidate
    When CJKQC is invoked with the database and "--extrapolate 2p low.jsonl high.jsonl 2 4.3 2.51 --out out.jsonl"
    Then the output record carries the correction
    And a stderr warning names "1am" as skipped

  @rq-69f2f193
  Scenario: A candidate without an electronic energy is skipped with a warning
    Given a low database with an ok "1am" record with scf_energy but no electronic_energy
    And a high database with an ok "1am" candidate
    When CJKQC is invoked with the database and "--extrapolate 2p low.jsonl high.jsonl 2 4.3 2.51 --out out.jsonl"
    Then the output record carries no correction
    And a stderr warning names "1am" as skipped

  @rq-5c26081b
  Scenario: A 2p candidate without scf_energy is skipped with a warning
    Given a low database with an ok "1am" record with electronic_energy but no scf_energy
    And a high database with a fully qualified "1am" candidate
    When CJKQC is invoked with the database and "--extrapolate 2p low.jsonl high.jsonl 2 4.3 2.51 --out out.jsonl"
    Then the output record carries no correction
    And a stderr warning names "1am"
    And the same holds when the high database's candidate lacks scf_energy

  @rq-b92f7576
  Scenario: 2s needs no scf_energy
    Given two source databases whose "1am" candidates carry electronic energies but no scf_energy
    When CJKQC is invoked with the database and "--extrapolate 2s low.jsonl high.jsonl 1.5 --out out.jsonl"
    Then the output record carries the correction

  @rq-3bad508e
  Scenario: Candidates no run record names are one aggregate warning
    Given a database with an ok record named "1am"
    And two source databases that also hold an ok candidate named "2am" each
    When CJKQC is invoked with the database and "--extrapolate 2s low.jsonl high.jsonl 1.5 --out out.jsonl"
    Then the output record carries the correction
    And a stderr warning reports 2 unmatched extrapolation records

  @rq-61cdb639
  Scenario: A corrupt line in a source is skipped and fails the run
    Given a low database with a valid candidate on line 1 and an invalid line 2
    And a high database with a valid candidate
    When CJKQC is invoked with the database and "--extrapolate 2s low.jsonl high.jsonl 1.5 --out out.jsonl"
    Then the exit code is 1
    And a stderr warning names "low.jsonl" and line 2
    And out.jsonl loads with the correction attached from line 1

  @rq-ae11003d
  Scenario: A missing source is fatal
    Given no file exists at "high.jsonl"
    When CJKQC is invoked with the database and "--extrapolate 2s low.jsonl high.jsonl 1.5 --out out.jsonl"
    Then the exit code is 1
    And out.jsonl does not exist

  @rq-03c8c0f9
  Scenario: A source that is a program output is fatal
    Given the file "high.log" holds a QC program output
    When CJKQC is invoked with the database and "--extrapolate 2s low.jsonl high.log 1.5 --out out.jsonl"
    Then the exit code is 1
    And out.jsonl does not exist

  @rq-9be77472
  Scenario: An empty source leaves every named record uncovered
    Given a database with an ok record named "1am"
    And an empty but valid low database
    And a high database with an ok "1am" candidate
    When CJKQC is invoked with the database and "--extrapolate 2s low.jsonl high.jsonl 1.5 --out out.jsonl"
    Then the output record carries no correction
    And a stderr warning names "1am" and "low.jsonl"
    And the exit code is 0

  @rq-d516398b
  Scenario: A match replaces a stored correction
    Given a database whose ok record named "1am" carries a correction with energy -76.0
    And two source databases with an "1am" candidate each
    When CJKQC is invoked with the database and "--extrapolate 2s low.jsonl high.jsonl 1.5 --out out.jsonl"
    Then the output record's correction energy is -76.15

  @rq-54c33c26
  Scenario: An uncovered record keeps its stored correction
    Given a database whose ok record named "2am" carries a correction with energy -76.0
    And two source databases with an "1am" candidate each
    When CJKQC is invoked with the database and "--extrapolate 2s low.jsonl high.jsonl 1.5 --out out.jsonl"
    Then the output record's correction energy is still -76.0

  # --- Provenance ---

  @rq-00078b6e
  Scenario: The --correction column shows an extrapolated correction's provenance
    Given a database with an ok record named "1am"
    And two source databases whose "1am" candidates carry commands "! def2-SVP Sp" and "! def2-TZVPP Sp" and program "orca" with version "6.0.0"
    When CJKQC is invoked with the database and "--extrapolate 2p low.jsonl high.jsonl 2 4.3 2.51 --correction" and no --out
    Then the standard --correction column of rqm/correction.md prints cbs orca 6.0.0 "! def2-SVP Sp" "! def2-TZVPP Sp"

  @rq-8a869bbd
  Scenario: --extrapolate prints nothing of its own
    Given a database with an ok record named "1am"
    And two source databases with a fully covered "1am" candidate
    When CJKQC is invoked with the database and "--extrapolate 2s low.jsonl high.jsonl 1.5 --out out.jsonl"
    Then stdout holds only the summary line
    And no table, column, or section is printed
    And out.jsonl loads with the correction attached

  @rq-7de7b92a
  Scenario: Absent commands are omitted
    Given two source databases whose "1am" candidates carry no command
    When CJKQC is invoked with the database and "--extrapolate 2p low.jsonl high.jsonl 2 4.3 2.51 --correction" and no --out
    Then the correction carries no commands
    And the --correction column prints only the kind, program, and version pieces

  # --- Pipeline ---

  @rq-b2e971a9
  Scenario: An extrapolating build run reports its corrections
    Given the xtb fixture output and two source databases with a candidate matching its record's name
    When CJKQC is invoked with the fixture output and "--extrapolate 2s low.jsonl high.jsonl 1.5 --out out.jsonl"
    Then stdout contains "collected 1 records into out.jsonl (0 failed, 0 truncated, 1 corrected)"

  @rq-187b51ac
  Scenario: An extrapolating merge run reports its corrections
    Given database "a.jsonl" with 2 records and two source databases with a candidate matching one of them
    When CJKQC is invoked with "a.jsonl --extrapolate 2s low.jsonl high.jsonl 1.5 --out out.jsonl"
    Then stdout contains "merged 2 records into out.jsonl (1 corrected)"

  @rq-478fad20
  Scenario: A manipulated run reports its corrections
    Given database "a.jsonl" with 2 records and two source databases with a candidate matching one of them
    When CJKQC is invoked with "a.jsonl --extrapolate 2s low.jsonl high.jsonl 1.5 --name --out out.jsonl"
    Then stdout contains "filtered 2 records into out.jsonl (1 corrected)"

  @rq-cca456ff
  Scenario: el stays raw while elcorr shows the extrapolated energy
    Given a database with an ok record with electronic energy -38.51, defined thermochemistry, and a fully covered name
    When CJKQC is invoked with the database and "--extrapolate 2s low.jsonl high.jsonl 1.5 --el --elcorr --gcorr" and no --out
    Then el prints -38.51
    And elcorr prints the correction energy and gcorr the corrected Gibbs

  @rq-14e9d366
  Scenario: The extrapolated correction follows the run's conditions
    Given a database with a corrected record
    When the same database is run with "--elcorr --formation-corr" at --temp 298.15 and at --temp 350
    Then the two runs' gcorr-derived formation values differ and the elcorr values are equal

  # --- Formulas (API) ---

  @rq-0a61e5eb
  Scenario: parse_extrapolation returns the three forms
    When parse_extrapolation is called with the values of the three valid invocations above
    Then it returns TwoPoint, SettingsBlend, and TwoPointBlended respectively
    And TwoPointBlended holds the pairs in invocation order and the fraction 1.5

  @rq-fa4b7ff4
  Scenario: parse_extrapolation rejects malformed invocations
    When parse_extrapolation is called with an unknown mode, a wrong value count, N 0, ALPHA 0, BETA -1, or a non-finite F
    Then it returns Err with a usage message naming the mode

  @rq-c5ea8cb9
  Scenario: two_point_energy matches the closed form
    When two_point_energy is called with scf_low -76.0, corr_low 0.0, scf_high -76.1, corr_high -0.05, cardinal 2, alpha 4.3, and beta 2.51
    Then it returns -76.21251721253894 within 1e-9

  @rq-19257e60
  Scenario: blend_energy scales the gap
    When blend_energy is called with -76.0, -76.1, and 1.5
    Then it returns -76.15

  @rq-a4fc7526
  Scenario: A non-finite result leaves the record uncovered
    Given sources whose contributing energies make the residual overflow to infinity
    When extrapolate_corrections is called
    Then the affected record receives no correction
    And it is counted under uncovered

  @rq-c8ad2edd
  Scenario: A source list of the wrong length is an error
    When extrapolate_corrections is called with one source slice and a two-point spec
    Then it returns Err(SourceCount { expected: 2, actual: 1 })
    And it returns Err(SourceCount { expected: 4, actual: 3 }) for three slices and a combined-form spec
```
