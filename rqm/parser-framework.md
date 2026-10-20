# Feature: Format Detection and Parser Framework <!-- rq-42e59efe -->

Every quantum-chemistry output file handed to CJKQC is turned into exactly
one record by the parser framework: content-based format detection, a parser
trait plus an ordered registry, a shared head+tail file reader, and shared
extraction helpers. Detection inspects the file's leading lines only — never
its name or extension — and routes the file to the parser whose signature
appears earliest in the head window; when several parsers match, the program
whose banner opens the file wins over a banner a wrapper embeds further down.

A file yields exactly one record, describing the file's final job: the
outcome of the last calculation step chain, the last frame of a multi-frame
file. Earlier jobs and earlier frames are not extracted; the per-format
parser requirements (milestone 3) define what "final job" means for each
format. The final job's data lies in the tail window, which is what keeps
one-record-per-file honest for files larger than the windows.

The framework ships with an empty registry: no output format is recognized
until the per-format parsers are registered (milestone 3), and every file
parsed in the interim yields an unrecognized-format record. The framework is
complete and testable without any real parser — tests register their own
small parsers behind the same trait.

Large outputs keep the wanted observables near their start (banners, route
sections, charge and multiplicity) and end (final energies, geometries,
frequencies, termination markers). The reader therefore presents each file
as two bounded windows — a head and a tail — and parsers treat the windows
as the entire file; no API exposes the elided middle. A file small enough to
fit in the windows is presented whole. Records carry the observable
vocabulary of `rqm/record.md` and feed the databases of `rqm/database.md`;
failures and truncations are status records — never panics, never aborted
runs (`rqm/ARCHITECTURE.md`).

## Feature API <!-- rq-b75cecef -->

### Types <!-- rq-d0ee2621 -->

- `FileWindows` — the head and tail view of one file's content. <!-- rq-16630511 -->
  - `head: String` and `tail: String` — the window texts, always valid UTF-8
    (invalid bytes become U+FFFD replacement characters).
  - `total_len: u64` — the file's size in bytes, before any lossy
    conversion.
  - `covered(&self) -> bool` — whether the whole file is visible: the file
    is no larger than the head window plus the tail window. When covered,
    `head` and `tail` are both the entire content.
  - `from_seekable(seekable: &mut S, head_bytes: usize, tail_bytes: usize)
    -> io::Result<FileWindows>` where `S: Read + Seek` — reads the head from
    the start and the tail from the end by seeking; the bytes read are
    bounded by the two window sizes regardless of file size.
  - `from_stream(reader: &mut R, head_bytes: usize, tail_bytes: usize)
    -> io::Result<FileWindows>` where `R: Read` — the fallback for
    non-seekable sources (pipes, stdin): reads the stream once from start
    to end, retaining at most the two windows in memory. For identical
    content and window sizes it yields windows identical to
    `from_seekable`'s.
  - `first_line_where(&self, predicate: impl Fn(&str) -> bool)
    -> Option<&str>` — the first visible line, in file order, that satisfies
    the predicate.
  - `last_line_where(&self, predicate: impl Fn(&str) -> bool)
    -> Option<&str>` — the last visible line, in file order, that satisfies
    the predicate.
  - `after_first_line_where(…)` / `after_last_line_where(…)` — iterators
    over the visible lines that follow the matched line, in file order.
    Iteration never crosses a window boundary: when the matched line lies in
    the head window of an uncovered file, iteration ends where the head
    window ends.
  - `locate(&self, patterns: &[LinePattern]) -> LineMatches` — the
    multi-pattern search: identifies every pattern at once — literal
    patterns by whole-window byte scans, structural (`Where`) patterns by
    one shared line walk — evaluating each pattern against each line at
    most once and never re-walking a window per marker. For any single
    pattern its answers are identical to the corresponding single-pattern
    method (`first` to `first_line_where`, `last` to `last_line_where`,
    `after_first` and `after_last` to their `…_line_where` counterparts,
    boundary rules included). The per-format parse paths use `locate`; the
    single-pattern methods are the one-pattern conveniences.

- `WindowBudget { head_bytes, tail_bytes }` — a pair of window sizes for <!-- rq-4311126f -->
  one reader call; `Default` is the framework's `DEFAULT_HEAD_BYTES` and
  `DEFAULT_TAIL_BYTES`. The CLI's `--head-bytes`/`--tail-bytes` flags
  build one, and `parse_file_with`, `collect_inputs_with`, and
  `gather_inputs_with` carry it, so a caller widens (or narrows) the
  windows past a seam a record's error names.

- `LinePattern` — a pattern a `locate` walk tests against each line. <!-- rq-fefddb16 -->
  - `Contains(&str)` — the line contains the substring.
  - `TrimmedEquals(&str)` — the whitespace-trimmed line equals the text.
  - `TrimmedStartsWith(&str)` — the leading-trimmed line starts with the text.
  - `Where(&dyn Fn(&str) -> bool)` — a structural predicate for signatures
    no literal text expresses (an ASCII-art banner, a column-shaped row).
    Its test allocates nothing before admitting a candidate line
    (`Performance Constraints`).

- `LineMatches<'w>` — the located lines of one `locate` call, borrowed from <!-- rq-c6dcf9fc -->
  the windows. `first(pattern)`, `last(pattern)`, `after_first(pattern)`,
  and `after_last(pattern)` take the pattern's index and answer exactly as
  the matching single-pattern method of `FileWindows` would.
  `first_in_head(pattern)` answers head-scoped queries — the "the route
  echo lives at the top of the job" rule of `rqm/parsers/gaussian.md` —
  returning the first matching line of the head window or `None` when no
  head line matches; a match confined to the tail is not a match in the
  head.
  `last_in_tail(pattern)` answers tail-scoped queries instead — the "the
  tail carries" status rules of the per-format parser documents — returning
  the last matching line of the tail window (its line index included, so
  several markers can be ordered without another walk) or `None` when no
  tail line matches; a match confined to the head is not a match in the
  tail.

- `TailLine<'w>` — the pair `last_in_tail` returns: `line` (the matched <!-- rq-6e71ffd1 -->
  text) and `index` (its zero-based line number within the tail window,
  usable to order two located markers).

- `Program` — a program token: a wrapper over a `&'static str` lowercase <!-- rq-0cc3c80f -->
  token — the value stamped into `Record::program`. Two `Program` values
  are equal exactly when their tokens are equal. The five shipped formats
  are published as associated constants — `Program::GAUSSIAN`,
  `Program::ORCA`, `Program::XTB`, `Program::MRCC`, `Program::XYZ`
  (tokens `"gaussian"`, `"orca"`, `"xtb"`, `"mrcc"`, `"xyz"`) — and
  `Program::new(&'static str) -> Program` is a `const fn` carrying any
  other token, so a parser defined elsewhere names its own format in its
  own module; no central vocabulary blocks a new format.
  - `as_str(&self) -> &str` — the lowercase token; `Display` writes it.

- `Parser` — the trait every output-format reader implements. The trait is <!-- rq-4dc770c8 -->
  `Send + Sync` so data-parallel collection can be added later without a
  redesign (no concurrency commitment in v1).
  - `program(&self) -> Program` — the format the parser handles.
  - `detect(&self, head: &str) -> Option<usize>` — the byte offset, into
    `head`, of the earliest line that carries this format's signature, or
    `None` when the head carries none. A parser claims the file exactly when
    it returns `Some`. A head without the signature is rejected at bounded
    cost — a constant number of whole-head substring searches, or a line
    walk whose per-line test is allocation-free and bounded by a small
    number of substring scans, with a leading-bytes reject where the
    signature's shape allows one (*Performance Constraints*).
  - `parse(&self, windows: &FileWindows) -> Record` — turns the windows into
    exactly one record. A parser never panics on any content; content-level
    failures are returned as records with status `failed` or `truncated` and
    a human-readable `error`, never as `Err`.

- `ParserRegistry` — the ordered set of parsers. <!-- rq-b28b6e69 -->
  - `new() -> Self` — an empty registry (the shipped state).
  - `register(&mut self, parser: Box<dyn Parser>)` — appends a parser;
    registration order breaks signature ties (equal signature offsets).
  - `parse_windows(&self, windows: &FileWindows) -> Record` — the core
    entry: detection on the head window (the earliest-signature parser
    wins, ties by registration order), then `parse` on the selected
    parser, then the record-assembly rules below. Callers reading from
    streams combine `FileWindows::from_stream` with this entry.
  - `parse_file(&self, path: &Path) -> Result<Record, ParseError>` — reads
    the file with the default window sizes (seekable), then behaves as
    `parse_windows`, stamping `name` with the file stem and `source_path`
    with the file's canonical absolute path (verbatim as given when
    canonicalization fails). Filesystem-level failures — a missing file, an
    unreadable file, a directory — return `Err`; every content-level
    outcome is `Ok` with one record.

- `ParserRegistry::parse_file_with(&self, path: &Path, budget: &WindowBudget) -> Result<Record, ParseError>` — <!-- rq-362c1c7f -->
  `parse_file` with an explicit window budget: the same read (seekable),
  detection, and assembly with the budget's window sizes; the API behind
  the CLI's `--head-bytes`/`--tail-bytes` flags. `parse_file` is this
  function with the default budget.

- `ParseError` — the error type of `parse_file`, with the single variant <!-- rq-814d8aa2 -->
  `Io { kind: std::io::ErrorKind, reason: String }` — a filesystem
  operation failed. `kind` is the underlying failure's kind (`NotFound`,
  `PermissionDenied`, …) so callers branch on the cause without parsing
  text; `reason` is the human-readable message. Mirrors `DbError::Io` in
  `rqm/database.md`.

### Shared Extractors <!-- rq-d59dac72 -->

Helpers shared by every per-format parser, in the framework's extract
module:

- `parse_fortran_f64(field: &str) -> Option<f64>` — parses a decimal number <!-- rq-39759c8a -->
  accepting `e`, `E`, `d`, and `D` exponents (Fortran-style output, e.g.
  `1.2345678D-05`). Returns `None` for non-numeric or non-finite fields.

- `element_symbol(z: u32) -> Option<String>` — maps an atomic number <!-- rq-295538b5 -->
  1–118 to its title-case element symbol (`1` → `"H"`, `17` → `"Cl"`,
  `118` → `"Og"`); `None` outside that range.

- `first_line_offset(head: &str, predicate) -> Option<usize>` — the public <!-- rq-166e917c -->
  home of the detection offset: the byte offset into `head` of the earliest
  line satisfying `predicate`, or `None` when no visible line does. A line is
  a `'\n'`-delimited span with its trailing `'\n'` withheld from the
  predicate, so every parser measures its signature offset the same way. The
  banner-signature parsers call it with their banner predicate, which is what
  makes the registry's earliest-offset routing meaningful; it is public so an
  external parser built through `Program::new` locates its offset identically
  rather than hand-rolling a line walk.

- `read_geometry_table(lines, spec: &TableSpec) -> Option<Geometry>` — reads <!-- rq-0a70b97d -->
  a columnar geometry block (the xyz-style table shared by the plain-xyz,
  Gaussian, ORCA, and MRCC layouts) from the lines that follow the block's
  header, returning the geometry in the record's canonical units.
  - `TableSpec` carries: `symbol_column: SymbolColumn` — `Symbol(usize)`
    for a column of element symbols (case-insensitive, normalized to title
    case) or `AtomicNumber(usize)` for a column of atomic numbers mapped
    via `element_symbol`; `coordinate_columns: [usize; 3]` — the x, y, z
    column indices; `units: LengthUnits` — `Angstrom` or `Bohr`; and
    `expected_rows: Option<usize>`.
  - A row is the line split on whitespace; it conforms when every
    referenced column exists, every referenced numeric field parses via
    `parse_fortran_f64`, and the symbol field is a known element. Columns
    beyond the referenced ones are ignored. The first non-conforming line
    ends the table.
  - `read_geometry_table` returns `None` when the table holds zero rows,
    when `expected_rows` is set and the row count found differs from it,
    or when the symbol column holds an unknown element. A table cut by a
    window boundary therefore yields `None`, never a short geometry — the
    seam-honesty rule.
  - `LengthUnits::Bohr` coordinates are converted to Ångström with the
    constant `BOHR_TO_ANGSTROM = 0.529177210903` (CODATA 2018), defined
    once in the extract module.

- `vibrational_modes(values: &[f64]) -> Vec<f64>` — keeps the genuine <!-- rq-c37111b1 -->
  vibrational modes of a parsed frequency list and drops the translational
  and rotational trivial modes: an entry survives exactly when its absolute
  value exceeds `TRIVIAL_MODE_CUTOFF = 0.01` cm⁻¹, in input order and with
  the sign preserved. Programs print the trivial modes of a full Cartesian
  Hessian as `0.00` or `-0.00` — magnitudes below 0.005, so they fall to
  the cut whatever their printed sign or position; a genuine soft mode
  (a weakly bound or floppy system legitimately printing, say, 0.5) or
  imaginary mode is kept, and — because programs such as xtb sort
  eigenvalues ascending and print imaginary modes before the zeros — no
  ordering assumption is needed. The cut is a printed-zero recognizer,
  deliberately distinct from the derivation's 1.0 cm⁻¹ participation test
  (`rqm/thermo.md`): the record archives every mode above 0.01 and the
  derivation re-filters at its own boundary, so a soft mode below 1.0 is
  stored and simply does not participate. The Gaussian, ORCA, and XTB
  parsers pass their parsed frequency lists through it before storing, so
  the record's vibrational-mode convention holds program-independently
  (`rqm/parsers/gaussian.md`, `rqm/parsers/orca.md`,
  `rqm/parsers/xtb.md`).

### Crate Layout <!-- rq-15f15563 -->

- The framework lives in `src/parser/`: `mod.rs` (the `Parser` trait,
  `Program`, `ParserRegistry`, `ParseError`, and the record-assembly
  rules), `windows.rs` (`FileWindows` and the window reader), and
  `extract.rs` (the shared extractors). The library root re-exports the
  public types.
- `tests/parser.rs` exercises the framework through small in-test parser <!-- rq-4da7b314 -->
  implementations and synthetic window content; boundary scenarios use
  explicit small window sizes. The per-format conformance suites of
  milestone 3 build on this module.
- No new dependencies: the framework uses only `std` (plus the existing
  `serde`/`serde_json` records); `Cargo.toml` is unchanged.

## Window Contract <!-- rq-6f803572 -->

- The default window sizes are `DEFAULT_HEAD_BYTES = 256 KiB` and
  `DEFAULT_TAIL_BYTES = 4 MiB`, owned by the framework and overridable per
  call. They fully cover every file of the current fixture corpus (largest:
  3.1 MiB) and leave an order of magnitude over the ~540 KiB final-job span
  of the multi-step Gaussian fixture; the fixture corpus keeps them honest
  as it grows.
- The CLI exposes the budget as `--head-bytes <N>` and `--tail-bytes <N>`
  (bytes, defaulting to the constants; either value below 1 is a usage
  error). They govern only build-mode reads; merge mode reads record
  databases and ignores them. A `WindowBudget` carries the pair through
  `parse_file_with`/`collect_inputs_with`/`gather_inputs_with`, so a user
  can widen the windows past a seam that a record's error names.
- A file no larger than `head_bytes + tail_bytes` is read whole once:
  covered, with `head` and `tail` both the entire content.
- A larger file yields disjoint windows: the head is the first
  `head_bytes` bytes trimmed back to its last complete line (at most one
  partial trailing line is dropped); the tail is the last `tail_bytes`
  bytes advanced to its first line start (at most one partial leading line
  is dropped). The file's final line is kept in the tail even when it
  lacks a trailing newline — the evidence of a truncated run.
- Window cut points respect UTF-8 character boundaries, and invalid UTF-8
  anywhere in a window becomes U+FFFD: parsing never aborts over stray
  bytes from crashed programs, and binary junk simply fails detection.
- Both constructors bound memory to the window sizes; only `from_stream`
  reads the whole stream (pipes cannot seek), and it retains no more than
  the two windows.
- Detection sees only the head window; parsing sees head and tail; the
  elided middle is visible to neither. Parsers must rely on no content
  outside the windows — that is what keeps the framework honest for files
  of any size.
- A `locate` call reads each visible window a constant number of times:
  whole-window byte scans (sharing one newline table) locate every literal
  pattern, and one shared line walk tests the structural (`Where`)
  patterns — never one walk per marker. Each pattern is evaluated against
  each line at most once. A covered window is read as one pass set: head
  and tail hold the same text, so the literal scans, the shared newline
  table, and the structural walk each cover the content once together and
  answer the first and the last questions from the same pass.

## Performance Constraints <!-- rq-15b1763d -->

Collection is parse-bound: a directory of thousands of outputs must reach
the database in seconds, and the framework keeps the per-record cost
proportional to one walk over the windows.

- One identify pass per record. A per-format parse locates every marker it
  needs — energy, geometry header, frequency section, population and dipole
  sections, identity fields, and its status markers — in a single `locate`
  call, then reads values from the located lines and the bounded line spans
  that follow them. A marker never costs a fresh walk or scan of a window.
  One bounded exception: MRCC's echoed input deck is extracted from the
  head window in its own scan (the deck runs from the `Input file:` marker
  to the first stop-key line, a span `locate`'s line-at-a-time answers
  cannot delimit), so MRCC scans the head once more — a marker search and
  a line loop over at most `head_bytes`; no other parser re-walks a
  window.
- Whole-window byte searches are cheap and permitted. `str::contains` on a
  window is one memory-bandwidth pass; it serves as the signature prefilter
  in detection and as whole-window status tests.
- Detection rejects cheaply. A parser whose signature is absent from the
  head rejects with a constant number of whole-head `contains` searches;
  where the signature is not literal text (the ORCA banner), a single line
  walk is permitted only when its per-line test is allocation-free and its
  per-line cost is bounded by a small number of substring scans — leading-
  bytes rejects are the rule and should be kept wherever the signature's
  shape (line start or fixed leading fields) allows one; a predicate that
  must `contains` the whole line for genuine candidates (a centered
  banner) is the accepted exception. Parsing one file never walks the head
  once per registered parser with allocation-heavy per-line predicates.
- Per-line tests allocate nothing before admitting a candidate. A predicate
  or `Where` pattern runs byte-level shape tests first and splits the line
  into fields only after the line is a candidate.
- The resulting budget: every window byte is touched a small constant
  number of times — the read, one identify pass, bounded local extraction —
  independent of how many markers the format carries, so single-core
  throughput approaches the whole-window byte-search rate. On commodity
  hardware the parse path of a 3 MiB covered output runs in single-digit
  milliseconds.

## Record Assembly <!-- rq-e4a815f2 -->

`parse_windows` and `parse_file` produce exactly one record per file:

- The parser fills the observables, `status`, and `error`; the framework
  then stamps `program` with the matched parser's token (authoritative),
  and — for `parse_file` only — `name` with the file stem and
  `source_path` with the file's canonical absolute path: symlinks and
  `.`/`..` segments resolved against the filesystem, and on Windows the
  `\\?\` verbatim prefix stripped, so the record locates its source
  independently of the invocation's working directory and argument
  spelling. A path that cannot be canonicalized keeps `source_path` as
  given, verbatim; stamping never fails a parse. `parse_windows` leaves
  `name` and `source_path` absent.
- A non-empty file whose head matches no registered parser yields a record
  with status `failed`, an error mentioning the unrecognized output format,
  and no `program`. An empty file yields a record with status `failed` and
  an error mentioning that the file is empty. Both carry `name` and
  `source_path` when a path was given. This failed-without-program signature
  is exposed as `Record::is_unrecognized` (`rqm/record.md`) so consumers can
  distinguish "no parser read this file" from a real failed job.
- A parser-returned record that fails `Record::validate` is replaced by a
  record with status `failed` and an error mentioning the invalid record
  and the violated invariant; `name`, `source_path`, and `program` are
  preserved.
- The parser's own `status` and `error` are otherwise carried through
  verbatim.
- When several registered parsers claim the same head, the one whose
  signature offset is smallest wins; equal offsets break by registration
  order. A wrapper's embedded banner therefore never routes a file away from
  the program whose banner opens it. The structural plain-xyz signature is
  the file's first line (an atom-count line), so a head that opens xyz-shaped
  — count, comment, then a row — is read as plain xyz even when a program
  banner appears further down the head; a banner line is never an all-digit
  count line, so the two signatures can never tie at offset 0. With an empty
  registry every file is unrecognized.
- The file name and extension never influence detection or parsing.

## Out of Scope <!-- rq-a0dfc43a -->

- The per-format parsers — signatures, extraction rules, granularity of
  "final job", status classification — one requirements document each,
  milestone 3 (plain xyz first), together with the conformance suite and
  fixture manifest over `tests/fixtures/`.
- A registry constructor assembling the standard set of format parsers —
  arrives with the milestone-3 parsers.
- The collection command line (`CJKQC <files>`, database append) —
  `rqm/collection.md`. Stdin collection is not part of that surface yet; the
  `from_stream` reader stands ready for it.
- Parallel parsing: the trait is `Send + Sync`, but v1 makes no concurrency
  commitment.
- Transparent decompression (`.gz` and friends); files are read as they
  are.
- Concatenated multi-program files: a file formed by appending one program's
  whole output onto another's (an xtb log followed by an ORCA log in one file)
  is not a supported shape. Detection picks the earliest banner, while the
  "final job" rule would describe the last program — the two disagree for such
  concatenations, which are neither a real program's output nor a wrapper
  case, and are out of scope.
- Any observable beyond the vocabulary of `rqm/record.md`, and any new
  dependency.

## Gherkin Scenarios <!-- rq-6702f697 -->

```gherkin
Feature: Format detection and parser framework

  # --- Detection and routing ---

  @rq-a7a34dab
  Scenario: An unregistered format yields a failed record
    Given the registry contains no parsers
    And a file "job.log" containing arbitrary text
    When parse_file is called on "job.log"
    Then Ok is returned with exactly one record
    And the record has status failed
    And the record's error mentions the unrecognized output format
    And the record has name "job" and source_path equal to the canonical absolute path of "job.log"
    And the record has no program

  @rq-033ff671
  Scenario: A file whose head matches a registered parser is parsed by it
    Given the registry contains a parser for the program token "xtb"
    that claims heads containing the line "MAGIC" and returns a record
    with electronic_energy -19.3
    And a file "run.out" whose first line is "MAGIC"
    When parse_file is called on "run.out"
    Then the record has program "xtb"
    And the record has name "run" and source_path equal to the canonical absolute path of "run.out"
    And the record has electronic_energy -19.3

  @rq-761f2b95
  Scenario: Detection ignores the file name and extension
    Given the registry contains a parser that claims heads containing "MAGIC"
    And three files "a.log", "a.xyz", and "a" with identical content starting "MAGIC"
    When each file is parsed
    Then all three are routed to the same parser
    And the three records differ only in name and source_path

  @rq-3562f71a
  Scenario: Registration order breaks a same-offset signature tie
    Given the registry contains a "gaussian" parser and an "orca" parser,
    both claiming heads containing "MAGIC", registered in that order
    And a file whose first line is "MAGIC"
    When the file is parsed
    Then the record has program "gaussian"

  @rq-86ee6f34
  Scenario: The earliest signature wins when a wrapper embeds a foreign banner
    Given the registry contains an "xtb" parser claiming heads containing "XTB BANNER" and an "orca" parser claiming heads containing "ORCA BANNER", registered xtb-then-orca
    And a file whose head opens with a line "ORCA BANNER" and carries a later line "XTB BANNER"
    When the file is parsed
    Then the record has program "orca"

  @rq-b7081115
  Scenario: A head that opens xyz-shaped wins over a banner further down
    Given the standard registry
    And a file whose head begins with an atom-count line, a comment line, and a row, and carries a program banner on a later line
    When the file is parsed
    Then the record has program "xyz"

  @rq-8209f22b
  Scenario: A plain xyz head that names a program in its comment stays xyz
    Given the standard registry
    And a file whose head is a one-atom count line, a comment line mentioning "Gaussian 16", and a single row
    When the file is parsed
    Then the record has program "xyz"

  @rq-1fd7e03d
  Scenario: A signature beyond the head window is not detected
    Given the registry contains a parser that claims heads containing "MAGIC"
    And the windows of a large file read with head 3 lines and tail 3 lines
    where the line "MAGIC" lies only in the elided middle
    When parse_windows is called on the windows
    Then the record has status failed
    And the record's error mentions the unrecognized output format

  @rq-ffaf4e5b
  Scenario: A signature anywhere in a covered file is detected
    Given the registry contains a parser that claims heads containing "MAGIC"
    And a file smaller than the window sizes whose line "MAGIC" lies deep inside
    When the file is parsed
    Then the file is routed to that parser

  # --- Window reading ---

  @rq-5ab5050d
  Scenario: A file within the window sizes is read whole
    Given a file of 10 lines read with head 16 lines and tail 16 lines
    When the windows are constructed
    Then covered is true
    And head and tail are both the entire content

  @rq-3172eb32
  Scenario: A large file yields disjoint line-aligned windows
    Given a file of 20 lines read with head 4 lines and tail 4 lines
    When the windows are constructed
    Then covered is false
    And head holds exactly the first 4 lines
    And tail holds exactly the last 4 lines
    And head and tail share no line

  @rq-72479b94
  Scenario: A caller-supplied budget replaces the default window sizes
    Given the registry contains a parser that claims heads containing "MAGIC" and records the tail's line count
    And a file of 20 lines whose first line is "MAGIC"
    When the file is parsed with the default budget and then with a budget of head 4 lines and tail 4 lines
    Then the default-budget record counts 20 tail lines, for the file is covered
    And the budgeted record counts 4 tail lines

  @rq-a37342d6
  Scenario: The tail keeps the file's final unterminated line
    Given a file larger than the windows whose last line lacks a trailing newline
    When the windows are constructed
    Then the tail ends with exactly that final line, newline or not

  @rq-5996dff9
  Scenario: Partial lines at the window seams are dropped
    Given a file of one-letter lines "abcdefghij" read with head 3 bytes
    and tail 3 bytes
    When the windows are constructed
    Then the head contains only whole lines from the start of the file
    And the tail contains only whole lines from the end of the file
    And no window contains a fragment of a line

  @rq-e8bb37c3
  Scenario: Invalid UTF-8 bytes become replacement characters
    Given a file whose head contains bytes that are not valid UTF-8
    When the windows are constructed
    Then Ok is returned
    And the head contains U+FFFD replacement characters

  @rq-0bd419c3
  Scenario: Streamed and seekable reads produce identical windows
    Given the same content read once through from_seekable and once
    through from_stream, with the same window sizes
    When the content fits the windows
    Then both constructions yield identical head, tail, and covered
    When the content exceeds the windows
    Then both constructions again yield identical head, tail, and covered

  # --- Record assembly ---

  @rq-662d12db
  Scenario: parse_windows stamps the program but not the identity
    Given the registry contains a parser for the program token "xyz"
    that claims heads containing "MAGIC" and returns a record
    When parse_windows is called on windows whose head starts "MAGIC"
    Then the record has program "xyz"
    And the record has no name and no source_path

  @rq-c5a229be
  Scenario: A relative path is stamped as the canonical absolute path
    Given a file "job.log" in the current working directory
    When parse_file is called on "./elsewhere/../job.log"
    Then the record's source_path is the file's canonical absolute path
    And source_path contains no "." or ".." segments

  @rq-75f85412
  Scenario: A symlinked path is stamped with the resolved target
    Given a file "real.log" in the current working directory
    And a symlink "link.log" pointing at "real.log"
    When parse_file is called on "link.log"
    Then the record's source_path equals the canonical absolute path of "real.log"

  @rq-f7dd8d6f
  Scenario: An uncanonicalizable path falls back to the path as given
    Given a source path whose canonicalization fails
    When the source_path stamping is applied to "job.log"
    Then the stamped source_path is "job.log", verbatim
    And stamping fails nothing

  @rq-219164a5
  Scenario: The parser's status and error are preserved verbatim
    Given the registry contains a parser that returns a record with
    status failed and error "SCF did not converge"
    When a matching file is parsed
    Then the record has status failed and error "SCF did not converge"
    And the framework adds no further error text

  @rq-dce37e6e
  Scenario: A parser record that fails validation is replaced
    Given the registry contains a parser that returns a record whose
    electronic_energy is NaN
    And a file "bad.log" matching that parser
    When parse_file is called on "bad.log"
    Then the record has status failed
    And the record's error mentions the invalid record
    And the record has name "bad", source_path equal to the canonical absolute path of "bad.log", and the parser's program

  @rq-58b86d07
  Scenario: An empty file yields a failed record
    Given the registry contains one parser claiming any head
    And an empty file "empty.log"
    When parse_file is called on "empty.log"
    Then the record has status failed
    And the record's error mentions that the file is empty
    And the record has name "empty" and source_path equal to the canonical absolute path of "empty.log"

  @rq-447f650f
  Scenario: A missing file is an Io error
    Given no file exists at "no/such.log"
    When parse_file is called on "no/such.log"
    Then Err(ParseError::Io(_)) is returned
    And the Io error's kind is NotFound

  @rq-86740b39
  Scenario: A parser from outside the shipped set carries its own token
    Given the registry contains a parser whose program token is "turbomole",
    claiming heads containing "MAGIC"
    And a file whose first line is "MAGIC"
    When the file is parsed
    Then the record has program "turbomole"

  @rq-f87aad94
  Scenario: A directory is an Io error
    Given a directory exists at "somedir"
    When parse_file is called on "somedir"
    Then Err(ParseError::Io(_)) is returned

  # --- Line search across windows ---

  @rq-f4a74fc9
  Scenario: last_line_where prefers the match in the tail
    Given uncovered windows where a matching line appears both in the
    head and in the tail
    When last_line_where is called with the predicate
    Then the line from the tail is returned

  @rq-4aa069d5
  Scenario: last_line_where falls back to the head
    Given uncovered windows where a matching line appears only in the head
    When last_line_where is called with the predicate
    Then the line from the head is returned

  @rq-eb75bf8e
  Scenario: first_line_where prefers the match in the head
    Given uncovered windows where a matching line appears both in the
    head and in the tail
    When first_line_where is called with the predicate
    Then the line from the head is returned

  @rq-fb368603
  Scenario: first_line_where falls back to the tail
    Given uncovered windows where a matching line appears only in the tail
    When first_line_where is called with the predicate
    Then the line from the tail is returned

  @rq-bdb74057
  Scenario: Lines in the elided middle are invisible
    Given uncovered windows where a matching line appears only in the
    elided middle
    When first_line_where and last_line_where are called with the predicate
    Then both return None

  @rq-be817932
  Scenario: after_last_line_where stops at the window boundary
    Given uncovered windows whose last matching line lies in the head,
    followed by more head lines and then the elided middle
    When after_last_line_where is called with the predicate
    Then it yields exactly the following visible head lines
    And it yields no line from the middle or the tail

  # --- Multi-marker locate ---

  @rq-a1bbe040
  Scenario: locate answers every pattern from one call
    Given a covered window whose lines are "A x", "B y", "A z"
    When locate is called with the patterns Contains("A") and Contains("y")
    Then first of Contains("A") is "A x" and last of Contains("A") is "A z"
    And first and last of Contains("y") are both "B y"

  @rq-e03d12a0
  Scenario: locate prefers the head for first and the tail for last
    Given uncovered windows in which "MARK" appears in both the head and the tail
    When locate is called with the pattern Contains("MARK")
    Then first returns the earliest head match
    And last returns the latest tail match

  @rq-1d792ead
  Scenario: after_last stops at the window boundary like its single-pattern twin
    Given uncovered windows whose last "MARK" match lies in the head,
    followed by more head lines and then the elided middle
    When after_last is taken for the pattern
    Then it yields exactly the following visible head lines
    And it yields no line from the middle or the tail

  @rq-97b6a68f
  Scenario: a covered window is walked once per locate call
    Given a covered window of N lines
    When locate is called with two Where matchers that count their invocations
    Then each matcher is invoked exactly N times

  @rq-7525d2fc
  Scenario: a pattern absent from the windows matches nothing
    Given windows and a pattern no visible line carries
    When locate is called
    Then first and last are None
    And after_first and after_last yield no lines

  @rq-abc3dfa6
  Scenario: TrimmedEquals and TrimmedStartsWith match on trimmed lines
    Given a covered window holding the lines "   VIBRATIONAL FREQUENCIES   " and "  Mulliken charges:"
    When locate is called with TrimmedEquals("VIBRATIONAL FREQUENCIES")
    Then that first line is the first and last match
    When locate is called with TrimmedStartsWith("Mulliken")
    Then the charges line is the first and last match

  @rq-75876d4c
  Scenario: last_in_tail sees only tail matches and reports their order
    Given uncovered windows in which "END" matches a head line and two tail lines
    And "HALT" matches only a head line
    When locate is called with the patterns Contains("END") and Contains("HALT")
    Then last_in_tail of Contains("END") is the later tail line with a larger index than the earlier tail match
    And last_in_tail of Contains("HALT") is None although last is Some

  # --- Geometry table extraction ---

  @rq-aeb8b3a8
  Scenario: A symbol table in Ångström parses to a Geometry
    Given table lines with a symbol column and three coordinate columns,
    in Ångström, with expected_rows matching
    When read_geometry_table is called on them
    Then a Geometry is returned with the symbols in title case
    And the coordinates equal the table values unchanged

  @rq-383eea52
  Scenario: An atomic-number column maps to element symbols
    Given table lines whose symbol column holds atomic numbers
    When read_geometry_table is called on them
    Then the Geometry's symbols are the title-case element symbols
    for those numbers

  @rq-18bc1945
  Scenario: Bohr coordinates are converted to Ångström
    Given table lines in bohr units
    When read_geometry_table is called on them
    Then the Geometry's coordinates equal the table values multiplied
    by BOHR_TO_ANGSTROM

  @rq-d737472c
  Scenario: A table ends at its first non-row line
    Given table lines followed by a blank line and unrelated text,
    with expected_rows satisfied by the preceding rows
    When read_geometry_table is called on them
    Then a Geometry is returned with exactly those rows

  @rq-582aea7a
  Scenario: A row count below expected_rows yields None
    Given table lines with fewer rows than expected_rows — because the
    table is cut by a window boundary, truncated by the file's end, or
    simply short — including zero rows
    When read_geometry_table is called on them
    Then None is returned, never a short geometry

  # --- Number and element helpers ---

  @rq-ed0686aa
  Scenario: Fortran float parsing accepts E, e, D, and d exponents
    When parse_fortran_f64 is called on "1.2345678D-05", "1.2345678d-05",
    "1.2345678E-05", "-1.0D+03", and "3.14"
    Then it returns 1.2345678e-05, 1.2345678e-05, 1.2345678e-05, -1000.0,
    and 3.14 respectively

  @rq-017a3a9c
  Scenario: Fortran float parsing rejects non-numeric and non-finite fields
    When parse_fortran_f64 is called on "abc", "", "NaN", and "Inf"
    Then it returns None for every one of them

  @rq-b9209565
  Scenario: Atomic numbers map to title-case element symbols
    When element_symbol is called on 1, 17, and 118
    Then it returns "H", "Cl", and "Og"

  @rq-39966431
  Scenario: Atomic numbers outside 1 to 118 map to nothing
    When element_symbol is called on 0 and 119
    Then it returns None for both

  # --- Vibrational mode filtering ---

  @rq-b0fc2983
  Scenario: Near-zero trivial modes are dropped and real modes are kept
    When vibrational_modes is called on [0.0, -0.0, 0.0, 0.5, -57.3, 3831.6]
    Then it returns [0.5, -57.3, 3831.6]

  @rq-caa759ba
  Scenario: A magnitude at or below the cutoff never survives
    When vibrational_modes is called on [0.01, -0.01, 0.004, -0.004]
    Then it returns an empty list

  @rq-27c965f2
  Scenario: A list of only trivial modes filters to empty
    When vibrational_modes is called on six printed zeros including -0.0
    Then it returns an empty list

  # --- Window honesty over the fixture corpus ---

  @rq-f61c0532
  Scenario: Default windows cover every file of the fixture corpus
    Given every output file under tests/fixtures
    When the windows are read with the default sizes
    Then every file is covered
```
