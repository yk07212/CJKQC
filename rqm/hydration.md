# Feature: Hydration and Solvation Distributions <!-- rq-206d4784 -->

Clusters in equilibrium with a vapor exchange solvent molecules: at a
given relative humidity, the hydrates `(sa)ₓ(am)ᵧ(w)ₙ` of one core
composition populate a Boltzmann distribution over the solvent count n.
JKQC prints such distributions with `-hydration` (water) and
`-solvation CODE` (any solvent), under the vapor pressure set by `-rh`
and `-psolvent`. CJKQC carries the feature over as the
`--hydration`/`--solvation` distribution sections — the last sections of
a run, printed after the formation sections — and builds them on the
formation machinery (`rqm/formation.md`): a state's distribution value
is the formation free energy the formation sections compute, with the
solvent's chemical potential set to its vapor pressure, so the
distribution at a relative humidity and the formation at given
concentrations are one consistent view of the same run.

The distribution is JKQC's math: within a core group the weights are
`(p/p°)ⁿ·exp(−(G − n·G_solvent)/(k_B T))`, because every non-solvent
term is common to the group and cancels in the normalization. That is
exactly the normalized Boltzmann weight of the concentration-corrected
formation free energies — the implied `--conc` spec of the Flag Surface
puts the vapor term inside the formation values, and the core-common
remainder cancels.

## Flag Surface <!-- rq-bcb7acc5 -->

- `--hydration`, `--solvation CODE`, `--hydration-corr`, and
  `--solvation-corr CODE` are print flags (`rqm/filtering.md`) that
  print distribution sections — the same raw/corrected convention as
  `--formation` and `--formation-corr` (`rqm/formation.md`): the plain
  spellings print the raw-level section, the `-corr` spellings the
  corrected-level section. Each requested level prints its section once,
  the raw section before the corrected one, whatever order the flags
  were given.
- The `--hydration` spellings fix the solvent code to `w`; the
  `--solvation` spellings take the code — a non-empty, letter-only code
  as `rqm/record.md` component validation requires, case-significant.
  All solvent flags given together must agree on one code — a second,
  different code is a usage error (exit 2), one solvent per run — while
  repeated spellings of the same code are redundant, not conflicting.
  `--hydration` alone is a visible outcome (`rqm/filtering.md`).
- `--rh VALUE` is a settings flag (`rqm/thermo.md`): the relative
  humidity of the vapor as a fraction — `0.5` is 50 %. The value must be
  a finite number greater than zero; supersaturation (values above 1) is
  accepted. `--rh` is water physics: with any solvent code other than
  `w` it is a usage error.
- `--psolvent VALUE` is a settings flag: the solvent's partial pressure,
  VALUE in the `--conc` grammar (`rqm/formation.md` VALUE grammar:
  Pa, `ppt`, `ppb`, `cmmc`). A value outside the grammar, non-finite, or
  not positive is a usage error. `--psolvent` is valid for every
  solvent and overrides `--rh`.
- Both flags are accepted no-ops without a hydration section, like
  `--conc` and `--bavg` (`rqm/formation.md`).
- When a hydration section is requested, the run's solvent code must
  appear as a component of at least one record in the section's record
  set — the run's final record set — or the run is a usage error (the
  `--conc` code-validation rule of `rqm/formation.md`). Naming the
  solvent code in an explicit `--conc` is a usage error (the implied
  spec below already owns it); `--conc` specs for other components ride
  along untouched.

## Solvent Pressure <!-- rq-d36686ec -->

- Resolution order: `--psolvent` wins over `--rh`; with neither, the
  default is `--rh 1` — saturated vapor. The resolved pressure enters
  the run as an **implied concentration spec**: a hydration section
  behaves as if `--conc CODE p` had been given, with p the resolved
  partial pressure in Pa. The formation sections' solvent term shifts by
  it, `--cnt` scales it like any spec, and the section's distribution
  values carry it (`rqm/formation.md` Formation at Given
  Concentrations) — one chemical potential per component, everywhere in
  the run.
- The water saturation-vapor pressure behind `--rh` is the Magnus
  correlation JKQC uses (Alduchov–Eskridge 1996):

  p_sat(T) = 610.94 · exp(17.625 · τ / (243.04 + τ)) Pa,  τ = T − 273.15,

  giving 610.94 Pa at 273.15 K and about 3.162e3 Pa at 298.15 K. The
  correlation is water-only and is not part of the Constants table of
  `rqm/thermo.md`; these two reference values pin it. `--rh` resolves
  p_w = VALUE · p_sat(T) at the run's temperature. A temperature at
  which the resolved pressure is not finite (the correlation's domain
  edge) is a usage error — the pressure must resolve to a finite
  positive number before the run starts.

## Hydration States <!-- rq-42cdecff -->

- The sections evaluate exactly the record set the formation sections
  evaluate (`rqm/formation.md`): the pipeline survivors for a
  manipulation run, every loaded record for a print-only run; under
  `--bavg` the collapsed ensembles. Both levels run through one
  level-parameterized implementation (the Level Parity rule of
  `rqm/population.md`): the raw level reads the formation free energies
  (the monomer references of `collect_monomers` under the run's sort
  ordering, `formation_values`), the corrected level the corrected ones
  (`corrected_monomer_references`, `corrected_formation_values`).
- A **state** is a components map (`rqm/composition.md`); its **core**
  is the map minus the solvent code. Records and ensembles without
  components are excluded. States whose core is empty — the pure
  solvent clusters (`1w`, `2w`, …) — are excluded: JKQC drops the
  core-less group, and the solvent monomer itself prints no
  distribution row.
- The core groups print in first-appearance order of their core among
  the printed set; the states within a group in first-appearance order
  of their components map.
- A state's distribution value at the section's level:
  - Uncollapsed, it is the **lowest free conformer**: the minimum over
    the state's members' defined level values. Members whose value is
    undefined are ignored — failed and truncated records among them,
    exactly as the pipeline treats them (`rqm/filtering.md`) — and a
    state with no defined member is undefined.
  - Under `--bavg`, the collapse already provides one ensemble per
    components map (`rqm/formation.md` Boltzmann-Averaged Sections), so
    the state's value is that ensemble's level value — the collapse
    replaces the conformer minimum, conformational entropy included.
- A state's formation value needs the monomer references of its full
  stoichiometry: a missing or level-less reference code leaves the state
  undefined at that level (`rqm/formation.md` Formation Values). A
  hydrated state whose solvent monomer was filtered away — or whose
  solvent monomer exists only as failed or frequency-less records —
  prints a dash while the solvent-free states of the same core stay
  defined.

## The Distribution <!-- rq-7fbc1c87 -->

- Within a core group, each defined state's weight is

  w_state = exp(−(v_state − v_min)/(k_B T)),

  with v_min the group's minimum defined value and T the run's
  temperature; its percentage is 100 · w_state / Σ w over the defined
  states. Undefined states contribute nothing and print `-`; a group
  with no defined states prints all dashes. A group with exactly one
  defined state prints 100.0.
- The pressure physics rides inside the values. The implied solvent
  spec's term −n·RT ln(p_w/p°) makes the weights equal JKQC's
  `(p_w/p°)ⁿ·exp(−(G − n·G_solvent°)/(k_B T))` form, and every
  non-solvent term — the core's monomer references, other components'
  `--conc` terms without `--cnt` — is common within the group and
  cancels in the normalization. `--cnt`'s (N−1)/N scaling varies with
  the state's total molecule count, so under `--cnt` the distribution
  simply follows the formation values wherever they lead.
- Numerics: the minimum subtraction keeps every exponent ≤ 0, so all
  factors lie in [0, 1] and no overflow is reachable
  (`rqm/population.md` Numerics); a state far above the minimum
  underflows to 0.0 without affecting the rest. k_B T comes from the
  Constants of `rqm/thermo.md`. Temperatures the CLI already rejects
  stay rejected.
- `--noname` never pools the core groups — the grouping is intrinsic to
  the distribution, like the formation collapse's always-by-composition
  rule (`rqm/formation.md`). Sorting and `--reverse` reorder the main
  table, never the section: the section has its own grouping order.

## Printing <!-- rq-5e154b92 -->

- No header row. Per core group, two rows: the states' composition
  tokens — the same alphabetical `2am3sa`-style pairs the `--name` column
  prints (`rqm/filtering.md` Printing) — then their percentages. The
  percentages print with one decimal, `-` for an undefined state; they
  are rounded for display and need not sum to exactly 100.0. A blank
  line follows every group's percentage row, so groups read as blocks
  (JKQC's layout).
- Columns align within a group: each column is as wide as the wider of
  its token and percentage cell, cells right-aligned, single space
  between columns:

  ```text
   1sa 1sa1w 1sa2w
  40.0  45.0  15.0
  ```

- Stdout order: main table, formation section, corrected formation
  section, hydration sections (raw before corrected), summary. The
  sections print over the same record set as the formation sections
  whether or not any formation section was requested.

## Feature API <!-- rq-84386009 -->

### Types <!-- rq-d8a5bbaf -->

- `SolventConditions` — the run's resolved solvent setting: <!-- rq-af47884f -->
  `code: String`, `pressure: f64` (Pa).
- `HydrationState` — one state: `components: Components`, <!-- rq-6bc74daa -->
  `value: Option<f64>` (Eh, the section's level's formation value).
- `HydrationGroup` — one core group: `core: Components`, <!-- rq-0b5e2729 -->
  `states: Vec<HydrationState>` in first-appearance order.

### Functions <!-- rq-6c6fec9b -->

- `saturation_pressure_water(temperature: f64) -> f64` — the Magnus <!-- rq-7e9e7176 -->
  correlation of Solvent Pressure, in Pa.
- `hydration_states(records: &[Record], sort: SortKey, memo: <!-- rq-2959f663 -->
  &ThermoMemo, concs: &ConcConditions, solvent: &str) ->
  Vec<HydrationGroup>` — the uncollapsed raw path: the monomer
  references of `collect_monomers` over the given records (the run
  passes its final record set and sort ordering; `rqm/formation.md`),
  the grouping and lowest-free-conformer values of Hydration States.
- `corrected_hydration_states(records: &[Record], sort: SortKey, <!-- rq-0cfc6832 -->
  memo: &ThermoMemo, concs: &ConcConditions, solvent: &str) ->
  Vec<HydrationGroup>` — the corrected twin, read through
  `corrected_monomer_references` and `corrected_formation_values`
  (`rqm/formation.md`); identical otherwise.
- `hydration_states_from_ensembles(ensembles: &[Ensemble], memo: <!-- rq-1b2f8c5e -->
  &ThermoMemo, concs: &ConcConditions, solvent: &str) ->
  Vec<HydrationGroup>` — the `--bavg` raw path over the collapsed
  ensembles (`rqm/formation.md`): each ensemble is a state, its value
  the ensemble's Gibbs value.
- `corrected_hydration_states_from_ensembles(ensembles: &[Ensemble], <!-- rq-366add70 -->
  memo: &ThermoMemo, concs: &ConcConditions, solvent: &str) ->
  Vec<HydrationGroup>` — the corrected twin over the
  `corrected_collapse_ensembles` output; identical otherwise. The raw
  and corrected spellings of each state builder share one
  level-parameterized implementation (the Level Parity rule of
  `rqm/population.md`).
- `hydration_percentages(group: &HydrationGroup, temperature: f64) -> <!-- rq-0a350842 -->
  Vec<Option<f64>>` — aligned with `states`: the normalized percentage
  of each defined state, `None` for undefined states and when no state
  of the group defines the level.
- `print_hydration_section(groups: &[HydrationGroup], temperature: f64) <!-- rq-70efc347 -->
  -> String` — the paired-row layout of Printing, percentages through
  `hydration_percentages`.

## Crate Layout <!-- rq-b97304bb -->

- The states, distribution, and sections live in `src/hydration.rs`;
  the library root re-exports the types and functions. No new
  dependencies.
- The CLI adds `--hydration`, `--hydration-corr`, `--solvation`,
  `--solvation-corr`, `--rh`, and `--psolvent`. It parses the
  `--psolvent` values through `parse_conc_value`/`ConcSpec::pressure`
  (`rqm/formation.md`), resolves the `SolventConditions` with the
  usage-error validations of the Flag Surface and Solvent Pressure
  sections, appends the implied `ConcSpec` to the run's
  `ConcConditions`, validates the solvent code against the final record
  set with the `--conc` rule, and prints the requested sections after
  the formation sections — over the ensembles when `--bavg` is set.
- `tests/hydration.rs` tests the pressure formula against the reference
  values, the state grouping and first-appearance order, the
  lowest-free-conformer rule and definedness, the ensemble path, the
  distribution arithmetic against hand-computed splits, and the
  percentage formatting; CLI coverage (flag grammar, conflicts, no-ops,
  usage errors, stdout order) extends `tests/cli.rs` per
  `rqm/filtering.md`.

## Out of Scope <!-- rq-42a7f0df -->

- Multiple solvents in one run: one solvent code per run, set by
  `--hydration` or `--solvation`.
- Per-conformer distribution columns: a state always contributes once —
  its lowest free conformer, or its ensemble under `--bavg`. JKQC's
  duplicate-token columns for same-composition conformers are not
  carried over.
- Electronic-level distributions: the sections run on formation free
  energies only; JKQC's `-g`/`-gout` choice maps onto the raw and
  corrected sections.
- Saturation-vapor pressures for non-water solvents: only water carries
  the Magnus correlation; every other solvent needs an explicit
  `--psolvent` (JKQC's fall-through to the water formula for any
  solvent is not carried over).
- The `SOLVATION` banner and the Temp/RH/p_solvent echo line: sections
  are bare tables, like the formation sections.
- Vapor-depletion or Kelvin-equation self-consistency beyond `--cnt`'s
  (N−1)/N scaling: inherited from the formation values, never solved
  for here.
- Activity coefficients, non-ideal mixtures, and liquid-phase reference
  states: the vapor is ideal and the pressure is a given input.
- Storing distributions in records or databases: like every derived
  quantity, they are computed at request time (`rqm/ARCHITECTURE.md`).

## Gherkin Scenarios <!-- rq-d2b5c941 -->

```gherkin
Feature: Hydration and solvation distributions

  Background:
    Given the default ThermoConditions of rqm/thermo.md

  # --- Flag surface ---

  @rq-09af191f
  Scenario: Bare --hydration distributes over the hydration states of water
    Given a database with a 1sa cluster, a 1sa1w cluster, and 1sa and 1w monomers
    When CJKQC is invoked with the database and "--hydration" and no --out
    Then one group prints the states 1sa and 1sa1w

  @rq-f0751abd
  Scenario: --solvation picks the solvent code
    Given a database with a 1am cluster, a 1am1sa cluster, and 1am and 1sa monomers
    When CJKQC is invoked with the database and "--solvation sa --psolvent 101325" and no --out
    Then one group prints the states 1am and 1am1sa

  @rq-524dd0dc
  Scenario: --hydration-corr prints only the corrected distribution
    Given a database with corrected 1sa and 1sa1w records and their monomers
    When CJKQC is invoked with the database and "--hydration-corr" and no --out
    Then one corrected section prints and no raw section appears

  @rq-c3e34342
  Scenario: The raw and corrected sections print together, raw first
    Given a database with corrected 1sa and 1sa1w records and their monomers
    When CJKQC is invoked with the database and "--hydration-corr --hydration" and no --out
    Then the raw section prints before the corrected one
    And each section prints exactly once

  @rq-534cfecc
  Scenario: --hydration and --solvation of another solvent are a usage error
    Given a database with 1sa and 1w monomers
    When CJKQC is invoked with the database and "--hydration --solvation sa" and no --out
    Then the exit code is 2

  @rq-fa54bb5b
  Scenario: A non-letter solvent code is a usage error
    Given a database with 1sa and 1w monomers
    When CJKQC is invoked with the database and "--solvation 2sa" and no --out
    Then the exit code is 2

  @rq-2dbfbb9a
  Scenario: --hydration alone is a visible outcome
    Given a database with 1sa and 1w monomers
    When CJKQC is invoked with the database and "--hydration" and no --out
    Then the section prints and no database file is written

  @rq-c0dea4f9
  Scenario: --rh without a hydration section is an accepted no-op
    Given a database with one record
    When CJKQC is invoked with the database and "--filename --el --rh 0.5" and no --out
    Then the table prints normally, identical to the run without --rh

  @rq-93c3be03
  Scenario: --psolvent without a hydration section is an accepted no-op
    Given a database with one record
    When CJKQC is invoked with the database and "--filename --el --psolvent 100ppt" and no --out
    Then the table prints normally, identical to the run without --psolvent

  # --- Solvent pressure ---

  @rq-eb09ca63
  Scenario: Without --rh and --psolvent the vapor is saturated
    Given a database with a 1sa1w cluster and 1sa and 1w monomers
    When the same database is run with "--hydration --rh 1" and with bare "--hydration"
    Then the two runs' sections are identical

  @rq-91512869
  Scenario: saturation_pressure_water matches its reference values
    When saturation_pressure_water is called at 273.15 and at 298.15
    Then it returns 610.94 Pa and about 3.162e3 Pa

  @rq-56966bc2
  Scenario: --rh scales the saturation vapor pressure
    Given a 1sa1w cluster whose formation free energy is g_form under the 1sa and 1w monomers
    When CJKQC is invoked with the database and "--formation --hydration --rh 0.5" and no --out
    Then the cluster's g formation value is g_form + R T ln(p°/(0.5 p_sat)), about 2.46 kcal/mol per water above the standard-state value

  @rq-b457ce50
  Scenario: --psolvent overrides --rh
    Given a database with a 1sa1w cluster and 1sa and 1w monomers
    When the same database is run with "--hydration --rh 0.5 --psolvent 101325" and with "--hydration --psolvent 101325"
    Then the two runs' sections are identical

  @rq-83eaf0d1
  Scenario: --psolvent reuses the concentration grammar
    Given a database with a 1sa1w cluster and 1sa and 1w monomers
    When the same database is run with "--hydration --psolvent 100ppt" and with "--hydration --psolvent 1.01325e-5"
    Then the two runs' sections are identical

  @rq-5a7bc7d5
  Scenario: A --psolvent value outside the grammar is a usage error
    Given a database with 1sa and 1w monomers
    When CJKQC is invoked with the database and "--hydration --psolvent 10^3ppt" and no --out
    Then the exit code is 2

  @rq-b9f78565
  Scenario: A non-positive --rh is a usage error
    Given a database with 1sa and 1w monomers
    When CJKQC is invoked with the database and "--hydration --rh 0" and no --out
    Then the exit code is 2
    And the same holds for "-0.5"

  @rq-49ef8ef3
  Scenario: Supersaturation above one is accepted
    Given a database with a 1sa1w cluster and 1sa and 1w monomers
    When CJKQC is invoked with the database and "--hydration --rh 1.5" and no --out
    Then the exit code is 0

  @rq-ee3e38de
  Scenario: A temperature outside the correlation's domain is a usage error
    Given a database with 1sa and 1w monomers
    When CJKQC is invoked with the database and "--temp 30 --hydration" and no --out
    Then the exit code is 2

  @rq-9ee7ff9c
  Scenario: --solvation of a non-water solvent without --psolvent is a usage error
    Given a database with 1sa and 1dma monomers and a 1sa1dma cluster
    When CJKQC is invoked with the database and "--solvation dma" and no --out
    Then the exit code is 2

  @rq-2b028a0d
  Scenario: --rh with a non-water solvent is a usage error
    Given a database with 1sa and 1dma monomers and a 1sa1dma cluster
    When CJKQC is invoked with the database and "--solvation dma --psolvent 100ppt --rh 0.5" and no --out
    Then the exit code is 2

  @rq-b994d13a
  Scenario: --conc naming the solvent code is a usage error
    Given a database with 1sa and 1w monomers
    When CJKQC is invoked with the database and "--hydration --conc w 100ppt" and no --out
    Then the exit code is 2

  @rq-acc842e9
  Scenario: The implied spec shifts the formation section's solvent term
    Given a database with a 1sa1w cluster and 1sa and 1w monomers
    When the same database is run with "--formation" and with "--formation --hydration --rh 0.5"
    Then the two runs' 1sa rows are equal
    And the hydration run's 1sa1w g formation value is about 2.46 kcal/mol above the plain run's

  @rq-68e223b9
  Scenario: A solvent absent from the final record set is a usage error
    Given a database with am and sa monomers and a 1am1sa cluster
    When CJKQC is invoked with the database and "--hydration" and no --out
    Then the exit code is 2

  @rq-6375f775
  Scenario: A filtered-away solvent monomer dashes the hydrated states
    Given a database with a 1sa1w cluster and 1sa and 1w monomers
    When CJKQC is invoked with the database, "--extract 1sa1w --hydration" and no --out
    Then the run exits 0 and the 1sa1w state prints -
    And the same run without --extract prints a defined percentage

  # --- States ---

  @rq-a99a27e1
  Scenario: States group by core composition
    Given a database with 1sa, 1sa1w, 1sa2w, 2am1sa, and 2am1sa1w records and the am, sa, and w monomers
    When CJKQC is invoked with the database and "--hydration" and no --out
    Then the first group holds the states 1sa, 1sa1w, and 1sa2w
    And the second group holds the states 2am1sa and 2am1sa1w

  @rq-b9897f33
  Scenario: The lowest free conformer represents a state
    Given a 1sa cluster, two 1sa1w conformers whose formation free energies differ by kB*T, and the 1sa and 1w monomers
    When CJKQC is invoked with the database and "--hydration" and no --out
    Then the 1sa1w state prints one column and its split against 1sa follows the lower conformer's value

  @rq-282ee600
  Scenario: Undefined conformers never hide a state's defined value
    Given a 1sa cluster, a 1sa1w conformer without frequencies, a 1sa1w conformer with full thermochemistry, and the 1sa and 1w monomers
    When CJKQC is invoked with the database and "--hydration" and no --out
    Then the 1sa1w state prints a defined percentage, its value from the conformer with thermochemistry

  @rq-ff1f09cb
  Scenario: A state without any defined member prints a dash and drops out
    Given a 1sa cluster, a 1sa1w cluster without frequencies, and 1sa and 1w monomers
    When CJKQC is invoked with the database and "--hydration" and no --out
    Then the 1sa state prints 100.0 and the 1sa1w state prints -

  @rq-e0cba629
  Scenario: Records without components are excluded
    Given a record without components, a 1sa cluster, and the 1sa monomer
    When CJKQC is invoked with the database and "--hydration" and no --out
    Then the section prints only the 1sa group

  @rq-4b684f8d
  Scenario: Pure solvent clusters print no distribution
    Given a database with 1w, 2w, and 1sa records and the 1sa and 1w monomers
    When CJKQC is invoked with the database and "--hydration" and no --out
    Then the section prints only the 1sa group and no w-containing state

  @rq-1c4d88c8
  Scenario: --bavg replaces the conformer minimum with the ensemble value
    Given two 1sa1w conformers whose gibbs values differ by kB*T*ln(3) and whose electronic energies are equal, and the 1sa and 1w monomers
    When the same database is run with "--hydration" and with "--hydration --bavg"
    Then the uncollapsed run's 1sa1w value is the best conformer's formation free energy
    And the averaged run's 1sa1w value is k_B*T*ln(4/3) below it, so its percentage is higher

  @rq-7067d53d
  Scenario: The corrected level reads the corrected values
    Given corrected 1sa and 1sa1w records whose corrected formation free energies differ from their raw ones, and corrected 1sa and 1w monomers
    When the same database is run with "--hydration" and with "--hydration-corr"
    Then the two runs' percentage rows differ

  @rq-bd08eb6c
  Scenario: A state without corrections dashes at the corrected level
    Given a corrected 1sa record, an uncorrected 1sa1w record, and corrected 1sa and 1w monomers
    When CJKQC is invoked with the database and "--hydration-corr" and no --out
    Then the 1sa state prints 100.0 and the 1sa1w state prints -

  # --- Distribution ---

  @rq-9a225d69
  Scenario: Two states one k_B*T apart split about 73.1 and 26.9
    Given a 1sa cluster and a 1sa1w cluster whose formation free energies at the run's vapor pressure differ by kB*T, and the 1sa and 1w monomers
    When CJKQC is invoked with the database and "--hydration" and no --out
    Then the lower state prints about 73.1 and the higher about 26.9

  @rq-0ccc07ee
  Scenario: A state far above the minimum underflows to 0.0
    Given a 1sa cluster, a 1sa1w cluster, and a 1sa2w cluster whose formation free energies are 0, 1, and 400 kB*T above the minimum, and the 1sa and 1w monomers
    When CJKQC is invoked with the database and "--hydration" and no --out
    Then the percentages are about 73.1, 26.9, and 0.0

  @rq-8ed6c203
  Scenario: A group without defined states prints all dashes
    Given two 1sa1w records without frequencies and a 1w monomer without frequencies
    When CJKQC is invoked with the database and "--hydration" and no --out
    Then the group's percentage row prints - for every state

  @rq-d60ca773
  Scenario: The temperature shifts the split
    Given a 1sa state and a 1sa1w state whose values differ by 1.0369e-3 Eh, about kB*T*ln(3) at 298.15 K
    When hydration_percentages is called on their group at 298.15 and at 596.3
    Then at 298.15 the split is about 75.0 and 25.0
    And at 596.3 the split is about 63.4 and 36.6

  @rq-fea096b0
  Scenario: A non-solvent --conc spec cancels within a core group
    Given a database with 1sa and 1sa1w clusters and the 1sa and 1w monomers
    When the same database is run with "--hydration" and with "--hydration --conc sa 100ppt"
    Then the two runs' sections are identical

  @rq-7b9426c0
  Scenario: --cnt scales the solvent term by (N-1)/N
    Given a database with a 1sa cluster and a 1sa1w cluster whose standard-state formation free energies are equal, and the 1sa and 1w monomers
    When the same database is run with "--hydration --rh 0.5" and with "--hydration --cnt --rh 0.5"
    Then the plain run's 1sa1w percentage is below the 1sa percentage
    And the cnt run's 1sa1w percentage is higher than the plain run's, the solvent term halved by (N-1)/N = 1/2

  @rq-da98d00b
  Scenario: --noname does not pool the core groups
    Given a database with 1sa, 1sa1w, 2am, and 2am1w records and the am, sa, and w monomers
    When CJKQC is invoked with the database and "--noname --hydration" and no --out
    Then the section still prints one group per core composition, not one global group

  # --- Printing ---

  @rq-cd509d24
  Scenario: The layout is a token row and a percentage row per group
    Given a database with 1sa, 1sa1w, 1sa2w, and 1w records and the 1sa and 1w monomers
    When CJKQC is invoked with the database and "--hydration" and no --out
    Then the 1sa group prints a "1sa 1sa1w 1sa2w" token row over a percentage row
    And no header line prints

  @rq-e52f165b
  Scenario: A blank line separates the groups
    Given a database with two core groups
    When CJKQC is invoked with the database and "--hydration" and no --out
    Then a blank line follows each group's percentage row

  @rq-2a926651
  Scenario: Columns align within a group
    Given a group with the states 1sa and 1sa1w
    When CJKQC is invoked with the database and "--hydration" and no --out
    Then the two rows' columns are right-aligned to the widest cell of each column

  @rq-3f2988b7
  Scenario: Percentages print with one decimal
    Given a group whose states split 1 kB*T apart
    When CJKQC is invoked with the database and "--hydration" and no --out
    Then the percentages print as 73.1 and 26.9

  @rq-6771a995
  Scenario: Undefined states print dashes in their columns
    Given a group with a defined 1sa state and an undefined 1sa1w state
    When CJKQC is invoked with the database and "--hydration" and no --out
    Then the 1sa1w column prints - in the percentage row

  @rq-a396d259
  Scenario: The section prints after the formation sections and before the summary
    Given a database with corrected 1sa and 1sa1w records and their monomers
    When CJKQC is invoked with the database and "--sort el --formation --formation-corr --hydration --hydration-corr --out kept.jsonl"
    Then stdout shows the formation section, the corrected formation section, both hydration sections, then "filtered <n> records into kept.jsonl"

  @rq-fbfe30a4
  Scenario: Rounded percentages need not sum to exactly 100
    Given three states whose exact percentages are 33.333... each
    When CJKQC is invoked with the database and "--hydration" and no --out
    Then the displayed percentages may sum to 99.9 or 100.1 without error
```
