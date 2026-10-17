# Feature: Project Scaffold and CJKQC Command Shell <!-- rq-486289b7 -->

The crate compiles to a single executable `CJKQC` — a clap-based command-line
shell in the JKQC tradition (one command, many flags; see
`rqm/ARCHITECTURE.md` for the eventual CLI style). The scaffold fixes the
executable's name, its `--help`/`--version` behavior, and its error behavior
for the period in which no domain functionality exists. The crate provides a
library target for unit and integration tests alongside the binary.

## Feature API <!-- rq-56514e37 -->

### Executable <!-- rq-0df0a5df -->

- `CJKQC` — the only binary target of the crate `complied-jkqc`. <!-- rq-899932a2 -->
  - The executable's file name is exactly `CJKQC` (uppercase; case-sensitive
    on Unix, a deliberate choice).
  - The CLI is built with clap; the usage line in help output displays
    `CJKQC`.
  - `--help` (and `-h`) prints the usage text to stdout and exits 0.
  - `--version` (and `-V`) prints the executable name and the crate version
    from `Cargo.toml` to stdout and exits 0.
  - An invocation containing `--help` or `-h` (respectively `--version` or
    `-V`) prints the help text (respectively the version) and exits 0, even
    when other arguments are present.
  - Every other invocation — no arguments, positional file arguments, or
    unknown flags — exits with code 2 and prints one uniform error message
    to stderr stating that no functionality is implemented yet and pointing
    to `--help`; clap's own parse diagnostics are not shown.
  - The scaffold declares no domain flags and no positional arguments; help
    output advertises only the built-in `--help`/`--version`. No JKQC flag
    (e.g. `-el`) or file argument (`*.log`, `*.pkl`, `*.xyz`) is recognized
    yet; they arrive with the milestones that implement them.

### Crate Layout <!-- rq-0e05c3af -->

- `Cargo.toml` declares the package `complied-jkqc`, the `CJKQC` binary <!-- rq-2072b6c4 -->
  target, and the dependencies `clap`, `serde`, and `serde_json`.
- `src/main.rs` holds the clap CLI definition and the stub error path. <!-- rq-c3792c41 -->
- `src/lib.rs` is the library target: crate-level documentation, module <!-- rq-8c5e4002 -->
  declarations for the domain modules, and re-exports of the record types.
  No placeholder functions from the project template exist anywhere in the
  crate, and no test references them.
- `tests/` contains integration tests; the CLI tests run the built binary <!-- rq-e61c2633 -->
  via the `CARGO_BIN_EXE_CJKQC` environment variable, and milestone features
  add library-level integration tests alongside them, all without any extra
  dev-dependencies.

## Out of Scope <!-- rq-e71fbb0f -->

- All domain functionality beyond the scaffold: parsing, database,
  filtering, thermochemistry, export (milestones 1+ of the implementation
  plan).
- The module map for later milestones (`parser/`, collection, export, …);
  each module is created by the milestone that requires it.
- Any exit codes beyond the usage-error policy above.

## Gherkin Scenarios <!-- rq-602d347c -->

```gherkin
Feature: CJKQC command scaffold

  @rq-f9a3759e
  Scenario: --help prints usage
    When CJKQC is invoked with "--help"
    Then the exit code is 0
    And stdout contains "Usage: CJKQC"

  @rq-e1c51ff7
  Scenario: Short help flag behaves like --help
    When CJKQC is invoked with "-h"
    Then the exit code is 0
    And stdout contains "Usage: CJKQC"

  @rq-1bbcf87c
  Scenario: --version prints the crate version
    When CJKQC is invoked with "--version"
    Then the exit code is 0
    And stdout contains "CJKQC" and the version from Cargo.toml

  @rq-843d152e
  Scenario: Short version flag behaves like --version
    When CJKQC is invoked with "-V"
    Then the exit code is 0
    And stdout contains the version from Cargo.toml

  @rq-72e0ded9
  Scenario: Help takes precedence over extra arguments
    When CJKQC is invoked with "--help extra"
    Then the exit code is 0
    And stdout contains "Usage: CJKQC"

  @rq-953a8521
  Scenario: Version takes precedence over extra arguments
    When CJKQC is invoked with "--version extra"
    Then the exit code is 0
    And stdout contains the version from Cargo.toml

  @rq-6b872f89
  Scenario: No arguments is a usage error
    When CJKQC is invoked with no arguments
    Then the exit code is 2
    And stderr contains "no functionality is implemented yet"

  @rq-be35a7b3
  Scenario: Positional arguments are a usage error
    When CJKQC is invoked with "output.log output2.out"
    Then the exit code is 2
    And stderr contains "no functionality is implemented yet"

  @rq-e003541d
  Scenario: JKQC-style invocations are usage errors until domain flags exist
    When CJKQC is invoked with "output.log -el"
    Then the exit code is 2
    And an error message is printed to stderr

  @rq-bd59ba92
  Scenario: Help output advertises no domain flags
    When CJKQC is invoked with "--help"
    Then the exit code is 0
    And the help text does not list any domain flags or positional file
    arguments

  @rq-2486bcc8
  Scenario: The build produces the exact executable name
    When cargo build is run
    Then an executable file named "CJKQC" (with the platform executable
    suffix, e.g. ".exe" on Windows) is produced in the target directory

  @rq-22fd07a1
  Scenario: The test suite passes on the scaffolded crate
    When cargo test is run
    Then all tests pass
    And no test or source file references a template placeholder "canvas"
    function
```
