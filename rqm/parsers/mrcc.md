# Feature: MRCC Parser <!-- rq-547a8fd6 -->

MRCC output files (the `minp` energy programs and friends) report a
correlated wavefunction calculation: an echoed input deck, a canonical
geometry printout, an energy build-up ending in the run's total energy, and
a normal/error termination banner. The MRCC parser reads such a file into
exactly one record for the run: the final total energy in hartree, the
printed geometry, and the run's identity (version, command, charge, spin).

Detection is the opening banner `MRCC program system`. The version and
command come from the head (banner line and echoed input); the energy comes
from the tail, where the program prints its final energy summary. The
canonical energy is the **last** total-energy line of that summary: for
local-correlation runs MRCC prints the canonical-method total and then the
approximation-corrected total (e.g. `Total LNO-CCSD(T) energy with MP2
corrections`), and the last one is the program's best answer — the value
JKQC's ground truth records (`rqm/parsers/conformance.md`).

Records use the canonical units of `rqm/record.md`. MRCC's canonical
geometry table is in bohr: the parser converts it to Ångström via the
shared `BOHR_TO_ANGSTROM` constant at extraction time
(`rqm/parser-framework.md`).

## Feature API <!-- rq-7415b243 -->

### Types <!-- rq-23172936 -->

- `MrccParser` — unit struct implementing the framework `Parser` trait with <!-- rq-d72bb4c5 -->
  `program()` = `Program::MRCC`. Lives in `src/parser/mrcc.rs`, re-exported
  by the library root.

### Detection <!-- rq-ad79643a -->

- `detect(head)` returns the byte offset of the earliest line containing <!-- rq-355e66c9 -->
  `MRCC program system`, or `None` when no such line is visible.

### Extraction <!-- rq-85ba0ac2 -->

- `program_version` — the token in `Version: <v>`. <!-- rq-a7d9edb9 -->
- `command` — from the echoed `Input file:` deck: the keyword lines from <!-- rq-b2af2117 -->
  the start of the deck (original case, taken from the echo rather than
  the lowercased `Keywords:` block) joined with single spaces, ending
  before the first of the `mem=`, `charge=`, `mult=`, or `geom=` lines
  (charge and multiplicity live in their own record fields); an empty
  block or a missing echo leaves the field absent
  (`"calc=LCCSD(T) basis=aug'-cc-pVTZ scfalg=direct"`).
- `charge` — the value of `Charge:           <n>`. <!-- rq-7c34ac9b -->
- `multiplicity` — the value of `Spin multiplicity:  <n>`. <!-- rq-a849a9cc -->
- `geometry` — the table under the header line <!-- rq-eb256ed1 -->
  `Cartesian coordinates [bohr]`: columns are index, element symbol, x, y,
  z, mass, Z; read with `read_geometry_table` using `Symbol(1)`,
  coordinate columns 2–4, `LengthUnits::Bohr`, and `expected_rows` from
  `Number of atoms:  <n>`.
- `electronic_energy` — the last line of the energy summary matching <!-- rq-3028c385 -->
  `Total <...> energy <...> [au]: <value>` whose text does not contain
  `correlation energy`; its value, in hartree. No such line → absent.
- `scf_energy` — the value of the last <!-- rq-ef61dab4 -->
  `***FINAL HARTREE-FOCK ENERGY: <value> [AU]` line, in hartree: the
  converged HF total the correlation method builds on. No such line →
  absent.

### Status Classification <!-- rq-b41a1001 -->

- `ok` — the tail carries `Normal termination of mrcc`. <!-- rq-97790ec7 -->
- `failed` — no normal marker and the tail carries MRCC's own error or <!-- rq-5825f5e3 -->
  abort report. The banner wording is version-dependent; it is pinned by
  the corpus failed fixture (`rqm/parsers/conformance.md`), not by this
  document. The error text names the reported message.
- `truncated` — neither marker present. <!-- rq-e6e7e0ad -->

## Out of Scope <!-- rq-8c38a83e -->

- Gradient and geometry-relaxation outputs, frequency calculations, and
  per-property printouts (dipoles, densities): the corpus covers
  single-point energy runs only, and no other situation is claimed.
- Multi-run files (several programs appended into one output): the final
  run's summary is the record, under the same last-occurrence rule.

## Gherkin Scenarios <!-- rq-7b35091a -->

```gherkin
Feature: MRCC parser

  Background:
    Given the MrccParser

  @rq-33ab6159
  Scenario: The MRCC banner is detected
    Given a head containing the line "MRCC program system"
    When detect is called
    Then it returns Some with the byte offset of the matching line

  @rq-8890cc1d
  Scenario: An unrelated head is not claimed
    Given a head without the MRCC banner
    When detect is called
    Then it returns None

  @rq-de12aef8
  Scenario: A normally terminated run yields the observable set
    Given MRCC output with the banner, "Version: 25.1.2", an input echo containing
    "calc=LCCSD(T)", "basis=aug'-cc-pVTZ", "scfalg=direct", "mem=86GB", "charge=0",
    and "mult=1", "Charge: 0", "Spin multiplicity: 1",
    a "Cartesian coordinates [bohr]" table for 10 atoms, an energy summary, and
    "Normal termination of mrcc."
    When the content is parsed
    Then the record has status ok
    And the record has program_version "25.1.2"
    And the record has command "calc=LCCSD(T) basis=aug'-cc-pVTZ scfalg=direct"
    And the record has charge 0 and multiplicity 1
    And the record's geometry equals the table multiplied by BOHR_TO_ANGSTROM

  @rq-e719ddf4
  Scenario: The corrected total energy wins
    Given an energy summary printing in order: "Total CCSD(T) energy [au]: -134.934017249766",
    "CCSD(T) correlation energy + MP2 corrections [au]: -0.641901761240", and
    "Total LNO-CCSD(T) energy with MP2 corrections [au]: -134.936294839321"
    When the content is parsed
    Then the record has electronic_energy -134.936294839321

  @rq-a60f442f
  Scenario: A run without local corrections reports its plain total
    Given an energy summary whose last non-correlation total line is
    "Total CCSD(T) energy [au]: -134.934017249766"
    When the content is parsed
    Then the record has electronic_energy -134.934017249766

  @rq-d82e314a
  Scenario: scf_energy is the final Hartree-Fock energy
    Given output whose last "***FINAL HARTREE-FOCK ENERGY:" line prints -134.2943930780812707
    and whose energy summary ends with
    "Total LNO-CCSD(T) energy with MP2 corrections [au]: -134.936294839321"
    When the content is parsed
    Then the record has scf_energy -134.2943930780812707
    And the record has electronic_energy -134.936294839321

  @rq-16ad220b
  Scenario: A run without the HF energy line carries no scf_energy
    Given an energy summary with no "***FINAL HARTREE-FOCK ENERGY:" line
    When the content is parsed
    Then the record has no scf_energy

  @rq-b8a853f8
  Scenario: The command block stops at the first resource or geometry key
    Given an input echo with "calc=MP2" and "scfalg=direct" followed by "charge=0"
    When the content is parsed
    Then the record has command "calc=MP2 scfalg=direct"

  @rq-91435a73
  Scenario: A recorded failed run is classified failed
    Given the MRCC failed fixture from the corpus (the program's own error
    report, with no normal termination banner)
    When the content is parsed
    Then the record has status failed
    And the record's error is non-empty
    And observables parsed from the head remain present

  @rq-be38168f
  Scenario: A run cut mid-flight is truncated
    Given otherwise valid output ending inside the CC iterations with no termination banner
    When the content is parsed
    Then the record has status truncated
    And the record has no electronic_energy

  @rq-fd767996
  Scenario: A hidden geometry table leaves geometry absent
    Given uncovered windows whose "Cartesian coordinates" header lies in the elided middle
    When the windows are parsed
    Then the record has no geometry

  @rq-465a02d6
  Scenario: The corpus fixture conforms
    Given the manifest entry for the successful MRCC fixture
    When the file is parsed through the standard registry
    Then all declared observables match under the harness comparison rules
```
