# Feature: Parser Conformance Harness and Standard Registry <!-- rq-db156cde -->

The five output-format parsers of CJKQC (plain xyz, XTB, ORCA, Gaussian,
MRCC — one requirements document each under `rqm/parsers/`) arrive together
with the machinery that proves them: a standard registry constructor that
assembles the shipped parser set in a fixed detection order, a fixture
manifest that declares the expected outcome for every file under
`tests/fixtures/`, and a conformance suite that replays the whole corpus
through the standard registry. A parser is "done" when the suite passes.

The registry is assembled in one place so detection order is a single
decision: banner-signature parsers first (XTB, ORCA, Gaussian, MRCC), the
structural plain-xyz parser last. Detection resolves a head by the earliest
signature in the head window — so a wrapper's embedded banner never wins a
file from the program whose banner opens it — with registration order as the
tie-break for equal offsets (`rqm/parser-framework.md`). Plain xyz is
registered last purely as a deterministic tie-break; because a structural
xyz signature is the first line (an atom count) and a banner line never is,
the two never tie, so an xyz-shaped head always wins on offset — and a head
that both opens xyz-shaped and bears a program banner is claimed by xyz.

The expected values in the manifest come from the JKQC ground-truth tables
checked into the corpus as `examples.pkl` (one per program directory); the
manifest is the machine-readable encoding of those tables, and the pickles
remain as provenance. A `vibrational_frequencies` expectation is stated in
the record's vibrational-mode convention: near-zero
translational/rotational entries in the raw tables or in the program's own
printing are not part of the expected list (`rqm/parser-framework.md`).
A fixture with no `examples.pkl` row — one added after
the tables were cut, such as an extra corpus case — carries expectations
hand-verified against the program's own printed values in the file. The
framework's window contract and record assembly
rules (`rqm/parser-framework.md`) apply unchanged; records carry the
observable vocabulary of `rqm/record.md`.

## Feature API <!-- rq-f695c066 -->

### Functions <!-- rq-9dc0e7f0 -->

- `ParserRegistry::standard() -> ParserRegistry` <!-- rq-212a38b5 -->
  - Returns a registry holding exactly one parser per supported format,
    registered in this order: `XtbParser`, `OrcaParser`, `GaussianParser`,
    `MrccParser`, `XyzParser`.
  - The construction is the only way the shipped set is assembled; callers
    (the milestone-4 collection command) parse files exclusively through it.

### Types <!-- rq-12fbb168 -->

- `XtbParser`, `OrcaParser`, `GaussianParser`, `MrccParser`, `XyzParser` — <!-- rq-daf59b28 -->
  unit structs implementing the framework's `Parser` trait; each is
  specified by its own document in this directory. Each lives in its own
  module under `src/parser/` and is re-exported by the library root.

### Crate Layout <!-- rq-1b84a43e -->

- `src/parser/mod.rs` gains `ParserRegistry::standard` and the five parser <!-- rq-1db98ae8 -->
  modules; no new dependencies.
- `tests/fixtures/manifest.json` — the fixture manifest (below). <!-- rq-e21fcb88 -->
- `tests/conformance.rs` — the manifest-driven suite; it builds <!-- rq-a1f4c207 -->
  `ParserRegistry::standard()`, calls `parse_file` on every manifest entry,
  and asserts the declared outcome.

## Fixture Corpus and Manifest <!-- rq-8011b3d2 -->

- The corpus is organized by format directory:
  `tests/fixtures/{xyz,xtb,orca,gaussian,mrcc}/`, plus per-program
  `examples.pkl` ground-truth files kept as provenance (not read by tests).
- `tests/fixtures/manifest.json` is a JSON array; every file under
  `tests/fixtures/` (except `manifest.json` and the `examples.pkl` files)
  appears exactly once, with the entry:
  - `file` — path relative to `tests/fixtures/`.
  - `program` — the program token the file must route to.
  - `status` — the expected `Record::status` (`"ok"`, `"failed"`, or
    `"truncated"`).
  - `expect` — an object whose keys mirror the record keys
    (`electronic_energy`, `charge`, `multiplicity`, `command`,
    `program_version`, `vibrational_frequencies`, `gradient`,
    `dipole_moment`, `mulliken_charges`, `rotational_symmetry_number`,
    `geometry`). Absent keys are not asserted; a key the record must leave
    absent is asserted with `null`.
  - `geometry` is either the full expected geometry (symbols and
    coordinates) or `{"atoms": N}` when only the atom count is pinned.
  - `vibrational_frequencies` is either the full expected list or
    `{"count": N}` when only the mode count is pinned.
- Every format directory holds at least one successful run, one failed run,
  and one truncated file (a run cut mid-output, by construction by cutting
  a real file at a line boundary). A failed run is genuine program error
  output: it is the normative statement of that program's failure banner,
  and the per-format parsers classify `failed` against it rather than
  against wording asserted in a requirements document.
- Companion files belong to the corpus by content, not by directory: an
  `.xyz` file inside a program directory is a plain-xyz fixture and is
  declared with `"program": "xyz"`.

## Conformance Assertions <!-- rq-58c33b85 -->

For every manifest entry the suite asserts, via `parse_file` on the
standard registry:

- The file parses without `Err`, its record's `program` equals the declared
  token, and its `status` equals the declared status.
- Every observable named in `expect` matches the record. Comparison is
  **exact by default**: a verbatim float (energy, frequency, charge column,
  unconverted coordinate) parses from the same decimal string the file
  prints, and decimal→f64 conversion is correctly rounded everywhere, so
  the manifest stores the file's decimal literally and the f64s compare
  bit-for-bit. Exact equality also covers integers, strings (trimmed),
  symbol lists, and frequency length (unless the entry pins only the
  count).
- The exception is a parser-converted quantity whose expected value comes
  from an independent source — geometry converted bohr→Å (MRCC) and dipole
  converted Debye→a.u. (Gaussian): the expected decimals were produced by
  a different program's conversion constants, so they compare with an
  absolute tolerance of 1e-8. The manifest marks these entries with a
  `converted` flag naming the observable.
- Observables absent from `expect` are unconstrained except where the entry
  declares `null`, which requires absence in the record.
- `name` and `source_path` are stamped by the framework and not asserted.

## Out of Scope <!-- rq-9fc00a11 -->

- Extraction rules, signatures, and status classification per format — the
  five format documents in this directory.
- Reading `examples.pkl` at test time (it requires Python/pandas; the
  manifest is its checked-in encoding).
- Transparent decompression and parallel collection — unchanged from
  `rqm/parser-framework.md`.
- Composing CLI-selected registries beyond `standard()` — milestone 4.

## Gherkin Scenarios <!-- rq-fa8178d7 -->

```gherkin
Feature: Parser conformance harness and standard registry

  @rq-441c138f
  Scenario: The standard registry holds all five parsers in detection order
    Given the standard registry
    When its parsers are listed in registration order
    Then they are "xtb", "orca", "gaussian", "mrcc", "xyz" in that order

  @rq-33130099
  Scenario: Every fixture file is declared exactly once
    Given the manifest at tests/fixtures/manifest.json
    And the set of files under tests/fixtures excluding the manifest and examples.pkl files
    When the two sets are compared
    Then every file appears exactly once in the manifest

  @rq-d2ba6fbf
  Scenario: Every format directory covers all three outcomes
    Given the manifest
    When entries are grouped by program
    Then every program has at least one ok, one failed, and one truncated fixture

  @rq-991e31a2
  Scenario: A declared fixture conforms
    Given any manifest entry
    When parse_file is called on that file through the standard registry
    Then Ok is returned
    And the record has the declared program and status
    And every observable in expect matches exactly, or within the 1e-8
    conversion tolerance where the entry is flagged converted

  @rq-2a0d8029
  Scenario: The Gaussian corpus fixture reproduces the JKQC ground truth
    Given the manifest entry for the successful Gaussian run
    When the file is parsed through the standard registry
    Then the record has program "gaussian" and status ok
    And the record has electronic_energy exactly -1526.66937796
    And the record has charge 0 and multiplicity 1

  @rq-646cd2be
  Scenario: The MRCC corpus fixture reproduces the JKQC ground truth
    Given the manifest entry for the successful MRCC run
    When the file is parsed through the standard registry
    Then the record has electronic_energy exactly -134.936294839321

  @rq-d2239555
  Scenario: The XTB corpus fixture reproduces the JKQC ground truth
    Given the manifest entry for the successful XTB run
    When the file is parsed through the standard registry
    Then the record has electronic_energy exactly -135.137949826927

  @rq-1475e428
  Scenario: The ORCA corpus fixture reproduces the JKQC ground truth
    Given the manifest entry for the successful ORCA run
    When the file is parsed through the standard registry
    Then the record has electronic_energy exactly -199.642065483666

  @rq-71a3e979
  Scenario: An ORCA run embedding an xtb banner routes to ORCA
    Given the manifest entry for the ORCA run whose head carries a GFN2-xTB version banner below the ORCA banner (tests/fixtures/orca/1sa1am.out)
    When the file is parsed through the standard registry
    Then the record has program "orca" and status ok
    And the record has a geometry, so its thermochemistry is defined

  @rq-d56ab087
  Scenario: Companion xyz files route to the plain-xyz parser
    Given the manifest entries for the .xyz files stored inside program directories
    When each is parsed through the standard registry
    Then every record has program "xyz"

  @rq-f1754e10
  Scenario: Failed and truncated fixtures yield status records
    Given the manifest entries with status failed or truncated
    When each is parsed through the standard registry
    Then Ok is returned with that status
    And every failed record carries a non-empty error
```
