# Feature: Gaussian Parser <!-- rq-33106344 -->

Gaussian log files chain jobs inside one file — an optimization followed by
a frequency calculation is the corpus shape — and the Gaussian parser
reads such a log into exactly one record for the **final job**: the last
`SCF Done` energy, the final job's geometry and population analysis, and
the frequency table if the final job is a frequency run
(`rqm/parser-framework.md`).

Detection is the version banner: every Gaussian log prints a line naming
its version (`Gaussian 03`, `Gaussian 09`, `Gaussian 13`, `Gaussian 16`, …)
within its leading lines. Every observable is read from the last visible
occurrence of its marker, because each chained job reprints the SCF header,
geometry, and property blocks; the final job's termination line, when
present, sits at the file end.

Records use the canonical units of `rqm/record.md`. Gaussian prints energies
and coordinates in the record's own units (hartree, Å), but its dipole
table in Debye: the parser converts Debye to atomic units with the CODATA
constant `DEBYE_PER_E_A0 = 2.541746473` at extraction time, so the record
stores e·a₀ like every other source.

## Feature API <!-- rq-0ef73ad1 -->

### Types <!-- rq-ea1f9893 -->

- `GaussianParser` — unit struct implementing the framework `Parser` trait <!-- rq-9ea94102 -->
  with `program()` = `Program::GAUSSIAN`. Lives in `src/parser/gaussian.rs`,
  re-exported by the library root.

### Detection <!-- rq-918b7b58 -->

- `detect(head)` returns the byte offset of the earliest line matching the <!-- rq-a24eec14 -->
  regex `Gaussian (03|09|11|13|16)`, or `None` when no such line is visible.

### Extraction <!-- rq-35f562fe -->

Each observable is taken from the **last visible** match; a marker hidden
in an elided window yields an absent observable, never a guess.

- `program_version` — from the first banner line <!-- rq-e9a83a65 -->
  ` Gaussian <n>, Revision <rev>,` as `"Gaussian <n> Rev. <rev>"`; when no
  Revision line exists, `"Gaussian <n>"`.
- `command` — the route section: the content of the `# …` line and its <!-- rq-6aa61c29 -->
  continuation lines (the echo wraps the route in all-dash rules, so a
  rule or a blank line ends the section), with `#` stripped and
  whitespace collapsed; stored verbatim as reported, including job
  keywords (`"wb97xd 6-31++g** opt freq gfinput IOP(6/7=1)"`). No route
  section → absent.
- `charge` / `multiplicity` — the values of the last <!-- rq-373378e9 -->
  `Charge = <c> Multiplicity = <m>` line (each job echoes one).
- `electronic_energy` — the value after `=` in the last <!-- rq-d1446f61 -->
  `SCF Done:  E(<model>) = <value>` line.
- `geometry` — the table under the last `Standard orientation:` header: <!-- rq-28fce0b1 -->
  columns are center number, atomic number, type, x, y, z; read with
  `read_geometry_table` using `AtomicNumber(1)`, coordinate columns 3–5,
  Ångström, and `expected_rows` from the last `NAtoms= <n>` when visible.
- `vibrational_frequencies` — every value on every <!-- rq-363a8a65 -->
  `Frequencies --` line of the last frequency job, in printed order
  (up to three values per line); a leading minus marks an imaginary mode.
  The parsed values pass through `vibrational_modes`
  (`rqm/parser-framework.md`) like every other program's: Gaussian
  prints no translational/rotational modes, so the filter is inert on
  its tables, but the record's vibrational-mode convention holds
  program-independently.
- `dipole_moment` — the `X= <x> Y= <y> Z= <z>` values of the last <!-- rq-d5dd0577 -->
  `Dipole moment (field-independent basis, Debye):` line, converted from
  Debye to e·a₀ by dividing by `DEBYE_PER_E_A0`.
- `mulliken_charges` — the third column of the rows under the last <!-- rq-5ba5c8ec -->
  `Mulliken charges:` header, ending at `Sum of Mulliken charges`, in
  geometry atom order.
- `rotational_symmetry_number` — the value of the last <!-- rq-6c1202df -->
  `Rotational symmetry number  <n>.` line.

### Status Classification <!-- rq-8ebc2a41 -->

- `ok` — the tail carries `Normal termination of Gaussian`. <!-- rq-b47001c1 -->
- `failed` — the content carries `Error termination` after the final job's <!-- rq-7dbde5b0 -->
  data; the error text quotes the reported message.
- `truncated` — neither marker present: the final job never printed its <!-- rq-b90c3497 -->
  end.
- Earlier jobs' `Normal termination` lines are irrelevant to the record;
  the final job's marker governs status because its data is what the
  parser extracted.

## Out of Scope <!-- rq-e9a48d20 -->

- Route parsing into separate functional/basis fields — `command` is the
  route text; normalization is a reporting concern.
- Link-0 echo sections (`$D`, `23\0\\` lines) and basis-set dumps.
- Thermochemistry recomputation — downstream (`rqm/ARCHITECTURE.md`).

## Gherkin Scenarios <!-- rq-7a42c9fb -->

```gherkin
Feature: Gaussian parser

  Background:
    Given the GaussianParser

  @rq-799f6155
  Scenario: The version banner is detected
    Given a head containing the line " Gaussian 16, Revision B.01,"
    When detect is called
    Then it returns Some with the byte offset of the matching line

  @rq-9f5aa509
  Scenario: An unrelated head is not claimed
    Given a head without any "Gaussian <version>" line
    When detect is called
    Then it returns None

  @rq-d8a0968b
  Scenario: An opt+freq log yields the final job's observables
    Given Gaussian log content with the G16 banner, the route line "# wb97xd 6-31++g** opt freq",
    two jobs each echoing "Charge =  0 Multiplicity = 1" and SCF Done lines, a final
    "Standard orientation:" table, "Frequencies --" lines, a "Mulliken charges:" block,
    a "Dipole moment (field-independent basis, Debye):" line, a "Rotational symmetry number  1."
    line, and a final "Normal termination of Gaussian 16"
    When the content is parsed
    Then the record has status ok
    And the record has program_version "Gaussian 16 Rev. B.01"
    And the record has command "wb97xd 6-31++g** opt freq"
    And the record has charge 0 and multiplicity 1
    And the record has electronic_energy equal to the last SCF Done value
    And the record has the final geometry and the concatenated frequency list
    And the record's dipole equals the printed Debye components divided by 2.541746473
    And the record has rotational_symmetry_number 1

  @rq-bf318b99
  Scenario: The last SCF Done wins
    Given two SCF Done lines with values -1526.66238486 and -1526.66937796
    When the content is parsed
    Then the record has electronic_energy -1526.66937796

  @rq-c7886a80
  Scenario: An imaginary frequency keeps its sign
    Given a line "Frequencies --   -105.2345               46.2953                54.2757"
    When the content is parsed
    Then the stored frequencies start with -105.2345, 46.2953, 54.2757

  @rq-e6338052
  Scenario: An error termination is failed
    Given a log ending with "Error termination via Lnk1e"
    When the content is parsed
    Then the record has status failed
    And the error mentions the error termination
    And observables parsed before the error remain present

  @rq-b721514a
  Scenario: A cut-off final job is truncated
    Given a log whose last job ends mid-frequency section with no termination line
    When the content is parsed
    Then the record has status truncated

  @rq-2d633569
  Scenario: A single-point log reports no frequencies
    Given a terminated log with one SCF job and no "Frequencies --" lines
    When the content is parsed
    Then the record has status ok
    And the record has no vibrational_frequencies

  @rq-58995f44
  Scenario: A hidden dipole section leaves the dipole absent
    Given uncovered windows whose final dipole section lies in the elided middle
    When the windows are parsed
    Then the record has no dipole_moment

  @rq-5e5adfe9
  Scenario: The corpus fixture conforms
    Given the manifest entry for the successful Gaussian fixture
    When the file is parsed through the standard registry
    Then all declared observables match under the harness comparison rules
```
