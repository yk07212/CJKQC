# Feature: Thermochemistry on Demand <!-- rq-3345a07b -->

Every thermodynamic quantity — zero-point energy, enthalpy, entropy, Gibbs
free energy at a temperature — is a derived property: a pure function of the
stored observables (`rqm/record.md`) and the run's thermochemistry
conditions, computed at request time and never stored ("store observables,
derive everything else", `rqm/ARCHITECTURE.md`). The derivation reproduces
ORCA 6's quasi-RRHO thermochemistry by default: vibrational entropy below a
reference frequency is interpolated between the harmonic-oscillator and
free-rotor expressions following S. Grimme, Chem. Eur. J. 18 (2012) 9955,
with the Chai–Head-Gordon damping function and the free-rotor moment of
inertia reduced by the molecule's average principal moment — the variant
ORCA, xtb, and GoodVibes implement. The reference frequency defaults to
100 cm⁻¹ and is set with `--fc`; `--fc 0` selects the pure harmonic
oscillator. Program-reported thermochemistry in output files is never
parsed or kept; recomputation is the single source of truth, so changing the
temperature, reference frequency, frequency scale, or imaginary-mode
handling never invalidates a database.

## Model <!-- rq-5e08c8be -->

A record's thermodynamics follow the ideal-gas rigid-rotor model with the
quasi-harmonic vibrational entropy. All quantities are per molecule in
hartree (Eh); a term quoted "per mole" below is divided by
`N_A · E_h` to reach Eh.

### Conditions <!-- rq-142ac35d -->

`ThermoConditions` carries the run's derivation settings:

- `temperature` — `--temp`, in K, default 298.15, must be > 0.
- `freq_cutoff` — `--fc`, the quasi-harmonic reference frequency ν̃₀ in
  cm⁻¹, default 100.0, must be ≥ 0. `0` disables the free-rotor
  interpolation (pure harmonic entropy).
- `freq_scale` — `--fscale`, the vibrational frequency scale factor
  approximating anharmonicity, default 1.0, must be > 0. The factor
  multiplies every vibrational frequency before any vibrational formula;
  stored observables are untouched.
- `makereal` — `--makereal`, a boolean, default off. On, every negative
  (imaginary) raw frequency enters the derivation as its absolute value
  (see Mode Selection); off, a record holding any negative frequency has
  undefined thermochemistry altogether.

The four flags are settings flags: they configure derivations, never count
as manipulations or print flags, and a run that passes only settings flags
still needs `--out` or a print flag (`rqm/filtering.md`).

### Inputs and Mode Selection <!-- rq-596e8ecd -->

- The record's `electronic_energy` (Eh), `geometry`, `vibrational_frequencies`
  (cm⁻¹), `multiplicity`, and `rotational_symmetry_number` are the inputs.
  Absent `multiplicity` reads as 1; absent `rotational_symmetry_number`
  (σ) reads as 1.
- **Participating modes**: a vibrational mode participates in the
  vibrational sums iff its frequency value — after `--makereal` flipping
  — is > 1.0 cm⁻¹. This drops the near-zero translational and rotational
  contaminant modes programs print (ORCA's "Cut-Off Frequency
  1.00 cm⁻¹"); a stored `−0.0` never participates.
- **Imaginary modes**: a negative raw frequency marks a saddle-point mode.
  With `makereal` off, a record holding any negative frequency has
  undefined thermochemistry — every zpe, h, s, and g value is undefined
  and prints `-` (`rqm/filtering.md`). With `makereal` on, each negative
  frequency is first flipped to its absolute value and then follows the
  participation test and the vibrational formulas like any real mode. The
  flip lives in the derivation only: the stored record, the `lf`
  observable, and the `--nimag` count keep the raw values.
- **Scaled frequency** ν̃′ᵢ = `freq_scale` · ν̃ᵢ for every participating
  mode; all vibrational formulas use ν̃′ᵢ. The 1.0 cm⁻¹ participation
  test runs on the flipped, unscaled frequency.
- **Species class**: `Monatomic` when the geometry has one atom;
  otherwise the mass-weighted principal moments of inertia
  I_a ≤ I_b ≤ I_c are computed from the geometry (in Å) with the standard
  atomic weights of `rqm/uniqueness.md`, and the species is `Linear` when
  I_a ≤ 10⁻⁴ · I_c, else `Nonlinear`. Two-atom geometries are always
  linear under this rule. A geometry with middle principal moment
  I_b ≤ 0 is degenerate — every atom coincides with the center of mass —
  has no species class, and has undefined thermochemistry.
- **Rotational constants** B̃ (cm⁻¹) = h / (8 π² c I) from the principal
  moments; B̃ decreases as I increases. A `Linear` species has one
  constant B̃_b (I_a ≈ 0); a `Nonlinear` species has
  B̃_a ≥ B̃_b ≥ B̃_c, listed in that descending order alongside the
  ascending moments.
- **Average moment** Ī (kg·m²) is the mean of the nonzero principal
  moments — (I_a+I_b+I_c)/3 for `Nonlinear`, I for `Linear`; a `Monatomic`
  record has no vibrational modes and never uses it.

### Terms <!-- rq-5b33312a -->

With T = `temperature`, M the total mass (standard atomic weights, u),
xᵢ = h c ν̃′ᵢ / (k_B T):

- **ZPE** (Eh) = Σᵢ ½ h c ν̃′ᵢ / E_h over participating modes — the
  harmonic zero-point energy; the quasi-harmonic treatment modifies only
  entropy, never ZPE or thermal energies.
- **E_vib** (Eh) = Σᵢ h c ν̃′ᵢ / (e^{xᵢ} − 1) / E_h.
- **E_rot** (Eh) = 0 (`Monatomic`), k_B T / E_h (`Linear`),
  3/2 · k_B T / E_h (`Nonlinear`).
- **E_trans** (Eh) = 3/2 · k_B T / E_h.
- **H** (Eh) = electronic_energy + ZPE + E_vib + E_rot + E_trans
  \+ k_B T / E_h — the last term is the pV = k_B T ideal-gas enthalpy term.
- **S_trans** (J mol⁻¹ K⁻¹) = R [ 5/2 + ln( (2 π M u k_B T / h²)^{3/2}
  · k_B T / p° ) ] with p° = 1 atm and u the atomic mass constant — the
  Sackur–Tetrode translational entropy of an ideal gas at 1 atm.
- **S_rot** (J mol⁻¹ K⁻¹) = 0 (`Monatomic`);
  R [ ln( k_B T / (σ h c B̃_b) ) + 1 ] (`Linear`);
  R [ ln( √π / σ · (k_B T)^{3/2} / √((h c B̃_a)(h c B̃_b)(h c B̃_c)) ) + 3/2 ]
  (`Nonlinear`) — the Herzberg rotational entropy.
- **S_vib** (J mol⁻¹ K⁻¹) = Σᵢ [ (1 − wᵢ) S_ho,ᵢ + wᵢ S_fr,ᵢ ] over
  participating modes, with the free-rotor weight
  wᵢ = 1 / (1 + (ν̃′ᵢ / ν̃₀)⁴) when ν̃₀ > 0, else wᵢ = 0:
  - harmonic entropy S_ho,ᵢ = R [ xᵢ / (e^{xᵢ} − 1) − ln(1 − e^{−xᵢ}) ];
  - free-rotor entropy S_fr,ᵢ = R [ ½ + ln( √(8 π³ μ′ᵢ k_B T) / h ) ]
    with μᵢ = h / (8 π² c ν̃′ᵢ) (kg·m²) and the reduced moment
    μ′ᵢ = μᵢ Ī / (μᵢ + Ī), which caps a mode's rotor moment at the
    molecule's average moment.
- **S_el** (J mol⁻¹ K⁻¹) = R ln(multiplicity) — the electronic spin
  entropy; 0 for singlets and absent multiplicity.
- **Entropy term** T·S (Eh) = T (S_trans + S_rot + S_vib + S_el) /
  (N_A E_h).
- **G** (Eh) = H − T·S.

The quasi-harmonic S_vib never exceeds the harmonic S_vib, so
G(quasi-harmonic) ≥ G(harmonic) at equal conditions; the interpolation is
finite for arbitrarily small positive frequencies.

### Definedness <!-- rq-ac683d6d -->

Thermochemistry is defined for a record iff `electronic_energy` is present,
`geometry` is present and non-degenerate (middle principal moment > 0),
every geometry symbol is in the atomic-weight table, no raw vibrational
frequency is negative (or `makereal` flips them), the species is
`Monatomic` or has at least one participating vibrational mode, and every
derived term is finite. Finiteness holds automatically inside the
Numerics range; beyond it, conditions the CLI accepts (an absurd
`--temp`) can leave a derivation non-finite, and such a derivation is
undefined rather than infinite. An undefined derivation prints `-`,
fails every cutoff comparison except `≠`, sorts after defined values, and
counts as an undefined Gibbs to `--glob` and as an undefined metric to
`--uniq` (`rqm/filtering.md`). A non-monatomic record without frequencies
(single-point outputs, plain xyz) has no thermochemistry.

### Numerics <!-- rq-7dc8ebb4 -->

All terms evaluate in f64. Harmonic-oscillator expressions use the
e^{−x} form (xᵢ e^{−xᵢ} / (1 − e^{−xᵢ}), −ln(1 − e^{−xᵢ})) so that low
temperatures and stiff modes underflow to 0 instead of overflowing;
e^{−x} underflows to exactly 0, making the limits exact. Every derived
value of a defined derivation is finite for any 0 < T ≤ 10⁵ K and
ν̃₀ ≥ 0; beyond that range f64 round-off in e^{−x} near 1 can make a term
non-finite, which leaves the derivation undefined (Definedness; see Out
of Scope).

## Constants <!-- rq-361306ea -->

CODATA 2018 values, the single set used by every conversion:

| Constant | Value |
|---|---|
| h | 6.62607015 × 10⁻³⁴ J s |
| c | 2.99792458 × 10¹⁰ cm s⁻¹ |
| k_B | 1.380649 × 10⁻²³ J K⁻¹ |
| N_A | 6.02214076 × 10²³ mol⁻¹ |
| R = N_A k_B | 8.31446261815324 J mol⁻¹ K⁻¹ |
| u | 1.66053906660 × 10⁻²⁷ kg |
| p° (1 atm) | 101325 Pa |
| E_h | 4.3597447222071 × 10⁻¹⁸ J |
| 1 kcal/mol | 4184 J/mol (exact) |
| Eh → kcal/mol | 627.5094740631 (= `KCAL_MOL_PER_HARTREE`) |

Energies are hartree internally; conversion to kcal/mol happens only at
the print boundary via the `--unit` flag (`rqm/filtering.md` Printing).

## Feature API <!-- rq-7da16174 -->

### Types <!-- rq-eaee0fc4 -->

- `ThermoConditions` — `temperature: f64`, `freq_cutoff: f64`, <!-- rq-3315a51c -->
  `freq_scale: f64`, `makereal: bool`; `Default` is 298.15 K, 100.0 cm⁻¹,
  1.0, false.
- `SpeciesClass` — `Monatomic | Linear | Nonlinear`. <!-- rq-fb4ef102 -->
- `Thermo` — one record's derivation at one condition set, all fields f64 <!-- rq-162b0bbe -->
  in Eh (the T·S fields are temperature-multiplied energy terms):
  `zpe`, `thermal_vibrational` (E_vib), `thermal_rotational`,
  `thermal_translational`, `enthalpy` (H), `ts_translational`,
  `ts_rotational`, `ts_vibrational`, `ts_electronic`, `entropy_term`
  (T·S total), `gibbs` (G).
- `ThermoMemo` — the per-run derivation memo of the Performance section: <!-- rq-f9178b9d -->
  `new(conditions)` starts an empty memo; `thermo(record)` returns the
  record's full derivation, computed at most once per identical record
  (every field equal); `value(key, record)` is a registry lookup served
  through the memo.

### Functions <!-- rq-c32bf4fa -->

- `thermochemistry(record: &Record, conditions: &ThermoConditions) -> Option<Thermo>` <!-- rq-bdd59f53 -->
  — the full model above; `None` exactly when the derivation is
  undefined. Pure: no caching, no mutation, deterministic in the inputs.
- `classify_species(record: &Record) -> Option<SpeciesClass>` — the <!-- rq-e1e88e58 -->
  species class, `None` without a geometry or with an unknown symbol.
- `derived_value(key: &str, record: &Record, conditions: &ThermoConditions) -> Option<f64>` <!-- rq-8f6da1c2 -->
  — the derived-property registry lookup (`rqm/ARCHITECTURE.md`
  extensibility): `key` maps to a derivation function returning the
  record's value in Eh. The registry holds `"el"` (the passthrough
  electronic energy), `"zpe"`, `"h"`, `"s"`, `"g"`. Printing, cutoffs,
  sorting, uniqueness, `--glob`, and the formation section resolve these
  keys through this single lookup — no pipeline stage hand-wires its own
  match arm to a derivation. A new derived property is a new registry
  row plus reference tests, with no record-schema change.

## Performance <!-- rq-8b44ad8d -->

A run derives each record's thermochemistry at most once, no matter how
many consumers read its derived keys. Sorting resolves the sort-key
value for every record before comparing (decorate–sort–undecorate), so
a sort performs one derivation per record, not one per comparison.
Printing, `--glob`, uniqueness, and `--formation` reuse a single
derivation per record across all stages and columns that consume it.
`thermochemistry` itself stays pure — no caching, no mutation — the
reuse lives in the calling pipeline.

### Crate Layout <!-- rq-60671cab -->

- The model lives in `src/thermo.rs`; the library root re-exports the
  types and functions above. No new dependencies.
- `tests/thermo.rs` tests the derivation as pure functions against <!-- rq-1ec71bfc -->
  hand-computed reference values (small linear, nonlinear, monatomic, and
  spin-carrying records) and reproduces the ORCA 6 reference values of the
  conformance fixture below; CLI coverage (flags, units, `--glob`,
  settings-flag usage errors) extends `tests/cli.rs` per
  `rqm/filtering.md`.

## ORCA Reference Values <!-- rq-41528ecc -->

The conformance fixture `tests/fixtures/orca/1pinic1sa-110_1_9-str9.out`
(ORCA 6.0.0, wB97x-3c, quasi-RRHO at 298.15 K, reference frequency
100.0 cm⁻¹, σ = 1, singlet) is the formula oracle. Deriving from the
fixture's parsed record (34 atoms, 96 participating modes) at default
conditions must reproduce ORCA's printed values within 1 × 10⁻⁴ Eh
(the fixture prints frequencies to 2 decimals; the residual is frequency
rounding, not model error):

| Quantity | ORCA prints | Derivation field |
|---|---|---|
| Zero point energy | 0.27547281 Eh | `zpe` |
| Thermal vibrational correction | 0.01541878 Eh | `thermal_vibrational` |
| Thermal rotational correction | 0.00141627 Eh | `thermal_rotational` |
| Thermal translational correction | 0.00141627 Eh | `thermal_translational` |
| Thermal enthalpy correction (pV) | 0.00094421 Eh | k_B T / E_h |
| Total enthalpy | −199.34739714 Eh | `enthalpy` |
| Translational entropy term | 0.02035094 Eh | `ts_translational` |
| Rotational entropy term | 0.01583464 Eh | `ts_rotational` |
| Vibrational entropy term | 0.02738792 Eh | `ts_vibrational` |
| Final entropy term | 0.06357350 Eh | `entropy_term` |
| Final Gibbs free energy | −199.41097065 Eh | `gibbs` |

The rotational constants recomputed from the fixture geometry and the
standard atomic weights (0.024157, 0.008371, 0.007528 cm⁻¹) match ORCA's
printed constants, pinning the moments-of-inertia path as well.

## Out of Scope <!-- rq-9613ceb9 -->

- BSSE / counterpoise corrections: not supported in v1.
- Single-point energy corrections (JKQC's `gout`/`elout`: re-deriving
  thermochemistry from a higher-level single-point energy file) — a future
  milestone; the record and derivation make no provision for them.
- Alternative low-frequency schemes beyond the Grimme interpolation and
  the pure harmonic oscillator (`--fc 0`): no hard truncation (raising
  frequencies below a cutoff to the cutoff), no Truhlar/RRQHO selector.
- Deleting imaginary modes (JKQC `-dropimg`): not supported; `--makereal`
  flips them into the derivation instead and never edits a stored record.
- Concentration or solvent-adjusted translational entropy: the standard
  state is the ideal gas at 1 atm; there is no pressure flag.
- Range validation beyond positivity/sign checks on the settings flags:
  `--temp` is checked only for positivity, so absurd temperatures
  (10¹⁶ K and above) are accepted; the Numerics section states the
  finite-value range, and derivations that turn non-finite beyond it are
  undefined (Definedness).
- Boltzmann-population or statistical-mechanics reporting beyond the
  quantities above (`rqm/ARCHITECTURE.md` open question).
- Formation/binding energies from monomer references: `rqm/formation.md`.
- Storing any derived quantity in a record or database.

## Gherkin Scenarios <!-- rq-05e6abe2 -->

```gherkin
Feature: Thermochemistry on demand

  Background:
    Given the default ThermoConditions are temperature 298.15 K, freq_cutoff 100.0 cm^-1, freq_scale 1.0, makereal off

  # --- Conformance against ORCA ---

  @rq-9936f168
  Scenario: The ORCA 6 fixture reproduces ORCA's quasi-RRHO thermochemistry
    Given the record parsed from tests/fixtures/orca/1pinic1sa-110_1_9-str9.out
    When thermochemistry is derived at default conditions
    Then zpe, thermal_vibrational, thermal_rotational, thermal_translational, enthalpy, ts_translational, ts_rotational, ts_vibrational, entropy_term, and gibbs each match the ORCA reference values within 1e-4 Eh

  @rq-3e2a7cc5
  Scenario: Rotational constants recompute from the stored geometry
    Given the record parsed from the ORCA fixture
    When the principal moments and rotational constants are computed
    Then they are 0.024157, 0.008371, and 0.007528 cm^-1 within 1e-5

  # --- Mode selection ---

  @rq-e1c07994
  Scenario: Near-zero modes do not participate
    Given a nonlinear 3-atom record with frequencies [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 500.0, 1000.0, 1500.0]
    When thermochemistry is derived
    Then the vibrational sums use exactly the modes 500.0, 1000.0, 1500.0

  @rq-3f95d2bc
  Scenario: An imaginary frequency leaves thermochemistry undefined
    Given a record with frequencies [1000.0, -300.0] and otherwise defined inputs
    When thermochemistry is derived at default conditions
    Then it returns None

  @rq-bf8fdacc
  Scenario: A saddle point's thermochemistry columns print dashes
    Given a database with one record holding an imaginary frequency
    When CJKQC is invoked with the database and "--name --zpe --h --s --g" and no --out
    Then the zpe, h, s, and g columns each print -

  @rq-f6d24cca
  Scenario: --makereal flips imaginary modes into the sums
    Given a record with frequencies [1000.0, -300.0] and a record with frequencies [1000.0, 300.0]
    When thermochemistry is derived for the first with makereal and for the second without
    Then the zpe, thermal_vibrational, and ts_vibrational values are pairwise equal

  @rq-9a4270bc
  Scenario: --makereal prints the saddle point's thermochemistry
    Given a database with one record holding an imaginary frequency
    When CJKQC is invoked with the database and "--name --g --makereal" and no --out
    Then the g column prints a defined value

  @rq-cc73801c
  Scenario: --makereal applies the participation test after flipping
    Given a nonlinear 3-atom record with frequencies [0.0, -0.5, 1200.0]
    When thermochemistry is derived with makereal
    Then the vibrational sums use only the mode 1200.0

  @rq-0bcbeeb1
  Scenario: --makereal leaves the raw observables alone
    Given a database with one record holding frequencies [1000.0, -300.0]
    When CJKQC is invoked with the database and "--name --nimag --g --makereal" and no --out
    Then nimag prints 1 and g prints a defined value
    And the lf cutoff property still sees -300.0, so "--filter-lt lf -200" keeps the record

  @rq-65b1e3cd
  Scenario: A single mode above the cutoff carries the thermochemistry
    Given a nonlinear 3-atom record with frequencies [0.5, 1200.0]
    When thermochemistry is derived
    Then it is defined and the vibrational sums use only the mode 1200.0

  # --- Species classes ---

  @rq-273399bb
  Scenario: A monatomic record has defined thermochemistry without frequencies
    Given a one-atom record with electronic_energy and no vibrational_frequencies
    When thermochemistry is derived
    Then zpe is 0, thermal_rotational is 0, ts_rotational is 0, ts_vibrational is 0, and gibbs is defined

  @rq-274f8c7e
  Scenario: A linear species uses the linear rotational terms
    Given a collinear 3-atom record with 4 frequencies and electronic_energy
    When thermochemistry is derived
    Then thermal_rotational is k_B T in Eh
    And ts_rotational equals R T ln(k_B T / (h c B)) + R T in molar-Eh terms

  @rq-a2ed39c8
  Scenario: A slightly bent triatomic is nonlinear
    Given a 3-atom record whose smallest principal moment is 1e-3 times its largest
    When classify_species is called
    Then it returns Nonlinear

  # --- Entropy pieces ---

  @rq-f03f140d
  Scenario: The symmetry number divides the rotational partition function
    Given two otherwise identical nonlinear records, one with rotational_symmetry_number 1 and one with 2
    When thermochemistry is derived for both
    Then the σ=2 record's ts_rotational is lower by T R ln 2

  @rq-16a6fe88
  Scenario: Electronic entropy carries the spin degeneracy
    Given a record with multiplicity 2
    When thermochemistry is derived
    Then ts_electronic is T R ln 2 / (N_A E_h) in Eh

  @rq-1c86c6f3
  Scenario: Quasi-harmonic entropy never exceeds harmonic entropy
    Given a nonlinear record with modes below the reference frequency
    When thermochemistry is derived with freq_cutoff 100 and with freq_cutoff 0
    Then the quasi-harmonic ts_vibrational is less than or equal to the harmonic one
    And the quasi-harmonic gibbs is greater than or equal to the harmonic one

  @rq-18e4b628
  Scenario: The reference frequency shifts the interpolation
    Given a nonlinear record with modes below 100 cm^-1
    When thermochemistry is derived with freq_cutoff 100 and with freq_cutoff 200
    Then the two ts_vibrational values differ

  @rq-927eb1c6
  Scenario: A mode at the reference frequency weighs half
    Given freq_cutoff 100
    When the free-rotor weight of a mode at exactly 100 cm^-1 is computed
    Then it is 0.5

  # --- Conditions ---

  @rq-ed4da76f
  Scenario: Temperature changes every thermal term but not zpe
    Given a record with defined thermochemistry
    When thermochemistry is derived at 298.15 K and at 273.15 K
    Then the two zpe values are equal
    And the gibbs values differ

  @rq-06cfcbfe
  Scenario: An absurd temperature leaves thermochemistry undefined
    Given a record with defined thermochemistry whose modes include one just above the 1 cm^-1 participation cutoff
    When thermochemistry is derived at 1e17 K
    Then it returns None

  @rq-64567efa
  Scenario: Absurd-temperature records print dashes and drop out of --glob
    Given a database whose group of records all derive to non-finite terms at 1e17 K
    When CJKQC is invoked with the database, "--temp 1e17 --glob --name" and no --out
    Then the printed table is only the header row

  @rq-cb57c609
  Scenario: The frequency scale multiplies ZPE linearly
    Given a record with defined thermochemistry
    When thermochemistry is derived with freq_scale 0.95
    Then zpe is exactly 0.95 times the freq_scale 1.0 value
    And ts_vibrational differs from the unscaled value

  @rq-f9e49657
  Scenario: Zero temperature is a usage error
    When CJKQC is invoked with "--temp 0"
    Then the exit code is 2

  @rq-87c46fa4
  Scenario: A negative reference frequency is a usage error
    When CJKQC is invoked with "--fc -100"
    Then the exit code is 2

  @rq-91d514c5
  Scenario: A zero frequency scale is a usage error
    When CJKQC is invoked with "--fscale 0"
    Then the exit code is 2

  @rq-0585b2ea
  Scenario: Settings flags alone do not make a visible outcome
    When CJKQC is invoked with a database and "--temp 300" and no --out and no print flag
    Then the exit code is 2

  # --- Definedness ---

  @rq-7fe12cf9
  Scenario: A missing electronic energy leaves thermochemistry undefined
    Given a record with geometry and frequencies but no electronic_energy
    When thermochemistry is derived
    Then it returns None

  @rq-7be2650a
  Scenario: A non-monatomic record without frequencies is undefined
    Given a 10-atom record with electronic_energy and geometry but no vibrational_frequencies
    When thermochemistry is derived
    Then it returns None

  @rq-66b6ca1d
  Scenario: An unknown element symbol is undefined
    Given a record whose geometry holds the symbol "Xx"
    When thermochemistry is derived
    Then it returns None

  @rq-aceaf96d
  Scenario: A degenerate geometry is undefined
    Given a record whose geometry has every atom at identical coordinates
    When thermochemistry is derived
    Then it returns None
    And classify_species returns None

  @rq-d04cc134
  Scenario: A degenerate geometry prints dashes, not infinities
    Given a database with one record whose atoms all coincide
    When CJKQC is invoked with the database and "--name --zpe --h --s --g" and no --out
    Then the zpe, h, s, and g columns each print -

  @rq-9dc99217
  Scenario: Undefined thermochemistry prints as a dash
    Given a database with one record lacking frequencies
    When CJKQC is invoked with the database and "--name --g" and no --out
    Then the g column prints -

  # --- Registry and pipeline integration ---

  @rq-df4e27e8
  Scenario: The registry exposes the five energy keys
    Given a record with defined thermochemistry
    When derived_value is called with "el", "zpe", "h", "s", and "g"
    Then each returns the corresponding value in Eh, and "s" returns the T*S entropy term

  @rq-c879d42f
  Scenario: Sorting by Gibbs with undefined values last
    Given records with gibbs -100.0, -100.5, and no electronic_energy
    When CJKQC is invoked with the database and "--sort g --name --g" and no --out
    Then the order is -100.5, -100.0, undefined

  @rq-60d713bc
  Scenario: A relative Gibbs cutoff window works per group in kcal/mol
    Given one group with gibbs values 0.0 and 0.02 Eh and another with -5.0 Eh
    When the cutoff cutr g 5 applies
    Then the 0.02 Eh record (12.6 kcal/mol above its minimum) is dropped and the -5.0 record survives

  @rq-573f5674
  Scenario: --glob keeps each group's lowest Gibbs record
    Given a group with three records of differing gibbs and another group with one
    When CJKQC is invoked with the database, "--glob --name" and no --out
    Then one record per group is printed, each the group's minimum gibbs

  @rq-58808413
  Scenario: --glob drops records with undefined Gibbs
    Given a group whose records all lack electronic_energy
    When --glob applies
    Then the group contributes no records

  @rq-4cd891c1
  Scenario: Uniqueness rounds Gibbs like any metric
    Given two records whose gibbs values agree to 3 decimals and a third that differs
    When CJKQC is invoked with the database and "--uniq g --name" and no --out
    Then the first of the two agreeing records is kept and the other dropped

  # --- Units ---

  @rq-7f65572f
  Scenario: The --unit flag converts the energy columns to kcal/mol
    Given a record with electronic_energy -1.5 Eh and defined thermochemistry
    When CJKQC is invoked with the database and "--el --zpe --h --s --g --unit" and no --out
    Then each energy column prints in kcal/mol with 4 decimals

  @rq-8b4b5dc5
  Scenario: Without --unit the energies remain hartree
    When CJKQC is invoked with the database and "--g" and no --out
    Then g prints in hartree with 10 decimals

  @rq-48b6dd65
  Scenario: A value passed to --unit is a usage error
    When CJKQC is invoked with the database and "--unit kcal"
    Then the exit code is 2
```
