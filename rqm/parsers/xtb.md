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
- `command` — the invocation echoed on the `program call` line: the <!-- rq-c01ba326 -->
  text after the `program call` label, whitespace-trimmed and stored
  verbatim except that the directory part of the executable path is
  stripped (everything up to and including the last `/` in the first
  token), so the value starts at the executable name
  (`"xtb 1dma.xyz --gxtb --acc 0.01 -ohess"` from
  `/home/x/Applications/xtb-6.7.1/bin/xtb 1dma.xyz --gxtb --acc 0.01 -ohess`).
  No `program call` line → absent.
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
- The frequency span and seam honesty follow the Gaussian rule
  (`rqm/parsers/gaussian.md`): with the last `projected vibrational
  frequencies` header visible in the tail the block is whole inside the
  windows; when it is not, the `eigval` lines visible after a head-window
  header plus the `eigval` lines at the head of the tail are gathered,
  and if the stored (post-filter) count then falls below 3N−6 while the
  geometry shows N atoms, the record is demoted to `truncated` with an
  error saying the frequency table may be incomplete and suggesting a
  rerun with larger `--head-bytes`/`--tail-bytes` windows.
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
- A banner-`ok` record whose fallback-gathered frequency block fails the
  3N−6 seam-honesty count is demoted to `truncated` (the frequency
  section above); `failed` outcomes and header-delimited blocks are
  never demoted.
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
    Given xtb log content with the banner, a "program call" line whose value is
    "/home/y/Applications/xtb-6.7.1/bin/xtb 1dma.xyz --gxtb --acc 0.01 -ohess",
    "molecular charge" 0.0, "number of unpaired electrons" 0.0, a "final structure:" frame,
    a "projected vibrational frequencies" block with eigval lines, an "Atomic charges and
    shell populations" block, an "Atomic dipole moments" block with total row,
    a boxed "TOTAL ENERGY -135.137949826927 Eh" line, and "finished run"
    When the content is parsed
    Then the record has status ok
    And the record has program_version "6.7.1" and command
    "xtb 1dma.xyz --gxtb --acc 0.01 -ohess"
    And the record has charge 0 and multiplicity 1
    And the record has electronic_energy -135.137949826927
    And the record has the given geometry, frequencies, Mulliken charges, and dipole

  @rq-20204b94
  Scenario: The last TOTAL ENERGY line wins
    Given log content with boxed TOTAL ENERGY lines -135.1376923 and -135.137949826927
    When the content is parsed
    Then the record has electronic_energy -135.137949826927

  @rq-1fb0a352
  Scenario: The program call is stored with the executable path stripped
    Given a program call line whose value is "/opt/xtb 1dma.xyz --gfnn -c 4"
    When the content is parsed
    Then the record's command is "xtb 1dma.xyz --gfnn -c 4"
    When a log holds no program call line
    Then the record has no command

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

  @rq-12042004
  Scenario: An elided frequency header falls back to the visible eigval lines
    Given uncovered windows of a three-atom run whose last "projected vibrational
    frequencies" header lies in the head with `eigval` lines visible below it and more
    genuine `eigval` values opening the tail, and a "finished run" banner
    When the windows are parsed
    Then the record has status ok
    And the record's frequencies hold every visible genuine eigenvalue in printed order

  @rq-a5c79f09
  Scenario: A seam-cut eigval block is stored truncated
    Given uncovered windows of a three-atom run in which the projected-frequency
    `eigval` values number fewer than 3N-6 genuine modes across the windows and the
    header is not in the tail, and a "finished run" banner
    When the windows are parsed
    Then the record has status truncated
    And the record's error says the frequency table may be incomplete

  @rq-ebeceeae
  Scenario: The corpus fixture conforms
    Given the manifest entry for the successful XTB fixture
    When the file is parsed through the standard registry
    Then all declared observables match under the harness comparison rules
```
