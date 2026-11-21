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
- **Manipulation flags** are: `--add-correction`
  (`rqm/correction.md`), `--extrapolate`
  (`rqm/extrapolation.md`), `--rename` (`rqm/composition.md`), `--extract`,
  `--nextract`, `--cut`,
  `--cutr`, `--pass`, `--passr`, `--is`, `--isnot`, `--filter-lt`,
  `--filter-ge`, `--rel-filter-lt`, `--rel-filter-ge`, `--uniq`,
  `--arbalign` (`rqm/arbalign.md`), `--reacted`, `--reacted2`, `--sort`,
  `--reverse`, `--select`, `--glob`, `--shuffle`, `--seed`, `--noname`.
  **Print flags**
  (`--filename`, `--name`, `--path`, `--el`, … — see Printing) select
  table columns; they print and
  manipulate nothing. **Settings flags** — `--temp`, `--fc`, `--fscale`,
  `--makereal`, `--ignoreimag` (`rqm/thermo.md`) and `--unit` — configure
  derivations and the print units; they never manipulate,
  print, or satisfy the visible-outcome rule on their own.
- A run must have a visible outcome. With neither manipulation nor print
  flags, `--out` is required and behavior is exactly collection's.
  `--out` writes the final records (atomic replacement and the `--fout`
  overwrite rule as in collection); print flags print the table to stdout.
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
2. `--rename`, in command order (`rqm/composition.md`)
3. `--extract` / `--nextract` (`rqm/filtering.md` below)
4. cutoffs, in the order the flags were given
5. `--uniq` (`rqm/uniqueness.md`)
6. `--reacted` / `--reacted2` (`rqm/detection.md`)
7. `--sort` (then `--reverse`)
8. `--select`
9. `--arbalign` (`rqm/arbalign.md`)
10. `--glob`
11. `--shuffle`

Renaming precedes every composition-dependent stage, so extraction,
relative cutoffs, uniqueness, reacted detection, arbalign filtering,
grouping, and the printed
sections all see the renamed composition (`rqm/composition.md`).

Implicit sort: when `--uniq` (keys mode), `--arbalign`, or `--select` is
present but `--sort` is not, records sort by `el` ascending first.
`--uniq dup` does not trigger it.

## Grouping <!-- rq-2f996ba6 -->

- Cutoffs (relative mode), `--uniq`, `--arbalign`, `--reacted`, `--select`,
  and `--extract` wildcard/exact composition matching treat records by
  **composition group**: records sharing the same `components` map
  (`rqm/composition.md`) form a group; records without `components` form
  one separate unnamed group.
- `--noname` collapses everything into a single global group;
  `--extract`/`--nextract` then compare their patterns verbatim against
  each record's `name` (no ranges, wildcards, or zero-count rules). The
  compared `name` is the record's `name` field — the `filename` column
  (`rqm/filtering.md` Printing) — not the composition token the `name`
  column prints. `--arbalign` is the exception: it always partitions by
  exact `components`, so `--noname` never makes structures of different
  composition comparable (`rqm/arbalign.md`).
- The population columns (`rqm/population.md`) group by the same rule: one
  population per composition group, or one global population under
  `--noname`.
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
| `elcorr` | attached correction energy (`rqm/correction.md`) | hartree | kcal/mol (× the Eh→kcal/mol constant of `rqm/thermo.md` Constants) |
| `gcorr` | Gibbs free energy from the attached correction (`rqm/correction.md`) | hartree | kcal/mol (× the Eh→kcal/mol constant of `rqm/thermo.md` Constants) |
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
  run's conditions, `rqm/thermo.md`), `elcorr`, `gcorr` (the corrected
  levels, `rqm/correction.md`), `rg`, `mass`, `filename`.
  Ascending unless `--reverse`. A record whose sort key is undefined sorts
  last ascending, first descending; records without a `name` sort first
  ascending under `filename`. `--reverse` only orients a sort (explicit
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

## Arbalign Filtering <!-- rq-2b057ce6 -->

`--arbalign <threshold>` removes geometrically redundant structures within
composition groups, using ArbAlign's principal-axis preprocessing, atom
assignment, and Kabsch RMSD (`rqm/arbalign.md`). The threshold is an RMSD
in ångström; it must be finite and non-negative, and a later record is
eliminated when its RMSD to an earlier survivor is strictly below it.
Principal-axis preprocessing is always applied; there is no flag to
disable it.

- The stage runs at step 9, after `--sort`, `--reverse`, and `--select`,
  and before `--glob` and `--shuffle`. When `--sort` is absent it
  triggers the implicit `el` sort, so the lowest-energy record of a
  duplicate set survives unless `--reverse` or an explicit sort says
  otherwise.
- Within a group, records are compared in their current order. A record
  already removed never eliminates another record, so the first surviving
  record in that order wins.
- Records without usable geometry (no geometry or an unknown element) are
  left untouched and counted on stderr as
  `arbalign: kept <k> records without usable geometry`. Pairs whose
  geometries have different atom counts or element multisets are not
  compared and neither record is removed.
- `--arbalign` is the only structure-alignment flag; `--MWarbalign` is not
  accepted.

## Printing <!-- rq-4ea8ca35 -->

- Printing follows JKQC: every printable column has its own flag —
  `--filename`, `--name`, `--path`, `--status`, `--program`, `--command`,
  `--correction`, `--el`, `--zpe`, `--h`, `--s`, `--g`, `--elcorr`,
  `--hcorr`, `--gcorr`, `--pop`, `--rg`, `--mass`, `--dip`, `--dips`,
  `--pol`, `--pols`, `--charge`, `--multiplicity`, `--rsn`, `--natoms`,
  `--lf`, `--freqs`, `--nfreq`, `--nimag`, `--rots`, `--components`.
  `--dip` prints the dipole magnitude; `--dips` prints the dipole
  vector; `--pol` prints the isotropic polarizability (one third of the
  polarizability tensor's trace); `--pols` prints the polarizability
  tensor. The dipole columns print in atomic units or Debye, the
  polarizability columns in atomic units or Å³, selected by `--unit`.
  There is no default
  selection: no print flag means no table is printed at all. The
  `--formation` flag (`rqm/formation.md`) prints a section after the
  table; it selects no column and counts as a print flag for the
  visible-outcome rule. The `--pop` flag (`rqm/population.md`) selects the
  population columns; it takes an optional level argument (`el`, `g`,
  `elcorr`, `gcorr` — `g` when omitted) and is repeatable, one column per
  requested level.
- Columns always appear in the canonical order of that list, regardless
  of the order the flags were given. `--charge` and `--multiplicity`
  denote printing here; the composition-override flags of the hybrid
  model (`rqm/ARCHITECTURE.md`) must keep distinct names when they
  arrive.
- The table is text: a header row of column names, then one row per
  record in run order, laid out `column -t` style — every column is as
  wide as its widest cell (the header cell included; a cell's width is
  its whole printed string, internal spaces included), with a single
  space between columns and no trailing whitespace: a line ends with
  its last cell, whatever that cell's alignment. The text columns
  `filename`, `name`, `path`, `status`, `program`, `command`,
  `correction`, and `components` align left; the
  numeric columns — the energy columns, the population columns, `rg`,
  `mass`, `dip`, `dips`, `pol`, `pols`, `charge`, `multiplicity`, `rsn`,
  `natoms`, `lf`, `freqs`, `nfreq`, `nimag`, and `rots` — align right; in both cases the
  header cell aligns with its column, so numbers line up under their
  header. Cells keep their per-column formats. The energy columns `el`,
  `zpe`, `h`, `s`, `g`, `elcorr`, `hcorr`, and `gcorr` print in hartree
  with 10 decimals (`s` is the T·S entropy term; `rqm/thermo.md` and
  `rqm/correction.md`). `rg`, `mass`, `dip`, `pol`, and each component of
  `dips` and `pols` print with 3 decimals; `dips` lists the stored dipole
  vector's x, y, z components in that order, comma-separated without
  spaces. `pol` prints the isotropic polarizability — one third of the
  trace of the record's `polarizability` tensor (`rqm/record.md`) — and
  is undefined when the tensor is absent; `--unit` selects Debye for the
  dipole columns and Å³ for the polarizability columns, atomic units
  otherwise. `pols` lists
  all nine tensor entries row-major
  (`xx,xy,xz,yx,yy,yz,zx,zy,zz`), comma-separated without spaces, and is
  likewise undefined when the tensor is absent. `lf`
  prints with 1 decimal — the lowest stored vibrational frequency in
  cm⁻¹, a raw observable so imaginary modes print negative whatever
  `--makereal` or `--ignoreimag` does, undefined when the record stores
  no frequencies. `freqs` prints every stored vibrational frequency in
  stored order with 1 decimal each, comma-separated without spaces, with
  the same raw-sign rule as `lf`. `rots` prints the rotational constants
  of `rqm/thermo.md` in their descending cm⁻¹ order with 6 decimals
  each, comma-separated without spaces. `rsn`, `charge`, `multiplicity`,
  `natoms`, and the frequency counts print as plain integers.
  `components` prints as `2am+3sa`-style alphabetical pairs; `name` as
  the same alphabetical pairs with the `+` separators dropped (`2am3sa`),
  undefined when the record has no components or an empty map. `filename`
  prints the record's `name` — the stem of the source file name
  (`rqm/record.md`); `path` prints the record's `source_path`
  (`rqm/record.md`) verbatim. An
  undefined or absent value prints `-`; a list-valued column (`dips`,
  `freqs`, `rots`) with no values also prints `-`, including a monatomic
  record's empty rotational constant list. The population columns `pop`,
  `popel`, `popelcorr`, `popgcorr` (`rqm/population.md`) print
  dimensionless probabilities with 4 decimals. The `program` column
  prints the program token and the
  version joined by one space (`xtb 6.7.1`), and the token alone when the
  version is absent. The `command` column wraps the stored command line
  in double quotes, since the value contains spaces; an absent command
  prints a bare unquoted `-`. The `correction` column prints a record's
  correction provenance (`rqm/correction.md`).
- `--unit` selects the practical print units. The energy columns (`el`,
  `zpe`, `h`, `s`, `g`, `elcorr`, `hcorr`, `gcorr`) and the `--formation`
  section's values (`rqm/formation.md`) convert from hartree to kcal/mol
  (4 decimals); the dipole columns (`dip`, `dips`) convert from atomic
  units to Debye (3 decimals); the polarizability columns (`pol`, `pols`)
  convert from atomic units to Å³ (3 decimals), using the conversion
  constants of `rqm/thermo.md` Constants. The flag is a plain boolean: it
  takes no value, and without it energies print in hartree and dipoles
  and polarizabilities print in atomic units. The relative-cutoff
  kcal/mol conversion above and the `dip` cutoff's atomic units are
  unaffected by `--unit`.
- A run that manipulates prints the pipeline survivors (all status
  `ok`); a run with print flags but no manipulation flag prints every
  record as loaded, in file order — printing alone is not an operation
  on the data. `--add-correction` (`rqm/correction.md`) is the one
  manipulation flag that does not change which records a run writes when
  it acts alone: an attachment-only run — no print flag, no other
  manipulation flag — keeps collection semantics, writing every loaded
  record under the collection summary line. Any print flag or any other
  manipulation flag switches the run to the pipeline semantics above.
- A writing run prints to stdout `filtered <n> records into <out>`; with
  `--add-correction` (`rqm/correction.md`) the line gains
  `(<k> corrected)`. The table (when requested) prints before that summary
  line. This ordering holds for manipulation runs and for a plain
  collection run that also prints (table first, then the collection
  summary), so a table is always the first thing on stdout when requested.

## Feature API <!-- rq-19b7a23e -->

### Types <!-- rq-ea151d18 -->

- `FilterSpec` — the whole manipulation: `extract: Option<(Vec<String>, <!-- rq-773db11b -->
  ExtractMode)>` with `ExtractMode::{Keep, Drop}`, `renames:
  Vec<RenameSpec>` (`rqm/composition.md`), `cutoffs:
  Vec<Cutoff>`, `uniq: Option<UniqMode>` (`rqm/uniqueness.md`),
  `arbalign: Option<f64>` (`rqm/arbalign.md`), `reacted:
  Option<ReactedMode>` (`rqm/detection.md`), `sort: Option<SortKey>`,
  `reverse: bool`, `select: Option<usize>`, `glob: bool`, `shuffle: bool`,
  `seed: u64`, `grouping: Grouping` (`Grouping::{Composition, Unit}`),
  `parallel: usize` (the run's `--parallel` worker count, read only by the
  arbalign stage).
- `ExtractMode` — `Keep` (`--extract`) or `Drop` (`--nextract`). <!-- rq-b0d6f2f5 -->
- `ExtractPattern` — a parsed pattern: pairs `(count, code)` (zero allowed) <!-- rq-0186810c -->
  and a wildcard `Wildcard::{None, Any, Exactly(u32), Only(Vec<String>),
  OnlyExactly(u32, Vec<String>)}`.
- `Cutoff` — `op: CmpOp` (`Lt, Le, Gt, Ge, Eq, Ne`), `mode: Mode` <!-- rq-8ad5e95a -->
  (`Absolute, Relative`), `property: Property`, `value: f64`.
- `Property` — `El, G, ElCorr, GCorr, Rg, Lf, Dip, Mass, Charge, <!-- rq-d157ebb2 -->
  Multiplicity, Natoms, Nfreq, Nimag`; `G` derives via `rqm/thermo.md`;
  `ElCorr` and `GCorr` via `rqm/correction.md`.
- `SortKey` — `El | G | ElCorr | GCorr | Rg | Mass | Filename`. <!-- rq-953da4ce -->
- `Column` — the print columns above, `Zpe | H | S | G | ElCorr | <!-- rq-38b619ff -->
  HCorr | GCorr | Correction | Dips | Rsn | Freqs | RotS` included,
  plus the population columns
  `Pop | PopEl | PopElCorr | PopGCorr` (`rqm/population.md`).
- `Unit` — `Au | Kcal`, the print-boundary unit mode: `Kcal` when the <!-- rq-8718aee5 -->
  `--unit` flag is present, `Au` otherwise. `Kcal` prints energies in
  kcal/mol, dipole columns in Debye, and polarizability columns in Å³;
  `Au` prints energies in hartree and dipole and polarizability columns
  in atomic units. The variant name `Kcal` is historical: it selects
  every practical print unit, not only kcal/mol, and is retained for API
  stability.
- `ExtractError` — parse failure detail for usage reporting. <!-- rq-881749dc -->
- `FilterOutcome` — the pipeline result: `records: Vec<Record>` and <!-- rq-87ba569d -->
  `arbalign: ArbAlignReport` (`rqm/arbalign.md`), whose skipped count the
  CLI reports on stderr.
- `FilterError` — why a pipeline run stopped: `Extract(ExtractError)` for a <!-- rq-2c3246dc -->
  query that fails to parse (a usage error, exit 2), or
  `Rename { record: Option<String>, error: RenameError }`
  (`rqm/composition.md`) for a component count overflow (a runtime error,
  exit 1). `record` is the `name` of the record whose components could not
  be renamed, absent when that record has no name.

### Functions <!-- rq-d6e00bdf -->

- `parse_extract_query(query: &str) -> Result<Vec<ExtractPattern>, ExtractError>` <!-- rq-2bc9762e -->
  — commas, ranges, choices, zeros, label comments, wildcards as above.
- `extract_matches(pattern: &ExtractPattern, components: Option<&Components>) -> bool` <!-- rq-1e1f9c11 -->
- `property_value(record: &Record, property: Property, memo: &ThermoMemo) -> Option<f64>` <!-- rq-b76b0761 -->
  — derived per the table (`rg`, `mass`, `dip` via `rqm/uniqueness.md`;
  `el`, `g`, `elcorr`, and `gcorr` through the derived-property registry on
  the run's memo, `rqm/thermo.md` and `rqm/correction.md`).
- `apply_filters(records: &[Record], spec: &FilterSpec, memo: &ThermoMemo) -> Result<FilterOutcome, <!-- rq-bc3e1482 -->
  FilterError>` — the full pipeline including the ok-only pre-filter and
  grouping; never mutates the input. The renames apply to the survivors in
  command order before extraction, and a merge beyond `u32::MAX` surfaces as
  `FilterError::Rename` with the offending record's name
  (`rqm/composition.md`). The memo carries the run's
  thermochemistry conditions (`rqm/thermo.md`) and feeds every
  Gibbs-valued stage — the `g` cutoff, the `g` sort key, the `g`
  uniqueness metric, and `--glob` — deriving each record at most once
  (`rqm/thermo.md` Performance). The stage order is fixed: the implicit
  `el` sort is triggered by `--uniq` in keys mode, `--arbalign`, or
  `--select`; arbalign runs after the explicit or implicit sort and after
  `--select`, before `--glob` and `--shuffle`. In composition grouping it
  parses the `extract` query strings and propagates an `ExtractError`
  rather than silently dropping records (the CLI pre-validates, so callers
  see `Ok` except for a rename overflow); under `Unit` grouping extract
  compares names verbatim and cannot fail to parse.
- `print_table(records: &[Record], columns: &[Column], memo: &ThermoMemo, unit: Unit, grouping: Grouping) -> String` <!-- rq-8420b870 -->
  — `grouping` is the run's grouping (`rqm/filtering.md` Grouping): the
  population columns (`rqm/population.md`) pool per composition group, or
  into one global pool under `--noname`.

### Crate Layout <!-- rq-bf062c8f -->

- The pipeline, extract parser, cutoffs, sort/select/shuffle, and printing
  live in `src/filter.rs`; the arbalign algorithm lives in
  `src/arbalign.rs` (`rqm/arbalign.md`), called from the pipeline. The
  library root re-exports the public types and functions.
  `FilterSpec::renames` reuses `RenameSpec` and
  `rename_component` from `src/composition.rs` (`rqm/composition.md`). No
  new dependencies.
- `tests/filter.rs` tests the pipeline as pure functions (grouping, <!-- rq-b21ce0b5 -->
  extract grammar incl. ranges/choices/wildcards, cutoff units and NaN
  behavior, implicit sort, arbalign ordering, select, seeded shuffle
  determinism, table
  formatting); `tests/cli.rs` covers the flag surface, `--out` optionality,
  stdout printing, summary and exclusion lines, and exit codes.

## Out of Scope <!-- rq-98e65100 -->

- Mass-weighted ArbAlign (`--MWarbalign`) and other ArbAlign variants;
  `--arbalign` is the only structure-alignment flag (`rqm/arbalign.md`).
- Adaptive sampling (`-sample`), the JKQC `bonded` cutoff property,
  `-dropimg` (deleting imaginary modes from the record — `--makereal`
  and `--ignoreimag` (`rqm/thermo.md`) fold or skip imaginary frequencies
  inside a derivation and never edit a record).
- Spreadsheet output: milestone 7 (`--movie` and `--write-xyz` are
  specified in `rqm/movie.md`).
- stdin input (`rqm/collection.md`).

## Gherkin Scenarios <!-- rq-ad88b915 -->

```gherkin
Feature: Filtering, sorting, and printing

  # --- Extract ---

  @rq-b73d0021
  Scenario: Exact composition extracts its records
    Given a database with records of components am = 2, sa = 3, and am = 1 sa = 1
    When CJKQC is invoked with the database, "--extract 2am3sa --filename" and no --out
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
    When CJKQC is invoked with the database, "--nextract 2am --filename" and no --out
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
    When CJKQC is invoked with the database, "--noname --extract 2am3sa-1conf --filename" and no --out
    Then only the record named "2am3sa-1conf" is printed

  @rq-32c242d5
  Scenario: Under noname, extract accepts names outside the composition grammar
    Given a database with records named "conf-A" and "conf-B"
    When CJKQC is invoked with the database, "--noname --extract conf-A --filename" and no --out
    Then only the record named "conf-A" is printed

  # --- Rename ---

  @rq-4b48a995
  Scenario: A rename count overflow is a runtime failure
    Given a database with a record of components p = 1 and sa = 4294967295
    When CJKQC is invoked with the database, "--rename p sa --out out.jsonl"
    Then the exit code is 1
    And stderr names the record
    And out.jsonl does not exist

  @rq-7b3a3a9c
  Scenario: A rename overflow names the offending record among several
    Given a database with an ok record "fine" of components p = 1
    And an ok record "overflowing" of components p = 1 and sa = 4294967295
    When CJKQC is invoked with the database, "--rename p sa --out out.jsonl"
    Then the exit code is 1
    And stderr names "overflowing" and not "fine"
    And out.jsonl does not exist

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

  # --- Corrected energies ---

  @rq-41bf3f48
  Scenario: A relative corrected-energy window is per composition group
    Given one group with corrected energies 0.0 and 0.02 Eh and another with -5.0 Eh
    When the cutoff cutr elcorr 5 applies
    Then the 0.02 Eh record (12.6 kcal/mol above its minimum) is dropped and the -5.0 record survives

  @rq-16bb6b61
  Scenario: Sorting by corrected Gibbs puts undefined values last
    Given records with corrected Gibbs -100.0 and -100.5 and one without a correction
    When CJKQC is invoked with the database and "--sort gcorr --filename" and no --out
    Then the order is -100.5, -100.0, uncorrected

  @rq-a8f483c7
  Scenario: The corrected columns sit in canonical order
    Given a database with one ok record
    When CJKQC is invoked with the database and "--gcorr --elcorr --correction --command --hcorr" and no --out
    Then the header orders command, correction, elcorr, hcorr, gcorr

  # --- CLI and printing ---

  @rq-6d5b5f10
  Scenario: Print flags print the table while --out writes
    Given a database with one ok record named "1dma" and energy -38.51
    When CJKQC is invoked with the database and "--sort el --filename --el --out kept.jsonl"
    Then kept.jsonl contains the single record
    And stdout starts with a header row naming the columns filename and el
    And the record's row prints the energy with 10 decimals in hartree

  @rq-acf714ae
  Scenario: Print columns follow canonical order regardless of flag order
    Given a database with one ok record whose geometry has an unknown element symbol
    When CJKQC is invoked with the database, "--mass --rg --filename" and no --out
    Then the table columns are filename, rg, mass in that order, undefined values printed as -

  @rq-be8df504
  Scenario: The table aligns its columns like column -t
    Given a database with one ok record named "1dma" whose radius of gyration is 2.345 and one ok record named "1" without a geometry
    When CJKQC is invoked with the database and "--filename --rg" and no --out
    Then the rg column is as wide as its widest cell, header included, and its cells right-align under the right-aligned header
    And the filename cells left-align under the left-aligned header
    And the undefined rg cell prints - right-aligned with the numbers

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

  @rq-c08f088e
  Scenario: The rsn column prints the rotational symmetry number
    Given a database with one ok record whose rotational_symmetry_number is 12
    When CJKQC is invoked with the database and "--rsn" and no --out
    Then the rsn column prints 12

  @rq-1014452e
  Scenario: The rsn column is undefined without a rotational symmetry number
    Given a database with one ok record without a rotational_symmetry_number
    When CJKQC is invoked with the database and "--rsn" and no --out
    Then the rsn column prints -

  @rq-c14b6917
  Scenario: The dips column prints the dipole vector components
    Given a database with one ok record whose dipole_moment is [0.1234, -0.4567, 0.7891]
    When CJKQC is invoked with the database and "--dips" and no --out
    Then the dips column prints 0.123,-0.457,0.789

  @rq-93f9d560
  Scenario: The dips column is undefined without a dipole moment
    Given a database with one ok record without a dipole_moment
    When CJKQC is invoked with the database and "--dips" and no --out
    Then the dips column prints -

  @rq-1b7b3af3
  Scenario: The pol column prints the isotropic polarizability
    Given a database with one ok record whose polarizability tensor is
    [3.0, 0.0, 0.0, 0.0, 6.0, 0.0, 0.0, 0.0, 9.0]
    When CJKQC is invoked with the database and "--pol" and no --out
    Then the pol column prints 6.000

  @rq-b6bcec13
  Scenario: The pol column is undefined without a polarizability tensor
    Given a database with one ok record without a polarizability
    When CJKQC is invoked with the database and "--pol" and no --out
    Then the pol column prints -

  @rq-77dd2f68
  Scenario: The pols column prints the polarizability tensor row-major
    Given a database with one ok record whose polarizability tensor is
    [3.0, 0.1, 0.2, 0.1, 6.0, 0.3, 0.2, 0.3, 9.0]
    When CJKQC is invoked with the database and "--pols" and no --out
    Then the pols column prints 3.000,0.100,0.200,0.100,6.000,0.300,0.200,0.300,9.000

  @rq-31d1c63f
  Scenario: The pols column is undefined without a polarizability tensor
    Given a database with one ok record without a polarizability
    When CJKQC is invoked with the database and "--pols" and no --out
    Then the pols column prints -

  @rq-ee2f64a3
  Scenario: --unit converts the dipole columns to Debye
    Given a database with one ok record whose dipole_moment is [0.1234, -0.4567, 0.7891]
    When CJKQC is invoked with the database and "--dip --dips --unit" and no --out
    Then the dip column prints 2.339
    And the dips column prints 0.314,-1.161,2.006

  @rq-ac4624f4
  Scenario: --unit converts the polarizability columns to cubic angstroms
    Given a database with one ok record whose polarizability tensor is
    [3.0, 0.1, 0.2, 0.1, 6.0, 0.3, 0.2, 0.3, 9.0]
    When CJKQC is invoked with the database and "--pol --pols --unit" and no --out
    Then the pol column prints 0.889
    And the pols column prints 0.445,0.015,0.030,0.015,0.889,0.044,0.030,0.044,1.334

  @rq-79642963
  Scenario: Without --unit the dipole and polarizability columns remain atomic units
    Given a database with one ok record whose dipole_moment is [0.1234, -0.4567, 0.7891]
    And whose polarizability tensor is [3.0, 0.0, 0.0, 0.0, 6.0, 0.0, 0.0, 0.0, 9.0]
    When CJKQC is invoked with the database and "--dip --pol" and no --out
    Then the dip column prints 0.920
    And the pol column prints 6.000

  @rq-c6b69701
  Scenario: --unit leaves the dip cutoff in atomic units
    Given a database with one ok record whose dipole_moment has magnitude 0.920 au
    When CJKQC is invoked with the database and "--unit --cut dip 1.0 --filename" and no --out
    Then the record survives because 0.920 au is at most 1.0 au

  @rq-003218f8
  Scenario: The freqs column prints every stored frequency in stored order
    Given a database with one ok record whose frequencies are [1000.0, -300.0, 150.0]
    When CJKQC is invoked with the database and "--freqs" and no --out
    Then the freqs column prints 1000.0,-300.0,150.0

  @rq-27634d1a
  Scenario: The freqs column is undefined without stored frequencies
    Given a database with one ok record with no vibrational_frequencies or an empty frequency list
    When CJKQC is invoked with the database and "--freqs" and no --out
    Then the freqs column prints -

  @rq-f321c97b
  Scenario: The rots column prints the descending rotational constants
    Given the record parsed from tests/fixtures/orca/1pinic1sa-110_1_9-str9.out
    When CJKQC is invoked with the database and "--rots" and no --out
    Then the rots column prints 0.024157,0.008371,0.007528

  @rq-b04ab918
  Scenario: The rots column prints one constant for a linear record
    Given a database with one ok linear record
    When CJKQC is invoked with the database and "--rots" and no --out
    Then the rots column prints one comma-separated constant

  @rq-3d385e42
  Scenario: The rots column is undefined when the rotational constants are undefined
    Given a database with one ok record without a geometry
    When CJKQC is invoked with the database and "--rots" and no --out
    Then the rots column prints -

  @rq-96b5a37a
  Scenario: The rots column is undefined for a degenerate geometry
    Given a database with one ok record whose atoms all coincide at the origin
    When CJKQC is invoked with the database and "--rots" and no --out
    Then the rots column prints -

  @rq-2550fa8c
  Scenario: The rots column is undefined for a monatomic record
    Given a database with one ok monatomic record
    When CJKQC is invoked with the database and "--rots" and no --out
    Then the rots column prints -

  @rq-af106fa7
  Scenario: The new print columns follow canonical order
    Given a database with one ok record
    When CJKQC is invoked with the database, "--rots --rsn --freqs --dips --dip --lf --filename" and no --out
    Then the table columns are filename, dip, dips, rsn, lf, freqs, rots in that order

  @rq-1f63ac38
  Scenario: The name column prints the composition token without separators
    Given a database with one ok record with components am = 2 sa = 3
    When CJKQC is invoked with the database and "--name" and no --out
    Then the name column prints 2am3sa

  @rq-7b0d8978
  Scenario: The name column is undefined without components
    Given a database with one ok record without components
    When CJKQC is invoked with the database and "--name" and no --out
    Then the name column prints -

  @rq-c0c700b2
  Scenario: The filename column prints the record's name
    Given a database with one ok record named "2am3sa-1conf"
    When CJKQC is invoked with the database and "--filename" and no --out
    Then the filename column prints 2am3sa-1conf

  @rq-9bea312b
  Scenario: The filename column is undefined without a name
    Given a database with one ok record without a name
    When CJKQC is invoked with the database and "--filename" and no --out
    Then the filename column prints -

  @rq-5ad1c077
  Scenario: The path column prints the record's source path
    Given a database with one ok record with source_path "/scratch/runs/xtb/1dma.log"
    When CJKQC is invoked with the database and "--path" and no --out
    Then the path column prints /scratch/runs/xtb/1dma.log

  @rq-3f2648cf
  Scenario: The path column is undefined without a source path
    Given a database with one ok record without a source_path
    When CJKQC is invoked with the database and "--path" and no --out
    Then the path column prints -

  @rq-6fb14f23
  Scenario: The identity columns lead the canonical order
    Given a database with one ok record
    When CJKQC is invoked with the database, "--status --path --name --filename" and no --out
    Then the table columns are filename, name, path, status in that order

  @rq-2371c178
  Scenario: --sort filename sorts by the record's name
    Given a database with records named "b" and "a" and one without a name
    When CJKQC is invoked with the database and "--sort filename --filename" and no --out
    Then the order is nameless, a, b
    When --reverse is added
    Then the order is b, a, nameless

  @rq-f9ccc35c
  Scenario: The program column prints the program and its version
    Given a database with one ok record whose program is "xtb" and program_version is "6.7.1"
    When CJKQC is invoked with the database and "--program" and no --out
    Then the program column prints xtb 6.7.1

  @rq-6decd555
  Scenario: The program column prints only the name without a version
    Given a database with one ok record whose program is "xyz" and whose program_version is absent
    When CJKQC is invoked with the database and "--program" and no --out
    Then the program column prints xyz

  @rq-777c8582
  Scenario: The command column quotes its value
    Given a database with one ok record whose command is "xtb 1dma.xyz --gxtb --acc 0.01"
    When CJKQC is invoked with the database and "--command" and no --out
    Then the command column prints "xtb 1dma.xyz --gxtb --acc 0.01" enclosed in double quotes

  @rq-804a53e2
  Scenario: The command column is undefined without a command
    Given a database with one ok record with no command
    When CJKQC is invoked with the database and "--command" and no --out
    Then the command column prints a bare - without quotes

  @rq-b81cce20
  Scenario: A manipulation with neither --out nor a print flag is a usage error
    When CJKQC is invoked with a database, "--sort el" and no --out and no print flag
    Then the exit code is 2

  @rq-6e881115
  Scenario: A print-only run prints the database without writing anything
    Given a database with 2 ok records and 1 failed record
    When CJKQC is invoked with the database, "--status --filename" and no --out
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
  Scenario: --MWarbalign remains an unknown usage error
    When CJKQC is invoked with "--MWarbalign 0.38"
    Then the exit code is 2
    And stderr is clap's unknown-flag diagnostic

  @rq-e753e60e
  Scenario: Arbalign runs after the sort and after select
    Given a database whose energy-sorted order is A, B, C and A eliminates B
    When CJKQC is invoked with the database, "--sort el --arbalign 0.1 --select 2 --filename" and no --out
    Then only A is printed

  @rq-6a0e38b6
  Scenario: Arbalign counts as a manipulation
    Given a database with one ok record
    When CJKQC is invoked with the database and "--arbalign 0.1" and no --out and no print flag
    Then the exit code is 2
```
