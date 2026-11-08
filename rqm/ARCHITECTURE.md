# Architecture <!-- rq-d993628d -->

## Purpose <!-- rq-958794a9 -->

CJKQC (Complied JKQC) is a standalone tool for collecting, organizing, filtering, and
post-processing quantum-chemistry (QC) output for molecular clusters. It is a
spiritual successor to JKQC from the JKCS2.1 framework: the same mission, in
Rust, with an internal design chosen on its own merits rather than inheriting
the original's pandas/pickle machinery.

The job, as inherited from JKQC: turn folders of QC output files into a compact
database of structures and properties, then support everything researchers do
with that database — printing and sorting properties, filtering (uniqueness,
frequency and energy cutoffs, reacted-structure detection), and
thermochemical post-processing (formation/binding energies and free energies
with BSSE, low-frequency treatment, anharmonicity and temperature corrections)
— plus export to xyz, movie, and spreadsheet formats.

**Scale and audience.** This is a broad-community tool, not an internal group
script. It must be easy to install (single Rust binary, minimal dependencies),
robust against the messy realities of real QC outputs (failed runs, truncated
files, format drift between program versions), and usable and documented by
people who have never seen JKCS. Typical users are computational chemists
batch-processing hundreds to tens of thousands of cluster structures.

**Human-readable data is a first-class requirement.** The original tool's
pickle-based database hindered adoption because users could not open it; the
successor's databases are JSON Lines files that anyone can inspect with a text
editor, `jq`, or pandas.

## Scope and Key Features <!-- rq-814a4e22 -->

All four capability pillars below are v1 commitments, mirroring JKQC's feature
surface:

- **Collection (parse → database).** Read outputs from Gaussian, ORCA, XTB,
  MRCC, and plain xyz files — the parser is chosen by inspecting the file's
  leading lines, never by its extension. Parsing extracts only the primary
  observables (electronic energy, vibrational frequencies, gradient,
  geometry, charge, multiplicity, …) and appends to a persistent database.
  Failed or truncated jobs are recorded as failed records, never crashes.
- **Manipulation and filtering.** Merge databases; extract subsets by
  composition; sort by property; print selected properties; remove duplicate
  structures (uniqueness/RMSD-based); apply cutoffs (imaginary frequencies,
  energy windows, radius of gyration); detect reacted/exploded structures.
- **Thermochemical post-processing.** All thermochemistry — ZPE, enthalpy,
  entropy, free energy at any temperature — is computed on demand from the
  stored observables. On top of that: formation/binding energies of clusters
  from monomer reference databases, counterpoise/BSSE corrections,
  treatment of low vibrational frequencies, anharmonicity corrections,
  temperature dependence, and unit conversion.
- **Export and reporting.** xyz files, `movie.xyz`, spreadsheet export, and
  plain-text property printing.

**Composition resolution (hybrid model).** JKQC-style filenames (e.g.
`2am3sa-1conf` = 2 ammonia + 3 sulfuric acid + label; `1am1p` = ammonium)
are parsed when present; explicit CLI flags and record-level fields can
override or supply what filenames cannot express. Whatever the source, the
resolved composition, charge, and multiplicity are always stored as explicit
fields in each database record, so downstream logic never re-parses names.

### Non-Goals <!-- rq-5c88d514 -->

Deliberately out of scope — these belong to other tools in the JKCS family or
are not attempted at all:

- **JKCS sampling core** — ABCluster-based global optimization and structure
  generation (JKCS0–2).
- **JKML** — ML potentials, QML/SchNetPack training workflows.
- **JKMD** — molecular dynamics, umbrella sampling, pulling simulations.
- **Cluster submission** — SLURM/GREASY job orchestration, remote execution.
- **Running QC calculations** — the tool consumes QC outputs; it never
  launches or drives a QC package itself.

## Architectural Decisions <!-- rq-3299396a -->

- **Language.** Rust (edition 2021), a single crate producing a single
  binary. No Python runtime, no FFI.
- **CLI.** One JKQC-style command with many flags
  (`cjkqc *.log -b -el …`), chosen for familiarity to JKQC users.
  Exact flags and semantics are per-feature decisions.
- **Database format.** JSON Lines (`.jsonl`), one record per structure, via
  `serde`/`serde_json`. The database is the interchange format between
  invocations, and is human-inspectable by design. Large per-structure data
  (gradients, frequency lists) are stored as JSON arrays within the record.
- **Data model — store observables, derive everything else.** Records hold
  only the primary observables extracted from QC outputs; every derived
  quantity (ZPE, enthalpy, entropy, free energy, formation/binding
  energies, all corrections, radius of gyration, …) is computed at request
  time from those observables and never stored. Program-reported
  thermochemistry in the output files is therefore not kept — recomputation
  from the electronic energy and vibrational frequencies is the single
  source of truth. Changing temperature or a correction scheme never
  invalidates a database, and records stay lean.
- **Concurrency.** Build-mode collection parses inputs across worker
  threads behind the CLI's `--parallel <N>` flag (`rqm/collection.md`),
  with 1 — fully sequential — as the default; no other stage is parallel.
  Internal layouts avoid shared mutable state and prefer
  structures-of-arrays, and the parse result of an input never depends on
  which worker reads it: records and errors are ordered by argument
  position, so a parallel run produces the same database as a sequential
  one, byte for byte.
- **Dependencies.** Keep the tree minimal: `clap` (CLI), `serde` +
  `serde_json` (records), and small helpers where hand-rolling would cost
  more than it saves. No heavyweight dataframe dependency (polars/arrow);
  the record schema and query operations stay small enough not to need one.
- **Numerics.** Energies are held internally in hartree and converted only
  at the reporting boundary; unit conversion is a presentation concern.
- **Error handling.** No panics on bad input. Malformed or failed outputs
  become records with an error status; the run continues and reports what
  succeeded and what failed.

## Extensibility <!-- rq-7147968f -->

Trait + registry, built on composition:

- **Format detection.** Parsers are selected by content, not extension: a
  sniffer inspects the leading lines of each file (program banners and
  signatures) and the registry maps detected signatures to parsers. Plain
  xyz files carry no banner; they are recognized structurally (atom count,
  comment line, element rows).
- Every QC-package reader implements a shared **parser trait** that turns
  file content into primary observables. Adding a package means
  implementing the trait, registering its signature(s), and adding
  conformance fixtures — no changes to the database or CLI layers.
- Parsers compose from shared extractors (e.g. a common xyz geometry reader
  reused by the Gaussian and ORCA parsers) rather than duplicating logic.
- **Derived properties** are pure functions over stored observables,
  registered in a derivation registry (free energy at a temperature,
  formation energies, corrections, geometric metrics, …). Adding one means
  implementing the function and its reference tests — no record-schema
  change. The record schema grows only with new observables; existing
  records tolerate missing fields (serde defaults), so old databases remain
  loadable.

## Testing Strategy <!-- rq-c04bd98a -->

Conformance harness:

- A checked-in **fixture corpus** of real, small QC outputs per package
  and per situation (successful run, failed run, truncated file) lives in
  the repo under `tests/fixtures`.
- Every parser must pass the same standardized suite against that corpus:
  required fields extracted to expected values (floats within tolerance),
  correct status handling for failed/truncated files, and correct routing —
  every fixture must be assigned to the right parser from its content
  alone. A new parser is "done" when it passes the conformance suite — the
  harness is the extension path, not an afterthought.
- Derived properties (free energies, formation energies, corrections, unit
  conversion, uniqueness metrics) are tested as pure functions over
  observables against hand-verified reference values, including at
  temperatures and correction schemes different from those used in the
  source output files.
- CI runs the full suite via `cargo test`; coverage is tracked.

## Open Questions <!-- rq-bf3e0e91 -->

- Distribution: how CJKQC ships (crates.io, conda, static binary) and the
  exact executable spelling (`cjkqc`).
- The JKQC property vocabulary: which flags carry over 1:1, which are
  renamed, which dropped. Decided feature-by-feature in `/rr-plan`, but the
  overall flag surface shapes the CLI's feel.
- Geometric uniqueness: which RMSD/alignment approach replaces ArbAlign,
  and whether it is a dependency or hand-rolled.
- Record schema versioning: how a `.jsonl` record declares its schema
  version so the format can evolve without breaking older databases.
