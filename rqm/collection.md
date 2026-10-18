# Feature: Collection Command Line <!-- rq-c1056d55 -->

`CJKQC <inputs>… --out <db>` turns files into a database. The command
classifies its inputs by content and takes one of two modes:

- **Build** (all inputs are QC program outputs): every input is parsed
  through the standard parser registry (`rqm/parser-framework.md`),
  composition is resolved from each record's name
  (`rqm/composition.md`), and the database at `--out` is written with
  exactly the collected records, replacing any previous content
  (`rqm/database.md`).
- **Merge** (all inputs are record databases): the databases are combined
  into `--out` with the records copied verbatim — no parsing, no
  re-resolution.

Mixing the two kinds of input in one invocation is a usage error: an
invocation is either all databases (merge) or all program outputs (build).
The database file at `--out` is required on every invocation; there is no
default path. A job whose output a known parser reads — even a failed or
truncated run — is stored as a status record: it is data, not a failure of
the run (`rqm/ARCHITECTURE.md`). A file that no parser reads at all
(unrecognized format, empty file) is not a job: it aborts the run with an
error and nothing is written.

Replacing an existing database at `--out` prints a warning to stderr unless
the run is forced with `--fout`; forcing changes the warning, never
what is written.

## Input Classification <!-- rq-8ad5063d -->

- An input is a **database input** when its first non-blank line, with
  leading whitespace trimmed, begins with `{` (record lines always do,
  program banners never do). An input with no non-blank line (an empty or
  all-blank file) is a program output; build mode aborts on it as an
  unrecognized input.
- Classification never consults the file name or extension. A database saved
  under any name merges; an output that happened to start with `{` is
  classified as a database and its lines are then reported as unreadable.
- All inputs are classified before any work begins. All databases → merge
  mode; all outputs → build mode; any mixture → usage error naming both
  groups (exit 2), nothing written.
- An input that cannot be opened (missing file, directory, permission
  denied) is a fatal input error (exit 1) discovered during classification,
  before anything is read or written.

## Build Mode <!-- rq-5d7eda5b -->

- Each input is parsed with `ParserRegistry::standard()` — content decides
  the parser, exactly one record per file, in argument order
  (`rqm/parser-framework.md`).
- A parsed record for which `Record::is_unrecognized` holds — status
  `failed` with no `program`, the framework's signature for an
  unrecognized format or an empty file (`rqm/parser-framework.md`,
  `rqm/record.md`) — aborts the run: the path is reported, nothing is
  written, exit 1. Records from a matched parser — successful, failed, or
  truncated — are always collected.
- `resolve_components` runs on every record — successful, failed, or
  truncated — before anything is written (`rqm/composition.md`).
- The output is **replaced**: all inputs are fully read, all records
  produced, and only then does `write_database` install the collected
  records as the complete new content of `--out`, in argument order,
  atomically (`rqm/database.md`) — a failure leaves any existing database
  intact, and replacing one warns unless `--fout` was given. All inputs are
  read before the target is written, so `--out` may
  name a file that is also an input.

## Merge Mode <!-- rq-f832a4fa -->

- The operation is `merge_databases`: inputs are read in argument order,
  their records written to `--out` concatenated in that order, and `--out`
  is replaced (as in build mode). `--out` may be one of
  the inputs (`rqm/database.md`).
- Records are copied verbatim: merge hands each input's original, validated
  line to the output without re-serialization (`rqm/database.md`), so name,
  components, charge, multiplicity, unknown keys, and every other field
  survive exactly as stored; names are never re-parsed.
- A corrupt line inside an input is skipped and reported (path and line
  number) without stopping the merge.

## Command Line <!-- rq-a4e0ba89 -->

```text
CJKQC <inputs>… --out <path>
CJKQC <inputs>… -o <path> [--fout]
```

- `<inputs>…` — one or more input files (positional, required).
- `--out` / `-o` — the database to write (required). Both modes replace it;
  its parent directory must already exist.
- `--fout` — skip the "replacing existing database" warning when
  `--out` already exists. It changes the warning, never the output; it is
  accepted in both modes.
- A literal `-` is an ordinary path, not stdin; stdin is not collected.
- Usage errors — no inputs, a missing `--out`, unknown flags (including JKQC
  flags that no feature has adopted, such as `-el`) — print clap's parse
  diagnostics to stderr and exit 2 (`rqm/scaffolding.md`). `--help` and
  `--version` behave unchanged and take precedence.
- Help output advertises the input positional and `--out` alongside the
  built-ins.

## Reporting and Exit Codes <!-- rq-a2681a7a -->

- Build completion prints to stdout:
  `collected <n> records into <out> (<f> failed, <t> truncated)` —
  `<f>`/`<t>` count records by status; writing them is success.
- Merge completion prints to stdout: `merged <n> records into <out>`.
- Replacing an existing database at `--out` prints a warning naming the path
  to stderr, unless `--fout` was given; the run still exits 0.
- Each skipped corrupt line prints to stderr a warning naming the input
  path and line number.
- Fatal errors (unreadable input, unrecognized format, empty input,
  unwritable `--out`, missing parent directory, a failed write) print an
  error naming the path to stderr.
- Exit codes: **0** — the run completed and skipped nothing; **1** — a
  fatal error (nothing written: IO failure, unrecognized or empty input) or
  a merge that skipped corrupt lines; **2** — usage error (bad flags, no
  inputs, mixed inputs). Failed and truncated records from a matched parser
  do not change the exit code.

## Feature API <!-- rq-3d76ed40 -->

### Types <!-- rq-64f91c42 -->

- `CollectionMode` — `Build` or `Merge`, the classification verdict. <!-- rq-4293ca31 -->
- `CollectOutcome` — what a completed run did: <!-- rq-d99aaa73 -->
  - Build: `written: usize`, `failed: usize`, `truncated: usize` (record
    counts by status).
  - Merge: `written: usize` and `problems: Vec<MergeProblem>` (the skipped
    lines, from `rqm/database.md`).
- `CollectError` — why a run wrote nothing: <!-- rq-989e6417 -->
  - `Mixed { databases: Vec<PathBuf>, outputs: Vec<PathBuf> }` — both input
    kinds were given (a usage error, exit 2).
  - `Unrecognized { path: PathBuf, reason: String }` — a build input
    carried no recognizable program output (unrecognized format or empty
    file); `reason` is the framework's error text.
  - `Io { kind: std::io::ErrorKind, reason: String }` — an input could not
    be read or the output could not be written (mirrors `DbError::Io` and
    `ParseError::Io`).
  - `Db(DbError)` — the database layer failed for a non-Io reason
    (serialization, invalid record) while writing.

### Functions <!-- rq-acd8f4f9 -->

- `classify_inputs(inputs: &[&Path]) -> Result<CollectionMode, CollectError>` <!-- rq-550ff715 -->
  - Applies the classification rule to every input (opening each file and
    reading up to its first non-blank line). Unanimous kinds yield the mode;
    a mixture yields `Mixed` listing the inputs of each kind; a read failure
    yields `Io`.
- `collect_inputs(inputs: &[&Path], output: &Path) -> Result<CollectOutcome, CollectError>` <!-- rq-9d8725d5 -->
  - Classifies, then performs the selected mode in full:
    build = parse every input with the standard registry, abort on any
    record with `Record::is_unrecognized` (`Unrecognized`), resolve
    compositions, then hand the serialized records to `write_database`;
    merge = one `merge_databases`.
  - An error return means the output is untouched: a pre-existing database
    is still intact, and a missing one was never created — parsing and
    serialization all complete before `write_database` begins, and
    `write_database` is itself atomic (`rqm/database.md`).
  - Whether a warning precedes the replacement of an existing `output` is a
    CLI concern (`--fout`); this function never warns.
  - `ParseError::Io` is reported as `CollectError::Io`; `DbError::Io` is
    reported as `CollectError::Io`; other database failures surface as `Db`.

### Crate Layout <!-- rq-336152ed -->

- `src/collect.rs` holds classification and the two pipelines; the library <!-- rq-fcb0174f -->
  root re-exports its public types and functions.
- `src/main.rs` holds the clap definition (inputs positional, `--out` <!-- rq-70fc34c2 -->
  required) and maps outcomes and errors to the stdout/stderr reporting and
  exit codes above; the scaffold's placeholder error path is gone.
- `tests/collect.rs` tests classification and `collect_inputs` over the <!-- rq-2d29c3a7 -->
  fixture corpus and temporary databases; `tests/cli.rs` tests exit codes,
  output database contents, and the summary lines through the binary.

## Out of Scope <!-- rq-92e3f3e9 -->

- Stdin input (a `-` argument or piping without arguments) — a later feature.
- CLI flags overriding or supplying composition, charge, or multiplicity
  (`rqm/ARCHITECTURE.md` hybrid model; `rqm/composition.md`).
- JKQC flags such as `-b` or `-el`, and filename printing; they arrive with
  the features that implement their behavior.
- Filtering, sorting, duplicate removal (`rqm/ARCHITECTURE.md`, milestone 5),
  recursive directory expansion (the shell globs), and transparent
  decompression.
- Parallel collection; v1 collects sequentially.
- Back-filling `components` on records that lack it when their names would
  resolve — merge is verbatim.
- Making `--out` optional. Build and merge always name a target database;
  operations that read and manipulate without storing (filtering, sorting,
  printing) arrive with their own output rules — printing to stdout when no
  `--out` is given among them.

## Gherkin Scenarios <!-- rq-084c74fa -->

```gherkin
Feature: Collection command line

  # --- Build mode ---

  @rq-3b9e8002
  Scenario: Collect one XTB output into a new database
    Given no file exists at "db.jsonl"
    When CJKQC is invoked with "tests/fixtures/xtb/1dma.log --out db.jsonl"
    Then the exit code is 0
    And db.jsonl loads as 1 record with program "xtb", name "1dma", and components dma = 1
    And stdout contains "collected 1 records into db.jsonl (0 failed, 0 truncated)"

  @rq-31d988e1
  Scenario: Multiple inputs are written in argument order
    Given no file exists at "db.jsonl"
    When CJKQC is invoked with "tests/fixtures/xtb/1dma.log tests/fixtures/orca/1pinic1sa-110_1_9-str9.out --out db.jsonl"
    Then the exit code is 0
    And db.jsonl loads as 2 records, the xtb record first, the second with components pinic = 1 and sa = 1

  @rq-e2843432
  Scenario: A repeated invocation replaces the database with a warning
    Given db.jsonl was built from "tests/fixtures/xtb/1dma.log"
    When CJKQC is invoked with "tests/fixtures/xyz/h2o.xyz --out db.jsonl"
    Then the exit code is 0
    And db.jsonl loads as the 1 xyz record only
    And a stderr warning names "db.jsonl" as being replaced

  @rq-503eebe7
  Scenario: --fout silences the replacement warning
    Given db.jsonl was built from "tests/fixtures/xtb/1dma.log"
    When CJKQC is invoked with "tests/fixtures/xyz/h2o.xyz --out db.jsonl --fout"
    Then the exit code is 0
    And db.jsonl loads as the 1 xyz record only
    And stderr contains no warning

  @rq-e4f87293
  Scenario: A truncated output is stored as a truncated record
    When CJKQC is invoked with "tests/fixtures/xtb/1dma-truncated.log --out db.jsonl"
    Then the exit code is 0
    And db.jsonl loads as 1 record with status truncated
    And stdout contains "(0 failed, 1 truncated)"

  @rq-720a810c
  Scenario: A genuine failed record from a matched parser is collected
    When CJKQC is invoked with "tests/fixtures/xyz/1am1x-bad.xyz --out db.jsonl"
    Then the exit code is 0
    And db.jsonl loads as 1 record with status failed, program "xyz", and components am = 1 and x = 1
    And stdout contains "(1 failed, 0 truncated)"

  @rq-180cebd1
  Scenario: An unrecognized output aborts the run and writes nothing
    Given a file "junk.txt" containing text no parser recognizes
    When CJKQC is invoked with "junk.txt --out db.jsonl"
    Then the exit code is 1
    And stderr names "junk.txt"
    And db.jsonl does not exist

  @rq-e50ec596
  Scenario: An empty input file aborts the run and writes nothing
    Given an empty file "empty.log"
    When CJKQC is invoked with "empty.log --out db.jsonl"
    Then the exit code is 1
    And stderr names "empty.log"
    And db.jsonl does not exist

  @rq-276adb9e
  Scenario: A missing input is fatal and writes nothing
    Given no file exists at "nope.log" or at "db.jsonl"
    When CJKQC is invoked with "nope.log tests/fixtures/xtb/1dma.log --out db.jsonl"
    Then the exit code is 1
    And stderr names "nope.log"
    And db.jsonl does not exist

  @rq-bedf480a
  Scenario: A directory input is fatal
    When CJKQC is invoked with "tests/fixtures/xtb --out db.jsonl"
    Then the exit code is 1

  @rq-0a2ed6f8
  Scenario: An unwritable output directory is fatal
    When CJKQC is invoked with "tests/fixtures/xtb/1dma.log --out no/such/dir/db.jsonl"
    Then the exit code is 1

  @rq-12d931b9
  Scenario: Classification ignores the file name
    Given a copy of "tests/fixtures/xtb/1dma.log" named "weird.dat"
    When CJKQC is invoked with "weird.dat --out db.jsonl"
    Then the exit code is 0
    And db.jsonl loads as 1 record with program "xtb"

  # --- Merge mode ---

  @rq-53463f17
  Scenario: Databases merge in argument order
    Given database "a.jsonl" with records A1 A2 and database "b.jsonl" with record B1
    When CJKQC is invoked with "a.jsonl b.jsonl --out merged.jsonl"
    Then the exit code is 0
    And merged.jsonl loads as A1 A2 B1 in that order
    And stdout contains "merged 3 records into merged.jsonl"

  @rq-34231bdc
  Scenario: Merge truncates an existing output
    Given database "a.jsonl" with record A1 and database "old.jsonl" with 5 records
    When CJKQC is invoked with "a.jsonl --out old.jsonl"
    Then the exit code is 0
    And old.jsonl loads as exactly the 1 record

  @rq-8f9a701f
  Scenario: Merge can write onto one of its inputs
    Given database "a.jsonl" with record A1 and database "b.jsonl" with record B1
    When CJKQC is invoked with "a.jsonl b.jsonl --out a.jsonl"
    Then the exit code is 0
    And a.jsonl loads as A1 B1

  @rq-d431309b
  Scenario: A corrupt line in a merged input is skipped and reported
    Given database "a.jsonl" with a valid record on line 1 and a corrupt line 2
    When CJKQC is invoked with "a.jsonl --out merged.jsonl"
    Then the exit code is 1
    And a stderr warning names "a.jsonl" and line 2
    And merged.jsonl loads as the valid record

  @rq-b51f8bc8
  Scenario: Merge copies records verbatim
    Given database "a.jsonl" holding a record named "2am3sa" with no components
    When CJKQC is invoked with "a.jsonl --out merged.jsonl"
    Then the exit code is 0
    And the merged record still has no components

  @rq-45238f22
  Scenario: A database recognized by content, not extension
    Given database "a.jsonl" copied to a file named "sneaky.dat"
    When CJKQC is invoked with "sneaky.dat --out merged.jsonl"
    Then the exit code is 0
    And the run is a merge, reported by its merge summary line

  # --- Modes and usage ---

  @rq-cdf88b68
  Scenario: Mixed inputs are refused
    Given database "a.jsonl" with 1 record and the file "tests/fixtures/xtb/1dma.log"
    When CJKQC is invoked with "a.jsonl tests/fixtures/xtb/1dma.log --out out.jsonl"
    Then the exit code is 2
    And stderr names both "a.jsonl" and "tests/fixtures/xtb/1dma.log"
    And out.jsonl does not exist

  @rq-f0785bb1
  Scenario: A missing --out is a usage error
    When CJKQC is invoked with "tests/fixtures/xtb/1dma.log"
    Then the exit code is 2
    And stderr is clap's diagnostic naming --out

  @rq-6cc1264d
  Scenario: Adopted JKQC-era unknown flags remain usage errors
    When CJKQC is invoked with "tests/fixtures/xtb/1dma.log --out db.jsonl -el"
    Then the exit code is 2
    And stderr is clap's diagnostic reporting the unknown flag

  @rq-54536800
  Scenario: A dash argument is an ordinary path, not stdin
    When CJKQC is invoked with "- --out db.jsonl"
    Then the exit code is 1
    And stderr names "-"
```
