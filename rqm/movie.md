# Feature: Movie Export (`--movie`) <!-- rq-6fb174a0 -->

The `--movie` flag writes the run's final record set — exactly the records
the run prints and the formation sections evaluate (`rqm/filtering.md`,
`rqm/formation.md`) — as a multi-frame xyz file: one frame per record that
carries a structure, in run order. The file is the viewing artifact of a
filtering run: its frames are the pipeline survivors, frame for frame the
rows a `--formation` section or the property table describes. This is the
movie export of the export pillar (`rqm/ARCHITECTURE.md`); the geometry
comes straight from the stored record (`rqm/record.md`).

## Run Semantics <!-- rq-fc05e5df -->

- `--movie` is a boolean flag: it takes no value and writes the fixed file
  name `movie.xyz` in the current working directory. It prints no table and
  no section, and it counts as a print flag for the visible-outcome rule of
  `rqm/filtering.md`: a run with `--movie` and neither `--out` nor another
  print or manipulation flag is valid, and like every print flag
  `--movie` manipulates nothing — a run that also carries manipulation
  flags switches to the pipeline semantics, a run that does not keeps
  print-only semantics.
- The movie's record set is the run's **final record set**: the pipeline
  survivors for a manipulation run, every loaded record for a print-only
  run — the same set the table prints and the formation and hydration
  sections evaluate (`rqm/filtering.md` Printing, `rqm/formation.md`,
  `rqm/population.md`).
- A frame is written for each record of that set whose status is `ok` and
  which carries a `geometry` (`rqm/record.md`), in the set's order — the
  same order the table prints, `--sort`, `--shuffle`, and `--select`
  included. A record of the set that is not `ok` (a print-only run's
  failed or truncated records) or lacks a geometry contributes no frame;
  when any record of the set is skipped the run reports on stderr
  `excluded <k> records from movie.xyz (no geometry or not ok)`. In a
  manipulation run the non-ok records already leave before the pipeline's
  first stage with their own stderr line (`rqm/filtering.md`), so there
  the movie's skipped count is the ok records without a geometry.
- The thermochemistry settings and `--bavg` change nothing: frames are per
  record, never collapsed into ensembles, and no derived value enters the
  movie — the comment line carries stored observables only.

## Frame Format <!-- rq-315762eb -->

Each frame is standard xyz, written from the record's stored geometry
(`rqm/record.md`):

```text
<natoms>
<comment>
<symbol> <x> <y> <z>
…
```

- The atom-count line is the geometry's atom count as a plain decimal
  integer.
- The comment line identifies the frame: the record's `name` (`-` when the
  record has none), then, when the record stores an `electronic_energy`, a
  space and `E=` followed by that energy in hartree with 10 decimals — the
  `el` column's format (`rqm/filtering.md` Printing). Without an
  electronic energy the `E=` part is omitted; a record with neither a name
  nor an electronic energy has the comment `-`. Line breaks in the name
  are replaced with spaces so the comment stays one line.
- Each atom line is the element symbol as stored (title case), then the
  three coordinates in Å with 6 decimals each, single-space separated:
  `O 0.000000 0.000000 0.117000`.

## File Handling <!-- rq-be312c87 -->

- `movie.xyz` is replaced atomically by the shared atomic writer
  (`src/atomic.rs`, also used by the database writer
  `rqm/database.md`): the content lands in a temporary file in the
  target's parent directory, is flushed and synced, then renamed over
  the target, so a failed write leaves any previous `movie.xyz` intact.
  Zero frames produce an empty file, mirroring an empty `--out`
  database.
- Replacing an existing `movie.xyz` is confirmed exactly like `--out`
  (`rqm/collection.md` Overwrite Confirmation): the question names the
  file, `--fout` forces it, and a no answer or a non-interactive run
  without `--fout` aborts before the file is written.
- When the file is written the run prints `wrote <k> frames into
  movie.xyz` as the last stdout line: after the table and sections, and
  after the `filtered <n> records into <out>` or collection summary line
  when `--out` is also given (`rqm/filtering.md`). A run with `--movie`
  and no `--out` has the line as its only summary.
- The `--out` write precedes the movie write; when the `--out` write
  fails, the run ends before the movie is written. A failed movie write
  prints its error and fails the run (exit 1) without the summary line,
  like a failed `--out` write.

## Feature API <!-- rq-1c79c0e4 -->

### Types <!-- rq-e63e8caf -->

- `MovieError` — the write failure: `Io(String)` carries a description of <!-- rq-e5292dca -->
  the failed filesystem operation, worded like the database writer's
  `DbError` (`rqm/database.md`).

### Functions <!-- rq-09ff296c -->

- `movie_frames(records: &[Record]) -> (Vec<&Record>, usize)` — the <!-- rq-78af10b3 -->
  frame-eligible subset of the given record set (status `ok`, geometry
  present) in input order, with the count of skipped records for the
  stderr note.
- `frame_text(record: &Record) -> String` — one frame's full text: the <!-- rq-4418be27 -->
  count line, the comment line, and the atom lines, each ended by a
  newline. A record without a geometry, or whose geometry's coordinate
  count disagrees with its atom count, yields an empty string.
- `write_movie_xyz(path: &Path, frames: &[&Record]) -> Result<(), MovieError>` <!-- rq-e5ec2a43 -->
  — the atomic replacement above; an empty slice writes an empty file.

### Crate Layout <!-- rq-8b51b16e -->

- The movie export lives in `src/movie.rs`; the library root re-exports
  `movie_frames`, `frame_text`, `write_movie_xyz`, and `MovieError`. No
  new dependencies.
- The CLI adds `--movie`, counts it among the print flags, gathers the
  final record set, prints the stderr skip note and the summary line, and
  applies the overwrite confirmation (`rqm/collection.md`). `tests/movie.rs` tests `movie_frames` and
  `frame_text` (eligibility, order, comment and atom-line formats) and
  `write_movie_xyz` (empty frames, replacement); `tests/cli.rs` covers the
  flag surface: run-order frames over filtered and print-only runs, the
  skip note, the summary line, the overwrite confirmation with `--fout`, and
  the visible-outcome rule.

## Out of Scope <!-- rq-a4899c02 -->

- `--xyz` (single-frame or per-structure xyz export) and spreadsheet
  export: they stay future work (`rqm/filtering.md` Out of Scope).
- A configurable output path or file name: the file is always `movie.xyz`
  in the working directory.
- Frames beyond the stored geometry: no animation of optimization steps or
  trajectory frames (a record holds one structure), and no decoration of
  the atom lines (charges, dipoles).
- Energies other than the stored electronic energy in the comment line:
  derived Gibbs values and corrections never appear there.
- Coordinate recentering, rotation, or unit conversion: geometries are
  written exactly as stored, in Å.

## Gherkin Scenarios <!-- rq-0c629a87 -->

```gherkin
Feature: Movie export

  # --- Record set and order ---

  @rq-76d13d9c
  Scenario: The frames are the pipeline survivors in run order
    Given a database with 3 ok records of one composition
    When CJKQC is invoked with the database and "--cutr el 5 --movie" and no --out
    Then movie.xyz holds exactly the surviving records as frames, in the order the table would print them

  @rq-d72194bc
  Scenario: The frame order follows the sort
    Given a database with 3 ok records whose energies differ
    When CJKQC is invoked with the database and "--sort el --reverse --movie" and no --out
    Then the movie frames follow the sorted order, matching the table's row order

  @rq-86a3deda
  Scenario: A print-only run movies every loaded ok record
    Given a database with 2 ok records and 1 failed record
    When CJKQC is invoked with the database and "--movie" and no --out
    Then movie.xyz holds the 2 ok records as frames in file order
    And no database file is written

  # --- Frames ---

  @rq-9e89b02f
  Scenario: The comment line names the record and its electronic energy
    Given an ok record named "1dma" with electronic energy -38.51
    When frame_text is called on it
    Then the comment line is "1dma E=-38.5100000000"

  @rq-fd2671de
  Scenario: A record without a name has a dash name
    Given an ok record without a name with electronic energy -1.0
    When frame_text is called on it
    Then the comment line is "- E=-1.0000000000"

  @rq-9b8707e4
  Scenario: A record without an electronic energy omits the E= part
    Given an ok record named "1dma" without an electronic energy
    When frame_text is called on it
    Then the comment line is "1dma"

  @rq-a4493f29
  Scenario: A record with neither name nor energy comments a dash
    Given an ok record without a name and without an electronic energy
    When frame_text is called on it
    Then the comment line is "-"

  @rq-545321a5
  Scenario: frame_text yields an empty string for a mismatched geometry
    Given a record whose geometry has 2 symbols and 3 coordinates
    When frame_text is called on it
    Then the result is the empty string

  @rq-79e2785e
  Scenario: a name's line breaks do not split the comment line
    Given a record with a geometry whose name contains a newline
    When frame_text is called on it
    Then the comment line is one line with the line breaks replaced by spaces
    And the frame has the atom-count line, the comment line, and one atom line per atom

  @rq-92d5adc8
  Scenario: Atom lines carry the stored geometry
    Given an ok record whose geometry is N at (0, 0, 0) and H at (0, 0, 1.01)
    When frame_text is called on it
    Then the count line is "2"
    And the atom lines are "N 0.000000 0.000000 0.000000" and "H 0.000000 0.000000 1.010000"

  # --- Skips ---

  @rq-21313a28
  Scenario: An ok record without a geometry is skipped and counted
    Given a database with an ok record with a geometry and an ok record without one
    When CJKQC is invoked with the database and "--movie" and no --out
    Then movie.xyz holds one frame
    And stderr contains "excluded 1 records from movie.xyz (no geometry or not ok)"

  @rq-d8fed97e
  Scenario: A print-only run skips a failed record with a geometry
    Given a database with an ok record with a geometry and a failed record with a geometry
    When CJKQC is invoked with the database and "--movie" and no --out
    Then movie.xyz holds only the ok record's frame
    And stderr contains "excluded 1 records from movie.xyz (no geometry or not ok)"

  @rq-b756a018
  Scenario: No skip note when every record becomes a frame
    Given a database with 2 ok records with geometries
    When CJKQC is invoked with the database and "--movie" and no --out
    Then stderr contains no excluded-records line for movie.xyz

  # --- File handling and CLI ---

  @rq-4aab0ef8
  Scenario: --movie alone is a visible outcome
    Given a database with one ok record with a geometry
    When CJKQC is invoked with the database and "--movie" and no --out and no print flag
    Then the exit code is 0
    And movie.xyz is written
    And stdout contains "wrote 1 frames into movie.xyz"

  @rq-d82889ab
  Scenario: --movie combines with --out
    Given a database with 3 ok records of one composition
    When CJKQC is invoked with the database and "--cutr el 5 --movie --out kept.jsonl"
    Then kept.jsonl contains the surviving records
    And stdout shows "filtered 2 records into kept.jsonl" and then "wrote 2 frames into movie.xyz"

  @rq-1e523f8f
  Scenario: Zero survivors write an empty movie
    Given a database with one ok record
    When CJKQC is invoked with the database and "--extract 9w --movie" and no --out
    Then movie.xyz is empty
    And stdout contains "wrote 0 frames into movie.xyz"

  @rq-146b84dd
  Scenario: Replacing movie.xyz follows the --out confirmation
    Given an existing file movie.xyz
    When CJKQC is invoked with the database and "--movie" and no --out with stdin piped, not a terminal
    Then the exit code is 1
    And movie.xyz is unchanged
    And stderr contains "error: movie.xyz exists; not interactive; use --fout to force"
    When the same run adds --fout
    Then the exit code is 0
    And movie.xyz is replaced and no question is asked

  @rq-d75cccd1
  Scenario: --add-correction with --movie uses pipeline semantics
    Given a database with ok and failed records and a correction database
    When CJKQC is invoked with the database, "--add-correction corr.jsonl --movie" and no --out
    Then movie.xyz holds the ok records' frames
```
