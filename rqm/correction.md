# Feature: Single-Point Energy Corrections <!-- rq-f8ab9d1b -->

A structure's thermochemistry is only as good as its electronic energy. The
JKQC workflow computes geometries and frequencies at an affordable level, then
re-evaluates the energy at a better one with separate single-point jobs — its
`elout`/`gout` columns print the resulting values. CJKQC attaches such
corrections to records by name: `--add-correction <db>` loads a second
database, matches its records to the run's records by `name`, and copies each
matched record's energy and provenance into the record's `correction` object
(`rqm/record.md`). The stored optimization energy is never altered; corrected
values appear only where explicitly requested — the `--elcorr`/`--hcorr`/`--gcorr`
columns, the `elcorr`/`gcorr` cutoff properties and sort keys, and the
corrected formation section (`rqm/formation.md`) — so every number's origin
stays inspectable: the record carries the correction's program, version,
commands, and kind. Extrapolated corrections (`rqm/extrapolation.md`) are a
second producer of the same object and flow through the same machinery.

## The Correction Database <!-- rq-233e0670 -->

- A correction database is an ordinary CJKQC database (`rqm/database.md`),
  typically built by an ordinary collection run over the single-point jobs'
  output files — so every correction record carries its provenance the usual
  way: `program`, `program_version`, and `command` (`rqm/record.md`). A
  database assembled by merging or by hand works identically; whatever a
  record does not carry is simply not copied into the attached correction.
- A **correction candidate** is a correction record with status `ok`, a
  present `electronic_energy`, and a present `name`. Only candidates attach.
- Two candidates sharing one `name` make the database ambiguous: that is a
  fatal error naming the name, and nothing is written (exit 1). Non-candidates
  never collide — a failed rerun collected into the same database as its ok
  replacement does not block the attachment.

## Attachment <!-- rq-587473fd -->

- `--add-correction <path>` is a manipulation flag (`rqm/filtering.md`): a
  run carrying it attaches corrections to the loaded records before the
  pipeline stages, in build and merge runs alike. It takes one path; repeating
  the flag is a usage error (exit 2). Like any manipulation, it does not
  satisfy the visible-outcome rule on its own.
- The path is always loaded as a database (`load_database`,
  `rqm/database.md`), never classified like the positional inputs. A missing
  or unreadable file is fatal (exit 1). A line that fails to load is skipped
  and reported exactly like a corrupt merged line — a warning naming the path
  and line number, and exit 1 — while the valid records still attach and the
  output is still written. A correction database that loads no record at all
  while reporting load problems — a program output handed to
  `--add-correction`, say — is fatal instead: the error is reported and
  nothing is written.
- Matching is exact, case-sensitive equality of `name`. Every input record
  whose name equals a candidate's name receives the correction — all of them
  when several share the name. Input records without a `name` never match, and
  attachment is independent of the input record's status: the pipeline's
  ok-only rule decides visibility as usual.
- The attached correction copies the candidate's values: `energy` is the
  candidate's `electronic_energy`, `program` and `program_version` the
  candidate's fields, `commands` a one-element list holding the candidate's
  `command` (absent when it has none), and `kind` is `sp`.
- An input record that already carries a correction has it replaced by a new
  match; a record the correction database does not name keeps whatever
  correction it already stores, so re-running a corrected database against a
  larger correction database only adds and updates.
- A correction record that is not ok, or an ok one without an
  `electronic_energy`, is skipped and warned individually (naming its name,
  `(unnamed)` when absent).
  Candidates that matched no input record are reported as one aggregate
  warning with their count. Warnings never change the exit code or the output.
- The run's records are enriched in memory: print flags show corrected values,
  and `--out` writes the enriched records. Without `--out` nothing is stored
  (the general rule, `rqm/filtering.md`). Writing re-serializes the modified
  records, so keys unknown to the record schema do not survive a correcting
  run (`rqm/record.md` ignores unknown keys on read).
- The completion lines report the enrichment: a run carrying
  `--add-correction` gains a `(<k> corrected)` suffix on whichever
  completion line it prints — `collected <n> records into <out> (<f>
  failed, <t> truncated, <k> corrected)`, `merged <n> records into <out>
  (<k> corrected)`, or `filtered <n> records into <out> (<k> corrected)`.
  `<k>` counts the written records that carry a correction, stored ones
  included (`rqm/collection.md`, `rqm/filtering.md`).

## Corrected Values <!-- rq-fd59200d -->

- The derived-property registry (`rqm/thermo.md`) gains three keys:
  - `elcorr` — the attached correction's energy in hartree; undefined without
    a correction.
  - `hcorr` — the enthalpy derived at the run's conditions (`rqm/thermo.md`)
    from the correction energy and the record's remaining observables: the
    standard derivation with the correction energy in place of
    `electronic_energy`. Undefined without a correction, and under the same
    definedness rules as any derivation (geometry, frequencies, and so on).
  - `gcorr` — the Gibbs free energy derived at the run's conditions
    (`rqm/thermo.md`) from the correction energy and the record's remaining
    observables: the standard derivation with the correction energy in place
    of `electronic_energy`. Undefined without a correction, and under the same
    definedness rules as any derivation (geometry, frequencies, and so on).
- The raw levels are untouched: `el`, `g`, `zpe`, `h`, `s`, the stored
  `electronic_energy`, the cutoffs and sorts on them, `--glob`, `--uniq`, and
  the raw `--formation` section (`rqm/formation.md`) all keep using the
  optimization-level values. Corrected values appear only where explicitly
  requested, and a record loaded from a previously corrected database carries
  its correction with it — no flag is needed to use stored corrections.

## Printing <!-- rq-85d2e095 -->

- Four print columns join the table (`rqm/filtering.md`), in canonical
  order: `correction` after `command`, and `elcorr`, `hcorr`, `gcorr` after
  `g`.
- `--elcorr` / `--hcorr` / `--gcorr` print the corrected values like
  `el`/`h`/`g` — hartree with 10 decimals, kcal/mol with 4 under `--unit` —
  and `-` without a correction or when undefined.
- `--correction` prints the provenance: the `kind`, then the program and
  version the way the `--program` column prints them (the token, or the token
  and version joined by one space), dropping absent pieces, then each command
  wrapped in double quotes (the `--command` rule), all separated by single
  spaces; `-` without a correction. Example:
  `sp orca 6.0.0 "! wB97x-3c def2-TZVPP Sp"`.

## Cutoffs and Sorting <!-- rq-b1054bc7 -->

- `elcorr` and `gcorr` are cutoff properties (`rqm/filtering.md` Cutoffs) with
  the same units as `el` and `g`: hartree absolute, kcal/mol in relative mode.
  Undefined values fail every comparison except `≠`, like any undefined
  property. `hcorr` is not a cutoff property, mirroring `h`, which is a
  print-only column.
- `elcorr` and `gcorr` are sort keys (`rqm/filtering.md` Sort): ascending,
  undefined values last (first under `--reverse`). They are not part of the
  implicit sort.
- `--glob` and `--uniq` keep operating on the raw Gibbs.

## Feature API <!-- rq-fdadf7b4 -->

### Types <!-- rq-7f894a81 -->

- `Correction` — the attached correction as stored on a record <!-- rq-a169ccdf -->
  (`rqm/record.md`): `energy: f64` (hartree), `program: Option<String>`,
  `program_version: Option<String>`, `commands: Option<Vec<String>>`,
  `kind: String`.
- `Attachment` — what an attach pass did: `records: Vec<Record>` (enriched <!-- rq-952b74c9 -->
  copies of the input, in input order; the input slice is never mutated),
  `attached: usize` (records that received a correction), `skipped: usize`
  (candidates rejected for status or missing energy), `unmatched: usize`
  (candidates no input record names).
- `CorrectionError` — error type of the attach pass: <!-- rq-874abd6a -->
  - `Io { kind: std::io::ErrorKind, reason: String }` — the correction
    database could not be read (mirrors `DbError::Io`).
  - `DuplicateName { name: String }` — two candidates share the name.

### Functions <!-- rq-123b0f89 -->

- `attach_corrections(records: &[Record], corrections: &[Record]) -> <!-- rq-a9dae115 -->
  Result<Attachment, CorrectionError>`
  - Pure: builds the candidate map from `corrections` (two candidates with
    equal names yield `Err(DuplicateName)`), returns the `Attachment` whose
    `records` are enriched copies of the input — the input slice is never
    mutated — alongside the counts.
  - The CLI loads the correction database with `load_database` (reporting its
    problems) and hands the valid records over.
- `corrected_energy(record: &Record) -> Option<f64>` — the attached <!-- rq-fc9218b9 -->
  correction's energy.
- `derived_value("elcorr" | "hcorr" | "gcorr", record, conditions)` — the <!-- rq-b52af539 -->
  registry keys above (`rqm/thermo.md`).

### Crate Layout <!-- rq-cfefec96 -->

- The attachment and the corrected keys live in `src/correction.rs`; the
  library root re-exports them. The corrected formation section lives in
  `src/formation.rs` (`rqm/formation.md`). `src/main.rs` declares the
  `--add-correction`, `--elcorr`, `--hcorr`, `--gcorr`, and `--correction`
  flags — with `--formation-corr` (`rqm/formation.md`) — and wires the
  warnings, summary suffixes, and exit codes. No new dependencies.
- `tests/correction.rs` tests attachment as a pure function (matching, <!-- rq-0f7d8c11 -->
  overwriting, duplicate names, skipped candidates, counts) and the corrected
  keys; CLI coverage (flags, warnings, summary lines, columns, exit codes)
  extends `tests/cli.rs`.

## Out of Scope <!-- rq-3a687494 -->

- More than one correction per record: the `correction` object holds one
  entry; an array form can arrive additively if a use appears.
- Matching by anything but the exact `name`: no composition, geometry, or
  fuzzy matching, and no case folding.
- Corrected `--glob` and `--uniq`: the group-minimum stages stay on the raw
  Gibbs.
- BSSE/counterpoise corrections (`rqm/thermo.md` Out of Scope).
- Supplying a correction energy on the command line: corrections come from a
  loaded database, provenance included.

## Gherkin Scenarios <!-- rq-64a6e8ac -->

```gherkin
Feature: Single-point energy corrections

  # --- Attachment ---

  @rq-973be061
  Scenario: An attached correction carries its provenance
    Given a database with an ok record named "1am" with electronic energy -38.51
    And a correction database with an ok record named "1am" with electronic energy -76.4109, program "orca", program_version "6.0.0", and command "! wB97x-3c def2-TZVPP Sp"
    When CJKQC is invoked with the database and "--add-correction sp.jsonl --out out.jsonl"
    Then out.jsonl loads as 1 record whose correction has energy -76.4109, program "orca", program_version "6.0.0", commands ["! wB97x-3c def2-TZVPP Sp"], and kind "sp"
    And the record's electronic_energy is still -38.51
    And stdout contains "(1 corrected)"

  @rq-6f572182
  Scenario: Matching is exact and case-sensitive
    Given a database with an ok record named "1am" and a correction database with a candidate named "1AM"
    When CJKQC is invoked with the database and "--add-correction sp.jsonl --out out.jsonl"
    Then the output record carries no correction
    And a stderr warning reports 1 unmatched correction record
    And the exit code is 0

  @rq-c411edfa
  Scenario: Every record sharing the name receives the correction
    Given a database with two ok records named "1am" and a correction database with one candidate named "1am"
    When CJKQC is invoked with the database and "--add-correction sp.jsonl --out out.jsonl"
    Then both output records carry the correction

  @rq-c907b696
  Scenario: A record without a name never matches
    Given a database with an ok record without a name and a correction database with a candidate named "1am"
    When CJKQC is invoked with the database and "--add-correction sp.jsonl --out out.jsonl"
    Then the output record carries no correction
    And a stderr warning reports 1 unmatched correction record

  @rq-ea21b840
  Scenario: A candidate without a name is unmatched
    Given a database with an ok record named "1am" and a correction database with an ok record without a name and with an electronic energy
    When CJKQC is invoked with the database and "--add-correction sp.jsonl --out out.jsonl"
    Then the output record carries no correction
    And a stderr warning reports 1 unmatched correction record

  @rq-0627beb4
  Scenario: A new match replaces a stored correction
    Given a database whose ok record named "1am" carries a correction with energy -76.0
    And a correction database with a candidate named "1am" with electronic energy -76.4109
    When CJKQC is invoked with the database and "--add-correction sp.jsonl --out out.jsonl"
    Then the output record's correction energy is -76.4109

  @rq-53c8c5fc
  Scenario: An unmatched record keeps its stored correction
    Given a database whose ok record named "2am" carries a correction with energy -76.0
    And a correction database with a candidate named "1am"
    When CJKQC is invoked with the database and "--add-correction sp.jsonl --out out.jsonl"
    Then the output record's correction energy is still -76.0
    And a stderr warning reports 1 unmatched correction record

  @rq-e59918d8
  Scenario: A failed correction record is skipped with a warning
    Given a database with an ok record named "1am" and a correction database with a failed record named "1am" carrying an electronic energy
    When CJKQC is invoked with the database and "--add-correction sp.jsonl --out out.jsonl"
    Then the output record carries no correction
    And a stderr warning names "1am" as skipped

  @rq-d599d450
  Scenario: A failed rerun does not collide with its ok replacement
    Given a correction database with a failed record named "1am" and an ok record named "1am" with electronic energy -76.4109
    When CJKQC is invoked with the database and "--add-correction sp.jsonl --out out.jsonl"
    Then the output record's correction energy is -76.4109
    And a stderr warning names "1am" as skipped

  @rq-a83c76ac
  Scenario: An ok correction record without an energy is skipped with a warning
    Given a correction database with an ok record named "1am" without an electronic energy
    When CJKQC is invoked with the database and "--add-correction sp.jsonl --out out.jsonl"
    Then the output record carries no correction
    And a stderr warning names "1am" as skipped

  @rq-e15a3e2d
  Scenario: A correction record without a command attaches without commands
    Given a correction database with an ok candidate named "1am" without a command
    When CJKQC is invoked with the database and "--add-correction sp.jsonl --out out.jsonl"
    Then the attached correction carries energy, program, and kind but no commands

  @rq-74350863
  Scenario: Duplicate candidate names are fatal
    Given a correction database with two ok candidates named "1am" with different electronic energies
    When CJKQC is invoked with the database and "--add-correction sp.jsonl --out out.jsonl"
    Then the exit code is 1
    And stderr names "1am"
    And out.jsonl does not exist

  @rq-922a22be
  Scenario: A corrupt line in the correction database is skipped and fails the run
    Given a correction database with a valid candidate on line 1 and an invalid line 2
    When CJKQC is invoked with the database and "--add-correction sp.jsonl --out out.jsonl"
    Then the exit code is 1
    And a stderr warning names "sp.jsonl" and line 2
    And out.jsonl loads with the correction attached from line 1

  @rq-9fd40c4e
  Scenario: A correction database that is a program output is fatal
    Given the file "sp.log" holds a QC program output
    When CJKQC is invoked with the database and "--add-correction sp.log --out out.jsonl"
    Then the exit code is 1
    And out.jsonl does not exist

  @rq-83e236b7
  Scenario: A missing correction database is fatal
    Given no file exists at "sp.jsonl"
    When CJKQC is invoked with the database and "--add-correction sp.jsonl --out out.jsonl"
    Then the exit code is 1
    And out.jsonl does not exist

  @rq-d1bd7404
  Scenario: Repeating --add-correction is a usage error
    When CJKQC is invoked with the database and "--add-correction a.jsonl --add-correction b.jsonl --out out.jsonl"
    Then the exit code is 2

  @rq-3fd0b9bb
  Scenario: --add-correction alone is not a visible outcome
    When CJKQC is invoked with the database and "--add-correction sp.jsonl" and no --out and no print flag
    Then the exit code is 2

  @rq-58d85e68
  Scenario: A correcting build run reports its corrections
    Given the xtb fixture output and a correction database with a candidate matching its record's name
    When CJKQC is invoked with the fixture output and "--add-correction sp.jsonl --out out.jsonl"
    Then stdout contains "collected 1 records into out.jsonl (0 failed, 0 truncated, 1 corrected)"

  @rq-66e4cb89
  Scenario: A correcting merge run reports its corrections
    Given database "a.jsonl" with 2 records and a correction database with a candidate matching one of them
    When CJKQC is invoked with "a.jsonl --add-correction sp.jsonl --out out.jsonl"
    Then stdout contains "merged 2 records into out.jsonl (1 corrected)"

  @rq-c0c3a1af
  Scenario: Unknown keys do not survive a correcting run
    Given a database whose only line holds a valid record plus a "future_field" key
    And a correction database with a candidate matching the record's name
    When CJKQC is invoked with the database and "--add-correction sp.jsonl --out out.jsonl"
    Then the output record has no "future_field" key
    And the output record carries the correction

  # --- Corrected values and columns ---

  @rq-b5e3b553
  Scenario: el and g stay raw while the corrected columns show the corrected values
    Given a database with an ok record with electronic energy -38.51, defined thermochemistry, and an attached correction with energy -76.4109
    When CJKQC is invoked with the database and "--el --h --g --elcorr --hcorr --gcorr" and no --out
    Then el, h, and g print the raw-level values
    And elcorr prints -76.4109 in hartree with 10 decimals
    And hcorr and gcorr print the corrected-level enthalpy and Gibbs

  @rq-04a56097
  Scenario: The corrected thermochemistry equals the correction energy plus the raw thermal correction
    Given a record with defined thermochemistry and an attached correction
    When derived_value is called with "hcorr" and "gcorr"
    Then hcorr equals elcorr + (h - el) and gcorr equals elcorr + (g - el) within 1e-12

  @rq-bca9f7b9
  Scenario: The corrected columns print - without a correction
    Given a database with an ok record without a correction
    When CJKQC is invoked with the database and "--elcorr --hcorr --gcorr" and no --out
    Then all three columns print -

  @rq-5aca0e6b
  Scenario: hcorr and gcorr are undefined without defined thermochemistry
    Given a non-monatomic record with a correction and no vibrational frequencies
    When CJKQC is invoked with the database and "--hcorr --gcorr" and no --out
    Then hcorr and gcorr print -

  @rq-21414dd3
  Scenario: The correction column prints kind, program, version, and quoted commands
    Given a record whose correction has program "orca", program_version "6.0.0", commands ["! wB97x-3c def2-TZVPP Sp"], and kind "sp"
    When CJKQC is invoked with the database and "--correction" and no --out
    Then the column prints sp orca 6.0.0 "! wB97x-3c def2-TZVPP Sp"

  @rq-be59d52a
  Scenario: The correction column drops absent pieces
    Given a record whose correction has program "orca" but no program_version and no commands
    When CJKQC is invoked with the database and "--correction" and no --out
    Then the column prints sp orca

  @rq-f3233093
  Scenario: The correction column prints - without a correction
    Given a database with an ok record without a correction
    When CJKQC is invoked with the database and "--correction" and no --out
    Then the column prints -

  @rq-e861c941
  Scenario: The new columns follow the canonical order
    Given a database with one ok record
    When CJKQC is invoked with the database and "--gcorr --elcorr --correction --command --hcorr" and no --out
    Then the header orders command, correction, elcorr, hcorr, gcorr

  @rq-c51906fe
  Scenario: --unit converts the corrected energy columns
    Given a database with an ok record with an attached correction
    When CJKQC is invoked with the database and "--elcorr --hcorr --gcorr --correction --unit" and no --out
    Then elcorr, hcorr, and gcorr print in kcal/mol with 4 decimals
    And the correction column is unchanged

  # --- Cutoffs and sorting ---

  @rq-234d7e28
  Scenario: A relative corrected-energy window is per composition group
    Given one group with corrected energies 0.0 and 0.02 Eh and another group with -5.0 Eh
    When the cutoff cutr elcorr 5 applies
    Then the 0.02 Eh record (12.6 kcal/mol above its group minimum) is dropped and the -5.0 record survives

  @rq-aa358806
  Scenario: A record without a correction fails corrected cutoffs except inequality
    Given a group with one corrected record and one uncorrected record
    When the cutoff pass elcorr 0 applies
    Then only the corrected record survives
    When the cutoff isnot elcorr 0 applies
    Then both records survive

  @rq-bdac8cdb
  Scenario: --sort elcorr orders ascending with undefined last
    Given records with corrected energies -76.5 and -76.4 and one without a correction
    When CJKQC is invoked with the database and "--sort elcorr --name" and no --out
    Then the order is -76.5, -76.4, uncorrected
    And with --reverse the uncorrected record sorts first

  @rq-d84cf2be
  Scenario: --sort gcorr orders ascending with undefined last
    Given records with corrected Gibbs -100.5 and -100.0 and one without a correction
    When CJKQC is invoked with the database and "--sort gcorr --name" and no --out
    Then the order is -100.5, -100.0, uncorrected
    And with --reverse the uncorrected record sorts first

  @rq-86219849
  Scenario: Stored corrections work without --add-correction
    Given a database whose records already carry corrections
    When CJKQC is invoked with the database and "--cutr elcorr 5 --name" and no --out
    Then the cutoff applies to the stored corrections

  # --- Registry ---

  @rq-8ce96e45
  Scenario: The registry exposes the corrected keys
    Given a record with defined thermochemistry and an attached correction
    When derived_value is called with "elcorr", "hcorr", and "gcorr"
    Then elcorr returns the correction energy and hcorr and gcorr the corrected enthalpy and Gibbs in Eh
    And all three return None for a record without a correction
```
