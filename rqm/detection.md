# Feature: Reacted / Exploded Structure Detection <!-- rq-4555c02d -->

Within one composition group, most conformers of a cluster share the same
set of molecular fragments; a conformer whose optimization reacted or
exploded shows a different fragment fingerprint. Detection rebuilds each
structure's fragment fingerprint from its geometry, takes the most frequent
fingerprint of the group as the consensus, and drops the deviants
(`--reacted`) or keeps exactly the deviants (`--reacted2`). This mirrors
the original JKQC procedure and needs no code-to-substance chemistry table
(`rqm/ARCHITECTURE.md`): the expected pattern is the group's own majority,
not a lookup.

## Bonded Graph <!-- rq-327095a8 -->

Two atoms are bonded when their distance (Å, from
`geometry.coordinates`) is below the pair-specific cutoff:

| Pair | Cutoff (Å) |
|---|---|
| C–N (either order) | < 1.75 |
| N–N | < 1.50 |
| S–O, O–S | < 1.90 |
| O–N, N–O | < 1.90 |
| H–H | < 0.80 |
| any pair involving H, other than H–H | never bonded |
| every other heavy–heavy pair | < 2.00 |

The cutoffs are fixed constants mirroring the original implementation; the
2.00 Å default is its `bonddistancethreshold` default. Hydrogen is bonded
only to another hydrogen below 0.80 Å — a faithful mirror of the original,
so a fingerprint is the heavy-atom skeleton plus any tight H₂ units, and a
structure shows as reacted or exploded when that skeleton differs from the
group's majority.

## Fragment Signature <!-- rq-7e73da19 -->

- Build the bonded graph of the geometry and take its connected
  components.
- Each component's **fragment formula** is its element symbols sorted
  alphabetically and concatenated (`"HNO"`, not a molecular formula).
- A record's **signature** is its fragment formulas sorted
  alphabetically, joined with `+` (a bonded carbon–nitrogen pair is the
  fragment formula `"CN"`; a lone nitrogen and a lone hydrogen are the
  fragments `"H"` and `"N"`, giving the signature `"H+N"`).
- A record without a geometry has signature `(none)`; it participates in
  the consensus like any other signature (it survives only when `(none)`
  is itself the most frequent signature of its group).

## Consensus Rule <!-- rq-c6746a5e -->

- Group records exactly as filtering groups them
  (`rqm/filtering.md`): by resolved composition, unnamed records as one
  separate group, or one global group under `--noname`.
- Within a group, the consensus is the most frequent signature. Ties
  resolve deterministically: highest count wins, then the signature seen
  earliest in record order (the original's set-iteration tie-break is
  replaced by this deterministic one).
- `Consensus` keeps records whose signature equals the consensus;
  `Deviants` keeps the complement. Empty groups contribute nothing.

## Feature API <!-- rq-b07a1042 -->

### Types <!-- rq-5c0b0b75 -->

- `ReactedMode` — `Consensus` (`--reacted`, keep the majority pattern) or <!-- rq-ef146284 -->
  `Deviants` (`--reacted2`, keep only the deviants).

### Functions <!-- rq-9e36a525 -->

- `fragment_signature(record: &Record) -> Option<String>` <!-- rq-f0b153d7 -->
  - The joined sorted fragment formulas of the geometry's bonded-graph
    components; `None` when the record has no geometry.
  - Uses the fixed bond table above via `are_bonded(dist: f64, a: &str,
    b: &str) -> bool`, which is public for tests.
- `filter_reacted(groups: &[(Vec<Record>)], mode: ReactedMode) -> Vec<Record>` <!-- rq-a921051d -->
  - Applies the consensus rule within each group and returns the kept
    records in input order (groups may be the caller's filtered,
    status-`ok` slice; grouping itself is `rqm/filtering.md`).

### Crate Layout <!-- rq-83c62ce0 -->

- Detection lives in `src/reacted.rs`; the library root re-exports
  `fragment_signature`, `are_bonded`, `ReactedMode`, `filter_reacted`.
  No new dependencies.
- `tests/reacted.rs` tests the bond table edge cases (exactly-at-cutoff <!-- rq-fe68801f -->
  distances are unbonded, H quirks), fragment decomposition, and the
  consensus rule with ties and deviant groups, on hand-built geometries.

## Out of Scope <!-- rq-89c26378 -->

- A user-tunable bond-distance threshold (fixed constants in v1).
- Per-molecule chemistry: expected monomer formulas, bond orders, or a
  code-to-substance table (`rqm/composition.md`).
- Comparing an initial and final geometry (records hold one structure).
- Any bonded-graph use in cutoffs (the JKQC `bonded` cutoff property is
  not adopted; `rqm/filtering.md`).

## Gherkin Scenarios <!-- rq-1b5c13f5 -->

```gherkin
Feature: Reacted and exploded structure detection

  @rq-00285b24
  Scenario: Hydrogen bonded only under 0.80 Å to another H
    Given two H atoms 0.75 Å apart and one O atom 0.97 Å from one H
    When fragment_signature is computed
    Then the signature is "HH+O" (the O–H pair is never bonded)

  @rq-239fd4b8
  Scenario: Hydrogens never join the heavy skeleton
    Given a water geometry: one O atom and two H atoms 0.97 Å from the O and 1.53 Å apart
    When fragment_signature is computed
    Then the signature is "H+H+O"

  @rq-dfb86bb9
  Scenario: A reacted heavy skeleton deviates from the consensus
    Given a group whose majority records show two N atoms 3.0 Å apart (signature "N+N")
    And one record of the same composition shows the two N atoms 1.4 Å apart (signature "NN")
    When filter_reacted runs in Consensus mode
    Then the deviant record is dropped and the majority records survive

  @rq-516d24b1
  Scenario: Deviants mode keeps exactly the deviants
    Given the same group
    When filter_reacted runs in Deviants mode
    Then only the deviant record is returned

  @rq-e2e0998d
  Scenario: Distance exactly at a cutoff is unbonded
    When are_bonded is called with distance 1.75 and pair C,N
    Then it returns false
    When are_bonded is called with distance 1.749 and pair C,N
    Then it returns true

  @rq-f2821d27
  Scenario: The 2.00 Å default applies to unlisted heavy pairs
    When are_bonded is called with distance 1.95 and pair C,C
    Then it returns true
    When are_bonded is called with distance 2.05 and pair C,C
    Then it returns false

  @rq-775c5278
  Scenario: Groups vote independently
    Given group A whose consensus is "HHO+HHO" containing one deviant
    And group B whose consensus is "N+HHH" with no deviants
    When filter_reacted runs in Consensus mode over both groups
    Then group A loses its deviant and group B is untouched

  @rq-bfd2765b
  Scenario: Tie in frequency breaks by earliest occurrence
    Given a group whose signatures in record order are Y, X, Y, X
    When the consensus is chosen
    Then Y is the consensus (earliest among the two equally frequent signatures)

  @rq-cc3c275c
  Scenario: A geometryless record survives only with a none-consensus
    Given a group of 3 records with real geometries sharing a signature and 4 records with no geometry
    When filter_reacted runs in Consensus mode
    Then the four none-signature records are kept and the geometry records are dropped

  @rq-6203c24a
  Scenario: Detection never panics on hostile geometries
    Given a record whose coordinates contain duplicate positions and zero-distance pairs
    When fragment_signature is computed
    Then it returns a signature without failing
```
