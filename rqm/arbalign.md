# Feature: ArbAlign Structure-Alignment Filtering <!-- rq-f9909f23 -->

`--arbalign <threshold>` is a manipulation-pipeline stage that removes
geometrically redundant structures. It implements the behaviour of
ArbAlignCPP's `--SORT` mode: every participating structure is first moved
to its principal axes (the `--MPA` preprocessing of ArbAlignCPP, always on
for this stage), then pairs of structures are compared with an
atom-assignment search followed by Kabsch RMSD. Atom order, overall
rotation, and axis sign/permutation therefore do not affect the comparison.

Records are considered in the pipeline's current order, which is the order
produced by `--sort` (`rqm/filtering.md` Pipeline Order). A record is
eliminated when its RMSD to an earlier surviving record of the same
composition is strictly below `threshold`, in ångström. Elimination is
pairwise, not a clustering: a record already removed by an earlier sweep is
never used to remove another record, so the first record in the current
order always wins.

`--align-within <keys>` optionally restricts each sweep to records that
share a uniqueness signature (`rqm/uniqueness.md`); it is only valid with
`--arbalign` and is described under Within-Subgroup Comparison.

## Threshold <!-- rq-2b831cd3 -->

- `--arbalign` takes exactly one value, the RMSD threshold in ångström.
- The value must parse as a finite `f64` that is greater than or equal to
  zero. A malformed value, NaN, either infinity, or a negative value is a
  usage error (exit 2).
- The comparison is strict: a pair is eliminated exactly when
  `rmsd < threshold`. A threshold of zero is valid and removes nothing,
  because an RMSD is never negative.

## Pipeline Integration <!-- rq-0ad6f888 -->

- `--arbalign` is a manipulation flag for the visible-outcome rule
  (`rqm/filtering.md`): a run using it must also write `--out` or request a
  print column.
- The stage runs after `--sort`, `--reverse`, and `--select`, and before
  `--glob` and `--shuffle`. `--uniq` and `--select` run earlier, so
  arbalign only sees their survivors; `--glob` operates on arbalign
  survivors.
- When `--sort` is absent, the presence of `--arbalign` triggers the
  implicit ascending `el` sort, exactly as `--uniq` in keys mode and
  `--select` do. `--reverse` orients that implicit sort, so with
  `--reverse` the first (surviving) record is the highest-energy one.
- The stage is available in every manipulation run regardless of whether
  records were collected from program outputs or merged from databases.

## Grouping and Comparability <!-- rq-9af43b1b -->

- Arbalign always partitions records by exact `components`
  (`rqm/composition.md`). Records without `components` form one separate
  unnamed group. `--noname` does not make records of different composition
  comparable for this stage.
- Within a group, a pair is compared only when both records are
  **preparable** and their geometries have the same atom count and the same
  multiset of element symbols. Other pairs are skipped without eliminating
  either record. Charge, multiplicity, and other non-geometric fields do
  not affect comparability.
- A record is preparable when it has a geometry with at least one atom,
  every coordinate is finite, and every geometry symbol is present in the
  atomic-mass table (`rqm/uniqueness.md` Mass Table). A record without a
  geometry, with a non-finite coordinate, or with an unknown symbol is left
  untouched: it never eliminates another record and is never eliminated.
  The number of such records is reported on stderr as
  `arbalign: kept <k> records without usable geometry` when non-zero.
- Records that are preparable but lack a comparable partner are likewise
  kept untouched.
- When `--align-within` is present, a pair is additionally comparable only
  when both records belong to the same alignment subgroup (below). Records
  in different subgroups are never compared, even when their RMSD is below
  the threshold.

## Within-Subgroup Comparison (`--align-within`) <!-- rq-b73bf2f4 -->

`--align-within <keys>` scopes the arbalign elimination sweep to subgroups
of records whose uniqueness signatures agree. It never filters records on
its own: it changes which pairs the `--arbalign` stage is allowed to
compare.

### Flag Value <!-- rq-089b9919 -->

- `--align-within` takes exactly one value, using the same syntax and
  parser as `--uniq` (`rqm/uniqueness.md`): the literal `dup`, or a comma
  list of `metric[decimals]` tokens such as `el8` or `el2.4,rg`.
- The metrics, default decimal counts, fractional decimal counts, floor
  rounding, duplicate-entry collapsing, and malformed-value handling are
  exactly those of `--uniq`. Unknown metrics, empty lists, and negative or
  malformed decimal counts are usage errors (exit 2).
- `--align-within` and `--uniq` are independent filters with independent
  values. When both are present, `--uniq` runs earlier in the pipeline
  (`rqm/filtering.md` Pipeline Order), so alignment subgroups are formed
  over the uniqueness survivors.
- `--align-within` requires `--arbalign`. Supplying `--align-within`
  without `--arbalign` is a usage error (exit 2), whether or not other
  manipulation flags are present.

### Subgroups <!-- rq-34fa9d89 -->

Alignment subgroups are formed inside each exact-`components` composition
group. `--noname` does not change this: records of different composition
are never compared, exactly as for `--arbalign` alone.

- **`keys` mode.** Two records belong to the same subgroup exactly when
  their rounded signature tuples agree on every selected key, using the
  uniqueness signature (`rqm/uniqueness.md` Uniqueness Model): values agree
  at `d` decimals when `floor(a · 10^d) == floor(b · 10^d)`, and undefined
  components are mutually equal.
- **`dup` mode.** Two records belong to the same subgroup exactly when both
  have the same present `name`. A record without a `name` forms its own
  singleton subgroup and is never compared with another record for this
  reason.
- The subgroup assignment and the `--uniq` first-wins masks derive from the
  same shared signature routine, so a metric added to `--uniq` is
  automatically available to `--align-within`.

### Elimination <!-- rq-e5ed6309 -->

- Each composition group is partitioned into subgroups, and the existing
  pairwise sweep runs independently within each subgroup. A later record
  is eliminated when its RMSD to an earlier survivor of the same subgroup
  is strictly below `threshold`.
- The first record of every subgroup in the current order survives.
  Elimination never crosses subgroup boundaries, so a record close to a
  member of another subgroup is kept.
- Survivors remain in their original relative order, and the
  `arbalign: kept <k> records without usable geometry` count is unchanged:
  non-preparable records survive regardless of subgroup.

## Algorithm <!-- rq-6463cb8b -->

The stage reproduces the ArbAlign algorithm used by ArbAlignCPP's `--SORT`
mode with `--MPA` enabled. The implementation is in-crate and introduces no
linear-algebra or assignment dependency.

### Principal-Axis Preprocessing <!-- rq-8ac48ef4 -->

- Translate the geometry so its center of mass is at the origin, using the
  standard atomic masses of `rqm/uniqueness.md`.
- Build the inertia tensor with diagonal entries
  `Σ m_i (y_i² + z_i²)`, `Σ m_i (x_i² + z_i²)`, `Σ m_i (x_i² + y_i²)` and
  off-diagonal entries `−Σ m_i x_i y_i`, `−Σ m_i x_i z_i`, `−Σ m_i y_i z_i`;
  the tensor is symmetric.
- Diagonalize the tensor with a symmetric 3×3 eigensolver. Express the
  coordinates in the eigenvector basis, ordered by ascending eigenvalue.
  Eigenvector signs are fixed deterministically so that repeated runs on
  the same geometry produce bit-identical prepared coordinates.

### Pairwise RMSD <!-- rq-fbc3a0c8 -->

For two prepared and comparable structures `a` and `b`:

- The RMSD is the minimum root-mean-square distance after optimally
  rotating one structure onto the other. Both structures are centered on
  their unweighted geometric centroid before rotation, matching
  ArbAlignCPP's Kabsch convention. The rotation is a proper rotation; the
  determinant correction of the Kabsch SVD is applied exactly as in
  ArbAlignCPP.
- The baseline RMSD uses the original atom order after principal-axis
  preprocessing. If it is below `0.001 Å`, that baseline is the result and
  the assignment search is skipped.
- Otherwise both structures are sorted by element symbol and then by their
  principal-axis `x`, `y`, `z` coordinates, and atoms are grouped by
  element symbol.
- The search covers the 48 axis operations of ArbAlignCPP: six axis
  permutations times eight sign combinations. For each operation:
  - the alphabetically first element group is pre-assigned by solving a
    minimum-total-distance linear assignment problem on its
    centroid-centered coordinates;
  - the operation and that pre-assignment are applied to the other
    structure's sorted coordinates;
  - then every element group, including the first group, is assigned by
    solving a minimum-total-distance linear assignment problem on the
    current transformed coordinates, and the full-structure Kabsch RMSD is
    evaluated after each group assignment; the minimum over all operations
    and group stages is kept.
- If the baseline RMSD is smaller than the best assignment result, the
  baseline is the result, mirroring ArbAlignCPP's final floor.
- The linear-assignment solver is an exact minimum-cost perfect matching
  for the square element-group matrices that arise here. Ties are broken
  deterministically.
- The RMSD is invariant to atom ordering, translation, rotation, and the
  axis sign/permutation ambiguities of principal-axis preprocessing.
  Reflections are admitted by the 48-operation search, matching
  ArbAlignCPP.

## Performance Constraints <!-- rq-975c0536 -->

The pairwise sweep is the performance-critical part of this feature and
must not regress against the C++ reference implementation.

- **Hard parity gate.** On the checked-in benchmark corpus, with the same
  thread count, always-on MPA, and the same threshold, the Rust end-to-end
  `--arbalign` command must be no slower than ArbAlignCPP `--SORT --MPA`.
  Timing is the median of at least five release-build runs. Both commands
  load and prepare the same geometry corpus, so the gate times comparable
  work. The harness also reports the Rust pairwise-sweep-only time for
  diagnosis; that diagnostic is not the gate.
- **Benchmark harness.** A reproducible harness builds or accepts an
  ArbAlignCPP binary, runs both implementations end-to-end on the same
  geometry corpus in the same order with the same threshold and thread
  count, and prints per-case and aggregate timings, the Rust sweep-only
  time, and both survivor sets. It fails when the survivor sets differ.
  The benchmark is release-only and is not part of the default `cargo test`
  run.
- **Parity oracle.** The checked-in parity corpus is 16 deterministic
  synthetic `(H2O)4(NH3)2` clusters (composition O4N2H14); the benchmark
  corpus is 64 clusters of the same composition. Both exercise three
  element groups and large hydrogen assignments. A survivor list generated
  by ArbAlignCPP makes survivor parity testable without requiring a C++
  build in CI. The Rust run must select the same survivor set, and the
  reference threshold is chosen at least `1e-3 Å` away from every pairwise
  RMSD on the corpus so the decision is not sensitive to floating-point
  noise.
- **Precomputation.** Each record is prepared at most once per run:
  principal-axis coordinates, sorted atom order, element groups, centroid,
  and radius of gyration are computed once and reused across every pair.
- **Safe pair rejection.** Two structures with different atom counts or
  element multisets are rejected before any numerical work. Because the
  optimal RMSD is bounded below by the absolute difference of the two
  unweighted radii of gyration about their geometric centroids, a pair
  whose radius-of-gyration difference is at least the threshold cannot be
  eliminated and skips the assignment search.
- **Allocation discipline.** Inner loops reuse per-worker scratch buffers
  for distance matrices, assignments, and rotated coordinates. The sweep
  must not allocate per pair or per operation in the common path.
- **Parallelism.** The inner `j` sweep of each survivor is distributed over
  the run's `--parallel <N>` worker count. Worker threads are created once
  per composition group and reused across that group's survivor sweeps; the
  sweep does not spawn threads per survivor. Scope ids never cause a
  second pool: with `--align-within`, the same pool skips pairs whose
  subgroup ids differ. Worker count 1 is fully sequential and is the
  default. The result is independent of the worker count: the same records
  survive for every `N`, and the survivor order is the sequential order.

## Feature API <!-- rq-23f0a802 -->

### Types <!-- rq-452b61ff -->

- `ArbAlignStructure` — a preparable record's alignment form: element <!-- rq-7330b65b -->
  symbols, principal-axis coordinates in the original atom order, the
  sorted atom order and element groups, the unweighted centroid, and the
  unweighted radius of gyration about that centroid (used only for safe
  pair rejection). Built once per record.
- `ArbAlignReport` — the stage's diagnostics: `skipped: usize`, the number <!-- rq-76495264 -->
  of records left untouched because they lack usable geometry.

### Functions <!-- rq-09c6f34e -->

- `parse_arbalign_threshold(value: &str) -> Result<f64, String>` <!-- rq-4ddb030f -->
  - Parses the `--arbalign` value. Returns the finite, non-negative
    threshold, or the `Err` text that feeds the CLI's usage reporting
    (exit 2).
- `prepare_arbalign(record: &Record) -> Option<ArbAlignStructure>` <!-- rq-a637fc31 -->
  - Principal-axis preprocessing and grouping. Returns `None` without a
    geometry, with an empty geometry, or when a geometry symbol is outside
    the atomic-mass table.
- `arbalign_rmsd(a: &ArbAlignStructure, b: &ArbAlignStructure) -> Option<f64>` <!-- rq-bb73fa3f -->
  - The ArbAlign RMSD in ångström for a comparable pair; `None` when atom
    counts or element multisets differ.
- `arbalign_mask(structures: &[Option<ArbAlignStructure>], threshold: f64, parallel: usize, scope: Option<&[usize]>) -> (Vec<bool>, ArbAlignReport)` <!-- rq-8b4b0278 -->
  - Runs the pairwise elimination over one composition group in the given
    order. Returns a keep mask that is `true` for every preparable survivor
    and for every non-preparable record, plus the report. Within each
    subgroup, the first record in order survives; a later record is dropped
    when its RMSD to an earlier survivor of the same subgroup is strictly
    below `threshold`. `parallel` is at least one and does not affect the
    mask.
  - `scope`, when present, holds one stable subgroup id per structure;
    pairs with different ids are never compared. `None` treats the whole
    slice as one subgroup. The worker pool is created once for the slice
    either way.
- `apply_arbalign(records: &mut Vec<Record>, threshold: f64, parallel: usize, scope: Option<&[usize]>) -> ArbAlignReport` <!-- rq-1967b16e -->
  - Groups by exact `components`, prepares each group's structures once,
    calls `arbalign_mask` once per group, and retains survivors in their
    original relative order. The caller is responsible for the warning line
    from the returned report.
  - `scope`, when present, holds one stable subgroup id per record in the
    input order. Within each composition group, records with different ids
    are never compared, while the sweep still runs once per composition
    group. `None` means every record in a composition group forms one
    subgroup, which is the `--arbalign`-only behaviour.

### Pipeline Integration <!-- rq-616ceeee -->

- `FilterSpec` carries `arbalign: Option<f64>`, <!-- rq-7c5578c0 -->
  `align_within: Option<UniqMode>` (`rqm/uniqueness.md`), and
  `parallel: usize`, the latter being the run's `--parallel` worker count;
  only arbalign reads it.
- `apply_filters` applies the arbalign stage after the sort and after <!-- rq-ae71d266 -->
  `--select`, before `--glob` and `--shuffle`, triggers the implicit `el`
  sort when `--arbalign` is present without `--sort`, and returns a
  `FilterOutcome` carrying the surviving records and the
  `ArbAlignReport`. When both `arbalign` and `align_within` are present it
  derives the scope ids from the shared uniqueness subgroup routine and
  passes them to `apply_arbalign`.
- The CLI rejects `--align-within` without `--arbalign` before the
  pipeline runs, and parses the value with the shared uniqueness parser;
  both are usage errors (exit 2) with the same validation as `--uniq`.

### Crate Layout <!-- rq-6f4605b7 -->

- The algorithm, assignment solver, eigensolver, and Kabsch RMSD live in
  `src/arbalign.rs`; the subgroup signature routine and `--align-within`
  parser live with the uniqueness code in `src/metrics.rs`; the library
  root re-exports the public types and functions. `src/filter.rs` wires the
  stage into the pipeline. No new dependencies.
- `tests/arbalign.rs` tests the pure alignment functions: principal-axis <!-- rq-212d67dd -->
  preprocessing, Kabsch and assignment RMSD on hand-built geometries,
  permutation/rotation/reflection invariance, strict threshold behaviour,
  non-transitive pairwise elimination, comparability, subgroup scoping,
  scoped worker-count determinism, and the `--uniq` then `--align-within`
  interaction. `tests/metrics.rs` tests the shared subgroup routine.
  `tests/cli.rs` covers flag validation, implicit sorting, pipeline
  position, the warning line, and exit codes.
- The benchmark harness and the C++-generated parity corpus live under the
  test/benchmark tree described in Performance Constraints.

## Out of Scope <!-- rq-6ea7c8ff -->

- Mass-weighted ArbAlign (`--MWarbalign`) and other ArbAlign variants;
  `--arbalign` is the only structure-alignment flag.
- Writing aligned structures or per-pair RMSD values as output; the stage
  filters records and writes no auxiliary files.
- Using alignment as a `--uniq` mode; arbalign is a separate pipeline
  stage with its own ordering rules (`rqm/uniqueness.md`).
- Comparing different compositions: `--align-within` subdivides each
  exact-`components` group and never makes structures of different
  composition comparable, even under `--noname`.

## Gherkin Scenarios <!-- rq-062f3774 -->

```gherkin
Feature: ArbAlign structure-alignment filtering

  # --- Threshold parsing ---

  @rq-ef412fee
  Scenario: A finite non-negative threshold is accepted
    When parse_arbalign_threshold("0.38") is called
    Then it returns Ok with 0.38

  @rq-15621246
  Scenario: Zero is a valid threshold
    When parse_arbalign_threshold("0") is called
    Then it returns Ok with 0.0

  @rq-34493b93
  Scenario: A negative threshold is rejected
    When parse_arbalign_threshold("-0.1") is called
    Then it returns Err

  @rq-29d2a817
  Scenario: Non-finite and malformed thresholds are rejected
    When parse_arbalign_threshold("NaN") is called
    Then it returns Err
    When parse_arbalign_threshold("inf") is called
    Then it returns Err
    When parse_arbalign_threshold("abc") is called
    Then it returns Err

  # --- Alignment basics ---

  @rq-48a54c37
  Scenario: Identical and reordered structures have zero RMSD
    Given a prepared structure and a copy with its atoms listed in reverse order
    When arbalign_rmsd is called
    Then it returns Some of 0 within 1e-9

  @rq-9e72baf5
  Scenario: Rotated and translated structures have zero RMSD
    Given a prepared structure and a rigidly rotated and translated copy
    When arbalign_rmsd is called
    Then it returns Some of 0 within 1e-9

  @rq-f85b53e6
  Scenario: A reflected structure is matched by the symmetry search
    Given a prepared structure and its mirror image
    When arbalign_rmsd is called
    Then it returns Some of 0 within 1e-9

  @rq-79b6a9ff
  Scenario: Different geometries have a positive RMSD
    Given two prepared structures of the same composition with different bond lengths
    When arbalign_rmsd is called
    Then it returns Some of a value greater than 0

  @rq-8f0d2e24
  Scenario: Principal-axis preprocessing centers on the center of mass
    Given a geometry with two atoms of different mass
    When prepare_arbalign is called
    Then the mass-weighted sum of the prepared coordinates is zero within 1e-12

  # --- Comparability and non-preparable records ---

  @rq-95322f48
  Scenario: Different element multisets are not compared
    Given prepared structures with compositions H2O and H2S
    When arbalign_rmsd is called
    Then it returns None

  @rq-8f809f5d
  Scenario: A record without geometry is left untouched
    Given a group with one record without a geometry and two identical structures
    When apply_arbalign is called on the group
    Then the geometry-less record survives
    And the report's skipped count is 1

  @rq-71cd97d3
  Scenario: An unknown element leaves a record untouched
    Given a record whose geometry holds the symbol "Xx"
    When prepare_arbalign is called
    Then it returns None

  @rq-44f6407c
  Scenario: A non-finite coordinate leaves a record untouched
    Given a record whose geometry holds a NaN coordinate
    When prepare_arbalign is called
    Then it returns None

  # --- Elimination semantics ---

  @rq-bfbfdd2b
  Scenario: A pair strictly below the threshold eliminates the later record
    Given two identical structures at RMSD 0 and threshold 0.1
    When the sweep visits the first structure before the second
    Then only the first survives

  @rq-8a55e4b4
  Scenario: A pair exactly at the threshold is kept
    Given two structures whose RMSD is exactly 0.1 and threshold 0.1
    When the sweep runs
    Then both structures survive

  @rq-daef887e
  Scenario: Threshold zero removes nothing
    Given two identical structures and threshold 0
    When the sweep runs
    Then both structures survive

  @rq-45705445
  Scenario: The first record in order always wins
    Given three structures in order A, B, C where A is close to B and B is close to C but A is not close to C
    When the sweep runs
    Then A and C survive and B is eliminated

  @rq-1fbb1d60
  Scenario: A removed record cannot eliminate another record
    Given structures A, B, C where A removes B and B would remove C
    When the sweep runs
    Then A and C survive

  @rq-21a7b4fc
  Scenario: The survivor order is the input order
    Given survivors A, C, D among input A, B, C, D
    When the sweep runs
    Then the survivors are ordered A, C, D

  # --- Within-subgroup comparison ---

  @rq-b94b3e6b
  Scenario: The --align-within value uses the --uniq syntax
    When parse_align_within("el2.4,rg") is called
    Then it returns Ok with keys El at 2.4 decimals and Rg at 2 decimals

  @rq-0f42db73
  Scenario: dup is accepted for --align-within
    When parse_align_within("dup") is called
    Then it returns Ok with Dup mode

  @rq-2e47f269
  Scenario: Malformed --align-within values are rejected
    When parse_align_within("el-1") is called
    Then it returns Err
    When parse_align_within("") is called
    Then it returns Err
    When parse_align_within("xyz") is called
    Then it returns Err

  @rq-8ac5fbd3
  Scenario: --align-within without --arbalign is a usage error
    When CJKQC is invoked with a database, "--align-within el8 --filename" and no --out
    Then the exit code is 2

  @rq-68964eb9
  Scenario: Different energy subgroups are never compared
    Given two identical geometries whose energies differ at 8 decimals
    When CJKQC is invoked with "--align-within el8 --arbalign 1.0 --filename" and no --out
    Then both records are printed

  @rq-fb0a8b46
  Scenario: The same energy subgroup is filtered by RMSD
    Given two identical geometries whose energies agree at 8 decimals
    When CJKQC is invoked with "--align-within el8 --arbalign 0.1 --filename" and no --out
    Then only one record is printed

  @rq-236d4dc3
  Scenario: The first record of each subgroup survives
    Given subgroups A, B in group 1 and C, D in group 2 in the order A, C, B, D, where A removes B and C removes D
    When apply_arbalign is called with the subgroup scope and threshold 0.1
    Then A and C survive in the order A, C

  @rq-d0949c67
  Scenario: Every selected key must agree for a pair to be compared
    Given two identical geometries with equal energies but Rg differing in the second decimal
    When CJKQC is invoked with "--align-within el,rg --arbalign 1.0 --filename" and no --out
    Then both records are printed

  @rq-b827e2d4
  Scenario: dup mode compares only records with the same name
    Given three identical geometries named "a", "b", "a" in that order
    When CJKQC is invoked with "--align-within dup --arbalign 0.1 --filename" and no --out
    Then the first "a" and "b" are printed

  @rq-dd8d9a63
  Scenario: Nameless records are singleton subgroups in dup mode
    Given two identical nameless geometries
    When CJKQC is invoked with "--align-within dup --arbalign 0.1 --filename" and no --out
    Then both records are printed

  @rq-52a3d5ea
  Scenario: Undefined metrics are mutually equal for subgrouping
    Given two identical geometries with no electronic energy
    When CJKQC is invoked with "--align-within el8 --arbalign 0.1 --filename" and no --out
    Then only one record is printed

  @rq-d08d83a4
  Scenario: Align-within never crosses composition groups
    Given identical geometries of H2O and H2S with equal energies
    When CJKQC is invoked with "--noname --align-within el8 --arbalign 10 --filename" and no --out
    Then both records are printed

  @rq-06bb80f9
  Scenario: Align-within forms subgroups after the uniq survivors
    Given a composition group with records x(-2), y(-1.000000002), z(-1.000000001), and x(-1), all with identical geometries
    And --uniq dup keeps the first x in the sorted order and drops the second
    When CJKQC is invoked with "--uniq dup --align-within el8 --arbalign 0.1 --filename" and no --out
    Then only x and y are printed

  # --- Pipeline integration ---

  @rq-36e90144
  Scenario: Arbalign uses the sorted order to choose the representative
    Given two geometrically identical records with energies -1.0 and -2.0
    When CJKQC is invoked with "--sort el --arbalign 0.1 --filename" and no --out
    Then only the -2.0 record is printed

  @rq-27c77ee9
  Scenario: Arbalign triggers the implicit energy sort
    Given two geometrically identical records with energies -1.0 and -2.0, the -1.0 record first in file order
    When CJKQC is invoked with "--arbalign 0.1 --filename" and no --out
    Then only the -2.0 record is printed

  @rq-df401c1b
  Scenario: Arbalign runs after sort and after select
    Given a group whose sorted order is A, B, C where A removes B
    When CJKQC is invoked with "--sort el --arbalign 0.1 --select 2 --filename" and no --out
    Then only A is printed

  @rq-b8a74911
  Scenario: Arbalign removes records before glob chooses a minimum
    Given a group whose arbalign survivors have Gibbs values -1.0 and -2.0
    When CJKQC is invoked with "--arbalign 0.1 --glob --filename" and no --out
    Then only the -2.0 survivor is printed

  @rq-da57e104
  Scenario: Different composition groups are never compared
    Given a group of two identical H2O structures and a group of two identical H2S structures
    When apply_arbalign is called with threshold 1.0
    Then each group keeps exactly one structure

  @rq-dfca14c2
  Scenario: Noname does not make different compositions comparable
    Given records of H2O and H2S with differing geometries
    When CJKQC is invoked with "--noname --arbalign 10 --filename" and no --out
    Then both records are printed

  @rq-4ebb0210
  Scenario: The warning counts non-preparable records once
    Given a group with two records lacking geometry
    When apply_arbalign is called
    Then the report's skipped count is 2
    And both records survive

  @rq-8ab03978
  Scenario: Arbalign requires a visible outcome
    When CJKQC is invoked with a database, "--arbalign 0.1" and no --out and no print flag
    Then the exit code is 2

  @rq-9cfce088
  Scenario: An invalid CLI threshold is a usage error
    When CJKQC is invoked with a database, "--arbalign -0.1 --filename" and no --out
    Then the exit code is 2

  # --- Determinism ---

  @rq-d72b5bc0
  Scenario: Worker count does not change survivors
    Given a corpus of mixed structures and threshold 0.2
    When arbalign_mask is called with parallel 1
    And arbalign_mask is called with parallel 4
    Then both masks are identical

  @rq-41077fb2
  Scenario: Worker count does not change scoped survivors
    Given a corpus of mixed structures with two subgroup ids and threshold 0.2
    When arbalign_mask is called with that scope and parallel 1
    And arbalign_mask is called with that scope and parallel 4
    Then both masks are identical

  @rq-72a1a9da
  Scenario: Repeated runs produce identical prepared coordinates
    Given the same geometry
    When prepare_arbalign is called twice
    Then both prepared coordinate arrays are bit-identical
```
