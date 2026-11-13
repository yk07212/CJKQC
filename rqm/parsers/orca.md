# Feature: ORCA Parser <!-- rq-0aab8d11 -->

ORCA output files chain driver jobs — a geometry relaxation followed by a
frequency calculation is the corpus shape — and the ORCA parser reads such
a file into exactly one record for the final job: the last
`FINAL SINGLE POINT ENERGY`, the final Cartesian geometry, the canonical
`VIBRATIONAL FREQUENCIES` section, and the final population, dipole,
polarizability, and point-group analyses (`rqm/parser-framework.md`).

Detection is the banner: ORCA opens every output with its ASCII art,
including the line `* O R C A *` (letters separated by runs of spaces).
Every observable is read from the last visible occurrence of its section
header, because each chained job reprints the SCF and property blocks for
the current geometry; the last occurrence is the final job's answer. The
record uses the canonical units of `rqm/record.md`; ORCA prints energies in
hartree, coordinates in Å, Mulliken charges and dipole components in atomic
units, frequencies in cm⁻¹, so extraction is unit-preserving. The raw
Cartesian polarizability tensor is likewise printed in atomic units and
stored unchanged.

The frequency list is the `VIBRATIONAL FREQUENCIES` section, not the
thermochemistry summary tables: that section carries all 3N modes with an
explicit minus sign on imaginary modes. The record stores only genuine
vibrational modes — the six near-zero translational/rotational entries
are dropped by the shared `vibrational_modes` extractor at extraction
time (`rqm/parser-framework.md`), the same convention as the Gaussian
table, which prints no trivial modes at all.

## Feature API <!-- rq-45c63967 -->

### Types <!-- rq-3bb529ad -->

- `OrcaParser` — unit struct implementing the framework `Parser` trait with <!-- rq-1831b7fb -->
  `program()` = `Program::ORCA`. Lives in `src/parser/orca.rs`, re-exported
  by the library root.

### Detection <!-- rq-93f2fd85 -->

- `detect(head)` returns the byte offset of the earliest line matching the <!-- rq-762ab556 -->
  banner pattern `\* O\s+R\s+C\s+A \*` (the ASCII-art title line), or `None`
  when no such line is visible.

### Extraction <!-- rq-34b49160 -->

Each observable is taken from the **last visible** match; a marker hidden
in an elided window yields an absent observable, never a guess.

- `program_version` — the token in `Program Version <v> - ... RELEASE`. <!-- rq-64d42f21 -->
- `command` — all text after `!` in the echoed keyword line of the input <!-- rq-527a66db -->
  block (the `INPUT FILE` section, line `| 1> ! <keywords...>`),
  whitespace-trimmed and stored verbatim with method and drivers alike
  (`"wB97x-3c Opt Freq"` from `! wB97x-3c Opt Freq`). No echoed `!`
  line → absent.
- `charge` — `Total Charge           Charge          .... <n>` value. <!-- rq-b846e66c -->
- `multiplicity` — `Multiplicity           Mult            .... <n>` value <!-- rq-b5b01447 -->
  (the summary form; the per-cycle `Multiplicity assumed :` lines are not
  the source).
- `electronic_energy` — the last `FINAL SINGLE POINT ENERGY <value>`. <!-- rq-610cdd9d -->
- `scf_energy` — the converged SCF total of the last SCF section, in <!-- rq-7331508e -->
  hartree: the `TOTAL ENERGY` value of the SCF summary box
  (`| TOTAL ENERGY  <value> Eh |`) in newer printings, the
  `Total Energy       :  <value> Eh` line at the section's end in older
  ones — the corpus pins both spellings against density-functional and
  xtb-in-ORCA printings; no correlated-method ORCA output pins them, so
  a post-SCF block that reused either shape would be read as the SCF
  total. This is the SCF cycle's own total
  (the Hartree–Fock energy of a correlated job, the DFT energy of a
  density-functional one), not the `FINAL SINGLE POINT ENERGY` value,
  which adds the method's correlation and any additive corrections on
  top. No visible SCF total → absent. The line test admits candidates on
  the line's leading bytes — the first field is `|` or `Total` — before
  any field iteration, and fields are iterated, never collected
  (`rqm/parser-framework.md`, Performance Constraints); the walk rejects
  the window's other lines at leading-byte cost.
- `geometry` — the table under the last `CARTESIAN COORDINATES <!-- rq-8d91e753 -->
  (ANGSTROEM)` header (between its dashed rules): `read_geometry_table`
  with `Symbol(0)`, coordinate columns 1–3, Ångström.
- `vibrational_frequencies` — the entries of the last `VIBRATIONAL <!-- rq-17559f33 -->
  FREQUENCIES` section: lines `<mode>: <value> cm**-1` (an optional
  trailing `i` on the value also marks an imaginary mode and is stored as
  the negative value); the section ends at its trailing rule. The parsed
  values pass through `vibrational_modes`: near-zero translational/
  rotational entries are dropped, every genuine mode — imaginary included
  — is stored as printed, and a section whose visible modes are all
  trivial leaves the field absent.
- The frequency span and seam honesty follow the Gaussian rule
  (`rqm/parsers/gaussian.md`): with the last `VIBRATIONAL FREQUENCIES`
  header visible in the tail the section is whole inside the windows;
  when it is not, the section rows visible after a head-window header
  plus every frequency-shaped row at the head of the tail are gathered,
  and if the stored (post-filter) count then falls below 3N−6 while the
  geometry shows N atoms, the record is demoted to `truncated` with an
  error saying the frequency table may be incomplete and suggesting a
  rerun with larger `--head-bytes`/`--tail-bytes` windows. The gather
  locates its candidate rows by a literal scan of the row marker
  (`cm**-1`) and verifies the row shape on the candidates only, in
  window order — never a field split of every line
  (`rqm/parser-framework.md`, Performance Constraints).
- `dipole_moment` — the `Total Dipole Moment    :     <x> <y> <z>` line of <!-- rq-21041eac -->
  the last `DIPOLE MOMENT` block, in atomic units, stored unchanged.
- `polarizability` — the nine values of the raw Cartesian tensor in the <!-- rq-2ec4e6d2 -->
  last visible `THE POLARIZABILITY TENSOR` block: the three rows under
  `The raw cartesian tensor (atomic units):`, read row-major as
  `[αxx, αxy, αxz, αyx, αyy, αyz, αzx, αzy, αzz]`, in atomic units and
  stored unchanged. The block qualifies only when all three rows carry
  exactly three finite values; a header without that complete table, or a
  block hidden in an elided window, leaves the field absent. The
  `Isotropic polarizability` line is not stored: the isotropic value is
  derived from the tensor at printing.
- `mulliken_charges` — the values of the rows `<idx> <El> : <q>` under the <!-- rq-80fdb0f3 -->
  last `MULLIKEN ATOMIC CHARGES` header, in geometry atom order. The symbol
  is printed in a two-character field that ends at the colon, so a
  two-character symbol reads `<idx> <El>: <q>` with no space before the
  colon; a row qualifies in either spelling, and its charge is the row's
  last field.
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
- A banner-`ok` record whose fallback-gathered frequency section fails
  the 3N−6 seam-honesty count is demoted to `truncated` (the frequency
  section above); `failed` outcomes and header-delimited sections are
  never demoted.
- Observables extracted before the failure are carried in the record
  (`rqm/record.md`).

## Out of Scope <!-- rq-29e0af6a -->

- Reading the thermodynamics blocks (`Final Gibbs free energy`, enthalpy,
  QRRHO tables): derived quantities are never stored.
- Basis-set and ECP citation extraction, and parsing the keyword line into
  separate method and driver fields — `command` is the verbatim keyword
  text; normalization is a reporting concern.
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
    Then it returns Some with the byte offset of that line

  @rq-5f31c98f
  Scenario: An unrelated head is not claimed
    Given a head without the ORCA banner line
    When detect is called
    Then it returns None

  @rq-eb7784d8
  Scenario: A normally terminated opt+freq run yields the full observable set
    Given ORCA output content with the banner, "Program Version 6.0.0", the input echo line
    "|  1> ! wB97x-3c Opt Freq", "Total Charge Charge .... 0", "Multiplicity Mult .... 1",
    FINAL SINGLE POINT ENERGY lines, a final CARTESIAN COORDINATES (ANGSTROEM) table,
    a VIBRATIONAL FREQUENCIES section starting with six 0.00 modes, a MULLIKEN ATOMIC
    CHARGES block, a DIPOLE MOMENT block, a THE POLARIZABILITY TENSOR block whose raw
    Cartesian rows are complete, a "Point Group:  C1, Symmetry Number:   1" line,
    and "****ORCA TERMINATED NORMALLY****"
    When the content is parsed
    Then the record has status ok
    And the record has program_version "6.0.0" and command "wB97x-3c Opt Freq"
    And the record has charge 0 and multiplicity 1
    And the record has electronic_energy equal to the last FINAL SINGLE POINT ENERGY
    And the record has the final geometry, every vibrational frequency
    except the six near-zeros, Mulliken charges, dipole, and polarizability tensor
    And the record has rotational_symmetry_number 1

  @rq-f11e09b1
  Scenario: Trivial modes are dropped and a genuine imaginary mode is kept
    Given a VIBRATIONAL FREQUENCIES section with six 0.00 modes followed by
    a -42.13 imaginary mode and real modes 38.55 and 3641.12
    When the content is parsed
    Then the stored frequencies are -42.13, 38.55, and 3641.12

  @rq-d3ed1e6c
  Scenario: Mulliken rows of both symbol spellings yield every charge
    Given a MULLIKEN ATOMIC CHARGES section whose rows read
    "   0 Na:    0.861759", "   1 I :   -0.797525", and "   2 H :    0.349900"
    When the content is parsed
    Then the record's mulliken_charges are 0.861759, -0.797525, and 0.349900

  @rq-48469bb2
  Scenario: A frequency section of only trivial modes leaves the field absent
    Given a VIBRATIONAL FREQUENCIES section holding only 0.00 modes
    When the content is parsed
    Then the record has no vibrational_frequencies

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

  @rq-7bca16e5
  Scenario: scf_energy is the SCF total, distinct from the final single point energy
    Given output whose last SCF summary box prints TOTAL ENERGY -28.322836452494 Eh
    and whose last FINAL SINGLE POINT ENERGY line prints -28.322836452490
    When the content is parsed
    Then the record has scf_energy -28.322836452494
    And the record has electronic_energy -28.322836452490

  @rq-97f5385b
  Scenario: A job without an SCF section carries no scf_energy
    Given output with no SCF total line in any spelling
    When the content is parsed
    Then the record has no scf_energy

  @rq-23b2c5e0
  Scenario: The SCF-total line test admits exactly the two spellings
    Given output whose SCF sections print, among ordinary output lines,
    the box line "| TOTAL ENERGY   -28.3 Eh   |" and the section line
    "Total Energy    :     -28.4 Eh", and near-miss lines
    "| TOTAL ENERGY   -28.5 |" (no Eh), "TOTAL ENERGY -28.6 Eh"
    (no leading bar), and "Total Energy   -28.7 Eh" (no colon)
    When the content is parsed
    Then the record's scf_energy is the later genuine spelling's value
    And no near-miss line contributed a value
    And the line test field-split none of the ordinary lines

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

  @rq-292d4c94
  Scenario: The raw Cartesian polarizability tensor is stored row-major
    Given a THE POLARIZABILITY TENSOR block whose raw cartesian rows are
    "68.96534 -4.98429 -0.17212", "-4.98429 54.79554 -1.20060", and
    "-0.17212 -1.20060 59.72225"
    When the content is parsed
    Then the record's polarizability is
    [68.96534, -4.98429, -0.17212, -4.98429, 54.79554, -1.20060,
    -0.17212, -1.20060, 59.72225]

  @rq-dff9b11a
  Scenario: An incomplete polarizability table leaves the tensor absent
    Given a THE POLARIZABILITY TENSOR block whose raw Cartesian table has fewer than three complete rows
    When the content is parsed
    Then the record has no polarizability

  @rq-9016333b
  Scenario: A hidden polarizability block leaves the tensor absent
    Given uncovered windows whose final THE POLARIZABILITY TENSOR block lies in the elided middle
    When the windows are parsed
    Then the record has no polarizability

  @rq-11ce9c38
  Scenario: The polarizability corpus fixture conforms
    Given the manifest entry for tests/fixtures/orca/1am1sa1w-111_7_673.out
    When the file is parsed through the standard registry
    Then the record has status ok
    And the record's polarizability is
    [68.96534, -4.98429, -0.17212, -4.98429, 54.79554, -1.20060,
    -0.17212, -1.20060, 59.72225]

  @rq-82f9c544
  Scenario: An elided frequency header falls back to the visible rows
    Given uncovered windows of a three-atom run whose final VIBRATIONAL FREQUENCIES
    header lies in the head with some genuine section rows visible below it and the
    section's remaining genuine rows opening the tail, and a normal termination banner
    When the windows are parsed
    Then the record has status ok
    And the record's frequencies hold every visible genuine mode, head-window rows
    first, in printed order

  @rq-7ab0f27f
  Scenario: A seam-cut frequency section is stored truncated
    Given uncovered windows of a three-atom run in which only two genuine modes of a
    VIBRATIONAL FREQUENCIES section are visible and its header is not in the tail,
    and a normal termination banner
    When the windows are parsed
    Then the record has status truncated
    And the record's error says the frequency table may be incomplete

  @rq-e770b7b2
  Scenario: The corpus fixture conforms
    Given the manifest entry for the successful ORCA fixture
    When the file is parsed through the standard registry
    Then all declared observables match under the harness comparison rules
```
