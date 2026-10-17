# Feature: Record Database (JSON Lines) <!-- rq-2a2407ed -->

A **database** is a JSON Lines file: UTF-8 text, one record per line, in any
order, inspectable with any JSONL tool (`jq`, pandas, a text editor). Databases
are the interchange format between CJKQC invocations. The record type and its
JSON form are defined by `rqm/record.md`; this feature defines the file-level
rules and the three database operations: load, append, and merge.

Failures never panic and never lose good data: an unreadable *line* becomes a
reported problem while the valid records load; only filesystem-level failures
(missing file, unwritable path) are errors.

## Feature API <!-- rq-b9eeedfe -->

### Functions <!-- rq-33d65b8c -->

- `load_database(path: &Path) -> Result<LoadOutcome, DbError>` <!-- rq-2cc9076e -->
  - The path must exist and be a readable file; otherwise
    `Err(DbError::Io(_))`.
  - Reads the file as UTF-8 lines. Blank lines are ignored silently.
  - Every non-blank line is deserialized into a `Record` and validated
    (`Record::validate`).
  - Valid records are returned in file order.
  - A line that fails deserialization or validation — invalid JSON, invalid
    UTF-8 bytes, an unknown status value, a violated record invariant — is
    skipped and reported as a `LoadProblem` carrying its 1-based line number
    and a reason.
  - Content problems never fail the load and never stop later lines from
    being read.

- `append_records(path: &Path, records: &[Record]) -> Result<(), DbError>` <!-- rq-aeb0d510 -->
  - Validates every record before writing anything (all-or-nothing): one
    invalid record means `Err(DbError::InvalidRecord(_))` and nothing is
    written.
  - An empty slice writes nothing, creates no file, and returns `Ok`.
  - Creates the file when missing; the parent directory must already exist.
  - Existing content is never rewritten: records are added after it, one
    compact line per record, every written line newline-terminated.
  - When the existing file is non-empty and does not end with a newline, a
    newline is written before the first new record so that no record shares
    a line with prior bytes.
  - Concurrent writers are not coordinated (no file locking); v1 assumes a
    single writer per database.

- `merge_databases(inputs: &[&Path], output: &Path) -> Result<MergeOutcome, DbError>` <!-- rq-13bcad95 -->
  - Loads every input in the given order with the same per-line rules as
    `load_database` (skip and report). A missing or unreadable input is
    `Err(DbError::Io(_))` and nothing is written.
  - After all inputs are read, writes every loaded record to `output` in
    input order — the first input's records, then the second's, and so on.
  - `output` is created or truncated (existing content is replaced); its
    parent directory must already exist.
  - `output` may be the same path as one of the inputs: all inputs are fully
    read before `output` is opened.
  - Zero inputs produce an empty output file.
  - Returns the number of records written and every problem encountered.

### Types <!-- rq-022a9f55 -->

- `LoadOutcome` — `records: Vec<Record>` (valid records, in file order) and <!-- rq-2c1dac26 -->
  `problems: Vec<LoadProblem>`.
- `LoadProblem` — `line: usize` (1-based) and `reason: String`. <!-- rq-39867e78 -->
- `MergeOutcome` — `records: usize` (count written) and <!-- rq-3a147f99 -->
  `problems: Vec<MergeProblem>`.
- `MergeProblem` — `input: PathBuf`, `line: usize` (1-based), and <!-- rq-96b8c74d -->
  `reason: String`.
- `DbError` — error type for database operations: <!-- rq-125f5313 -->
  - `Io { kind: std::io::ErrorKind, reason: String }` — a filesystem
    operation failed: missing input file, uncreatable or unwritable
    output, missing parent directory, permission failure. `kind` is the
    underlying failure's kind (`NotFound`, `PermissionDenied`, …) so
    callers branch on the cause without parsing text; `reason` is the
    human-readable message.
  - `Serialize(String)` — a record could not be serialized to its JSON
    line while writing (append or merge). This is a record-content
    failure, never reported as `Io`.
  - `InvalidRecord(RecordInvalidity)` — a record handed to
    `append_records` failed validation (see `rqm/record.md`); nothing was
    written.

### Crate Layout <!-- rq-0c3f8c54 -->

- The database operations live in `src/db.rs` and operate on the record
  types from `src/record.rs`. No dependency beyond `serde`/`serde_json`.

## Database File Rules <!-- rq-1d1a1d61 -->

- UTF-8 text; one record per line; every complete line ends with `\n`.
- A database may be empty (zero bytes, or only blank lines) and loads as
  zero records with no problems.
- The `.jsonl` file name suffix is a convention only: content, not the name,
  makes a file a database, mirroring the content-over-extension rule for QC
  outputs in `rqm/ARCHITECTURE.md`.
- A partially written last line (for example after an interrupted run) does
  not corrupt the rest of the file: it is skipped and reported on load, and
  the next append seals it with a newline.
- Records may repeat; duplicate structures are a filtering concern, not a
  storage concern (milestone 5).

## Out of Scope <!-- rq-dfae85df -->

- Producing records from QC output files — the parser framework, milestones
  2–3 of the implementation plan.
- The `CJKQC` command-line surface for building and merging databases —
  milestone 4.
- Duplicate removal, subsetting, sorting — milestone 5.
- File locking, concurrent writers, and compressed or non-JSONL storage.

## Gherkin Scenarios <!-- rq-c6367ec2 -->

```gherkin
Feature: Record database

  # --- Load ---

  @rq-58f8e962
  Scenario: Load returns all records in file order
    Given a database file with 3 valid records in a known order
    When load_database is called on it
    Then Ok is returned with 3 records in file order
    And no problems are reported

  @rq-f0bf0879
  Scenario: An empty database loads as zero records
    Given an empty database file
    When load_database is called on it
    Then Ok is returned with 0 records
    And no problems are reported

  @rq-cb5c083c
  Scenario: Blank lines are ignored
    Given a database file with a blank line between two records
    When load_database is called on it
    Then Ok is returned with 2 records
    And no problems are reported

  @rq-61efc73e
  Scenario: Loading a missing file is an error
    Given no file exists at "missing.jsonl"
    When load_database is called on it
    Then Err(DbError::Io(_)) is returned
    And the Io error's kind is NotFound

  @rq-8f1add9c
  Scenario: Loading a directory is an error
    Given a directory exists at "somedir"
    When load_database is called on it
    Then Err(DbError::Io(_)) is returned

  @rq-2ff394b0
  Scenario: A corrupt line is skipped and reported
    Given a database file where line 2 is not valid JSON
    When load_database is called on it
    Then Ok is returned with the records from lines 1 and 3
    And one problem reports line 2

  @rq-50bb7e83
  Scenario: A partially written last line is skipped and reported
    Given a database file with 2 valid records and a third line of truncated JSON without a trailing newline
    When load_database is called on it
    Then Ok is returned with 2 records
    And one problem reports line 3

  @rq-77c19ef4
  Scenario: A line with invalid UTF-8 is skipped and reported
    Given a database file where line 2 contains bytes that are not valid UTF-8
    When load_database is called on it
    Then Ok is returned with the records from the other lines
    And one problem reports line 2

  @rq-1736b3f9
  Scenario: A record that fails validation is skipped and reported
    Given a database file whose only line holds a record whose gradient mismatches its geometry
    When load_database is called on it
    Then Ok is returned with 0 records
    And one problem reports line 1

  # --- Append ---

  @rq-e4fad868
  Scenario: Append creates a missing database
    Given no file exists at "new.jsonl"
    When append_records is called on "new.jsonl" with 2 valid records
    Then Ok is returned
    And "new.jsonl" contains the 2 records, one per line
    And the file ends with a newline

  @rq-b1b26ed5
  Scenario: Append preserves existing content
    Given a database file with 2 records
    When append_records is called on it with 1 valid record
    Then Ok is returned
    And the file's first 2 lines are byte-identical to before
    And the file now has 3 lines

  @rq-82900eac
  Scenario: Append seals a missing final newline
    Given a database file whose last line lacks a trailing newline
    When append_records is called on it with 1 valid record
    Then Ok is returned
    And the new record starts on its own line
    And the previous bytes are otherwise unchanged

  @rq-9f86cf20
  Scenario: Append with an invalid record writes nothing
    Given a database file with 2 records
    When append_records is called on it with a valid record followed by a record with a NaN energy
    Then Err(DbError::InvalidRecord(_)) is returned
    And the file still contains exactly the original 2 records

  @rq-1d5bcfda
  Scenario: Append to a path with a missing parent directory fails
    Given the directory "no/such/dir" does not exist
    When append_records is called on "no/such/dir/db.jsonl" with 1 valid record
    Then Err(DbError::Io(_)) is returned

  @rq-b177c1e4
  Scenario: Appending zero records to a missing file creates nothing
    Given no file exists at "new.jsonl"
    When append_records is called on "new.jsonl" with no records
    Then Ok is returned
    And no file is created

  @rq-2bffd61d
  Scenario: Appended records round-trip
    Given 2 valid records
    When append_records is called on a fresh path with them
    And load_database is called on that path
    Then the loaded records equal the originals exactly

  # --- Merge ---

  @rq-95c630bc
  Scenario: Merge concatenates inputs in order
    Given database "a.jsonl" with records A1 A2 and database "b.jsonl" with record B1
    When merge_databases is called on them with output "c.jsonl"
    Then Ok is returned with 3 records written
    And "c.jsonl" loads as A1 A2 B1 in that order
    And "a.jsonl" and "b.jsonl" are unchanged

  @rq-b032b41c
  Scenario: Merge can write onto an input path
    Given database "a.jsonl" with record A1 and database "b.jsonl" with record B1
    When merge_databases is called on them with output "a.jsonl"
    Then Ok is returned
    And "a.jsonl" loads as A1 B1
    And "b.jsonl" is unchanged

  @rq-15bf030c
  Scenario: Merge truncates an existing output
    Given database "a.jsonl" with 1 record and database "old.jsonl" with 5 records
    When merge_databases is called on "a.jsonl" alone with output "old.jsonl"
    Then Ok is returned
    And "old.jsonl" loads as exactly the 1 record

  @rq-bdd60556
  Scenario: Merge skips and reports corrupt lines in inputs
    Given database "a.jsonl" with a valid record on line 1 and a corrupt line 2, and database "b.jsonl" with 1 valid record
    When merge_databases is called on them with output "c.jsonl"
    Then Ok is returned with 2 records written
    And one problem reports input "a.jsonl" line 2

  @rq-23fa0de7
  Scenario: Merge with a missing input writes nothing
    Given database "a.jsonl" exists with 1 record and "missing.jsonl" does not exist
    When merge_databases is called on them with output "c.jsonl"
    Then Err(DbError::Io(_)) is returned
    And "c.jsonl" is not created

  @rq-62bf0467
  Scenario: Merge to a path with a missing parent directory fails
    Given the directory "no/such/dir" does not exist
    When merge_databases is called on one valid input with output "no/such/dir/c.jsonl"
    Then Err(DbError::Io(_)) is returned

  @rq-4dda3d71
  Scenario: Merging zero inputs creates an empty output
    Given no file exists at "c.jsonl"
    When merge_databases is called with no inputs and output "c.jsonl"
    Then Ok is returned with 0 records written
    And "c.jsonl" exists and is empty
```
