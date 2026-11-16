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

Replacing an existing database at `--out` is confirmed before it is
overwritten; `--fout` forces the replacement without asking
(Overwrite Confirmation below).

## Overwrite Confirmation <!-- rq-eff8dce8 -->

- A target that already exists — the `--out` database in either mode, the
  per-structure xyz files of `--write-xyz`, or `movie.xyz`
  (`rqm/movie.md`) — is never replaced silently. Immediately
  before its atomic write the run prints the question
  `<path> already exists; overwrite? [y/N]` to stderr and reads one answer
  line from stdin. The line is trimmed; `y` and `yes` in any casing mean
  yes; every other answer — an empty line, `n`, anything else, or an
  end-of-input — means no.
- A no answer aborts the run: stderr reports
  `error: <path> exists; not overwriting`, the target is left untouched,
  no later write is attempted, and the exit code is 1.
- When stdin is not an interactive terminal the question is never printed
  and stdin is never read: the replacement is refused with
  `error: <path> exists; not interactive; use --fout to force`, the target
  is left untouched, and the exit code is 1.
- `--fout` forces the replacement in every context: no question is
  printed, stdin is not read, and the write proceeds. It changes nothing
  else — a target that does not exist is written the same way with or
  without it.
- Each target is confirmed at its own write, in write order: the `--out`
  database before the per-structure xyz files of `--write-xyz`, and those
  before `movie.xyz` (`rqm/movie.md`), so a no or a refusal at `--out` ends
  the run before the xyz exports are considered, while a confirmed and
  written `--out` stays written when a later export is declined. The
  `--write-xyz` export confirms the whole file set once, naming the first
  existing target in run order; a yes replaces every existing target, and a
  no or refusal aborts before any of its files is written. The export's
  duplicate-name and output-collision checks (`rqm/movie.md`) run before the
  `--out` write, so a duplicate or colliding target aborts the run before
  the database is replaced.
- The question and the answer are CLI logic (`src/overwrite.rs`): the
  collection pipeline invokes a caller-supplied hook immediately before
  replacing an existing output, and every other write site asks directly.
  The library itself never asks.

## Input Classification <!-- rq-8ad5063d -->

- An input is a **database input** when its first non-blank line, with
  leading whitespace trimmed, begins with `{` (record lines always do,
  program banners never do). An input with no non-blank line (an empty or
  all-blank file) is a program output; build mode aborts on it as an
  unrecognized input.
- Classification never consults the file name or extension. A database saved
  under any name merges; an output that happened to start with `{` is
  classified as a database and its lines are then reported as unreadable.
- Every input's classification decides the mode before any record is kept
  or written: all databases → merge mode; all outputs → build mode; any
  mixture → usage error naming both groups (exit 2), nothing written. A
  run may read and parse an input before later inputs are classified,
  provided every observable outcome — the mode, the reported first error,
  the records written — is the outcome of classifying every input first.
- An input that cannot be opened (missing file, directory, permission
  denied) is a fatal input error (exit 1) discovered at its input's
  classification; the run writes nothing.
- Classification reads concurrently when the run carries `--parallel N > 1`
  (Parallel Collection below): the first non-blank line of each input is
  read by one of N workers. The verdict, the `Mixed` listing, and the
  reported first error in argument order are identical to a sequential
  classification of the same inputs.
- Each build pass opens each build-mode input once: the handle whose first
  non-blank line decided the classification is the handle the window
  reader reads the head and tail windows from, so a pass issues one open
  per input, not one per pass — the shape that keeps metadata round-trips
  low on parallel network filesystems. A print flag plus `--out` performs
  the re-read, one open per input per pass. Merge inputs are opened by the
  merge reader.
- A run holds a small multiple of the worker count of input handles at
  a time, plus its ordinary process handles, whatever the input count:
  an input's handle lives from its classification to its parsed record,
  never longer, so a batch larger than the process's open-file limit
  collects successfully.

## Build Mode <!-- rq-5d7eda5b -->

- Each input is parsed with `ParserRegistry::standard()` — content decides
  the parser, exactly one record per file, in argument order
  (`rqm/parser-framework.md`). With `--parallel N > 1` the parsing runs
  across N workers (Parallel Collection below); the records and the
  database are the same either way.
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
  intact, and replacing one is confirmed or forced with `--fout`
  (Overwrite Confirmation). All inputs are
  read before the target is written, so `--out` may
  name a file that is also an input.

## Parallel Collection <!-- rq-3e15605c -->

- `--parallel <N>` sets the worker count of the run's build-mode passes —
  classification and parsing, including the parse pass behind a print
  flag's re-read — and, when the manipulation pipeline runs `--arbalign`,
  the worker count of that pairwise sweep (`rqm/arbalign.md`). The default
  is 1: fully sequential. A value below 1 is a usage error (exit 2). In
  collection it changes nothing else in the invocation.
- With N > 1 the inputs are distributed over N workers; each worker opens,
  window-reads, and parses whole inputs on its own (a record is never
  split across workers), so the per-input cost stays the sequential one
  and the workers' reads overlap — the shape parallel network
  filesystems (Lustre and friends) serve best, where N concurrent large
  reads and a single metadata round-trip per input dominate the wall time.
- The result never depends on N. Records are produced, ordered, and
  written in argument order whatever the worker count; the database is
  byte-identical to the one a sequential run writes.
- Errors are sequential-identical. A parallel run aborts on exactly the
  error a sequential run would report first in argument order — the same
  variant, the same text naming the same path, the same exit code — and
  writes nothing. When several inputs would fail a sequential run, the
  reported one is the earliest in argument order, whatever the workers
  discovered first.
- An open that fails at the process's open-file limit is a fatal input
  error whose message names the limit and the worker knob: fewer workers
  or a higher open-file limit fits the batch.
- Merge-mode collection reads sequentially at any N: its inputs are the
  tool's own JSON Lines databases. A merge run that also requests
  `--arbalign` still uses N workers for that pairwise sweep.

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
- `--fout` — force the replacement of an existing `--out` (or the xyz
  export files, `rqm/movie.md`) without the confirmation question
  (Overwrite Confirmation). It is accepted in both modes.
- `--parallel <N>` — the worker count of the run's build-mode passes
  (Parallel Collection) and of the `--arbalign` pairwise sweep
  (`rqm/arbalign.md`), an integer of 1 or more; the default is 1
  (sequential), and a value below 1 is a usage error (exit 2). It is
  accepted in both modes; merge-mode collection ignores it, but a merge run
  that also requests `--arbalign` still uses it for that sweep. Help
  advertises it alongside `--head-bytes`/`--tail-bytes`.
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
  `<f>`/`<t>` count records by status; writing them is success. With
  `--add-correction` (`rqm/correction.md`) the parentheses gain
  `, <k> corrected`, counting written records that carry a correction.
- Merge completion prints to stdout: `merged <n> records into <out>`. With
  `--add-correction` (`rqm/correction.md`) the line gains
  `(<k> corrected)`.
- The overwrite question prints to stderr and the answer is read from
  stdin (Overwrite Confirmation); a replacement answered no, or refused
  for the lack of a terminal without `--fout`, is a fatal error naming the
  path: exit 1, the existing file untouched.
- Each skipped corrupt line prints to stderr a warning naming the input
  path and line number.
- Fatal errors (unreadable input, unrecognized format, empty input,
  unwritable `--out`, missing parent directory, a failed write) print an
  error naming the path to stderr.
- Exit codes: **0** — the run completed and skipped nothing; **1** — a
  fatal error (nothing written: IO failure, unrecognized or empty input,
  an overwrite answered no or refused without `--fout`) or
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
  - `NotConfirmed` — the confirmation hook declined the replacement of an
    existing output; nothing was written. The CLI prints the reason itself
    and never surfaces this variant's own text.
- `OverwriteDecision` — `Proceed`, `Declined`, or `Refused`, the <!-- rq-b0fad254 -->
  confirmation verdict for one target (Overwrite Confirmation).

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
  - The confirmation of an existing `output` is the caller's concern
    (`--fout`, Overwrite Confirmation): the CLI injects its hook through
    `collect_inputs_with`; `collect_inputs` itself runs with an
    unconditional confirmation and never asks.
  - `ParseError::Io` is reported as `CollectError::Io`; `DbError::Io` is
    reported as `CollectError::Io`; other database failures surface as `Db`.

- `collect_inputs_with(inputs: &[&Path], output: &Path, budget: &WindowBudget, workers: usize, confirm: &mut dyn FnMut() -> bool) -> Result<CollectOutcome, CollectError>` — <!-- rq-3b5861a5 -->
  `collect_inputs` with an explicit window budget and worker count: the
  budget sizes build-mode windows (the CLI's `--head-bytes`/
  `--tail-bytes`), and `workers` sets how many worker threads run the
  build-mode passes (the CLI's `--parallel`; `1` is sequential, and the
  outcome is identical for any worker count — Parallel Collection); merge
  mode reads record databases and ignores both. `confirm` is invoked
  immediately before an existing output is replaced; `false` aborts with
  nothing written (`CollectError::NotConfirmed`). `collect_inputs` is
  this function with the default budget, one worker, and an
  unconditional confirmation.

- `gather_inputs_with(inputs: &[&Path], budget: &WindowBudget, workers: usize) -> Result<Gathered, CollectError>` — <!-- rq-024c9db0 -->
  `gather_inputs` (classify and read records without writing) with the
  same budget and worker-count overrides and the same merge-mode
  indifference to them; `gather_inputs` is this function with the default
  budget and one worker.

- `confirm_overwrite(path: &Path, prompt: &mut dyn Write, input: &mut dyn BufRead, interactive: bool) -> OverwriteDecision` <!-- rq-59b68ca8 -->
  - The confirmation of Overwrite Confirmation for one target. Interactive:
    the question `<path> already exists; overwrite? [y/N]` is written to
    `prompt` (the CLI's stderr) and one answer line is read from `input`,
    trimmed, and parsed; `Proceed` only for a yes answer, `Declined`
    otherwise, end-of-input included. A question that cannot be written to
    `prompt` is treated like the non-interactive case: the verdict is
    `Refused` and `input` is untouched. Non-interactive: neither stream is
    touched and the verdict is `Refused`.
- `parse_overwrite_answer(line: &str) -> bool` <!-- rq-298c8373 -->
  - The answer grammar: the trimmed line equals `y` or `yes`,
    case-insensitively.
- `overwrite_error(decision: OverwriteDecision, path: &Path) -> Option<String>` <!-- rq-7e0e7fa6 -->
  - The stderr error line for a refused replacement:
    `error: <path> exists; not overwriting` for `Declined` and
    `error: <path> exists; not interactive; use --fout to force` for
    `Refused`; `None` for `Proceed`.

### Crate Layout <!-- rq-336152ed -->

- `src/collect.rs` holds classification and the two pipelines; the library <!-- rq-fcb0174f -->
  root re-exports its public types and functions.
- The overwrite confirmation lives in `src/overwrite.rs` —
  `OverwriteDecision`, `confirm_overwrite`, `parse_overwrite_answer`,
  `overwrite_error`, re-exported by the library root; `main.rs` supplies
  `stdin().is_terminal()` (`std::io::IsTerminal`, no new dependencies),
  stderr as the prompt sink, and maps the decision through
  `overwrite_error` to the exit-1 abort.
- `src/main.rs` holds the clap definition (inputs positional, `--out` <!-- rq-70fc34c2 -->
  required) and maps outcomes and errors to the stdout/stderr reporting and
  exit codes above; the scaffold's placeholder error path is gone.
- `tests/collect.rs` tests classification and `collect_inputs` over the <!-- rq-2d29c3a7 -->
  fixture corpus and temporary databases — the parallel pass included:
  the same corpus collected with several worker counts yields one
  byte-identical database, and a corpus with a failing input reports the
  argument-order-first error at any worker count; `tests/overwrite.rs` tests the
  confirmation decisions (answer grammar, end-of-input, the
  non-interactive refusal, the question text) with injected streams;
  `tests/cli.rs` tests exit codes,
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
- An automatic worker count (`--parallel` takes an explicit count; the
  default stays 1) and parallel merge-mode reads — merge inputs are the
  tool's own small databases, read sequentially.
- Readahead advice (`posix_fadvise` and friends): with workers reading
  whole inputs, a parallel run already overlaps its input reads, and a
  sequential run reads one input at a time at the filesystem's own pace.
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
  Scenario: A non-interactive run refuses to replace an existing database
    Given db.jsonl was built from "tests/fixtures/xtb/1dma.log"
    When CJKQC is invoked with "tests/fixtures/xyz/h2o.xyz --out db.jsonl" with stdin piped, not a terminal
    Then the exit code is 1
    And db.jsonl still loads as the 1 xtb record only
    And stderr contains "error: db.jsonl exists; not interactive; use --fout to force"

  @rq-503eebe7
  Scenario: --fout forces the replacement without asking
    Given db.jsonl was built from "tests/fixtures/xtb/1dma.log"
    When CJKQC is invoked with "tests/fixtures/xyz/h2o.xyz --out db.jsonl --fout"
    Then the exit code is 0
    And db.jsonl loads as the 1 xyz record only
    And stderr contains no overwrite question

  @rq-02fe3cee
  Scenario: A new target is written without any question
    Given no file exists at "db.jsonl"
    When CJKQC is invoked with "tests/fixtures/xtb/1dma.log --out db.jsonl"
    Then the exit code is 0
    And stderr contains no overwrite question

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
    When CJKQC is invoked with "a.jsonl --out old.jsonl --fout"
    Then the exit code is 0
    And old.jsonl loads as exactly the 1 record

  @rq-8f9a701f
  Scenario: Merge can write onto one of its inputs
    Given database "a.jsonl" with record A1 and database "b.jsonl" with record B1
    When CJKQC is invoked with "a.jsonl b.jsonl --out a.jsonl --fout"
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

  @rq-bb4d61fb
  Scenario: A zero window budget is a usage error
    When CJKQC is invoked with "tests/fixtures/xtb/1dma.log --out db.jsonl --head-bytes 0"
    Then the exit code is 2
    And stderr reports that the window budget must be at least 1

  @rq-54536800
  Scenario: A dash argument is an ordinary path, not stdin
    When CJKQC is invoked with "- --out db.jsonl"
    Then the exit code is 1
    And stderr names "-"

  # --- Overwrite confirmation ---

  @rq-ee9ac740
  Scenario: confirm_overwrite proceeds on yes answers
    Given an existing target "db.jsonl" and an interactive confirmation
    When confirm_overwrite reads the answer line "y"
    Then the decision is Proceed
    And the same holds for "Y", "yes", "YES", and " yes "

  @rq-83ccc28e
  Scenario: confirm_overwrite declines on any other answer
    Given an existing target "db.jsonl" and an interactive confirmation
    When confirm_overwrite reads the answer line "n"
    Then the decision is Declined
    And the same holds for "", "no", and "maybe"

  @rq-42f7e417
  Scenario: confirm_overwrite declines at end of input
    Given an existing target "db.jsonl", an interactive confirmation, and an input already at end of input
    When confirm_overwrite is called
    Then the decision is Declined

  @rq-466cd609
  Scenario: confirm_overwrite refuses without a terminal
    Given an existing target "db.jsonl" and a non-interactive confirmation
    When confirm_overwrite is called
    Then the decision is Refused
    And neither the prompt nor the input stream is touched

  @rq-ed1ff9fd
  Scenario: The interactive question names the target
    Given an existing target "db.jsonl" and an interactive confirmation
    When confirm_overwrite is called
    Then the prompt shows "db.jsonl already exists; overwrite? [y/N]"

  @rq-13aa8b07
  Scenario: A declined overwrite aborts with the file untouched
    Given an existing target "db.jsonl" whose confirmation is Declined
    When overwrite_error maps the decision
    Then the error line is "error: db.jsonl exists; not overwriting"
    And the run exits 1
    And db.jsonl is untouched

  @rq-0727ffaf
  Scenario: A refused overwrite names --fout
    Given an existing target "db.jsonl" whose confirmation is Refused
    When overwrite_error maps the decision
    Then the error line is "error: db.jsonl exists; not interactive; use --fout to force"

  @rq-42da75bb
  Scenario: A refusal at --out ends the run before movie.xyz
    Given an existing database "kept.jsonl", an existing file "movie.xyz", and a database with 3 ok records of one composition
    When CJKQC is invoked with the database, "--cutr el 5 --out kept.jsonl --movie" with stdin piped, not a terminal
    Then the exit code is 1
    And kept.jsonl and movie.xyz are both unchanged
    And the refusal on stderr names "kept.jsonl"

  @rq-8918146b
  Scenario: collect_inputs_with does not consult the hook for a new output
    Given a fresh output path that does not exist
    When collect_inputs_with is called with a hook that always declines
    Then the collection succeeds and the output is written

  # --- Parallel collection ---

  @rq-c974ee61
  Scenario: A parallel run writes the sequential run's database
    Given three program outputs of three different formats and no file at "db.jsonl"
    When CJKQC is invoked with the outputs, "--out db.jsonl --parallel 3"
    Then the exit code is 0
    And db.jsonl is byte-identical to the database the same outputs
    collected without --parallel produce

  @rq-7286bd48
  Scenario: Records keep argument order at any worker count
    Given program outputs A, B, and C in that argument order
    When CJKQC is invoked with "A B C --out db.jsonl --parallel 3"
    Then db.jsonl loads as A's record, then B's, then C's

  @rq-23b4fd18
  Scenario: A parallel run reports the argument-order-first error
    Given five program outputs of which the second and the fourth are
    unrecognized, in that argument order
    When CJKQC is invoked with all five, "--out db.jsonl --parallel 4"
    Then the exit code is 1
    And stderr names the second output
    And db.jsonl does not exist

  @rq-ecff2560
  Scenario: A parallel run writes nothing when any input fails
    Given program outputs of which the last cannot be read
    When CJKQC is invoked with all of them, "--out db.jsonl --parallel 4"
    Then the exit code is 1
    And db.jsonl does not exist

  @rq-27e5a6b2
  Scenario: A zero worker count is a usage error
    When CJKQC is invoked with "tests/fixtures/xtb/1dma.log --out db.jsonl --parallel 0"
    Then the exit code is 2
    And stderr reports that --parallel must be at least 1

  @rq-60a23fe4
  Scenario: Merge ignores the worker count
    Given database "a.jsonl" with records A1 A2
    When CJKQC is invoked with "a.jsonl --out merged.jsonl --parallel 4"
    Then the exit code is 0
    And merged.jsonl loads as A1 A2
    And stdout contains "merged 2 records into merged.jsonl"

  @rq-c8e29272
  Scenario: Mixed inputs are refused identically in a parallel run
    Given database "a.jsonl" with 1 record, the file "tests/fixtures/xtb/1dma.log",
    and the file "tests/fixtures/orca/1pinic1sa-110_1_9-str9.out"
    When CJKQC is invoked with all three, "--out out.jsonl --parallel 3"
    Then the exit code is 2
    And stderr names the database and both outputs
    And out.jsonl does not exist

  @rq-0c8eff8b
  Scenario: A batch larger than the open-file limit collects
    Given 600 program outputs and a process open-file limit of 256
    When CJKQC is invoked with the outputs, "--out db.jsonl --parallel 8"
    Then the exit code is 0
    And db.jsonl holds 600 records

  @rq-9b736232
  Scenario: A mixture is reported over an earlier parse failure
    Given an unrecognized program output first and a database input last,
    in that order
    When CJKQC is invoked with both, "--out db.jsonl --parallel 2"
    Then the exit code is 2
    And stderr reports the mixture, naming both inputs
    And db.jsonl does not exist

  @rq-8617d3fb
  Scenario: An open-file-limit failure names the limit
    Given a process open-file limit of 24, 128 program outputs whose
    parses overlap under "--parallel 128"
    When the run collects into "--out db.jsonl"
    Then the exit code is 1
    And stderr names the open-file limit and the worker count
```
