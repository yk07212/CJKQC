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
apply to them like to any derived property. The `--conc` and `--cnt`
flags further evaluate the free-energy level at vapors of named
components at given concentrations (Formation at Given Concentrations).

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
- The section is a table: a header row `ct el g`, then one row per
  printed record in run order. `ct` is the record's composition token —
  the same alphabetical `2am3sa`-style pairs the `--ct` column prints
  (`rqm/filtering.md` Printing) — and `-` when the record has no
  components; `el` and `g` are the formation energy and formation free
  energy in the run's energy unit — kcal/mol under `--unit`, hartree
  otherwise (`rqm/filtering.md` Printing) — formatted like the
  corresponding energy columns, and `-` when undefined.
- `--formation` computes values for exactly the records the run prints:
  the pipeline survivors for a manipulation run, every loaded record for a
  print-only run.

## Corrected Formation Section <!-- rq-2f4611d0 -->

- `--formation-corr` prints a second formation section evaluated at the
  attached single-point corrections (`rqm/correction.md`). Every input energy
  is a corrected value; a record with no correction has no values at all —
  corrected formation numbers never mix levels.
- The section uses the same reference monomers as the raw section: the same
  monomer set and the same sort-based selection (Monomer References above). A
  code's corrected electronic reference is that monomer's correction energy,
  and its corrected Gibbs reference the monomer's `gcorr` value; a level's
  reference is undefined when the selected monomer carries no correction (or
  its corrected Gibbs is undefined).
- For a printed record with components map C = {codeᵢ: nᵢ}:
  - **Corrected formation energy** (Eh) = the record's correction energy −
    Σᵢ nᵢ · Ecorr_ref(codeᵢ).
  - **Corrected formation free energy** (Eh) = gcorr(record) − Σᵢ nᵢ ·
    Gcorr_ref(codeᵢ).
  - A level is defined iff the record's own corrected value at that level is
    defined and every referenced code has a defined corrected reference at
    that level.
- The flag is a print flag like `--formation`: it satisfies the
  visible-outcome rule of `rqm/filtering.md` on its own and adds no column to
  the normal table. The corrected section prints after the raw formation
  section (when `--formation` requested one) and before any summary line, so
  stdout is always: main table, formation section, corrected formation
  section, summary. A run with `--formation-corr` but no other print flag
  prints only the corrected section.
- The section is a table: a header row `ct elcorr gcorr`, then one row per
  printed record in run order. The composition token and the number
  formatting — the run's energy unit, kcal/mol under `--unit`
  (`rqm/filtering.md` Printing), formatted like the corresponding energy
  columns, `-` when undefined — are exactly the raw section's rules.

## Formation at Given Concentrations <!-- rq-10341821 -->

- `--conc CODE VALUE` (repeatable) and `--cnt` evaluate the formation
  free energies at vapors of named components whose concentrations differ
  from the standard state: the chemical potential of a vapor at partial
  pressure c is lower than its 1 atm standard-state value by R T ln(c/p°)
  per mole, and forming a cluster withdraws nᵢ vapor molecules of each
  component. The flags are settings flags (`rqm/thermo.md`): they
  configure the formation derivations, never count as manipulations or
  print flags, and a run that passes only them still needs `--out` or a
  print flag.
- VALUE is a partial pressure in Pa — a decimal or scientific-notation
  number — optionally suffixed, lowercase and without whitespace, with a
  unit: `100ppt` and `10ppb` are the mole fractions 10⁻¹² and 10⁻⁹ of the
  standard atmosphere (100·10⁻¹²·p° and 10·10⁻⁹·p° Pa), and `1e10cmmc` is
  10¹⁰ molecules per cm³, converted as c·10⁶·k_B·T Pa at the run's
  temperature. A value outside this grammar, non-finite, or not positive
  is a usage error (exit 2); the `^` power syntax other tools accept is
  not part of the grammar. The unit constants are the CODATA 2018 set of
  `rqm/thermo.md` Constants (p° = 101325 Pa).
- Each spec names one component code: a non-empty, letter-only code as
  `rqm/record.md` component validation requires. Naming the same code in
  two specs is a usage error. When a formation section is printed, every
  spec's code must appear as a component of at least one record in the
  section's record set — the run's final record set — or the run is a
  usage error; a code whose monomers were filtered away may still appear
  inside cluster compositions and simply forms no reference (Monomer
  References).
- With specs S and a record whose components map C = {codeᵢ: nᵢ}, the
  formation section's `g` value is

  g_conc = g_form − Σ_{codeᵢ ∈ S} nᵢ,eff · R T ln(cᵢ/p°) / (N_A E_h),

  with cᵢ the spec's resolved partial pressure and nᵢ,eff = nᵢ. Under
  `--cnt`, the self-consistent form, nᵢ,eff = nᵢ·(N−1)/N where
  N = Σ nᵢ over the record's whole components map. A spec whose code is
  absent from C contributes nothing, so coverage may be partial.
- The self-consistent form keeps the concentration term of every
  monomer at zero under any concentration, so the reference monomer's
  own formation free energy stays exactly 0 while a non-reference
  conformer keeps its conformer difference against the reference; the
  total concentration term of a fully covered N-molecule cluster is
  (N−1)·R T-weighted instead of N (JKQC's `-cnt`, "see Wyslouzil"; CJKQC
  keeps the flag name `--cnt`).
- Only the Gibbs levels shift: the raw section's `g` and the corrected
  section's `gcorr` (Corrected Formation Section) each subtract the same
  correction from their standard-state value. The electronic levels `el`
  and `elcorr` stay at the standard state — the concentration term is a
  chemical-potential (entropy) correction with no electronic counterpart,
  where JKQC applies its correction to every numeric formation column. A
  level that is undefined stays undefined; the correction never creates
  a value. A magnitude so extreme that the resolved pressure underflows
  below the smallest positive double, or the term itself overflows,
  makes the level undefined rather than infinite — the crate's numerics
  rule (`rqm/thermo.md` Numerics), never a printed infinity.
- p° is the 1 atm standard pressure of `rqm/thermo.md` Constants — the
  same standard state the translational entropy is evaluated at — so a
  spec at exactly p° reproduces the uncorrected values. A concentration
  above p° is accepted and lowers the formation free energy. The main
  table, cutoffs, sorting, `--glob`, and the composition token are
  untouched: `--conc` changes only the formation sections' `g`/`gcorr`
  values, printed in the run's energy unit like any formation value.

## Feature API <!-- rq-e814b912 -->

### Types <!-- rq-85b79495 -->

- `MonomerReferences` — the per-code references in Eh: <!-- rq-a14c7897 -->
  `electronic: BTreeMap<String, f64>`, `gibbs: BTreeMap<String, f64>`.
- `CorrectedMonomerReferences` — the per-code corrected references in Eh: <!-- rq-67b44e63 -->
  `electronic: BTreeMap<String, f64>`, `gibbs: BTreeMap<String, f64>`; a
  code appears only when its selected monomer defines that level's
  corrected value (`rqm/correction.md`).
- `Formation` — a record's two derived levels: <!-- rq-103447d2 -->
  `electronic: Option<f64>`, `gibbs: Option<f64>` (Eh).
- `ConcUnit` — `Pa | Ppt | Ppb | Cmmc`, the VALUE grammar's units. <!-- rq-7d64cce1 -->
- `ConcSpec` — one `--conc` spec: `code: String`, `magnitude: f64`, <!-- rq-c92b9ff1 -->
  `unit: ConcUnit`; `pressure(temperature: f64) -> f64` resolves the
  partial pressure in Pa: `Pa` the magnitude itself, `Ppt`/`Ppb`
  magnitude·10⁻¹²/10⁻⁹·p°, `Cmmc` magnitude·10⁶·k_B·temperature.
- `ConcConditions` — the run's concentration settings: <!-- rq-a0748032 -->
  `specs: Vec<ConcSpec>`, `self_consistent: bool`; `Default` is no specs
  and `--cnt` off.

### Functions <!-- rq-ded51dfb -->

- `parse_conc_value(text: &str) -> Option<(f64, ConcUnit)>` — the VALUE <!-- rq-f8a2fb33 -->
  grammar; `None` for a value outside the grammar, non-finite, or not
  positive (the CLI turns `None` into a usage error).
- `collect_monomers(records: &[Record], memo: &ThermoMemo, sort: SortKey) -> MonomerReferences` <!-- rq-9e898298 -->
  — the reference scan over the given records (the run passes its final
  record set; `rqm/filtering.md`) with the sort-based selection above.
  The memo carries the run's thermochemistry conditions and derives each
  record at most once (`rqm/thermo.md` Performance).
- `formation_values(record: &Record, references: &MonomerReferences, memo: &ThermoMemo, concs: &ConcConditions) -> Formation` <!-- rq-6ffbf9aa -->
  — the two levels with their independent definedness; the record's Gibbs
  value resolves through the derived-property registry on the memo
  (`rqm/thermo.md`) and, when defined, has the concentration term of
  `concentration_correction` subtracted; a correction that is not finite
  (an absurd magnitude) leaves the level undefined rather than infinite.
- `concentration_correction(components: &Components, concs: &ConcConditions, temperature: f64) -> f64` <!-- rq-2333d9cc -->
  — the term subtracted from a defined formation free energy, in Eh:
  Σ over the specs whose code is in the components map of
  nᵢ,eff · k_B T ln(cᵢ/p°) / E_h, with nᵢ,eff = nᵢ or, under
  `self_consistent`, nᵢ·(N−1)/N; 0 when no spec matches.
- `print_formation_section(records: &[Record], references: &MonomerReferences, memo: &ThermoMemo, unit: Unit, concs: &ConcConditions) -> String` <!-- rq-9503bcb2 -->
  — the section table with header `ct el g`, one row per record in the
  given order, `-` for undefined values and for the composition token
  when the record has no components.
- `corrected_monomer_references(records: &[Record], memo: &ThermoMemo, sort: SortKey) -> CorrectedMonomerReferences` <!-- rq-789635d3 -->
  — the same selection as `collect_monomers`, read through the corrected
  keys `elcorr`/`gcorr` (`rqm/correction.md`).
- `corrected_formation_values(record: &Record, references: &CorrectedMonomerReferences, memo: &ThermoMemo, concs: &ConcConditions) -> Formation` <!-- rq-fb892986 -->
  — the two corrected levels with their independent definedness; the
  corrected Gibbs value has the same concentration term subtracted, with
  the same non-finite guard.
- `print_formation_corr_section(records: &[Record], references: &CorrectedMonomerReferences, memo: &ThermoMemo, unit: Unit, concs: &ConcConditions) -> String` <!-- rq-4f20e170 -->
  — the corrected section table with header `ct elcorr gcorr`.

### Crate Layout <!-- rq-22e7999c -->

- The scan, values, and sections live in `src/formation.rs`; the library
  root re-exports the types and functions. No new dependencies; the CLI
  adds `--formation`, `--formation-corr`, `--conc`, and `--cnt`, parses
  the specs into `ConcConditions` with the usage-error validation above,
  and wires both sections into the printing block.
- `tests/formation.rs` tests the scan and the values as pure functions <!-- rq-eb7da353 -->
  (sort-based reference selection, per-level definedness, missing
  references, stoichiometry), the section formatting, the corrected
  section's selection and definedness, the VALUE grammar and pressure
  resolution, and the concentration correction's stoichiometry and
  self-consistent factor; CLI coverage extends `tests/cli.rs`.

## Out of Scope <!-- rq-39f86935 -->

- A separate monomer reference file or database: references come only
  from the run's own inputs.
- Formation energies for records without components (no composition, no
  reference stoichiometry) — they print `-`.
- Per-cluster reference overrides (pointing a code at a hand-picked
  conformer rather than the sort-selected one).
- Standard-state corrections to the thermochemistry model itself:
  `rqm/thermo.md` stays the ideal gas at 1 atm — `--conc` shifts only the
  formation levels and never re-derives a record's translational entropy —
  and liquid/solvent-phase reference states are not modeled.
- Vapor-depletion or Kelvin-equation self-consistency beyond `--cnt`'s
  (N−1)/N scaling: the given concentrations are taken as fixed inputs, not
  solved for.
- Boltzmann-population and other ensemble-derived properties beyond the
  formation sections: `rqm/population.md` computes populations on the run's
  final record set, like the formation values.

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

  @rq-2d5b9aba
  Scenario: The section identifies rows by composition token
    Given a database with a 2am1sa cluster and am and sa monomers
    When CJKQC is invoked with the database and "--formation" and no --out
    Then the section header is "ct el g"
    And the cluster's row starts with the composition token 2am1sa

  @rq-ed9e179c
  Scenario: A record without components has a dash composition token
    Given a database with a single ok record without components
    When CJKQC is invoked with the database and "--formation" and no --out
    Then its section row is "- - -"

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

  # --- Corrected formation section ---

  @rq-39641660
  Scenario: The corrected section evaluates the attached corrections
    Given a 1am1sa cluster and am and sa monomers all carrying corrections
    When CJKQC is invoked with the database and "--formation-corr" and no --out
    Then the header is "ct elcorr gcorr" and the cluster's row shows defined values

  @rq-20ab3295
  Scenario: A record without a correction has dashes in the corrected section
    Given a 1am1sa cluster without a correction and corrected am and sa monomers
    When CJKQC is invoked with the database and "--formation-corr" and no --out
    Then the cluster's row is "1am1sa - -"

  @rq-5ca4f95d
  Scenario: A reference monomer without a correction leaves both levels undefined
    Given a 1am cluster with a correction whose am monomer has none
    When CJKQC is invoked with the database and "--formation-corr" and no --out
    Then the cluster's row shows - for both levels

  @rq-b6409abd
  Scenario: The corrected section reuses the raw section's reference monomers
    Given two am monomers where the sort selects the one without a correction even though the other carries a lower corrected energy
    When CJKQC is invoked with the database and "--sort el --formation-corr" and no --out
    Then the cluster's corrected formation row shows - for both levels

  @rq-34d743c7
  Scenario: The corrected section follows the raw section and precedes the summary
    Given a database with a corrected cluster and its corrected monomers
    When CJKQC is invoked with the database and "--formation --formation-corr --out kept.jsonl"
    Then stdout shows the main table, the formation section, the corrected formation section, then "filtered <n> records into kept.jsonl"

  @rq-09e99e7b
  Scenario: --formation-corr alone is a visible outcome
    Given a database with a corrected cluster and its corrected monomers
    When CJKQC is invoked with the database and "--formation-corr" and no --out
    Then the corrected section is printed and no database file is written

  @rq-6e0560c2
  Scenario: The corrected section obeys the unit setting
    Given a cluster whose corrected formation energy is -0.01 Eh
    When CJKQC is invoked with the database and "--formation-corr --unit" and no --out
    Then the elcorr column prints the value in kcal/mol with 4 decimals

  @rq-f99b56ad
  Scenario: Filtered-away monomers serve as no corrected references
    Given corrected am and sa monomers and a corrected 1am1sa cluster
    When CJKQC is invoked with the database, "--extract 1am1sa --formation-corr" and no --out
    Then the section prints one row for the cluster with - for both levels

  # --- Concentrations ---

  @rq-e982f0e0
  Scenario: A plain value is a partial pressure in Pa
    When parse_conc_value is called with "0.00001"
    Then it returns 0.00001 in unit Pa

  @rq-d7a0c24f
  Scenario: A ppt value is a mole fraction of the standard atmosphere
    When parse_conc_value is called with "100ppt"
    Then it returns 100 in unit Ppt
    And the spec's pressure at 298.15 K is 100*10^-12*p° = 1.01325e-5 Pa

  @rq-183b0f2b
  Scenario: A ppb value is a mole fraction of the standard atmosphere
    When parse_conc_value is called with "10ppb"
    Then it returns 10 in unit Ppb
    And the spec's pressure at 298.15 K is 10*10^-9*p° = 1.01325e-3 Pa

  @rq-0f5b209d
  Scenario: A cmmc value converts through the ideal-gas law at the run's temperature
    When parse_conc_value is called with "1e10cmmc"
    Then it returns 1e10 in unit Cmmc
    And the spec's pressure at 298.15 K is 1e10*10^6*k_B*298.15 = 4.1164e-5 Pa
    And the spec's pressure at 350 K is 4.8323e-5 Pa

  @rq-9423306b
  Scenario: A value outside the grammar is a usage error
    Given a database with am and sa monomers
    When CJKQC is invoked with the database and "--formation --conc sa 100ppm" and no --out
    Then the exit code is 2

  @rq-890737af
  Scenario: A non-positive or non-finite value is a usage error
    Given a database with am and sa monomers
    When CJKQC is invoked with the database and "--formation --conc sa 0" and no --out
    Then the exit code is 2
    And the same holds for "-1e-7", "nan", and "inf"

  @rq-ae14b02d
  Scenario: The caret power syntax is not part of the grammar
    Given a database with am and sa monomers
    When CJKQC is invoked with the database and "--formation --conc sa 10^3ppt" and no --out
    Then the exit code is 2

  @rq-3384f0c4
  Scenario: A non-letter code is a usage error
    Given a database with am and sa monomers
    When CJKQC is invoked with the database and "--formation --conc 2sa 100ppt" and no --out
    Then the exit code is 2

  @rq-fecf8cb9
  Scenario: Naming one code twice is a usage error
    Given a database with am and sa monomers
    When CJKQC is invoked with the database and "--formation --conc sa 100ppt --conc sa 1ppb" and no --out
    Then the exit code is 2

  @rq-98440673
  Scenario: The named component shifts the g level by its chemical potential
    Given a database with a 1am1sa cluster whose formation free energy is -10.0 Eh under the am and sa monomers
    When CJKQC is invoked with the database and "--formation --conc sa 100ppt" and no --out
    Then the cluster's g formation value is -10.0 Eh + R T ln(10^10), about 13.6 kcal/mol higher
    And the cluster's el formation value is still -10.0 Eh

  @rq-85150fe0
  Scenario: Several --conc flags each cover their own component
    Given a database with a 2am1sa cluster whose formation free energy is g_form under the am and sa monomers
    When CJKQC is invoked with the database and "--formation --conc am 100ppt --conc sa 10ppb" and no --out
    Then the cluster's g formation value is g_form + 2 R T ln(10^10) + R T ln(10^8)

  @rq-bbf4e72d
  Scenario: A component without a spec stays at the standard state
    Given a 2am1sa record with references and a single spec for sa
    When formation_values is called
    Then the gibbs level is g_form - R T ln(c_sa/p°) with no am term

  @rq-1b039c1f
  Scenario: --cnt scales a fully covered cluster's correction by (N-1)/N
    Given a 3sa record with the sa reference and a sa spec
    When formation_values is called with self_consistent off and with self_consistent on
    Then the off gibbs level is g_form - 3 R T ln(c_sa/p°)
    And the on gibbs level is g_form - 2 R T ln(c_sa/p°)

  @rq-040dd0d3
  Scenario: --cnt keeps a monomer's formation free energy at zero
    Given a database with a 1sa monomer with defined thermochemistry
    When CJKQC is invoked with the database and "--formation --cnt --conc sa 100ppt" and no --out
    Then the monomer's g formation value is 0
    And without --cnt it is +R T ln(10^10), about 13.6 kcal/mol

  @rq-fd428ce5
  Scenario: A concentration above the standard state lowers the formation free energy
    Given a database with a 2sa cluster whose formation free energy is -5.0 Eh under the sa monomer
    When CJKQC is invoked with the database and "--formation --conc sa 202650" and no --out
    Then the cluster's g formation value is -5.0 Eh - 2 R T ln(2)

  @rq-b5d0f3d1
  Scenario: A spec at the standard pressure reproduces the uncorrected values
    Given a database with a 1am1sa cluster and its am and sa monomers
    When the same database is run with "--formation" and with "--formation --conc sa 101325 --conc am 101325"
    Then the two runs' formation sections are identical

  @rq-9b67220d
  Scenario: The correction never creates a defined value
    Given a database with am and sa monomers and a 1am1sa cluster
    When CJKQC is invoked with the database, "--extract 1am1sa --formation --conc sa 100ppt" and no --out
    Then the cluster's row shows - for both levels

  @rq-c984bd29
  Scenario: Undefined thermochemistry stays undefined under --conc
    Given a sa monomer with defined thermochemistry and a 1sa cluster record without vibrational frequencies
    When CJKQC is invoked with the database and "--formation --conc sa 100ppt" and no --out
    Then the cluster's g formation value prints -

  @rq-e8ea3b7f
  Scenario: The corrected section shifts gcorr the same way
    Given a 1am1sa cluster and am and sa monomers all carrying corrections
    When CJKQC is invoked with the database and "--formation-corr --conc sa 100ppt" and no --out
    Then the cluster's gcorr formation value is its standard-state value + R T ln(10^10)
    And the cluster's elcorr formation value is unchanged

  @rq-985cc1a5
  Scenario: A spec for a code in no final record is a usage error
    Given a database with am and sa monomers and a 1am1sa cluster
    When CJKQC is invoked with the database and "--formation --conc w 100ppt" and no --out
    Then the exit code is 2

  @rq-38b48ce6
  Scenario: A spec whose monomers were filtered away is not a usage error
    Given a database with am and sa monomers and a 1am1sa cluster
    When CJKQC is invoked with the database, "--extract 1am1sa --formation --conc am 100ppt --conc sa 100ppt" and no --out
    Then the exit code is 0 and the cluster's row shows - for both levels

  @rq-75ac7572
  Scenario: --conc without a formation section is an accepted no-op
    Given a database with one record
    When CJKQC is invoked with the database and "--name --el --conc sa 100ppt" and no --out
    Then the table prints normally, identical to the run without --conc

  @rq-d6fc375d
  Scenario: --conc alone is not a visible outcome
    When CJKQC is invoked with a database and "--conc sa 100ppt" and no --out and no print flag
    Then the exit code is 2

  @rq-6f3e4732
  Scenario: The main table and --glob ignore the concentrations
    Given a database with a 2am1sa cluster and its am and sa monomers
    When CJKQC is invoked with the database and "--ct --g --glob --formation --conc sa 100ppt" and no --out
    Then the main table's g column equals the run without --conc
    And the formation section's g value carries the concentration term

  @rq-ef6c9159
  Scenario: The ppt correction scales linearly with temperature
    Given a 1am1sa cluster with defined thermochemistry and a sa spec of 100ppt
    When the same database is run with "--formation --conc sa 100ppt" at --temp 298.15 and at --temp 350
    Then the shift from the standard-state value at 350 K is 350/298.15 times the shift at 298.15 K

  @rq-f623ed28
  Scenario: An absurd magnitude leaves the level undefined rather than infinite
    Given a database with an am monomer, an sa monomer, and a 2am1sa cluster with full thermochemistry
    When CJKQC is invoked with the database and "--formation --conc sa 1e-320" and no --out
    Then the run exits 0
    And every sa-covered row's g formation value prints -
```
