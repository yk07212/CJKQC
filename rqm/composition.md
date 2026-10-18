# Feature: Composition Resolution from JKQC Names <!-- rq-1308259f -->

A record's `name` — the stem of its source file name — often follows the JKQC
cluster-naming convention: a composition part built from count-and-component
pairs, optionally followed by a hyphen and a free-form label. `2am3sa-1conf`
means two ammonia plus three sulfuric acid with the conformer label `1conf`;
`1am1p` (one ammonia plus one proton) is ammonium. Composition resolution
reads that convention out of the name and stores the result as an explicit
`components` field on the record (defined in `rqm/record.md`), so downstream
logic never re-parses names (`rqm/ARCHITECTURE.md`, hybrid model).

Component codes are arbitrary letter tokens with significant case. CJKQC
holds no table of codes: a code is chemistry to the user, and the
code-to-substance mapping is the concern of later features (monomer
references, `rqm/ARCHITECTURE.md`). Resolution therefore only ever produces
counts per code. Charge and multiplicity never come from the name: they
arrive from the parser or the record itself.

## Filename Grammar <!-- rq-8e661b0d -->

A name resolves when it matches this shape in full:

```text
name       := composition ( "-" label )?
composition:= ( count code )+        (at least one pair)
count      := digit+                  (ASCII digits, value 1 to 4 294 967 295)
code       := letter+                 (ASCII letters, either case)
label      := any text                (never parsed, never stored)
```

- The name splits at its **first** hyphen: everything before it must match
  the composition part; everything after it (hyphens included) is the label
  and is discarded — it survives only inside the record's `name`.
- Counts are decimal integers; leading zeros are permitted (`02am` is two of
  `am`). A count of zero, or a digit string whose value exceeds `u32::MAX`,
  makes the whole name unresolvable.
- Codes are matched case-sensitively and case is significant: `1am1sa` and
  `1Am1sa` are different compositions (`am` vs `Am`). A digit inside a
  code, a non-ASCII character, or any other symbol makes the name
  unresolvable.
- Repeated codes across pairs sum — case-sensitively: `1am1am` resolves to
  `am` = 2, and `1am1Am` resolves to `am` = 1 and `Am` = 1.
- Names that do not match — chemical formulas (`h2o`), descriptive stems
  (`movie`, `trimer-truncated`, `water-energy`), implicit counts (`am2sa`),
  digit-only (`123`), an empty composition part (`-1conf`)
  — resolve to nothing. This is never an error: the record simply carries no
  `components`.

Examples over the fixture corpus: `1dma` → {dma: 1}; `2cha` (from
`2cha-020_1_0-str3`) → {cha: 2}; `1pinic1sa` (from `1pinic1sa-110_1_9-str9`)
→ {pinic: 1, sa: 1}; `h2o` and `movie` resolve to nothing.

## Resolution Model <!-- rq-bb72c241 -->

Three sources can supply a composition: CLI flags, per-record fields, and the
filename, in descending priority (`rqm/ARCHITECTURE.md`). CLI flags are not
part of the current command surface (`rqm/collection.md`, Out of Scope), so
resolution applies one rule: a composition already present on the record is
kept; otherwise the record's `name` is parsed and, when it resolves, stored.
Resolution runs during collection (build mode, `rqm/collection.md`) over every
parsed record — a failed or truncated job still has a known composition.
Merging never re-resolves: merged records are copied verbatim.

## Feature API <!-- rq-fa218770 -->

### Types <!-- rq-f00ebeae -->

- `Components` — the record field's type: a `BTreeMap<String, u32>` mapping <!-- rq-ffc1bde0 -->
  component code to count. Iteration order is alphabetical, which makes the
  JSON deterministic. Invariants (enforced by `Record::validate`, see
  `rqm/record.md`): the map is non-empty, every code is one or more ASCII
  letters of either case, every count is at least 1.

### Functions <!-- rq-26ba140d -->

- `parse_jkqc_name(name: &str) -> Option<Components>` <!-- rq-eabf1669 -->
  - Returns the composition of a name following the grammar above, or `None`
    when the name does not match it. Never panics, never errors: every input
    is a name and `None` is a legitimate answer.
  - The label after the first hyphen is ignored entirely.
  - Repeated codes sum.

- `resolve_components(record: &mut Record)` <!-- rq-230f406d -->
  - When `record.components` is absent and `record.name` is present and
    `parse_jkqc_name` returns a composition, sets `record.components`.
  - A `components` value already on the record is never replaced or merged.
  - Absent names and unresolvable names leave the record unchanged.
  - Runs on records of any status.

### Crate Layout <!-- rq-fe2700dc -->

- The grammar and resolution live in `src/composition.rs`; the library root
  re-exports `parse_jkqc_name` and `resolve_components` (`Components` is
  re-exported with the record types, `rqm/record.md`).
- No new dependencies: `std` and the existing `serde`/`serde_json` suffice.

## Out of Scope <!-- rq-f94ef108 -->

- CLI flags that override or supply composition, charge, or multiplicity —
  the architecture's top-priority source; they arrive with a later feature.
- Any code-to-substance (chemistry) vocabulary or validation, and any
  derivation of charge or multiplicity from component codes.
- Storing the conformer/run label as its own field; the label stays inside
  `name` only.
- Re-resolving compositions of records loaded from databases (merge is
  verbatim, `rqm/collection.md`).
- Case-insensitive or forgiving name matching.

## Gherkin Scenarios <!-- rq-19b11795 -->

```gherkin
Feature: Composition resolution from JKQC names

  # --- parse_jkqc_name ---

  @rq-3a95d0f8
  Scenario: A two-component name resolves
    When parse_jkqc_name("2am3sa") is called
    Then it returns Some with am = 2 and sa = 3

  @rq-522c8a7d
  Scenario: A conformer label after a hyphen is ignored
    When parse_jkqc_name("2am3sa-1conf") is called
    Then it returns Some with am = 2 and sa = 3

  @rq-18ba8ffb
  Scenario: A label containing hyphens and underscores is ignored
    When parse_jkqc_name("1pinic1sa-110_1_9-str9") is called
    Then it returns Some with pinic = 1 and sa = 1

  @rq-306e6285
  Scenario: A proton adduct resolves
    When parse_jkqc_name("1am1p") is called
    Then it returns Some with am = 1 and p = 1

  @rq-cae3826b
  Scenario: A single-component name resolves
    When parse_jkqc_name("1dma") is called
    Then it returns Some with dma = 1

  @rq-fdd7be52
  Scenario: Repeated codes sum
    When parse_jkqc_name("1am1am") is called
    Then it returns Some with am = 2

  @rq-9d155d29
  Scenario: Leading zeros in a count are accepted
    When parse_jkqc_name("02am") is called
    Then it returns Some with am = 2

  @rq-8197fd08
  Scenario: A multi-part name from the fixture corpus resolves via its stem
    When parse_jkqc_name("2cha-020_1_0-str3") is called
    Then it returns Some with cha = 2

  @rq-1adfe3e9
  Scenario: An implicit count does not resolve
    When parse_jkqc_name("am2sa") is called
    Then it returns None

  @rq-3091cb1e
  Scenario: Uppercase codes resolve and differ from lowercase ones
    When parse_jkqc_name("2Am3sa") is called
    Then it returns Some with Am = 2 and sa = 3

  @rq-5d9fe801
  Scenario: Codes differing only in case are distinct
    When parse_jkqc_name("1am1Am") is called
    Then it returns Some with am = 1 and Am = 1

  @rq-fb64102b
  Scenario: A chemical formula does not resolve
    When parse_jkqc_name("h2o") is called
    Then it returns None

  @rq-3275eeb1
  Scenario: A descriptive stem does not resolve
    When parse_jkqc_name("trimer-truncated") is called
    Then it returns None

  @rq-a1b00922
  Scenario: A digit-only name does not resolve
    When parse_jkqc_name("123") is called
    Then it returns None

  @rq-160a5957
  Scenario: A zero count makes the name unresolvable
    When parse_jkqc_name("0am1p") is called
    Then it returns None

  @rq-dcf4a545
  Scenario: A count beyond u32::MAX makes the name unresolvable
    When parse_jkqc_name("99999999999am") is called
    Then it returns None

  @rq-cb100db2
  Scenario: An empty name does not resolve
    When parse_jkqc_name("") is called
    Then it returns None

  @rq-6eeaa8ed
  Scenario: An empty composition part does not resolve
    When parse_jkqc_name("-1conf") is called
    Then it returns None

  @rq-c66abc39
  Scenario: A non-ASCII character in a code does not resolve
    When parse_jkqc_name("1ãm") is called
    Then it returns None

  # --- resolve_components ---

  @rq-a3a948bc
  Scenario: A record gains components from its resolvable name
    Given a record with name "2am3sa-1conf" and no components
    When resolve_components is called on it
    Then the record has components am = 2 and sa = 3

  @rq-9215f506
  Scenario: A record-carried composition wins over the name
    Given a record with name "2am3sa" and components w = 1
    When resolve_components is called on it
    Then the record still has components w = 1

  @rq-b992e561
  Scenario: A nameless record is left unchanged
    Given a record with no name and no components
    When resolve_components is called on it
    Then the record has no components

  @rq-ab9f329c
  Scenario: An unresolvable name leaves components absent
    Given a record with name "h2o" and no components
    When resolve_components is called on it
    Then the record has no components

  @rq-0f1c73be
  Scenario: A failed record still gains its composition
    Given a failed record with name "1dma" and no components
    When resolve_components is called on it
    Then the record has components dma = 1
```
