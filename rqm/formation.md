# Feature: Formation Energies from Monomer References <!-- rq-f8ebbc84 -->

Clusters are put in context by their formation energies: how much a
structure's energy differs from a stoichiometric mix of isolated,
single-component reference structures. A cluster database already carries
its own references — the monomers. JKQC-style, CJKQC scans the run's
final records — exactly the records the run prints — for monomers, and
the `--formation` flag prints each printed record's formation energy and
formation free energy as a section after the normal property table.
Nothing is stored: formation values are derived at request time from the
records' observables under the run's thermochemistry conditions
(`rqm/thermo.md`), so temperature, reference frequency, and unit changes
apply to them like to any derived property.

## Monomer References <!-- rq-1b571f91 -->

- A **monomer record** is a record with status `ok` whose `components` map
  (`rqm/composition.md`) is exactly one entry with count 1 — components
  `{am: 1}` qualifies, `{am: 2}` and `{am: 1, sa: 1}` do not. Records
  without components never qualify.
- Monomers are collected from the run's **final record set** — exactly the
  records the run prints: the pipeline survivors after every manipulation
  stage (`--extract`, cutoffs, `--uniq`, `--reacted`, `--select`, `--glob`;
  `rqm/filtering.md`) for a manipulation run, every loaded record for a
  print-only run. A monomer a filter removes is gone: it prints no row and
  serves as no reference.
- For each component code present in that set, the **reference** is a
  single monomer record: the first of that code's monomers under the run's
  sort ordering (`rqm/filtering.md` Sort) — `--sort g` the lowest Gibbs
  free energy, `--sort rg`, `--sort mass`, `--sort name` the first under
  that key, and `--sort el` or no `--sort` at all the lowest electronic
  energy. The ordering is ascending — `--reverse` orients the table,
  never the reference — ties keep the first in load order, and for the
  numeric keys a monomer whose sort-key value is undefined orders after
  every defined one. Under `--sort name` the undefined case is a nameless
  monomer, which orders before every named one — the nameless-first
  carve-out of `rqm/filtering.md` Sort.
- Both reference values come from that one record: the **electronic
  reference** is its `electronic_energy`, the **Gibbs reference** its
  Gibbs free energy derived at the run's conditions (`rqm/thermo.md`).
  Under `--sort g` the lowest-G conformer supplies the electronic
  reference too, even when another conformer's electronic energy is lower.
  A level whose value the chosen record lacks (no electronic energy,
  undefined thermochemistry) simply has no reference at that level.
- A code with no qualifying monomer has no reference.

## Formation Values <!-- rq-c3425396 -->

For a printed record with components map C = {codeᵢ: nᵢ}:

- **Formation energy** (Eh) = `electronic_energy` − Σᵢ nᵢ ·
  E_ref(codeᵢ), the electronic monomer reference above.
- **Formation free energy** (Eh) = G(record) − Σᵢ nᵢ · G_ref(codeᵢ), with
  both Gibbs values derived at the run's conditions (`rqm/thermo.md`).
- A level's value is undefined when the record lacks `components`, lacks
  the level's input energy (electronic energy for the el level, defined
  thermochemistry for the g level), or any referenced code lacks that
  level's monomer reference.
- Undefined is a property of the level: a record can have a defined
  formation energy and an undefined formation free energy (a monomer
  without frequencies, say), and vice versa.

The values follow the sign convention "negative = lower than the isolated
monomers"; the binding energy is the same number negated, and no separate
binding column exists.

## Printing <!-- rq-8dee4f4e -->

- `--formation` is a print flag: it produces output and satisfies the
  visible-outcome rule of `rqm/filtering.md` on its own, but it adds no
  column to the normal table.
- The **formation section** prints after the normal table (when other
  print flags requested one) and before any summary line, so stdout is
  always: main table, formation section, summary. A run with `--formation`
  but no column flags prints only the formation section.
- The section is a table: a header row `name el g`, then one row per
  printed record in run order. `name` is the record's name or `-`;
  `el` and `g` are the formation energy and formation free energy in the
  run's energy unit — kcal/mol under `--unit`, hartree otherwise
  (`rqm/filtering.md` Printing) — formatted like the corresponding energy
  columns, and `-` when undefined.
- `--formation` computes values for exactly the records the run prints:
  the pipeline survivors for a manipulation run, every loaded record for a
  print-only run.

## Feature API <!-- rq-e814b912 -->

### Types <!-- rq-85b79495 -->

- `MonomerReferences` — the per-code references in Eh: <!-- rq-a14c7897 -->
  `electronic: BTreeMap<String, f64>`, `gibbs: BTreeMap<String, f64>`.
- `Formation` — a record's two derived levels: <!-- rq-103447d2 -->
  `electronic: Option<f64>`, `gibbs: Option<f64>` (Eh).

### Functions <!-- rq-ded51dfb -->

- `collect_monomers(records: &[Record], memo: &ThermoMemo, sort: SortKey) -> MonomerReferences` <!-- rq-9e898298 -->
  — the reference scan over the given records (the run passes its final
  record set; `rqm/filtering.md`) with the sort-based selection above.
  The memo carries the run's thermochemistry conditions and derives each
  record at most once (`rqm/thermo.md` Performance).
- `formation_values(record: &Record, references: &MonomerReferences, memo: &ThermoMemo) -> Formation` <!-- rq-6ffbf9aa -->
  — the two levels with their independent definedness; the record's Gibbs
  value resolves through the derived-property registry on the memo
  (`rqm/thermo.md`).
- `print_formation_section(records: &[Record], references: &MonomerReferences, memo: &ThermoMemo, unit: Unit) -> String` <!-- rq-9503bcb2 -->
  — the section table, one row per record in the given order, `-` for
  undefined values and absent names.

### Crate Layout <!-- rq-22e7999c -->

- The scan, values, and section live in `src/formation.rs`; the library
  root re-exports the types and functions. No new dependencies; the CLI
  adds `--formation` and wires the section into the printing block.
- `tests/formation.rs` tests the scan and the values as pure functions <!-- rq-eb7da353 -->
  (sort-based reference selection, per-level definedness, missing
  references, stoichiometry) and the section formatting; CLI coverage
  extends `tests/cli.rs`.

## Out of Scope <!-- rq-39f86935 -->

- A separate monomer reference file or database: references come only
  from the run's own inputs.
- Formation energies for records without components (no composition, no
  reference stoichiometry) — they print `-`.
- Per-cluster reference overrides (pointing a code at a hand-picked
  conformer rather than the sort-selected one).
- Standard-state corrections beyond `rqm/thermo.md`'s ideal gas at 1 atm.
- Boltzmann-population and other ensemble-derived properties: future work;
  like formation values, they compute on the run's final record set, never
  on the pre-pipeline database.

## Gherkin Scenarios <!-- rq-077d9eed -->

```gherkin
Feature: Formation energies from monomer references

  Background:
    Given the default ThermoConditions of rqm/thermo.md

  # --- Reference scan ---

  @rq-34845056
  Scenario: A single-component count-one record is a monomer
    Given an ok record with components am = 1
    When collect_monomers is called with it
    Then the electronic reference for code "am" is that record's electronic energy

  @rq-56585302
  Scenario: Multi-component and multi-count records are not monomers
    Given ok records with components am = 2 and am = 1 sa = 1
    When collect_monomers is called with them
    Then no reference for "am" or "sa" is recorded from them

  @rq-64526ba5
  Scenario: Without --sort the lowest-energy conformer is the reference
    Given two ok records with components am = 1 and electronic energies -50.0 and -50.5
    When collect_monomers is called with them and the el sort key
    Then the electronic reference for "am" is -50.5

  @rq-64a72e10
  Scenario: --sort g makes the lowest-gibbs conformer the reference for both levels
    Given conformer A with the lowest electronic energy and conformer B with the lowest gibbs free energy, both am = 1
    When collect_monomers is called with them and the g sort key
    Then the electronic reference is conformer B's energy and the gibbs reference is conformer B's free energy

  @rq-90e2ea3e
  Scenario: --sort name takes the nameless monomer as the reference
    Given a nameless ok monomer with components am = 1 and electronic energy -50.0 and a named ok am = 1 monomer with electronic energy -50.5
    When collect_monomers is called with them and the name sort key
    Then the electronic reference for "am" is -50.0

  @rq-feddee4c
  Scenario: --reverse does not change the reference
    Given a 1am cluster and two am monomers whose gibbs values are -50.0 and -50.5
    When CJKQC is invoked with the database and "--sort g --reverse --formation" and no --out
    Then the cluster's g formation value is computed against the -50.5 monomer's gibbs

  @rq-7da88087
  Scenario: The reference's undefined level leaves that level undefined
    Given a na monomer chosen by the sort key that has an energy but no frequencies
    When collect_monomers is called with the rg sort key
    Then the electronic reference is defined and the gibbs reference is absent

  @rq-536786ea
  Scenario: Failed monomer records never qualify
    Given a failed record with components am = 1 and an ok record with components am = 1
    When collect_monomers is called with them
    Then the references come from the ok record alone

  @rq-9063d573
  Scenario: A monomer without thermochemistry has no Gibbs reference
    Given an ok single-component record without vibrational frequencies
    When collect_monomers is called with it
    Then the electronic reference is defined and the gibbs reference is absent

  # --- Values ---

  @rq-44b86475
  Scenario: A 2am1sa cluster forms from its stoichiometric monomers
    Given references am = -50.0 and sa = -40.0 and a cluster record with components am = 2 sa = 1 and electronic energy -141.0
    When formation_values is called
    Then the formation energy is -1.0 Eh

  @rq-36de901e
  Scenario: A monomer's own formation energy is zero
    Given a record with components am = 1 and the am references
    When formation_values is called
    Then the formation energy is 0 and the formation free energy is 0

  @rq-1b5e6c7a
  Scenario: A missing code reference leaves that level undefined
    Given a cluster with components am = 1 na = 1 and a database holding only the am monomer
    When formation_values is called
    Then the electronic level is None

  @rq-650192f2
  Scenario: The two levels are independently undefined
    Given a cluster whose only na monomer has an energy but no frequencies
    When formation_values is called
    Then the electronic level is Some and the gibbs level is None

  @rq-b02bcba1
  Scenario: Records without components form at neither level
    Given a record without components
    When formation_values is called
    Then both levels are None

  # --- Printing ---

  @rq-1b6b5757
  Scenario: The formation section follows the table and precedes the summary
    Given a database with a 2am cluster and am and sa monomers
    When CJKQC is invoked with the database and "--sort el --name --el --formation --out kept.jsonl"
    Then stdout shows the main table, then the formation section, then "filtered 3 records into kept.jsonl"

  @rq-0f78a33b
  Scenario: --formation alone is a visible outcome
    Given a database with a cluster and its monomers
    When CJKQC is invoked with the database and "--formation" and no --out
    Then the formation section is printed and no database file is written

  @rq-043c29fe
  Scenario: Undefined levels print as dashes
    Given a database with a cluster referencing a missing monomer code
    When CJKQC is invoked with the database and "--formation" and no --out
    Then the cluster's formation row shows - for both levels

  @rq-31a55ccc
  Scenario: Filtered-away monomers serve as no references
    Given a database with am and sa monomers and a 1am1sa cluster
    When CJKQC is invoked with the database, "--extract 1am1sa --formation" and no --out
    Then the section prints one row for the cluster with - for both levels

  @rq-b46a6e05
  Scenario: A print-only run sees every loaded monomer
    Given a database with am and sa monomers and a 1am1sa cluster
    When CJKQC is invoked with the database and "--formation" and no --out
    Then the cluster's formation row shows defined values

  @rq-9c5b3a40
  Scenario: The section obeys the unit setting
    Given a cluster with formation energy -0.01 Eh
    When CJKQC is invoked with the database and "--formation --unit" and no --out
    Then the el column prints the value in kcal/mol with 4 decimals

  @rq-e7f0ef7b
  Scenario: The g level follows the run's temperature
    Given a cluster and its monomers with defined thermochemistry
    When the same database is run with "--formation" at --temp 298.15 and at --temp 350
    Then the two runs' g columns differ
    And the two runs' el columns are equal
```
