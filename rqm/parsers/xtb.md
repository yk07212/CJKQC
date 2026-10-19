# Feature: XTB Parser <!-- rq-cd5bd6c0 -->

xtb (Grimme group, GFN-xTB family) writes one stdout log per run; a run may
chain a geometry optimization, a numerical Hessian, and a final single
point. The XTB parser reads such a log into exactly one record for the
whole run (`rqm/parser-framework.md`'s final-job rule): the optimized
structure, the final `TOTAL ENERGY`, the Hessian frequencies, and the
population/dipole analysis of the last single point.

Detection is the version banner: every xtb log prints a line
`* xtb version <v> (<hash>) ...` near its start. Observables come from the
last visible occurrence of each marker block — xtb repeats single-point
sections per optimization cycle, and the final occurrence is the run's
answer (`rqm/parsers/conformance.md` asserts this over the corpus
fixture). Records carry the canonical units of `rqm/record.md`; xtb prints
energies in hartree, coordinates in Å, charges and dipoles in atomic units,
frequencies in cm⁻¹, so extraction is unit-preserving.

## Feature API <!-- rq-4223dba9 -->

### Types <!-- rq-117fdecb -->

- `XtbParser` — unit struct implementing the framework `Parser` trait with <!-- rq-597bef47 -->
  `program()` = `Program::XTB`. Lives in `src/parser/xtb.rs`, re-exported
  by the library root.

### Detection <!-- rq-7b39d1a8 -->

- `detect(head)` returns the byte offset of the earliest line matching <!-- rq-d2cba02a -->
  `xtb version` (the banner line `* xtb version <version> (...)`), or `None`
  when no such line is visible (`rqm/parser-framework.md` routes by the
  earliest signature, so an embedded banner never wins a file from the
  program whose banner opens it).

### Extraction <!-- rq-bd3986f2 -->

Each observable is taken from the **last visible** match; a marker hidden
in an elided window yields an absent observable, never a guess.

- `program_version` — the version token from the banner: `xtb version <!-- rq-cec83ed0 -->
  (\S+) \(`.
- `method` — from the `program call` line, mapping the method flag: <!-- rq-c01ba326 -->
  `--gxtb` → `"g-xTB"`, `--gfno` → `"GFN0-xTB"`, `--gfn1` → `"GFN1-xTB"`,
  `--gfnn` or `--gfn2` → `"GFN2-xTB"`, `--gfnff` → `"GFN-FF"`; no method
  flag → `"GFN2-xTB"` (the xtb 6 default). No `program call` line → absent.
- `charge` — `molecular charge` value (e), rounded to an integer. <!-- rq-e3f9be75 -->
- `multiplicity` — `number of unpaired electrons` value `u`, rounded, plus <!-- rq-02fde316 -->
  one.
- `electronic_energy` — the boxed final line `| TOTAL ENERGY <value> Eh |` <!-- rq-157f85ee -->
  (`TOTAL ENERGY\s+(\S+)\s+Eh`), which closes every xtb run.
- `geometry` — the `final structure:` block: an xyz-style table <!-- rq-99a2e2ce -->
  (count, comment, symbol/Å rows) read with `read_geometry_table`.
- `vibrational_frequencies` — the values of every `eigval :` line following <!-- rq-03fd1a40 -->
  the last `projected vibrational frequencies` header, up to the first
  non-`eigval` line; the parsed values pass through `vibrational_modes`,
  so the near-zero translational/rotational eigenvalues (printed `-0.00`
  or `0.00`) are dropped and every genuine mode keeps its printed sign (a
  leading minus marks an imaginary mode); a block whose visible
  eigenvalues are all trivial leaves the field absent.
- `dipole_moment` — the `total` row of the last `Atomic dipole moments` <!-- rq-f4c1072b -->
  block, `(x, y, z)` in atomic units (e·a₀), stored unchanged.
- `mulliken_charges` — the charge column (`q`) of the last `Atomic charges <!-- rq-e7bbca13 -->
  and shell populations` block, in geometry atom order.
- `rotational_symmetry_number` — absent: xtb describes its point group in <!-- rq-3ac29482 -->
  words (`cs symmetry found`) but never prints a numeric symmetry number;
  downstream derivation owns that quantity (`rqm/ARCHITECTURE.md`).

### Status Classification <!-- rq-329b3c89 -->

- `ok` — the tail carries `finished run` (xtb's normal-exit banner). <!-- rq-9c076f14 -->
- `failed` — no normal marker and the tail carries xtb's own error report. <!-- rq-a652c946 -->
  The banner wording is version-dependent; it is pinned by the corpus
  failed fixture (`rqm/parsers/conformance.md`), not by this document. The
  record's error text quotes the reported message.
- `truncated` — neither marker present: the run never reached its end. <!-- rq-b3135fcd -->
- Extracted observables are carried alongside `failed`/`truncated` statuses
  whenever visible (`rqm/record.md`).

## Out of Scope <!-- rq-6da52c3f -->

- Gradient extraction: the corpus runs print gradient norms and write
  gradient files, but do not print gradient tables to stdout.
- Conformer/parallel `xtb` modes (`--parallel`, `--ensemble`) beyond their
  effect on the observables above.
- Program-reported thermochemistry (`THERMODYNAMIC` boxes): derived
  quantities are never stored (`rqm/ARCHITECTURE.md`).

## Gherkin Scenarios <!-- rq-1d6dade2 -->

```gherkin
Feature: XTB parser

  Background:
    Given the XtbParser

  @rq-35c8e2c9
  Scenario: The version banner is detected
    Given a head containing the line "* xtb version 6.7.1 (26dd68d) compiled by 'x' on 2026-05-14"
    When detect is called
    Then it returns Some with the byte offset of that line

  @rq-0bb4b64f
  Scenario: An unrelated head is not claimed
    Given a head without an "xtb version" line
    When detect is called
    Then it returns None

  @rq-68850338
  Scenario: A completed run yields the full observable set
    Given xtb log content with the banner, a "program call" line ending "--gxtb --acc 0.01 -ohess",
    "molecular charge" 0.0, "number of unpaired electrons" 0.0, a "final structure:" frame,
    a "projected vibrational frequencies" block with eigval lines, an "Atomic charges and
    shell populations" block, an "Atomic dipole moments" block with total row,
    a boxed "TOTAL ENERGY -135.137949826927 Eh" line, and "finished run"
    When the content is parsed
    Then the record has status ok
    And the record has program_version "6.7.1" and method "g-xTB"
    And the record has charge 0 and multiplicity 1
    And the record has electronic_energy -135.137949826927
    And the record has the given geometry, frequencies, Mulliken charges, and dipole

  @rq-20204b94
  Scenario: The last TOTAL ENERGY line wins
    Given log content with boxed TOTAL ENERGY lines -135.1376923 and -135.137949826927
    When the content is parsed
    Then the record has electronic_energy -135.137949826927

  @rq-1fb0a352
  Scenario: Method flags map to method names
    When the program call line carries "--gfnn"
    Then the record's method is "GFN2-xTB"
    When it carries no method flag
    Then the record's method is "GFN2-xTB"
    When it carries "--gfno"
    Then the record's method is "GFN0-xTB"

  @rq-5882d984
  Scenario: Negative frequencies keep their sign
    Given an eigval line containing -0.00 and -57.3
    When the content is parsed
    Then the stored frequencies include -57.3
    And the stored frequencies exclude the -0.0 entry

  @rq-c2963543
  Scenario: A leading imaginary mode survives the trivial-mode drop
    Given eigval lines holding, in ascending order, -57.3 then six
    near-zero eigenvalues then real modes
    When the content is parsed
    Then the stored frequencies start with -57.3 and hold no near-zero value

  @rq-d2e47781
  Scenario: A block of only trivial eigenvalues leaves the field absent
    Given an eigval block holding only -0.00 and 0.00 values
    When the content is parsed
    Then the record has no vibrational_frequencies

  @rq-ce1b9305
  Scenario: A run without its end marker is truncated
    Given otherwise complete log content lacking the "finished run" line
    When the content is parsed
    Then the record has status truncated
    And the extracted observables are still present

  @rq-62e1382e
  Scenario: A recorded failed run is classified failed
    Given the xtb failed fixture from the corpus (the program's own error
    report, with no "finished run" banner)
    When the content is parsed
    Then the record has status failed
    And the record's error is non-empty

  @rq-afe58819
  Scenario: A hidden final structure leaves geometry absent
    Given uncovered windows whose "final structure:" block lies in the elided middle
    When the windows are parsed
    Then the record has no geometry

  @rq-ebeceeae
  Scenario: The corpus fixture conforms
    Given the manifest entry for the successful XTB fixture
    When the file is parsed through the standard registry
    Then all declared observables match under the harness comparison rules
```
