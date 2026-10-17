# Feature: Record Schema of Primary Observables <!-- rq-0ae95856 -->

Every quantum-chemistry job collected by CJKQC — successful, failed, or
truncated — is represented by one **record**: a single JSON object holding the
job's primary observables and nothing else. Records are the unit of storage: a
database is a sequence of records (see `rqm/database.md` for the file format
and the load, append, and merge operations). Derived quantities (zero-point
energies, enthalpies, free energies, …) are computed on demand from the
observables and never appear in a record ("store observables, derive everything
else", `rqm/ARCHITECTURE.md`).

## Feature API <!-- rq-4f9a9a29 -->

### Types <!-- rq-97a677ce -->

- `Record` — one QC job's primary observables. Serializable to one line of <!-- rq-43be8f79 -->
  compact JSON (via `serde`) and deserializable from any JSON object carrying
  the keys below, in any order.
  - `status: RecordStatus` — the job's outcome. Present on every written
    record; reads as `ok` when the key is absent.
  - `error: Option<String>` — human-readable explanation of a failed or
    truncated job; present and non-empty whenever status is `failed`.
  - `name: Option<String>` — the structure's identity: the stem of the
    source file name (e.g. `2am3sa-1conf` from `2am3sa-1conf.log`), carried
    verbatim; the key that matches the same structure across databases
    (single-point corrections, monomer references) and the input to
    composition resolution (milestone 4).
  - `source_path: Option<String>` — path of the QC output file the record was
    parsed from, verbatim as given to the tool.
  - `program: Option<String>` — lowercase program token (`"xtb"`, `"gaussian"`,
    `"orca"`, `"mrcc"`, `"xyz"`); the parser framework owns the exact
    vocabulary.
  - `program_version: Option<String>` — version string as printed by the
    program (e.g. `"Gaussian 16 Rev. B.01"`, `"6.7.1"`).
  - `method: Option<String>` — level of theory or Hamiltonian as reported by
    the program (e.g. `"GFN2-xTB"`, `"wB97X-D/def2-TZVP"`).
  - `electronic_energy: Option<f64>` — total electronic energy of the final
    structure, in hartree.
  - `charge: Option<i32>` — net charge in elementary charges.
  - `multiplicity: Option<u32>` — spin multiplicity, at least 1.
  - `geometry: Option<Geometry>` — the structure (see `Geometry`).
  - `vibrational_frequencies: Option<Vec<f64>>` — harmonic frequencies in
    cm⁻¹, sign preserved (a negative value marks an imaginary mode).
  - `gradient: Option<Vec<f64>>` — Cartesian energy gradient in hartree/Å,
    row-major: exactly three values per atom, in geometry atom order.
  - `dipole_moment: Option<[f64; 3]>` — dipole moment vector in atomic units
    (e·a₀).
  - `mulliken_charges: Option<Vec<f64>>` — per-atom Mulliken charges in
    elementary charges, in geometry atom order.
  - `rotational_symmetry_number: Option<u32>` — point-group rotational
    symmetry number, at least 1; an input to rotational entropy.
  - The default record has status `ok` and every observable absent.
  - An absent observable means "not extracted or not applicable"; it is omitted
    from the JSON line, never written as `null`.

- `RecordStatus` — enum with variants `Ok`, `Failed`, `Truncated`, serialized <!-- rq-f8505d7b -->
  as the lowercase strings `"ok"`, `"failed"`, `"truncated"`.
  - `ok` — the job converged; its observables are final.
  - `failed` — the job did not finish; observables may be partially present.
  - `truncated` — the output file ends mid-run; observables may be partially
    present.

- `Geometry` — a structure as parallel arrays. <!-- rq-e17eea18 -->
  - `symbols: Vec<String>` — element symbols in title case (`"N"`, `"Cl"`);
    at least one atom.
  - `coordinates: Vec<f64>` — positions in Å, row-major: exactly three values
    per atom, in `symbols` order.

- `RecordInvalidity` — the reason a record fails validation. Variants: <!-- rq-4eefe504 -->
  - `NonFiniteFloat { field }` — a floating-point field holds NaN or ±∞; JSON
    cannot represent the value and it must not be written.
  - `MalformedGeometry { symbols, coordinates }` — zero symbols, or a
    coordinates length that is not exactly three times the symbols length.
  - `LengthMismatch { field, expected, actual }` — a per-atom array
    (`gradient`, `mulliken_charges`) whose length disagrees with the
    geometry's atom count.
  - `MissingError` — status is `failed` without an error message.
  - `InvalidScalar { field, reason }` — `multiplicity` or
    `rotational_symmetry_number` below 1.

### Functions <!-- rq-340e4add -->

- `Record::validate(&self) -> Result<(), RecordInvalidity>` <!-- rq-1d31610c -->
  - Checks, reporting the first violation found: every floating-point field is
    finite; the geometry shape is well formed; per-atom arrays agree with the
    geometry's atom count when a geometry is present; a `failed` record
    carries a non-empty error message; `multiplicity` and
    `rotational_symmetry_number` are at least 1.
  - Per-atom array lengths are not checked when no geometry is present.

### Crate Layout <!-- rq-9a96e81c -->

- The record types live in `src/record.rs` and are re-exported by the library
  crate root.
- `Cargo.toml` declares the `serde` and `serde_json` dependencies; no other <!-- rq-41e77e57 -->
  dependency is introduced.

## Canonical Units and Values <!-- rq-72f50fe8 -->

- Energies: hartree (`electronic_energy`).
- Coordinates: Ångström (`geometry.coordinates`).
- Gradient: hartree/Å (`gradient`).
- Frequencies: cm⁻¹ (`vibrational_frequencies`).
- Dipole moment: atomic units, e·a₀ (`dipole_moment`).
- Charges: elementary charge e (`charge`, `mulliken_charges`).
- Multiplicity and rotational symmetry number: dimensionless integers.

Conversions to any other unit happen only at the print/export boundary;
records and databases always carry the canonical units above.

## JSON Representation <!-- rq-5e237eee -->

- One record serializes to one line of compact JSON: no spaces, no embedded
  newlines, keys in double quotes.
- Field names are snake_case and stable.
- The writer emits keys in a fixed order — `status`, `error`, `name`,
  `source_path`, `program`, `program_version`, `method`, `electronic_energy`,
  `charge`, `multiplicity`, `geometry`, `vibrational_frequencies`, `gradient`,
  `dipole_moment`, `mulliken_charges`, `rotational_symmetry_number` — with
  absent observables omitted. The reader accepts keys in any order.
- `geometry` serializes as nested parallel arrays:
  `{"symbols": [...], "coordinates": [...]}`.
- Unknown keys in an incoming JSON object are ignored on read. The schema is
  additive-only: later versions add optional fields and never rename or
  repurpose existing ones, so databases written by older versions of CJKQC
  remain loadable and lines written by newer versions remain readable by
  older tools. No version number appears in the record.

Example line:

```json
{"status":"ok","name":"1dma","source_path":"xtb/1dma.log","program":"xtb","method":"GFN2-xTB","electronic_energy":-38.51,"charge":0,"multiplicity":1,"geometry":{"symbols":["N","H"],"coordinates":[0.0,0.0,0.0,0.0,0.0,1.01]},"vibrational_frequencies":[3421.1,1689.7,1023.4,3600.2,1490.3,1100.9]}
```

## Out of Scope <!-- rq-bed9e94b -->

- Composition fields (resolved component counts, cluster labels) — milestone
  4 of the implementation plan; they extend this schema additively.
- Any derived quantity (ZPE, enthalpy, entropy, free energy, formation or
  binding energies, corrections) — computed on demand, milestone 6.
- Reading and writing sequences of records (databases) — `rqm/database.md`.
- Producing records from QC output files — the parser framework, milestones
  2–3.

## Gherkin Scenarios <!-- rq-6bf96b51 -->

```gherkin
Feature: Record schema

  # --- Serialization and reading ---

  @rq-636984f8
  Scenario: Round-trip a record with every observable set
    Given a record with every observable field set to a finite value
    When the record is serialized to JSON and deserialized back
    Then the result equals the original record

  @rq-9b8f2e7b
  Scenario: A minimal record serializes to an exact canonical line
    Given a record with status "ok", electronic_energy -1.5, and all other fields absent
    When the record is serialized
    Then the output is exactly {"status":"ok","electronic_energy":-1.5}

  @rq-c8bfb90d
  Scenario: Absent observables are omitted, never null
    Given a record with dipole_moment absent
    When the record is serialized
    Then the output contains no "dipole_moment" key and no null value

  @rq-009c7dab
  Scenario: Geometry serializes as parallel arrays
    Given a record whose geometry is one oxygen and two hydrogens
    When the record is serialized
    Then the output contains "geometry":{"symbols":["O","H","H"],"coordinates":[...]}

  @rq-d5e85a47
  Scenario: Unknown keys are ignored on read
    Given a JSON line {"status":"ok","electronic_energy":-1.5,"components":{"am":2}}
    When the line is deserialized into a record
    Then the record has status "ok" and electronic_energy -1.5
    And no error is reported

  @rq-f8d87123
  Scenario: A missing status key reads as ok
    Given a JSON line {"electronic_energy":-1.5}
    When the line is deserialized into a record
    Then the record has status "ok"

  @rq-9c420eee
  Scenario: An unknown status value fails deserialization
    Given a JSON line {"status":"weird"}
    When the line is deserialized into a record
    Then deserialization fails

  # --- Validation ---

  @rq-0d722861
  Scenario: validate rejects a non-finite energy
    Given a record whose electronic_energy is NaN
    When validate is called
    Then it returns Err(RecordInvalidity::NonFiniteFloat { field: "electronic_energy" })

  @rq-c4aff078
  Scenario: validate rejects a geometry whose coordinates are not three per atom
    Given a record whose geometry has 2 symbols and 5 coordinates
    When validate is called
    Then it returns Err(RecordInvalidity::MalformedGeometry { symbols: 2, coordinates: 5 })

  @rq-d9b83cf2
  Scenario: validate rejects an empty geometry
    Given a record whose geometry has 0 symbols and 0 coordinates
    When validate is called
    Then it returns Err(RecordInvalidity::MalformedGeometry { symbols: 0, coordinates: 0 })

  @rq-3a6b7e46
  Scenario: validate rejects a gradient length that mismatches the geometry
    Given a record whose geometry has 3 atoms and whose gradient has 8 values
    When validate is called
    Then it returns Err(RecordInvalidity::LengthMismatch { field: "gradient", expected: 9, actual: 8 })

  @rq-a0202680
  Scenario: validate rejects Mulliken charges that mismatch the geometry
    Given a record whose geometry has 3 atoms and whose mulliken_charges has 2 values
    When validate is called
    Then it returns Err(RecordInvalidity::LengthMismatch { field: "mulliken_charges", expected: 3, actual: 2 })

  @rq-ffeb5456
  Scenario: validate accepts a gradient without a geometry
    Given a record with a gradient of 9 values and no geometry
    When validate is called
    Then it returns Ok(())

  @rq-f9547ca9
  Scenario: validate rejects a failed record without an error message
    Given a record with status "failed" and no error message
    When validate is called
    Then it returns Err(RecordInvalidity::MissingError)

  @rq-b2bab408
  Scenario: validate accepts a truncated record with partial observables
    Given a record with status "truncated", an electronic energy, and no error message
    When validate is called
    Then it returns Ok(())

  @rq-26e2fe07
  Scenario: validate accepts a failed record with only status and error
    Given a record with status "failed" and error "SCF did not converge"
    When validate is called
    Then it returns Ok(())

  @rq-af1baabc
  Scenario: validate rejects multiplicity zero
    Given a record with multiplicity 0
    When validate is called
    Then it returns Err(RecordInvalidity::InvalidScalar { field: "multiplicity" })
```
