# Feature: Filtering, Sorting, and Printing (Manipulation Pipeline) <!-- rq-9b4dda85 -->

A loaded database is a bag of records; the **manipulation pipeline** turns
it into a smaller, ordered bag and either writes it as a new database or
prints it. Every operation is a pure function over records
(`rqm/ARCHITECTURE.md`, `rqm/database.md` for storage); the pipeline never
mutates its input. The flag surface mirrors JKQC's filtering commands,
spelled with `--` because clap does not support single-dash long flags.

```text
CJKQC <inputs>… [--manipulation flags] [print flags…] [settings flags…] [--out <path>]
```

- `<inputs>…` are classified exactly as in collection (`rqm/collection.md`
  ): all program outputs build, all databases merge, a mixture is a usage
  error. The collected/merged records then flow through the pipeline.
- **Manipulation flags** are: `--extract`, `--nextract`, `--cut`,
  `--cutr`, `--pass`, `--passr`, `--is`, `--isnot`, `--filter-lt`,
  `--filter-ge`, `--rel-filter-lt`, `--rel-filter-ge`, `--uniq`,
  `--reacted`, `--reacted2`, `--sort`, `--reverse`, `--select`,
  `--glob`, `--shuffle`, `--seed`, `--noname`. **Print flags** (`--name`,
  `--el`, … — see Printing) select table columns; they print and
  manipulate nothing. **Settings flags** — `--temp`, `--fc`, `--fscale`,
  `--makereal` (`rqm/thermo.md`) and `--unit` — configure derivations and
  the energy unit at printing; they never manipulate, print, or satisfy
  the visible-outcome rule on their own.
- A run must have a visible outcome. With neither manipulation nor print
  flags, `--out` is required and behavior is exactly collection's.
  `--out` writes the final records (atomic replacement and the `--fout`
  warning rule as in collection); print flags print the table to stdout.
  A run with manipulation flags but neither `--out` nor a print flag is a
  usage error (exit 2): the manipulated database is stored nowhere and
  nothing prints, so the run would do nothing.
- Failed and truncated records (`rqm/record.md`) leave the pipeline before
  its first stage; their count is reported on stderr as
  `excluded <k> records with status failed or truncated` when non-zero.

## Pipeline Order <!-- rq-0137b5f9 -->

The stages run in this fixed order (mirroring JKQC's `filter`), each per
group as defined below:

1. drop non-`ok` records
2. `--extract` / `--nextract` (`rqm/filtering.md` below)
3. cutoffs, in the order the flags were given
4. `--uniq` (`rqm/uniqueness.md`)
5. `--reacted` / `--reacted2` (`rqm/detection.md`)
6. `--sort` (then `--reverse`)
7. `--select`
8. `--glob`
9. `--shuffle`

Implicit sort: when `--uniq` (keys mode) or `--select` is present but
`--sort` is not, records sort by `el` ascending first. `--uniq dup` does
not trigger it.

## Grouping <!-- rq-2f996ba6 -->

- Cutoffs (relative mode), `--uniq`, `--reacted`, `--select`, and
  `--extract` wildcard/exact composition matching treat records by
  **composition group**: records sharing the same `components` map
  (`rqm/composition.md`) form a group; records without `components` form
  one separate unnamed group.
- `--noname` collapses everything into a single global group;
  `--extract`/`--nextract` then compare their patterns verbatim against
  each record's `name` (no ranges, wildcards, or zero-count rules).
- Groups are processed in order of first appearance; results preserve
  global record order within each stage.

## Extract and Nextract <!-- rq-7b7e2114 -->

`--extract <query>` keeps records matching the query; `--nextract <query>`
keeps those that do not. A query is a comma-separated list of **patterns**
(match on any one). A pattern is a sequence of tokens:

```text
query   := pattern ( "," pattern )*
pattern := ( pair | range | choice | bare-star | count-star | count-star-code | star-code )+
pair    := count code
range   := count "-" count code           (inclusive expansion of counts)
choice  := "(" count ( "," count )* ")" code
bare-star          := "*"
count-star         := count "*"
count-star-code    := count "*" code
star-code          := "*" code
count   := digit+ (decimal, ≤ u32::MAX)
code    := letter+ (ASCII, case-sensitive)
```

- The query is matched against each record's `components` map: every
  pattern pair `(count>0, code)` must equal the record's count for that
  code; a pair with count `0` requires the code to be absent; codes the
  pattern does not mention are **extras** governed by the wildcard rule.
- Wildcard forms: bare `*` matches every record; `N*` allows extras whose
  counts sum to exactly `N`; `*code` allows only the named codes as extras
  (any counts); `N*code` combines both (extras from those codes summing to
  exactly `N`). Without a wildcard, a pattern requires an exact
  composition match.
- `range` expands to one pattern per count from the low end to the high
  end inclusive (e.g. `1am0-2w` ≡ `1am,1am1w,1am2w`); `choice` expands to
  one pattern per listed count. Expansions multiply within a pattern. To
  bound memory, a single range expanding to more than 1000 count values
  (so `1am1-1000w` is the widest accepted range and `1am1-1001w` is
  rejected), or a pattern that
  expands beyond 10000 patterns, is a usage error (exit 2); a range whose
  low end exceeds its high end (e.g. `3-1w`) is likewise a usage error.
- Anything after a `-` that a range did not consume is a label comment and
  is ignored (so a full JKQC file-stem can be pasted as a pattern). An
  underscore is not a valid pattern character: a pattern containing `_`
  fails to parse (usage error), so a nonsense stem is rejected rather than
  silently truncated.
- A pattern that fails to parse is a usage error (exit 2); an empty
  resulting pattern list is a usage error.
- Records without `components` behave as if their composition were empty:
  they match only patterns with no pairs (a bare `*`, or extras-only
  forms).

## Cutoffs <!-- rq-5dc8fe52 -->

Each cutoff flag compares a record's **property value** against a number;
the property set and units:

| Property | Value | Absolute units | Relative units |
|---|---|---|---|
| `el` | `electronic_energy` | hartree | kcal/mol (× the Eh→kcal/mol constant of `rqm/thermo.md` Constants) |
| `g` | Gibbs free energy (`rqm/thermo.md`) | hartree | kcal/mol (× the Eh→kcal/mol constant of `rqm/thermo.md` Constants) |
| `rg` | radius of gyration (`rqm/uniqueness.md`) | Å | Å |
| `lf` | lowest (minimum) vibrational frequency | cm⁻¹ | cm⁻¹ |
| `dip` | dipole magnitude (`rqm/uniqueness.md`) | atomic units | atomic units |
| `mass` | total mass (`rqm/uniqueness.md`) | u | u |
| `charge`, `multiplicity` | record fields | counts | counts |
| `natoms`, `nfreq`, `nimag` | geometry atom count, frequency count, count of negative frequencies | counts | counts |

- Flag → comparison: `--cut` `≤`, `--filter-lt` `<`, `--pass` `>`,
  `--filter-ge` `≥`, `--is` `==`, `--isnot` `≠`; `--cutr` `≤`,
  `--rel-filter-lt` `<`, `--passr` `>`, `--rel-filter-ge` `≥` in relative
  mode. Flags repeat; cutoffs chain in command order.
- **Relative mode** first subtracts the group minimum of the property
  (over defined values; `el` after the kcal/mol conversion), then applies
  the comparison. `--cutr el 10` keeps records within 10 kcal/mol of their
  group's minimum; with `--noname` the minimum is global.
- A record whose property is undefined (absent observable, unknown
  element for mass metrics) fails `<`, `≤`, `>`, `≥`, `==` and passes `≠`,
  mirroring the original's NaN comparison behavior.

## Sort, Select, Shuffle <!-- rq-17ea1903 -->

- `--sort <key>` with keys `el`, `g` (Gibbs free energy derived at the
  run's conditions, `rqm/thermo.md`), `rg`, `mass`, `name`.
  Ascending unless `--reverse`. A record whose sort key is undefined sorts
  last ascending, first descending; records without a `name` sort first
  ascending under `name`. `--reverse` only orients a sort (explicit
  `--sort` or the implicit sort); given with no sort in effect it changes
  nothing, though it still counts the run as a manipulation.
- `--select <n>` keeps the first `n` records of each group (after the
  sort); groups shorter than `n` pass whole.
- `--glob` keeps, per composition group, the record with the lowest
  Gibbs free energy at the run's conditions (`rqm/thermo.md`); ties keep
  the first in order. Records with undefined Gibbs are dropped; a group
  whose records all lack Gibbs contributes nothing. Under `--noname` the
  single group keeps the one global minimum. `--glob` does not trigger
  the implicit sort.
- `--shuffle` randomly permutes the final result with Fisher–Yates driven
  by a splitmix64 PRNG seeded from `--seed <u64>` (default 0), making any
  run reproducible.

## Printing <!-- rq-4ea8ca35 -->

- Printing follows JKQC: every printable column has its own flag —
  `--name`, `--status`, `--program`, `--method`, `--el`, `--zpe`, `--h`,
  `--s`, `--g`, `--rg`, `--mass`, `--dip`, `--charge`, `--multiplicity`,
  `--natoms`, `--lf`, `--nfreq`, `--nimag`, `--components`, `--ct`.
  There is no default
  selection: no print flag means no table is printed at all. The
  `--formation` flag (`rqm/formation.md`) prints a section after the
  table; it selects no column and counts as a print flag for the
  visible-outcome rule.
- Columns always appear in the canonical order of that list, regardless
  of the order the flags were given. `--charge` and `--multiplicity`
  denote printing here; the composition-override flags of the hybrid
  model (`rqm/ARCHITECTURE.md`) must keep distinct names when they
  arrive.
- The table is text: a header row of column names, then one row per
  record in run order, values separated by a single space. `el` prints
  in hartree with 10 decimals; the thermochemistry columns `zpe`, `h`,
  `s`, `g` (`rqm/thermo.md`; `s` is the T·S entropy term) print like
  `el`; `rg`, `mass`, `dip` with 3; `lf` with 1 — the lowest stored
  vibrational frequency in cm⁻¹, a raw observable so imaginary modes
  print negative whatever `--makereal` does, undefined when the record
  stores no frequencies; integer fields plainly; `components`
  as `2am+3sa`-style alphabetical pairs; `ct` as the same alphabetical
  pairs with the `+` separators dropped (`2am3sa`), undefined when the
  record has no components or an empty map; an undefined or absent value
  prints `-`.
- `--unit` converts every Hartree-valued print quantity — the energy
  columns (`el`, `zpe`, `h`, `s`, `g`) and the `--formation` section's
  values (`rqm/formation.md`) — from hartree to kcal/mol at this
  boundary: kcal/mol prints with 4 decimals. The flag is a plain
  boolean: it takes no value, and without it energies print in hartree.
  The relative-cutoff kcal/mol conversion above is unaffected by
  `--unit`.
- A run that manipulates prints the pipeline survivors (all status
  `ok`); a run with print flags but no manipulation flag prints every
  record as loaded, in file order — printing alone is not an operation
  on the data.
- A writing run prints to stdout `filtered <n> records into <out>`; the
  table (when requested) prints before that summary line. This ordering
  holds for manipulation runs and for a plain collection run that also
  prints (table first, then the collection summary), so a table is always
  the first thing on stdout when requested.

## Feature API <!-- rq-19b7a23e -->

### Types <!-- rq-ea151d18 -->

- `FilterSpec` — the whole manipulation: `extract: Option<(Vec<String>, <!-- rq-773db11b -->
  ExtractMode)>` with `ExtractMode::{Keep, Drop}`, `cutoffs:
  Vec<Cutoff>`, `uniq: Option<UniqMode>` (`rqm/uniqueness.md`), `reacted:
  Option<ReactedMode>` (`rqm/detection.md`), `sort: Option<SortKey>`,
  `reverse: bool`, `select: Option<usize>`, `glob: bool`, `shuffle: bool`,
  `seed: u64`, `grouping: Grouping` (`Grouping::{Composition, Unit}`).
- `ExtractMode` — `Keep` (`--extract`) or `Drop` (`--nextract`). <!-- rq-b0d6f2f5 -->
- `ExtractPattern` — a parsed pattern: pairs `(count, code)` (zero allowed) <!-- rq-0186810c -->
  and a wildcard `Wildcard::{None, Any, Exactly(u32), Only(Vec<String>),
  OnlyExactly(u32, Vec<String>)}`.
- `Cutoff` — `op: CmpOp` (`Lt, Le, Gt, Ge, Eq, Ne`), `mode: Mode` <!-- rq-8ad5e95a -->
  (`Absolute, Relative`), `property: Property`, `value: f64`.
- `Property` — `El, G, Rg, Lf, Dip, Mass, Charge, Multiplicity, Natoms, <!-- rq-d157ebb2 -->
  Nfreq, Nimag`; `G` derives via `rqm/thermo.md`.
- `SortKey` — `El | G | Rg | Mass | Name`. <!-- rq-953da4ce -->
- `Column` — the print columns above, `Zpe | H | S | G` included. <!-- rq-38b619ff -->
- `Unit` — `Au | Kcal`, the print-boundary energy unit: `Kcal` when the <!-- rq-8718aee5 -->
  `--unit` flag is present, `Au` (hartree) otherwise.
- `ExtractError` — parse failure detail for usage reporting. <!-- rq-881749dc -->

### Functions <!-- rq-d6e00bdf -->

- `parse_extract_query(query: &str) -> Result<Vec<ExtractPattern>, ExtractError>` <!-- rq-2bc9762e -->
  — commas, ranges, choices, zeros, label comments, wildcards as above.
- `extract_matches(pattern: &ExtractPattern, components: Option<&Components>) -> bool` <!-- rq-1e1f9c11 -->
- `property_value(record: &Record, property: Property, memo: &ThermoMemo) -> Option<f64>` <!-- rq-b76b0761 -->
  — derived per the table (`rg`, `mass`, `dip` via `rqm/uniqueness.md`;
  `el` and `g` through the derived-property registry on the run's memo,
  `rqm/thermo.md`).
- `apply_filters(records: &[Record], spec: &FilterSpec, memo: &ThermoMemo) -> Result<Vec<Record>, <!-- rq-bc3e1482 -->
  ExtractError>` — the full pipeline including the ok-only pre-filter and
  grouping; never mutates the input. The memo carries the run's
  thermochemistry conditions (`rqm/thermo.md`) and feeds every
  Gibbs-valued stage — the `g` cutoff, the `g` sort key, the `g`
  uniqueness metric, and `--glob` — deriving each record at most once
  (`rqm/thermo.md` Performance). In composition grouping it parses the
  `extract` query strings and propagates an `ExtractError` rather than
  silently dropping records (the CLI pre-validates, so callers see `Ok`);
  under `Unit` grouping extract compares names verbatim and cannot fail to
  parse.
- `print_table(records: &[Record], columns: &[Column], memo: &ThermoMemo, unit: Unit) -> String` <!-- rq-8420b870 -->

### Crate Layout <!-- rq-bf062c8f -->

- The pipeline, extract parser, cutoffs, sort/select/shuffle, and printing
  live in `src/filter.rs`; the library root re-exports the public types
  and functions. No new dependencies.
- `tests/filter.rs` tests the pipeline as pure functions (grouping, <!-- rq-b21ce0b5 -->
  extract grammar incl. ranges/choices/wildcards, cutoff units and NaN
  behavior, implicit sort, select, seeded shuffle determinism, table
  formatting); `tests/cli.rs` covers the flag surface, `--out` optionality,
  stdout printing, summary and exclusion lines, and exit codes.

## Out of Scope <!-- rq-98e65100 -->

- RMSD/structure-alignment uniqueness (`--arbalign`, `--MWarbalign`): not
  supported; the flags are unknown flags and stay usage errors
  (`rqm/uniqueness.md`).
- Single-point-corrected energies (`gout`, `elout`): re-deriving
  thermochemistry from a higher-level single-point energy is a future
  milestone (`rqm/thermo.md` Out of Scope).
- Adaptive sampling (`-sample`), the JKQC `bonded` cutoff property,
  `-dropimg` (deleting imaginary modes — `--makereal`
  (`rqm/thermo.md`) is the supported way to fold imaginary frequencies
  into a derivation).
- `-xyz`, `-movie`, spreadsheet output: milestone 7.
- stdin input (`rqm/collection.md`).

## Gherkin Scenarios <!-- rq-ad88b915 -->

```gherkin
Feature: Filtering, sorting, and printing

  # --- Extract ---

  @rq-b73d0021
  Scenario: Exact composition extracts its records
    Given a database with records of components am = 2, sa = 3, and am = 1 sa = 1
    When CJKQC is invoked with the database, "--extract 2am3sa --name" and no --out
    Then the printed table lists exactly the am = 2 sa = 3 record

  @rq-9fa28264
  Scenario: A comma list unions
    When the query "2am3sa,1am1sa" is parsed and matched
    Then records of both compositions match

  @rq-5c68d6cb
  Scenario: A count range expands inclusively
    When parse_extract_query("1am0-2w") is called
    Then the patterns are the compositions {am:1}, {am:1,w:1}, {am:1,w:2}

  @rq-767b9e05
  Scenario: A choice expands
    When parse_extract_query("(2,3)am1w") is called
    Then the patterns are {am:2,w:1} and {am:3,w:1}

  @rq-4dbf6ef0
  Scenario: A wildcard admits any extras
    Given records am = 2, am = 2 w = 1, and am = 1
    When the query "2am*w" is matched
    Then the am = 2 and am = 2 w = 1 records match and am = 1 does not

  @rq-7fe408ee
  Scenario: A bare star matches everything
    When the query "*" is matched against any record with components
    Then it matches

  @rq-80b74fa7
  Scenario: An exact pattern rejects extras
    Given a record with components am = 2 w = 1
    When the pattern "2am" is matched
    Then it does not match

  @rq-443f2709
  Scenario: nextract inverts the selection
    Given a database with a record of components am = 2 and a record of components sa = 3
    When CJKQC is invoked with the database, "--nextract 2am --name" and no --out
    Then only the sa = 3 record is printed

  @rq-49f1442c
  Scenario: A label comment in a pattern is ignored
    When parse_extract_query("2am3sa-1conf") is matched against a record with components am = 2 sa = 3
    Then it matches

  @rq-60c7278b
  Scenario: A count-star-code wildcard combines both restrictions
    Given records am = 2 w = 2, am = 2 w = 1 sa = 1, and am = 2
    When the query "2am2*w" is matched
    Then only the am = 2 w = 2 record matches

  @rq-071ab7e7
  Scenario: An underscore in a pattern is a usage error
    When CJKQC is invoked with "--extract 2am3sa_1conf"
    Then the exit code is 2

  @rq-e1e64cd9
  Scenario: A pathological count range is a usage error
    When parse_extract_query("1am1-4294967295w") is called
    Then it returns Err

  @rq-03ad3478
  Scenario: The count-range cap holds exactly 1000 values
    When parse_extract_query("1am1-1000w") is called
    Then it expands to 1000 patterns
    When parse_extract_query("1am1-1001w") is called
    Then it returns Err

  @rq-98d88fc4
  Scenario: A reversed count range is a usage error
    When parse_extract_query("1am3-1w") is called
    Then it returns Err

  @rq-f43b5dbf
  Scenario: An unparsable query is a usage error
    When CJKQC is invoked with "--extract 2am$"
    Then the exit code is 2

  @rq-b6a5add7
  Scenario: Under noname, extract compares names verbatim
    Given a database with records named "2am3sa-1conf" and "2am3sa-2conf"
    When CJKQC is invoked with the database, "--noname --extract 2am3sa-1conf --name" and no --out
    Then only the record named "2am3sa-1conf" is printed

  @rq-32c242d5
  Scenario: Under noname, extract accepts names outside the composition grammar
    Given a database with records named "conf-A" and "conf-B"
    When CJKQC is invoked with the database, "--noname --extract conf-A --name" and no --out
    Then only the record named "conf-A" is printed

  # --- Cutoffs ---

  @rq-77d0cf6c
  Scenario: A relative energy window is per composition group
    Given one group with energies 0.0 and 0.01 hartree and another group with energy -5.0 hartree
    When the cutoff cutr el 5 (kcal/mol) applies
    Then the group's 0.01 hartree record (6.3 kcal/mol above its minimum) is dropped and the -5.0 record survives

  @rq-15a24c78
  Scenario: An absolute energy cutoff uses hartree
    Given records with energies -38.0 and -37.0 hartree
    When the cutoff cut el -37.5 applies
    Then only the -38.0 record survives

  @rq-4319b627
  Scenario: The lowest-frequency pass drops imaginary modes
    Given a record with frequencies [-120.0, 400.0] and one with [25.0, 400.0]
    When the cutoff pass lf 0 applies
    Then only the all-positive record survives

  @rq-e64bbc62
  Scenario: Undefined values fail comparisons except inequality
    Given a record with no dipole moment
    When the cutoff pass dip 1.0 applies
    Then the record is dropped
    When the cutoff isnot dip 1.0 applies
    Then the record is kept

  @rq-6e0398ec
  Scenario: An Rg cutoff uses angstroms
    Given a compact and an extended geometry
    When the cutoff cut rg 10 applies
    Then only the geometry with Rg ≤ 10 Å survives

  @rq-297b5bf7
  Scenario: Boundary values sit on the inclusive side of their operator
    Given records with energies -1.0, -0.5, and 0.0 hartree
    When the cutoff cut el -0.5 applies
    Then the -1.0 and -0.5 records survive
    When the cutoff filter_lt el -0.5 applies
    Then only the -1.0 record survives
    When the cutoff pass el -0.5 applies
    Then only the 0.0 record survives
    When the cutoff filter_ge el -0.5 applies
    Then the -0.5 and 0.0 records survive
    When the cutoff is el -0.5 applies
    Then only the -0.5 record survives
    When the cutoff isnot el -0.5 applies
    Then the -1.0 and 0.0 records survive

  # --- Sort, select, shuffle ---

  @rq-2d073323
  Scenario: Sorting by energy ascending with reverse
    Given records with energies -2.0, -1.0, and no energy
    When sort el applies
    Then the order is -2.0, -1.0, undefined-last
    When --reverse is added
    Then the order is -1.0, -2.0, with the undefined record first

  @rq-297da2a2
  Scenario: Select keeps the first N per group
    Given a group of 3 sorted records and a group of 1
    When select 2 applies
    Then the first group contributes 2 records and the second its 1 record

  @rq-2f2ca923
  Scenario: Uniq implies sorting by energy
    Given records with energies -38.5122 and -38.5123, which agree when rounded to 3 decimals, listed high energy first
    When --uniq el runs without --sort
    Then the -38.5123 record is kept as the lowest-energy representative

  @rq-6a872802
  Scenario: Shuffle is deterministic per seed
    Given a set of 5 records
    When shuffle with seed 7 runs twice
    Then both runs produce identical order
    And a run with seed 8 produces the same multiset

  # --- Pipeline and status ---

  @rq-cd33a3de
  Scenario: Non-ok records are excluded and counted
    Given a database with 2 ok records and 1 failed record
    When a manipulation runs
    Then the failed record is absent from the output
    And stderr reports 1 excluded record with status failed or truncated

  @rq-d1cfac2d
  Scenario: Stage order is fixed
    Given a database whose extract leaves 2 records and whose cutoff leaves 1 of those
    When both flags are given in either order on the command line
    Then the result is the same single record

  @rq-658aba8c
  Scenario: --glob keeps each group's Gibbs minimum among the selected
    Given a group with three records of differing gibbs and a group with one
    When --select 2 applies (after the sort) and then --glob
    Then the first group contributes its lowest-gibbs record among the two selected and the second its only record

  # --- CLI and printing ---

  @rq-6d5b5f10
  Scenario: Print flags print the table while --out writes
    Given a database with one ok record named "1dma" and energy -38.51
    When CJKQC is invoked with the database and "--sort el --name --el --out kept.jsonl"
    Then kept.jsonl contains the single record
    And stdout starts with a header row "name el"
    And the record's row prints the energy with 10 decimals in hartree

  @rq-acf714ae
  Scenario: Print columns follow canonical order regardless of flag order
    Given a database with one ok record whose geometry has an unknown element symbol
    When CJKQC is invoked with the database, "--mass --rg --name" and no --out
    Then the table columns are name, rg, mass in that order, undefined values printed as -

  @rq-207aec4d
  Scenario: The lf column prints the lowest raw frequency
    Given a database with one ok record whose frequencies are [1000.0, -300.0, 150.0]
    When CJKQC is invoked with the database and "--lf" and no --out
    Then the lf column prints -300.0

  @rq-4633531a
  Scenario: The lf column is undefined without stored frequencies
    Given a database with one ok record with no vibrational_frequencies
    When CJKQC is invoked with the database and "--lf" and no --out
    Then the lf column prints -

  @rq-1f63ac38
  Scenario: The ct column prints components without separators
    Given a database with one ok record with components am = 2 sa = 3
    When CJKQC is invoked with the database and "--ct" and no --out
    Then the ct column prints 2am3sa

  @rq-7b0d8978
  Scenario: The ct column is undefined without components
    Given a database with one ok record without components
    When CJKQC is invoked with the database and "--ct" and no --out
    Then the ct column prints -

  @rq-b81cce20
  Scenario: A manipulation with neither --out nor a print flag is a usage error
    When CJKQC is invoked with a database, "--sort el" and no --out and no print flag
    Then the exit code is 2

  @rq-6e881115
  Scenario: A print-only run prints the database without writing anything
    Given a database with 2 ok records and 1 failed record
    When CJKQC is invoked with the database, "--status --name" and no --out
    Then all 3 records are listed in file order
    And no database file is written

  @rq-2b862de1
  Scenario: --out writes the filtered database and prints a summary
    Given a database with 3 ok records of one composition
    When CJKQC is invoked with the database and "--cutr el 1 --out kept.jsonl"
    Then kept.jsonl contains exactly the surviving records
    And stdout contains "filtered <n> records into kept.jsonl"
    And stdout contains no table header

  @rq-9b28d28a
  Scenario: Missing --out is still required without manipulation flags
    When CJKQC is invoked with a database and no manipulation flags and no --out
    Then the exit code is 2

  @rq-3bbefb05
  Scenario: ArbAlign flags remain unknown usage errors
    When CJKQC is invoked with "--arbalign 0.38"
    Then the exit code is 2
    And stderr is clap's unknown-flag diagnostic
```
