# Feature: ORCA Parser <!-- rq-0aab8d11 -->

ORCA output files chain driver jobs — a geometry relaxation followed by a
frequency calculation is the corpus shape — and the ORCA parser reads such
a file into exactly one record for the final job: the last
`FINAL SINGLE POINT ENERGY`, the final Cartesian geometry, the canonical
`VIBRATIONAL FREQUENCIES` section, and the final population, dipole, and
point-group analyses (`rqm/parser-framework.md`).

Detection is the banner: ORCA opens every output with its ASCII art,
including the line `* O R C A *` (letters separated by runs of spaces).
Every observable is read from the last visible occurrence of its section
header, because each chained job reprints the SCF and property blocks for
the current geometry; the last occurrence is the final job's answer. The
record uses the canonical units of `rqm/record.md`; ORCA prints energies in
hartree, coordinates in Å, Mulliken charges and dipole components in atomic
units, and frequencies in cm⁻¹, so extraction is unit-preserving.

The frequency list is the `VIBRATIONAL FREQUENCIES` section, not the
thermochemistry summary tables: that section carries all 3N modes with an
explicit minus sign on imaginary modes, and near-zero translational/
rotational entries are filtered downstream, not in the parser.

## Feature API <!-- rq-45c63967 -->

### Types <!-- rq-3bb529ad -->

- `OrcaParser` — unit struct implementing the framework `Parser` trait with <!-- rq-1831b7fb -->
  `program()` = `Program::ORCA`. Lives in `src/parser/orca.rs`, re-exported
  by the library root.

### Detection <!-- rq-93f2fd85 -->

- `detect(head)` claims content with a line matching the banner pattern <!-- rq-762ab556 -->
  `\* O\s+R\s+C\s+A \*` (the ASCII-art title line).

### Extraction <!-- rq-34b49160 -->

Each observable is taken from the **last visible** match; a marker hidden
in an elided window yields an absent observable, never a guess.

- `program_version` — the token in `Program Version <v> - ... RELEASE`. <!-- rq-64d42f21 -->
- `method` — the first keyword after `!` in the echoed input block (the <!-- rq-527a66db -->
  `INPUT FILE` section, line `| 1> ! <method> <drivers...>`): the method or
  compound-method keyword (`"wB97x-3c"` from `! wB97x-3c Opt Freq`). No
  input echo → absent.
- `charge` — `Total Charge           Charge          .... <n>` value. <!-- rq-b846e66c -->
- `multiplicity` — `Multiplicity           Mult            .... <n>` value <!-- rq-b5b01447 -->
  (the summary form; the per-cycle `Multiplicity assumed :` lines are not
  the source).
- `electronic_energy` — the last `FINAL SINGLE POINT ENERGY <value>`. <!-- rq-610cdd9d -->
- `geometry` — the table under the last `CARTESIAN COORDINATES <!-- rq-8d91e753 -->
  (ANGSTROEM)` header (between its dashed rules): `read_geometry_table`
  with `Symbol(0)`, coordinate columns 1–3, Ångström.
- `vibrational_frequencies` — the entries of the last `VIBRATIONAL <!-- rq-17559f33 -->
  FREQUENCIES` section: lines `<mode>: <value> cm**-1` (an optional
  trailing `i` on the value also marks an imaginary mode and is stored as
  the negative value); the section ends at its trailing rule. All 3N
  entries are stored as printed, the six near-zero modes included.
- `dipole_moment` — the `Total Dipole Moment    :     <x> <y> <z>` line of <!-- rq-21041eac -->
  the last `DIPOLE MOMENT` block, in atomic units, stored unchanged.
- `mulliken_charges` — the values of the rows `<idx> <El> : <q>` under the <!-- rq-80fdb0f3 -->
  last `MULLIKEN ATOMIC CHARGES` header, in geometry atom order.
- `rotational_symmetry_number` — the value of `Symmetry Number: <n>` in the <!-- rq-89cb2071 -->
  line `Point Group:  <group>, Symmetry Number:   <n>` of the last
  thermodynamics block; absent when no frequency/thermo block is visible.

### Status Classification <!-- rq-8b1c135f -->

- `ok` — the tail carries `****ORCA TERMINATED NORMALLY****`. <!-- rq-f6a569e1 -->
- `failed` — no normal marker and the tail carries ORCA's own error or <!-- rq-80f193f5 -->
  abort report. The banner wording is version-dependent; it is pinned by
  the corpus failed fixture (`rqm/parsers/conformance.md`), not by this
  document. The error text names the reported cause.
- `truncated` — neither marker present. <!-- rq-95cbb226 -->
- Observables extracted before the failure are carried in the record
  (`rqm/record.md`).

## Out of Scope <!-- rq-29e0af6a -->

- Reading the thermodynamics blocks (`Final Gibbs free energy`, enthalpy,
  QRRHO tables): derived quantities are never stored.
- Basis-set and ECP citation extraction into `method` (the keyword line is
  the method of record).
- Input-deck-only files and `****END OF INPUT****` stubs with no job output
  — such content is an unrecognized or truncated outcome by the rules
  above, not a special case.

## Gherkin Scenarios <!-- rq-32f97fe7 -->

```gherkin
Feature: ORCA parser

  Background:
    Given the OrcaParser

  @rq-5b9c015a
  Scenario: The ASCII-art banner is detected
    Given a head containing the line "                                 * O   R   C   A *"
    When detect is called
    Then it returns true

  @rq-5f31c98f
  Scenario: An unrelated head is not claimed
    Given a head without the ORCA banner line
    When detect is called
    Then it returns false

  @rq-eb7784d8
  Scenario: A normally terminated opt+freq run yields the full observable set
    Given ORCA output content with the banner, "Program Version 6.0.0", the input echo line
    "|  1> ! wB97x-3c Opt Freq", "Total Charge Charge .... 0", "Multiplicity Mult .... 1",
    FINAL SINGLE POINT ENERGY lines, a final CARTESIAN COORDINATES (ANGSTROEM) table,
    a VIBRATIONAL FREQUENCIES section starting with six 0.00 modes, a MULLIKEN ATOMIC
    CHARGES block, a DIPOLE MOMENT block, a "Point Group:  C1, Symmetry Number:   1" line,
    and "****ORCA TERMINATED NORMALLY****"
    When the content is parsed
    Then the record has status ok
    And the record has program_version "6.0.0" and method "wB97x-3c"
    And the record has charge 0 and multiplicity 1
    And the record has electronic_energy equal to the last FINAL SINGLE POINT ENERGY
    And the record has the final geometry, all frequencies including the six near-zeros,
    Mulliken charges, and dipole
    And the record has rotational_symmetry_number 1

  @rq-b7387503
  Scenario: A single-point-only run reports the last job's values
    Given output with two FINAL SINGLE POINT ENERGY lines -199.634522052117 and -199.642065483666
    When the content is parsed
    Then the record has electronic_energy -199.642065483666

  @rq-9d9e798e
  Scenario: Imaginary modes keep their sign
    Given a VIBRATIONAL FREQUENCIES line "     6:     -42.13 cm**-1"
    When the content is parsed
    Then the stored frequencies include -42.13

  @rq-a5fee06d
  Scenario: A frequency calculation without a terminated run is truncated
    Given otherwise complete output ending inside the frequency section, with no termination marker
    When the content is parsed
    Then the record has status truncated
    And observables visible in the windows are still present

  @rq-0f8ebb73
  Scenario: A recorded failed run is classified failed
    Given the ORCA failed fixture from the corpus (the program's own error
    report, in place of the normal marker)
    When the content is parsed
    Then the record has status failed
    And the record's error is non-empty

  @rq-69c48453
  Scenario: A single-point file reports no frequencies or symmetry number
    Given output of one terminated SCF job with no VIBRATIONAL FREQUENCIES section
    When the content is parsed
    Then the record has status ok
    And the record has no vibrational_frequencies and no rotational_symmetry_number

  @rq-ec125e27
  Scenario: A hidden dipole block leaves the dipole absent
    Given uncovered windows whose final DIPOLE MOMENT block lies in the elided middle
    When the windows are parsed
    Then the record has no dipole_moment

  @rq-e770b7b2
  Scenario: The corpus fixture conforms
    Given the manifest entry for the successful ORCA fixture
    When the file is parsed through the standard registry
    Then all declared observables match under the harness comparison rules
```
