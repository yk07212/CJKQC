# Feature: Geometric Metrics and Property Uniqueness <!-- rq-21d1d450 -->

Four scalar metrics summarize a record — total mass, mass-weighted
radius of gyration, dipole magnitude, and Gibbs free energy — computed on
demand from the stored observables and never stored ("store observables,
derive everything else", `rqm/ARCHITECTURE.md`). The Gibbs metric derives
through the thermochemistry model (`rqm/thermo.md`) and follows the run's
`--temp`, `--fc`, `--fscale`, `--makereal`, and `--ignoreimag` settings.
The metrics are inputs to filtering and uniqueness (`rqm/filtering.md`).
Uniqueness removes redundant structures:
records whose name repeats (`dup` mode) or records that share a rounded
property signature with an earlier record of the same composition group
(`keys` mode). RMSD/structure-alignment uniqueness is not supported
(`rqm/ARCHITECTURE.md` open question, deferred).

## Metric Definitions <!-- rq-4a909f01 -->

- **Total mass** — `Σ m(symbol_i)` over the geometry's atoms, in unified
  atomic mass units (u).
- **Radius of gyration (Rg)** — mass-weighted radius of gyration about the
  center of mass, in Å:
  `Rg = sqrt( Σ m_i · |r_i − R_cm|² / Σ m_i )` with
  `R_cm = Σ m_i·r_i / Σ m_i`. This matches the JKQC definition (ASE
  center-of-mass / masses).
- **Dipole magnitude** — `sqrt(x² + y² + z²)` of the record's
  `dipole_moment` vector, in atomic units (e·a₀); conversion to Debye is a
  presentation concern (`rqm/ARCHITECTURE.md`).
- **Gibbs free energy (g)** — the record's Gibbs free energy under the
  run's thermochemistry conditions (`rqm/thermo.md`), in hartree; like
  every quantity there it is derived, never stored.

A metric is **undefined** for a record when its inputs are absent (no
geometry, no dipole, undefined thermochemistry) or when a geometry symbol
is not in the mass table.
An undefined metric flows through comparisons like NaN: cutoff comparisons
`<`, `<=`, `>`, `>=`, `==` drop such records, `!=` keeps them
(`rqm/filtering.md`), and uniqueness treats undefined values as equal to
each other.

## Mass Table <!-- rq-12d8fdf2 -->

- A static table of standard atomic weights (u) for element symbols H
  through Og (Z = 1–118), title-case keys, embedded in the binary; no
  dependency and no runtime download.
- Lookup matches the geometry's title-case symbols exactly (`"Cl"`, not
  `"CL"` or `"cl"`); an unknown symbol leaves total mass and Rg undefined
  for that record.
- Monatomic ions reuse the neutral atom's standard atomic weight; electron
  mass is not accounted for.

## Uniqueness Model <!-- rq-8472dc37 -->

Both modes operate within the composition groups defined by
`rqm/filtering.md` (with `--noname`, one global group) and are **stable**:
the first occurrence of a signature in the current record order survives.
Callers that want "keep the lowest-energy representative" sort by energy
first — the CLI does exactly that when `--uniq` is used without `--sort`
(`rqm/filtering.md`).

- **`dup` mode** — records sharing a `name` with an earlier record are
  dropped. Records without a `name` are never dropped as duplicates.
- **`keys` mode** — the signature of a record is the tuple of its selected
  rounded metrics. Two signatures are equal when every component is equal;
  two undefined (NaN-like) components compare equal. Rounded equality:
  values `a` and `b` agree at `d` decimals exactly when
  `floor(a · 10^d) == floor(b · 10^d)` (floating-point `10^d`, floor
  toward −∞, so negative energies round on the number line, not away
  from zero).
- Keys are written `metric[decimals]` in a comma list (e.g.
  `el2.4,rg`); the decimal count is optional and defaults per metric:
  `el` 3, `g` 3, `rg` 2, `mass` 2, `dip` 1. Decimals are decimal numbers ≥ 0
  and may be fractional — `el2.4` is valid and rounding then uses the
  floating-point factor 10^2.4 (the original accepted this); a negative
  or malformed decimal count is a usage error (exit 2).
- Entries naming the same metric with the same decimals collapse to one;
  the same metric at different decimals is a genuine two-component
  signature and both entries stay.

## Feature API <!-- rq-7c56284e -->

### Types <!-- rq-db938553 -->

- `UniqKey` — a metric `El | G | Rg | Mass | Dip` with an explicit decimal <!-- rq-159da4d1 -->
  count (`f64`, at least 0); the metric's default applies when the CLI
  key string omits one.
- `UniqMode` — `Dup` or `Keys(Vec<UniqKey>)` (non-empty, identical <!-- rq-990dde7d -->
  metric+decimals entries deduplicated).

### Functions <!-- rq-2ce828ba -->

- `metric_total_mass(record: &Record) -> Option<f64>` <!-- rq-9f4db37a -->
  - `None` without a geometry or with a symbol outside the mass table.
- `metric_radius_of_gyration(record: &Record) -> Option<f64>` <!-- rq-b7959747 -->
  - Mass-weighted definition above; `None` under the same conditions.
- `metric_dipole_magnitude(record: &Record) -> Option<f64>` <!-- rq-dab135ee -->
  - `None` when `dipole_moment` is absent.
- `atomic_mass(symbol: &str) -> Option<f64>` <!-- rq-3e2c966b -->
  - Standard atomic weight in u; exact title-case match, `None` otherwise.
- `dedup_names(records: &[Record]) -> Vec<Record>` <!-- rq-a7757bca -->
  - Keeps the first record for each distinct present `name`; nameless
    records always kept; input order preserved.
- `unique_by_properties(records: &[Record], keys: &[UniqKey], memo: &ThermoMemo) -> Vec<Record>` <!-- rq-7650a23d -->
  - Stable first-wins selection on the rounded signature tuples, over the
    whole slice (grouping is the caller's concern,
    `rqm/filtering.md`). The memo carries the run's thermochemistry
    conditions (`rqm/thermo.md`) and feeds the Gibbs metric; the other
    metrics ignore it.
  - Signatures including undefined metrics are mutually equal in those
    components.
- `parse_uniq_mode(value: &str) -> Result<UniqMode, String>` <!-- rq-07f5aebd -->
  - Parses a `--uniq` value: the literal `dup` yields `UniqMode::Dup`;
    anything else is parsed as a key list (below). The `Err` text feeds
    the CLI's usage reporting (exit 2).
- `parse_uniq_keys(list: &str) -> Result<Vec<UniqKey>, String>` <!-- rq-0f6147ac -->
  - Parses a comma list of `metric[decimals]` tokens (`el2.4,rg`);
    unknown metrics, negative or malformed decimal counts, and empty
    lists yield the `Err` text for CLI usage exit 2.

### Crate Layout <!-- rq-88cb3513 -->

- Metrics and uniqueness live in `src/metrics.rs`; the library root
  re-exports them. No new dependencies.
- `tests/metrics.rs` tests the metrics and both uniqueness modes as pure <!-- rq-d7383be9 -->
  functions, including hand-verified Rg values for small fixtures (e.g. a
  two-atom and a three-atom geometry) and the floor-rounding equality on
  negative energies.

## Out of Scope <!-- rq-3c8142e6 -->

- RMSD / structure-alignment uniqueness (ArbAlign replacement) — deferred;
  no flag for it is accepted (`rqm/filtering.md`).
- Adaptive-precision sampling (`-sample` in JKQC).
- Storing any metric in a record — all are derived at request time.

## Gherkin Scenarios <!-- rq-2a53d618 -->


```gherkin
Feature: Geometric metrics and property uniqueness

  # --- Metrics ---

  @rq-a38b5428
  Scenario: Rg of a two-atom system matches the hand-verified value
    Given a record with a geometry of one H and one Cl 1.2746 Å apart along an axis
    When metric_radius_of_gyration is called
    Then it returns Some of the mass-weighted hand-verified Rg within 1e-9

  @rq-d59cd7df
  Scenario: Mass-weighted Rg differs from unweighted for mixed masses
    Given a record whose geometry has two H and one O in a known asymmetric layout
    When metric_radius_of_gyration is called
    Then the result differs from the equal-atom-weight Rg of the same coordinates

  @rq-ea25c7ce
  Scenario: Total mass sums atomic weights
    Given a record with a geometry of two H and one O
    When metric_total_mass is called
    Then it returns Some of 2·m(H) + m(O) within 1e-12

  @rq-5f4593c6
  Scenario: Unknown element leaves mass-based metrics undefined
    Given a record whose geometry holds the symbol "Xx"
    When metric_total_mass and metric_radius_of_gyration are called
    Then both return None

  @rq-fe2a60bf
  Scenario: Missing geometry yields no metrics
    Given a record with no geometry
    When metric_total_mass and metric_radius_of_gyration are called
    Then both return None

  @rq-80e048db
  Scenario: Dipole magnitude from the stored vector
    Given a record with dipole_moment [0.0, 3.0, 4.0]
    When metric_dipole_magnitude is called
    Then it returns Some(5.0)

  # --- dup mode ---

  @rq-23365bec
  Scenario: Duplicate names collapse to the first occurrence
    Given records named "a", "b", "a" in that order
    When dedup_names is called
    Then the result is "a" then "b", the first "a" kept

  @rq-04bf58f3
  Scenario: Nameless records are never duplicates
    Given two records with no name
    When dedup_names is called
    Then both records are kept

  # --- keys mode ---

  @rq-74c176bd
  Scenario: Energies within the rounding agree and collapse
    Given two ok records with energies -38.5123 and -38.5124 and the key El
    When unique_by_properties is called
    Then only the first record is kept

  @rq-d54c69c6
  Scenario: Energies one rounding step apart both survive
    Given two ok records with energies -38.5123 and -38.5099 and the key El
    When unique_by_properties is called
    Then both records are kept

  @rq-9ea7dfb5
  Scenario: Rounding is floor-based for negative values
    Given two ok records with energies -38.5124 and -38.5131 and the key El
    When unique_by_properties is called
    Then both records are kept (floor distinguishes them at 3 decimals)

  @rq-627522b6
  Scenario: A multi-key signature requires agreement on every component
    Given two ok records with equal energies but Rg differing in the second decimal
    When unique_by_properties is called with the keys El and Rg
    Then both records are kept

  @rq-ca0185f6
  Scenario: Undefined metrics are mutually equal
    Given two ok records with identical energies and no geometry
    When unique_by_properties is called with the keys El and Rg
    Then only the first record is kept

  @rq-03784756
  Scenario: Identical keys collapse, differing decimals do not
    When the key list "el2,el,el2" is parsed with parse_uniq_keys
    Then the result is two keys: El at 2 decimals and El at the default 3

  @rq-c74aa6d8
  Scenario: Custom decimals loosen the signature
    Given two ok records with energies -38.5119 and -38.5121
    When unique_by_properties is called with the key El at its default 3 decimals
    Then both records are kept
    When unique_by_properties is called with the key El at 2 decimals
    Then only the first record is kept

  @rq-46ff4c56
  Scenario: Fractional decimal counts are accepted
    Given two ok records with energies -38.5100 and -38.5101
    When unique_by_properties is called with the key El at 2 decimals
    Then both records are kept
    When unique_by_properties is called with the key El at 2.4 decimals
    Then only the first record is kept

  @rq-ebb500d9
  Scenario: A negative decimal count is rejected
    When parse_uniq_keys is called with "el-1"
    Then it returns Err

  @rq-8237e2b7
  Scenario: Gibbs values within the rounding agree and collapse
    Given two ok records with gibbs values -100.0012 and -100.0013 Eh and the key G
    When unique_by_properties is called with default conditions
    Then only the first record is kept

  @rq-ad851478
  Scenario: The Gibbs metric follows the run's conditions
    Given two ok records whose gibbs values agree at 3 decimals at 298.15 K but disagree at 273.15 K
    When unique_by_properties is called with the key G at 273.15 K
    Then both records are kept, unlike the 298.15 K run

  @rq-96a03a44
  Scenario: Undefined Gibbs metrics are mutually equal
    Given two ok records with no electronic energy and the key G
    When unique_by_properties is called with default conditions
    Then only the first record is kept
```
