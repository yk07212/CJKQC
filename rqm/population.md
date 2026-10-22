# Feature: Boltzmann Population Columns <!-- rq-9e28654c -->

A conformer set is characterized not only by its members' energies but by
how the thermal ensemble populates them. JKQC prints such a column with
`-pop` (population probability from the Gibbs free energy) and `-popEL`
(population probability from the electronic energy); CJKQC exposes both
through one flag, `--pop`, whose optional level argument picks the energy
basis. Populations are ensemble values: they are computed at printing time
over exactly the records the run prints, grouped like the pipeline's
composition groups, and never stored (`rqm/ARCHITECTURE.md`).

## Level Parity <!-- rq-1514262e -->

Every population level is the same computation over a different energy
resolver. Any rule stated for the `el` or `g` basis holds verbatim for the
`elcorr` and `gcorr` bases, which resolve their values through the corrected
derivations (`rqm/correction.md`). No population rule is implemented or
specified twice: the corrected bases differ only in where the per-record
value comes from, and every scenario below that is stated for a plain basis
applies to its corrected counterpart through the same function.

## Bases and Flags <!-- rq-35d66314 -->

- `--pop` is a print flag (`rqm/filtering.md`): repeatable, each occurrence
  taking an optional level value — one of `el`, `g`, `elcorr`, `gcorr`,
  lowercase and exact. A missing value means `g`: bare `--pop` prints
  JKQC's `-pop` column, and `--pop el` prints JKQC's `-popEL` column. A
  value outside the four names is a usage error (exit 2).
- Each requested level contributes one table column: `g` → `pop`, `el` →
  `popel`, `elcorr` → `popelcorr`, `gcorr` → `popgcorr`. Repeating a level
  prints its column once. The columns appear in the canonical order `pop`,
  `popel`, `popelcorr`, `popgcorr` — after `gcorr`, before `rg`
  (`rqm/filtering.md` Printing) — whatever order the flags were given.
- `--pop` is a visible outcome on its own (`rqm/filtering.md`): a run with
  `--pop` and no `--out` prints the table and writes nothing.

## Computation <!-- rq-b7d07857 -->

- The population set is exactly the records the run prints: the pipeline
  survivors for a manipulation run, every loaded record for a print-only
  run (`rqm/filtering.md` Printing) — the same set `--formation` evaluates
  (`rqm/formation.md`). Populations renormalize over that set:
  `--select 2` computes each group's populations over its two survivors.
- Groups follow the run's grouping (`rqm/filtering.md` Grouping): records
  sharing the same `components` map form a group, records without
  `components` form the unnamed group, and `--noname` collapses everything
  into one global group.
- For a group and level, vᵢ is record i's level value in Eh: for `el` the
  record's `electronic_energy`, for `g` the Gibbs free energy derived at
  the run's conditions (`rqm/thermo.md`), for `elcorr` and `gcorr` the
  corrected values (`rqm/correction.md`). A record whose level value is
  undefined is excluded from the group's partition sum and prints `-`;
  the defined members' populations stay defined (JKQC's population printer
  turns a whole group `nan` when one member lacks the level — CJKQC does
  not carry that behavior over).
- When at least one member of the group defines the level, record i's
  population is

  pᵢ = exp(−(vᵢ − v_min)/(k_B T)) / Σⱼ exp(−(vⱼ − v_min)/(k_B T))

  over the defined members, with v_min the group's minimum defined value
  and T the run's temperature (`--temp`, `rqm/thermo.md` Conditions); k_B T
  in Eh comes from the Constants of `rqm/thermo.md`. When no member of the
  group defines the level, every member prints `-`.
- The `el` basis needs only the electronic energy: records without
  vibrational frequencies (single-point outputs, plain xyz) have defined
  `popel` values while their `g` is undefined. The basis is the plain
  `electronic_energy`: JKQC's `-popEL` help text says the same, though its
  code evaluates the zero-point-corrected energy (electronic + ZPE);
  CJKQC follows the documented intent, not the implementation.
- Populations are dimensionless probabilities in [0, 1] printed with 4
  decimals, `-` when undefined. `--unit` never affects them. Sorting and
  `--reverse` reorder rows without changing values; only the membership of
  the printed set changes them.

## Numerics <!-- rq-12614633 -->

The exponent −(vᵢ − v_min)/(k_B T) is ≤ 0 by construction, so every factor
lies in [0, 1] and the minimum's own factor is exactly 1: the partition sum
is at least 1, overflow and division by zero are unreachable, and members
far above the minimum underflow to a 0.0000 population without affecting
the rest. Temperatures the CLI already rejects stay rejected: `--temp` must
be > 0 (`rqm/thermo.md` Conditions).

## Feature API <!-- rq-e9bd21e0 -->

### Types <!-- rq-05c32f0e -->

- `PopLevel` — `El | G | ElCorr | GCorr`, the population bases; bare <!-- rq-b954f3fb -->
  `--pop` selects `G`.

### Functions <!-- rq-22bb815b -->

- `parse_pop_level(text: &str) -> Option<PopLevel>` — the level grammar <!-- rq-cd221e62 -->
  above; `None` for anything outside the four names (the CLI turns `None`
  into a usage error).
- `pop_level_value(record: &Record, level: PopLevel, memo: &ThermoMemo) -> Option<f64>` <!-- rq-6c00aaee -->
  — the level's basis value in Eh through the derived-property registry on
  the run's memo (`rqm/thermo.md`): `El` the passthrough electronic energy,
  `G` the derived Gibbs free energy, `ElCorr`/`GCorr` the corrected values
  (`rqm/correction.md`). One resolver for all levels — the corrected levels
  run the identical path (Level Parity).
- `populations(records: &[Record], level: PopLevel, memo: &ThermoMemo, <!-- rq-540b4c18 -->
  grouping: Grouping) -> Vec<Option<f64>>` — per-record populations aligned
  index-wise with `records` over the given (printed) set: groups per the
  grouping, Boltzmann weights at the memo's temperature, `None` when the
  record's value is undefined or no member of its group defines the level.

## Crate Layout <!-- rq-312442a6 -->

- The computation lives in `src/population.rs`; the library root re-exports
  the types and functions. `print_table` (`rqm/filtering.md`) renders the
  population columns through this module — no per-level duplicated
  formatting. No new dependencies.
- `tests/population.rs` tests the level grammar, the per-level resolvers,
  grouping and renormalization, definedness, and the Boltzmann arithmetic
  against hand-computed splits; CLI coverage (flag grammar, column headers
  and canonical order, the visible-outcome rule, usage errors) extends
  `tests/cli.rs` per `rqm/filtering.md`.

## Out of Scope <!-- rq-2e5e70c1 -->

- Populations as pipeline inputs — cutoff properties, sort keys, uniqueness
  metrics, or `--glob` criteria: the pipeline fixes the printed set before
  populations exist, and a population cutoff would be circular.
- Populations over anything but the printed set: no flag computes them
  over the pre-filter database, and filtered-away records never contribute.
- Concentration- or formation-adjusted populations: `--conc` shifts only
  the formation sections (`rqm/formation.md`); the `g` basis is the
  standard-state derived Gibbs free energy. The choice mirrors JKQC, whose
  concentration correction lives entirely in its formation printer and
  never rewrites a record's Gibbs value — and within a composition group
  the concentration term is common to every member (the components map is
  shared), so it cancels in the Boltzmann ratio anyway; only a `--noname`
  pool could ever notice the difference, and JKQC applies none there.
- Degeneracy, symmetry, or any weighting beyond the plain Boltzmann factor.
- Storing populations in records or databases: like every derived
  quantity, they are computed at request time (`rqm/ARCHITECTURE.md`).

## Gherkin Scenarios <!-- rq-95fd0e27 -->

```gherkin
Feature: Boltzmann population columns

  Background:
    Given the default ThermoConditions of rqm/thermo.md

  # --- Flag surface ---

  @rq-4d3860a0
  Scenario: Bare --pop prints the Gibbs-based column
    Given a database with one ok record with defined thermochemistry
    When CJKQC is invoked with the database and "--pop" and no --out
    Then the header is "pop" and the column prints 1.0000

  @rq-032b5e42
  Scenario: --pop el prints the electronic-based column
    Given a database with one ok record with an electronic energy and no frequencies
    When CJKQC is invoked with the database and "--pop el" and no --out
    Then the header is "popel" and the column prints a defined value

  @rq-4d3ba8f2
  Scenario: The corrected levels print their own columns
    Given a database with one corrected ok record with defined thermochemistry
    When CJKQC is invoked with the database and "--pop elcorr --pop gcorr" and no --out
    Then the header is "popelcorr popgcorr" and both columns print 1.0000

  @rq-9c420929
  Scenario: Repeating --pop prints one column per level in canonical order
    Given a database with one ok record with defined thermochemistry
    When CJKQC is invoked with the database and "--pop el --pop" and no --out
    Then the header is "pop popel"

  @rq-0c93ce7a
  Scenario: Repeating one level prints its column once
    Given a database with one ok record with defined thermochemistry
    When CJKQC is invoked with the database and "--pop --pop g" and no --out
    Then the header is "pop" with a single pop column

  @rq-09f8b051
  Scenario: An unknown level is a usage error
    Given a database with one ok record
    When CJKQC is invoked with the database and "--pop x" and no --out
    Then the exit code is 2

  @rq-b6a6c98c
  Scenario: --pop alone is a visible outcome
    Given a database with one ok record
    When CJKQC is invoked with the database and "--pop" and no --out
    Then the table prints and no database file is written

  @rq-0c46871d
  Scenario: The population columns sit in canonical order
    Given a database with one ok record with defined thermochemistry
    When CJKQC is invoked with the database and "--rg --pop --pop el --el" and no --out
    Then the header is "el pop popel rg"

  # --- Grouping and normalization ---

  @rq-7efc4513
  Scenario: Two conformers split by their Boltzmann factors
    Given two conformers of one composition whose gibbs values differ by kB*T*ln(3) at the run temperature
    When CJKQC is invoked with the database and "--pop" and no --out
    Then the lower conformer's pop is 0.7500 and the higher's is 0.2500

  @rq-49b5e21e
  Scenario: Populations are per composition group
    Given two composition groups of two conformers each, every pair's gibbs values 1 kB*T apart
    When CJKQC is invoked with the database and "--pop" and no --out
    Then each group's two pop values are about 0.7311 and 0.2689
    And each group's pop values sum to 1

  @rq-ae4f8a69
  Scenario: Ties share the population
    Given two conformers with equal gibbs values
    When CJKQC is invoked with the database and "--pop" and no --out
    Then both pop values are 0.5000

  @rq-2415978d
  Scenario: --select renormalizes over the survivors
    Given three conformers whose gibbs values are 0, 1, and 2 kB*T above the minimum
    When CJKQC is invoked with the database and "--sort g --select 2 --pop" and no --out
    Then the survivors' pop values are about 0.7311 and 0.2689
    And they are not the full-set values 0.6652 and 0.2447

  @rq-df30182b
  Scenario: --select 1 leaves the survivor at the full population
    Given three conformers whose gibbs values are 0, 1, and 2 kB*T above the minimum
    When CJKQC is invoked with the database and "--sort g --select 1 --pop" and no --out
    Then the survivor's pop is 1.0000

  @rq-82fee485
  Scenario: --glob leaves each survivor at the full population
    Given two composition groups of conformers with differing gibbs values
    When CJKQC is invoked with the database and "--glob --pop" and no --out
    Then every printed pop is 1.0000

  @rq-40e5e575
  Scenario: --noname pools every record into one population
    Given two composition groups of conformers with differing gibbs values
    When CJKQC is invoked with the database and "--noname --pop" and no --out
    Then all printed pop values sum to 1

  @rq-94d96047
  Scenario: Records without components form their own group
    Given two records without components whose electronic energies differ by kB*T*ln(3) and one am monomer
    When CJKQC is invoked with the database and "--pop el" and no --out
    Then the two records show 0.7500 and 0.2500 and the monomer shows 1.0000

  @rq-6f2e5003
  Scenario: Sorting never changes population values
    Given three conformers with distinct gibbs values
    When CJKQC is invoked with the database and "--sort g --pop" and with "--sort g --reverse --pop"
    Then the two runs print the same three pop values in different row orders

  @rq-fd6f3491
  Scenario: A print-only run sees every loaded record
    Given a database with two ok conformers and one failed record
    When CJKQC is invoked with the database and "--pop" and no --out
    Then all three rows print, the failed row's pop is -, and the ok rows' pop values sum to 1

  # --- Definedness ---

  @rq-33dafbb7
  Scenario: A record without an electronic energy prints - at every level
    Given a group of two conformers where one lacks electronic_energy
    When CJKQC is invoked with the database and "--pop --pop el" and no --out
    Then the lacking record's pop and popel both print -
    And the other record's pop and popel both print 1.0000

  @rq-b19284dd
  Scenario: A record without frequencies has defined popel and undefined pop
    Given two single-point records without frequencies whose electronic energies differ by kB*T*ln(3)
    When CJKQC is invoked with the database and "--pop --pop el" and no --out
    Then the popel column shows 0.7500 and 0.2500
    And the pop column shows - for both records

  @rq-1ee82da7
  Scenario: A record without a correction is undefined at the corrected levels alone
    Given two conformers where one carries a correction and the other does not
    When CJKQC is invoked with the database and "--pop gcorr" and no --out
    Then the corrected record's popgcorr is 1.0000 and the other's is -

  @rq-d0b32669
  Scenario: A group where no record defines the level prints all dashes
    Given a group of two records without electronic energy
    When CJKQC is invoked with the database and "--pop" and no --out
    Then both rows print -

  @rq-33c2faa3
  Scenario: A saddle point pops under --makereal
    Given a group of two conformers, one holding an imaginary frequency
    When CJKQC is invoked with the database and "--pop --makereal" and no --out
    Then both pop values are defined and sum to 1
    And without --makereal the imaginary record's pop is - and the other's is 1.0000

  # --- Conditions and numerics ---

  @rq-b25a95ec
  Scenario: The temperature shifts the split
    Given two conformers whose gibbs values differ by 1.0369e-3 Eh, about kB*T*ln(3) at 298.15 K
    When the same database is run with "--pop" at --temp 298.15 and at --temp 596.3
    Then at 298.15 the split is 0.7500 and 0.2500
    And at 596.3 the split is about 0.6340 and 0.3660

  @rq-ebf60f52
  Scenario: A very high temperature flattens the populations
    Given two conformers whose gibbs values differ by 1e-6 Eh
    When CJKQC is invoked with the database and "--temp 100000 --pop" and no --out
    Then both pop values print 0.5000

  @rq-9b20809b
  Scenario: A member far above the minimum underflows to zero
    Given three conformers whose gibbs values are 0, 1, and 400 kB*T above the minimum
    When CJKQC is invoked with the database and "--pop" and no --out
    Then the pop values are about 0.7311, 0.2689, and 0.0000
    And the first two sum to 1 within rounding

  @rq-8bae3ee9
  Scenario: --unit leaves populations alone
    Given a group of two conformers with distinct gibbs values
    When CJKQC is invoked with the database and "--pop --unit" and without "--unit"
    Then the two runs' pop columns are identical

  @rq-2c1cd4e2
  Scenario: --conc leaves populations alone
    Given a group of two conformers with distinct gibbs values
    When CJKQC is invoked with the database and "--pop --conc am 100ppt" and without "--conc"
    Then the two runs' pop columns are identical

  # --- Level parity ---

  @rq-5a5c97f2
  Scenario: pop_level_value resolves all four levels through one path
    Given a corrected record with defined thermochemistry
    When pop_level_value is called at El, G, ElCorr, and GCorr
    Then El returns the electronic energy, G the derived gibbs value, and ElCorr and GCorr the corrected values

  @rq-4c429ba4
  Scenario: The corrected basis splits like the plain basis at equal gaps
    Given two uncorrected conformers whose gibbs values differ by kB*T*ln(3) and two corrected conformers whose gcorr values differ by the same amount
    When populations is called at level G for the first pair and at level GCorr for the second
    Then the two splits are equal
```
