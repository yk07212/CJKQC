# Feature: Formation Energies from Monomer References <!-- rq-f8ebbc84 -->

Clusters are put in context by their formation energies: how much a
structure's energy differs from a stoichiometric mix of isolated,
single-component reference structures. A cluster database already carries
its own references — the monomers. JKQC-style, CJKQC scans the run's
final records — exactly the records the run prints — for monomers, and
the `--formation` flag prints each printed record's formation energy,
formation enthalpy, formation entropy term, and formation free energy as
a section after the normal property table.
Nothing is stored: formation values are derived at request time from the
records' observables under the run's thermochemistry conditions
(`rqm/thermo.md`), so temperature, reference frequency, and unit changes
apply to them like to any derived property. The `--conc` and `--cnt`
flags further shift the free-energy and entropy-term levels at vapors of
named components at given concentrations (Formation at Given
Concentrations),
and `--bavg` collapses each printed section to one Boltzmann-averaged
row per composition (Boltzmann-Averaged Sections).

## Monomer References <!-- rq-1b571f91 -->

- A **monomer record** is a record with status `ok` whose `components` map
  (`rqm/composition.md`) is exactly one entry with count 1 — components
  `{am: 1}` qualifies, `{am: 2}` and `{am: 1, sa: 1}` do not. Records
  without components never qualify.
- Monomers are collected from the run's **final record set** — exactly the
  records the run prints: the pipeline survivors after every manipulation
  stage (`--extract`, cutoffs, `--uniq`, `--reacted`, `--select`, `--glob`;
  `rqm/filtering.md`) for a manipulation run, every loaded record for a
  print-only run. A monomer a filter removes is gone: it prints no row and
  serves as no reference.
- For each component code present in that set, the **reference** is a
  single monomer record: the first of that code's monomers under the run's
  sort ordering (`rqm/filtering.md` Sort) — `--sort g` the lowest Gibbs
  free energy, `--sort rg`, `--sort mass`, `--sort filename` the first under
  that key, and `--sort el` or no `--sort` at all the lowest electronic
  energy. The ordering is ascending — `--reverse` orients the table,
  never the reference — ties keep the first in load order, and for the
  numeric keys a monomer whose sort-key value is undefined orders after
  every defined one. Under `--sort filename` the undefined case is a nameless
  monomer, which orders before every named one — the nameless-first
  carve-out of `rqm/filtering.md` Sort.
- All reference values come from that one record: the **electronic
  reference** is its `electronic_energy`, the **enthalpy reference** its
  H, the **entropy-term reference** its T·S in energy units, and the
  **Gibbs reference** its G, all derived at the run's conditions
  (`rqm/thermo.md`). Under `--sort g` the lowest-G conformer supplies
  every reference, even when another conformer's electronic energy or
  enthalpy is lower. A level whose value the chosen record lacks (no
  electronic energy, undefined thermochemistry) simply has no reference
  at that level; the enthalpy, entropy-term, and Gibbs references are
  defined together.
- A code with no qualifying monomer has no reference.

## Formation Values <!-- rq-c3425396 -->

For a printed record with components map C = {codeᵢ: nᵢ}:

- **Formation energy** (Eh) = `electronic_energy` − Σᵢ nᵢ ·
  E_ref(codeᵢ), the electronic monomer reference above.
- **Formation enthalpy** (Eh) = H(record) − Σᵢ nᵢ · H_ref(codeᵢ).
- **Formation entropy term** (Eh) = T·S(record) − Σᵢ nᵢ ·
  ST_ref(codeᵢ). This is the temperature-multiplied entropy as an energy
  term, the same quantity the main table's `s` column prints, not an
  entropy in J mol⁻¹ K⁻¹.
- **Formation free energy** (Eh) = G(record) − Σᵢ nᵢ · G_ref(codeᵢ), with
  the Gibbs values derived at the run's conditions (`rqm/thermo.md`).
- The four levels satisfy g_form = h_form − st_form, and the electronic
  level is independent of the other three.
- A level's value is undefined when the record lacks `components`, lacks
  the level's input energy (electronic energy for the el level, defined
  thermochemistry for the h, st, and g levels), or any referenced code
  lacks that level's monomer reference. The enthalpy, entropy-term, and
  Gibbs levels are defined together, since all three require defined
  thermochemistry and their references come from the same selected
  monomer; the electronic level's definedness is independent.
- Undefined is a property of the level: a record can have a defined
  formation energy and undefined thermal levels (a monomer without
  frequencies, say), and vice versa.

The values follow the sign convention "negative = lower than the isolated
monomers"; the binding energy is the same number negated, and no separate
binding column exists.

## Printing <!-- rq-8dee4f4e -->

- `--formation` is a print flag: it produces output and satisfies the
  visible-outcome rule of `rqm/filtering.md` on its own, but it adds no
  column to the normal table.
- The **formation section** prints after the normal table (when other
  print flags requested one) and before any summary line, so stdout is
  always: main table, formation section, summary. A run with `--formation`
  but no column flags prints only the formation section.
- The section is a table: a header row `name el h st g`, then one row per
  printed record in run order, aligned like the main table
  (`rqm/filtering.md` Printing): `name` a text column, `el`, `h`, `st`,
  and `g` numeric ones, each column as wide as its widest cell (header
  included), header cells aligning with their columns. `name` is the
  record's composition token —
  the same alphabetical `2am3sa`-style pairs the `--name` column prints
  (`rqm/filtering.md` Printing) — and `-` when the record has no
  components; `el`, `h`, `st`, and `g` are the formation energy,
  formation enthalpy, formation entropy term, and formation free energy
  in the run's energy unit — kcal/mol under `--unit`, hartree
  otherwise (`rqm/filtering.md` Printing) — formatted like the
  corresponding energy columns, and `-` when undefined.
- `--formation` computes values for exactly the records the run prints:
  the pipeline survivors for a manipulation run, every loaded record for a
  print-only run.

## Corrected Formation Section <!-- rq-2f4611d0 -->

- `--formation-corr` prints a second formation section evaluated at the
  attached single-point corrections (`rqm/correction.md`). Every input energy
  is a corrected value; a record with no correction has no values at all —
  corrected formation numbers never mix levels.
- The section uses the same reference monomers as the raw section: the same
  monomer set and the same sort-based selection (Monomer References above). A
  code's corrected electronic reference is that monomer's correction energy,
  its corrected enthalpy reference the monomer's `hcorr`, its corrected
  entropy-term reference the monomer's T·S, and its corrected Gibbs reference
  the monomer's `gcorr` value. A reference is undefined when the selected
  monomer carries no correction or its corrected thermochemistry is undefined;
  the corrected enthalpy, entropy-term, and Gibbs references are defined
  together.
- For a printed record with components map C = {codeᵢ: nᵢ}:
  - **Corrected formation energy** (Eh) = the record's correction energy −
    Σᵢ nᵢ · Ecorr_ref(codeᵢ).
  - **Corrected formation enthalpy** (Eh) = hcorr(record) − Σᵢ nᵢ ·
    Hcorr_ref(codeᵢ).
  - **Corrected formation entropy term** (Eh) = T·S(record) − Σᵢ nᵢ ·
    ST_ref(codeᵢ). The correction does not alter any thermal term, so this
    value equals the raw section's `st` whenever both are defined; it is
    nevertheless gated by the corrected section's definedness.
  - **Corrected formation free energy** (Eh) = gcorr(record) − Σᵢ nᵢ ·
    Gcorr_ref(codeᵢ).
  - The corrected levels satisfy gcorr_form = hcorr_form − stcorr_form.
  - A level is defined iff the record's own corrected value at that level is
    defined and every referenced code has a defined corrected reference at
    that level. A record without a correction has no corrected value at any
    level.
- The flag is a print flag like `--formation`: it satisfies the
  visible-outcome rule of `rqm/filtering.md` on its own and adds no column to
  the normal table. The corrected section prints after the raw formation
  section (when `--formation` requested one) and before any summary line, so
  stdout is always: main table, formation section, corrected formation
  section, summary. A run with `--formation-corr` but no other print flag
  prints only the corrected section.
- The section is a table: a header row `name elcorr hcorr stcorr gcorr`,
  then one row per printed record in run order, aligned like the raw
  section's table. The composition token and the number formatting — the
  run's energy unit, kcal/mol under `--unit`
  (`rqm/filtering.md` Printing), formatted like the corresponding energy
  columns, `-` when undefined — are exactly the raw section's rules.

## Formation at Given Concentrations <!-- rq-10341821 -->

- `--conc CODE VALUE` (repeatable) and `--cnt` evaluate the formation
  free energies at vapors of named components whose concentrations differ
  from the standard state: the chemical potential of a vapor at partial
  pressure c is lower than its 1 atm standard-state value by R T ln(c/p°)
  per mole, and forming a cluster withdraws nᵢ vapor molecules of each
  component. The flags are settings flags (`rqm/thermo.md`): they
  configure the formation derivations, never count as manipulations or
  print flags, and a run that passes only them still needs `--out` or a
  print flag.
- VALUE is a partial pressure in Pa — a decimal or scientific-notation
  number — optionally suffixed, lowercase and without whitespace, with a
  unit: `100ppt` and `10ppb` are the mole fractions 10⁻¹² and 10⁻⁹ of the
  standard atmosphere (100·10⁻¹²·p° and 10·10⁻⁹·p° Pa), and `1e10cmmc` is
  10¹⁰ molecules per cm³, converted as c·10⁶·k_B·T Pa at the run's
  temperature. A value outside this grammar, non-finite, or not positive
  is a usage error (exit 2); the `^` power syntax other tools accept is
  not part of the grammar. The unit constants are the CODATA 2018 set of
  `rqm/thermo.md` Constants (p° = 101325 Pa).
- Each spec names one component code: a non-empty, letter-only code as
  `rqm/record.md` component validation requires. Naming the same code in
  two specs is a usage error. When a formation section is printed, every
  spec's code must appear as a component of at least one record in the
  section's record set — the run's final record set — or the run is a
  usage error; a code whose monomers were filtered away may still appear
  inside cluster compositions and simply forms no reference (Monomer
  References).
- With specs S and a record whose components map C = {codeᵢ: nᵢ}, let

  c_term = Σ_{codeᵢ ∈ S} nᵢ,eff · R T ln(cᵢ/p°) / (N_A E_h),

  with cᵢ the spec's resolved partial pressure and nᵢ,eff = nᵢ. Under
  `--cnt`, the self-consistent form, nᵢ,eff = nᵢ·(N−1)/N where
  N = Σ nᵢ over the record's whole components map. A spec whose code is
  absent from C contributes nothing, so coverage may be partial. The
  formation section's values become

  g_conc = g_form − c_term,   st_conc = st_form + c_term,

  so the concentration term is an entropy shift: it lowers the formation
  free energy and raises the formation entropy term by the same
  magnitude, keeping g_conc = h_form − st_conc.
- The self-consistent form keeps the concentration term of every
  monomer at zero under any concentration, so the reference monomer's
  own formation free energy stays exactly 0 while a non-reference
  conformer keeps its conformer difference against the reference; the
  total concentration term of a fully covered N-molecule cluster is
  (N−1)·R T-weighted instead of N (JKQC's `-cnt`, "see Wyslouzil"; CJKQC
  keeps the flag name `--cnt`). The monomer's formation entropy term
  stays exactly 0 for the same reason.
- The entropy levels and the Gibbs levels shift together: the raw
  section's `g` and `st`, and the corrected section's `gcorr` and
  `stcorr` (Corrected Formation Section), each carry the same `c_term`,
  subtracted from the Gibbs value and added to the entropy-term value.
  The electronic levels `el` and `elcorr` and the enthalpy levels `h` and
  `hcorr` stay at the standard state — the concentration term is a
  chemical-potential (entropy) correction with no electronic or enthalpy
  counterpart, where JKQC applies its correction to every numeric
  formation column. A level that is undefined stays undefined; the
  correction never creates a value. A magnitude so extreme that the
  resolved pressure underflows below the smallest positive double, or the
  term itself overflows, makes the Gibbs and entropy levels undefined
  rather than infinite — the crate's numerics rule (`rqm/thermo.md`
  Numerics), never a printed infinity.
- p° is the 1 atm standard pressure of `rqm/thermo.md` Constants — the
  same standard state the translational entropy is evaluated at — so a
  spec at exactly p° reproduces the uncorrected values. A concentration
  above p° is accepted and lowers the formation free energy. The main
  table, cutoffs, sorting, `--glob`, and the composition token are
  untouched: `--conc` changes only the formation sections'
  `g`/`gcorr` and `st`/`stcorr` values, printed in the run's energy unit
  like any formation value.

## Boltzmann-Averaged Sections <!-- rq-dd65e289 -->

- `--bavg` reshapes the printed formation sections: each section's record
  set is first collapsed into one **ensemble** per composition group, and
  the section then computes its references, values, concentrations, and
  formatting over the ensembles exactly as it does over records. The flag
  is a settings flag (`rqm/thermo.md`), like `--conc` and `--cnt`: it
  configures the formation sections' presentation, never counts as a
  manipulation or print flag, and a run that passes only it still needs
  `--out` or a print flag. It takes no value; without a printed formation
  section it is an accepted no-op, and the `-bavgout` spelling is not part
  of the surface (an unknown-flag usage error, `rqm/scaffolding.md`). The
  main table is untouched: it always prints one row per record, whatever
  `--bavg` is given.
- The collapse groups the section's record set by `components` map
  (`rqm/filtering.md` Grouping), always by composition — `--noname` never
  pools the formation rows into one global ensemble — in order of first
  appearance, with records without components forming one unnamed
  ensemble. A group's **members** are its `ok` records that define the
  section's Gibbs level: the derived Gibbs free energy for the raw
  section (`rqm/thermo.md`), `gcorr` for the corrected section
  (`rqm/correction.md`). Failed and truncated records contribute nothing
  even when they still carry the observables — the collapse treats them
  exactly as the pipeline does (`rqm/filtering.md`) — and an `ok` record
  without that value contributes nothing (the definedness rule of
  `rqm/population.md`); a group with no members yields an ensemble
  without values. The ok-gate keeps the collapsed references consistent
  with the uncollapsed ones, whose monomer references are `ok` records
  by definition (Monomer References).
- An ensemble with members carries four values in Eh, defined together or
  not at all — a member that defines the Gibbs level always defines the
  other three, since the enthalpy, entropy term, and Gibbs all derive
  from the member's thermochemistry and the electronic value from its
  energy:
  - the **electronic value** is the members' population-weighted mean
    Σⱼ pⱼ·eⱼ of the section's electronic-level inputs — the record's
    `electronic_energy` for the raw section, the correction energy
    (`rqm/correction.md`) for the corrected one — with pⱼ the members'
    Boltzmann populations (`rqm/population.md`) at the run's temperature;
  - the **enthalpy value** is the members' population-weighted mean
    Σⱼ pⱼ·Hⱼ of their enthalpy-level values — the enthalpy for the raw
    section, `hcorr` for the corrected one;
  - the **Gibbs value** is the ensemble free energy
    −k_B·T·ln(Σⱼ exp(−gⱼ/(k_B·T))) over the members' Gibbs-level values —
    equivalently g_min − k_B·T·ln Z, computed against the members' minimum
    as `rqm/population.md` Numerics prescribes. This is the
    conformationally stabilized free energy of the group (JKQC's "-g is
    treated correctly"): a plain population-weighted mean would drop the
    −k_B·T·ln Z entropy term;
  - the **entropy-term value** is H_ens − G_ens, so the collapse keeps
    g_ens = h_ens − st_ens and the conformational stabilization appears
    in the entropy term rather than lowering the enthalpy.
- Over the collapsed set every standing rule applies unchanged. An
  ensemble whose components map is exactly one entry with count 1 is its
  code's **monomer ensemble** and supplies that level's electronic,
  enthalpy, entropy-term, and Gibbs reference values, so a monomer row's
  formation values are exactly 0 at every level; an ensemble without
  values supplies no reference at any level. The treatment is
  uniform — monomers and clusters are ensembled alike — and a user who
  wants a fixed baseline curates it themselves: with a single monomer
  conformer per code in the record set, the collapsed row reduces to the
  global-minimum formation value plus the conformer-entropy correction
  −k_B·T·ln(Σⱼ exp(−(ΔGⱼ − ΔG_0)/(k_B·T))) against that baseline (the
  conformer correction of Partanen 2016, JKQC's `-bavg` intent), which is
  then never above the best conformer's value. The formation values
  subtract Σᵢ nᵢ·ref(codeᵢ) and the `--conc`/`--cnt` term exactly
  as for records — the concentration term is constant within a group, so
  concentrating and collapsing produce the same numbers in either order.
  The section prints one row per ensemble in first-appearance order —
  `name` the ensemble's composition token, `-` for the unnamed ensemble —
  under the section's standing header, unit, and formatting.
- Consequences worth naming: a composition whose records all lack the
  section's Gibbs level — a single-point-only group — prints one row of
  dashes where the uncollapsed section would print electronic-level
  values, and a code whose monomers all lack it loses its reference at
  every level at once. Undefined members never poison their group: the
  surviving members define the ensemble (JKQC's NaN-poisoned columns are
  not carried over). A group whose single member defines the Gibbs level
  collapses to itself, so `--glob` makes the collapse the identity.
- The collapse always stabilizes a group's own ensemble —
  −k_B·T·ln Z ≤ 0 — but a formation value is a difference of two
  ensembles, so averaging shifts it either way: it turns less favorable
  when the monomer ensembles carry more conformational entropy than the
  cluster ensemble (forming the cluster costs the monomers' conformer
  freedom) and more favorable in the opposite case, with the terms
  cancelling for equally floppy cluster and monomers. The stabilization
  is visible in the entropy-term column, since st_ens = h_ens − g_ens.

## Feature API <!-- rq-e814b912 -->

### Types <!-- rq-85b79495 -->

- `MonomerReferences` — the per-code references in Eh: <!-- rq-a14c7897 -->
  `electronic: BTreeMap<String, f64>`, `enthalpy: BTreeMap<String, f64>`,
  `entropy_term: BTreeMap<String, f64>`, `gibbs: BTreeMap<String, f64>`.
- `CorrectedMonomerReferences` — the per-code corrected references in Eh: <!-- rq-67b44e63 -->
  `electronic: BTreeMap<String, f64>`, `enthalpy: BTreeMap<String, f64>`,
  `entropy_term: BTreeMap<String, f64>`, `gibbs: BTreeMap<String, f64>`; a
  code appears only when its selected monomer defines that level's
  corrected value (`rqm/correction.md`). The entropy-term reference is the
  monomer's T·S, which the correction does not alter.
- `Formation` — a record's four derived levels: <!-- rq-103447d2 -->
  `electronic: Option<f64>`, `enthalpy: Option<f64>`,
  `entropy_term: Option<f64>`, `gibbs: Option<f64>` (Eh); `enthalpy`,
  `entropy_term`, and `gibbs` are `Some` together or `None` together —
  except that a non-finite concentration term (an absurd magnitude,
  Formation at Given Concentrations) leaves the enthalpy defined while
  the entropy term and Gibbs go undefined — and `electronic` is
  independent.
- `ConcUnit` — `Pa | Ppt | Ppb | Cmmc`, the VALUE grammar's units. <!-- rq-7d64cce1 -->
- `ConcSpec` — one `--conc` spec: `code: String`, `magnitude: f64`, <!-- rq-c92b9ff1 -->
  `unit: ConcUnit`; `pressure(temperature: f64) -> f64` resolves the
  partial pressure in Pa: `Pa` the magnitude itself, `Ppt`/`Ppb`
  magnitude·10⁻¹²/10⁻⁹·p°, `Cmmc` magnitude·10⁶·k_B·temperature.
- `ConcConditions` — the run's concentration settings: <!-- rq-a0748032 -->
  `specs: Vec<ConcSpec>`, `self_consistent: bool`; `Default` is no specs
  and `--cnt` off.
- `Ensemble` — one collapsed composition group (Boltzmann-Averaged <!-- rq-35671b9e -->
  Sections): `components: Option<Components>` (`None` for the unnamed
  ensemble), `electronic: Option<f64>`, `enthalpy: Option<f64>`,
  `entropy_term: Option<f64>`, `gibbs: Option<f64>` (Eh); the four values
  are `Some` together or `None` together, and `entropy_term` is
  `enthalpy − gibbs`.

### Functions <!-- rq-ded51dfb -->

- `parse_conc_value(text: &str) -> Option<(f64, ConcUnit)>` — the VALUE <!-- rq-f8a2fb33 -->
  grammar; `None` for a value outside the grammar, non-finite, or not
  positive (the CLI turns `None` into a usage error).
- `collect_monomers(records: &[Record], memo: &ThermoMemo, sort: SortKey) -> MonomerReferences` <!-- rq-9e898298 -->
  — the reference scan over the given records (the run passes its final
  record set; `rqm/filtering.md`) with the sort-based selection above.
  The memo carries the run's thermochemistry conditions and derives each
  record at most once (`rqm/thermo.md` Performance).
- `formation_values(record: &Record, references: &MonomerReferences, memo: &ThermoMemo, concs: &ConcConditions) -> Formation` <!-- rq-6ffbf9aa -->
  — the four levels with their definedness; the record's enthalpy,
  entropy term, and Gibbs come from one whole-record thermochemistry
  derivation on the memo (`rqm/thermo.md`) rather than three registry
  lookups, keeping the three levels defined together; when the Gibbs
  and entropy levels are defined, the concentration term of
  `concentration_correction` is subtracted from the Gibbs value and
  added to the entropy-term value; a correction that is not finite (an
  absurd magnitude) leaves both undefined rather than infinite.
- `concentration_correction(components: &Components, concs: &ConcConditions, temperature: f64) -> f64` <!-- rq-2333d9cc -->
  — the entropy term `c_term` in Eh: Σ over the specs whose code is in the
  components map of nᵢ,eff · k_B T ln(cᵢ/p°) / E_h, with nᵢ,eff = nᵢ or,
  under `self_consistent`, nᵢ·(N−1)/N; 0 when no spec matches. The
  formation printers subtract it from the Gibbs value and add it to the
  entropy-term value.
- `print_formation_section(records: &[Record], references: &MonomerReferences, memo: &ThermoMemo, unit: Unit, concs: &ConcConditions) -> String` <!-- rq-9503bcb2 -->
  — the section table with header `name el h st g`, one row per record in
  the given order, `-` for undefined values and for the composition token
  when the record has no components.
- `corrected_monomer_references(records: &[Record], memo: &ThermoMemo, sort: SortKey) -> CorrectedMonomerReferences` <!-- rq-789635d3 -->
  — the same selection as `collect_monomers`, read through the corrected
  keys `elcorr`/`hcorr`/`gcorr` (`rqm/correction.md`) and the raw T·S
  entropy term.
- `corrected_formation_values(record: &Record, references: &CorrectedMonomerReferences, memo: &ThermoMemo, concs: &ConcConditions) -> Formation` <!-- rq-fb892986 -->
  — the four corrected levels with their definedness; the corrected Gibbs
  value has the concentration term subtracted and the corrected
  entropy-term value has it added, with the same non-finite guard; a
  record without a correction has no corrected value at any level.
- `print_formation_corr_section(records: &[Record], references: &CorrectedMonomerReferences, memo: &ThermoMemo, unit: Unit, concs: &ConcConditions) -> String` <!-- rq-4f20e170 -->
  — the corrected section table with header `name elcorr hcorr stcorr
  gcorr`.
- `collapse_ensembles(records: &[Record], memo: &ThermoMemo) -> Vec<Ensemble>` <!-- rq-18dba668 -->
  — the raw section's collapse (Boltzmann-Averaged Sections): groups by
  components map in first-appearance order, the records without
  components last as the unnamed ensemble; members, weights, and the four
  values per that section, with the entropy-term value computed as
  H_ens − G_ens.
- `corrected_collapse_ensembles(records: &[Record], memo: &ThermoMemo) -> Vec<Ensemble>` <!-- rq-f162476c -->
  — the corrected section's collapse: members and inputs resolved through
  `hcorr`, `gcorr`, and the correction energy (`rqm/correction.md`),
  otherwise identical.
- `ensemble_references(ensembles: &[Ensemble]) -> MonomerReferences` <!-- rq-27a3b339 -->
  — the reference scan over ensembles: each monomer ensemble supplies its
  four reference values for its code; ensembles without values contribute
  nothing.
- `corrected_ensemble_references(ensembles: &[Ensemble]) -> CorrectedMonomerReferences` <!-- rq-4b8a9f98 -->
  — the same scan read as the corrected references.
- `ensemble_formation_values(ensemble: &Ensemble, references: &MonomerReferences, memo: &ThermoMemo, concs: &ConcConditions) -> Formation` <!-- rq-eafc5e8e -->
  — the four levels of one collapsed row: the ensemble's values minus the
  stoichiometric references, with the concentration term subtracted from
  the Gibbs value and added to the entropy-term value, using the same
  non-finite guard; every level is undefined when the ensemble has no
  values.
- `corrected_ensemble_formation_values(ensemble: &Ensemble, references: &CorrectedMonomerReferences, memo: &ThermoMemo, concs: &ConcConditions) -> Formation` <!-- rq-f5583f17 -->
  — the corrected row's four levels, same rule.
- `print_formation_ensembles(ensembles: &[Ensemble], references: &MonomerReferences, memo: &ThermoMemo, unit: Unit, concs: &ConcConditions) -> String` <!-- rq-30ea9ebc -->
  — the collapsed raw section: header `name el h st g`, one row per
  ensemble in the given order, `-` for undefined values and for the
  unnamed ensemble's token.
- `print_formation_corr_ensembles(ensembles: &[Ensemble], references: &CorrectedMonomerReferences, memo: &ThermoMemo, unit: Unit, concs: &ConcConditions) -> String` <!-- rq-4c1bcb87 -->
  — the collapsed corrected section: header `name elcorr hcorr stcorr
  gcorr`.

### Crate Layout <!-- rq-22e7999c -->

- The scan, values, and sections live in `src/formation.rs`; the library
  root re-exports the types and functions. No new dependencies; the CLI
  adds `--formation`, `--formation-corr`, `--conc`, `--cnt`, and
  `--bavg`, parses the specs into `ConcConditions` with the usage-error
  validation above, and wires both sections into the printing block —
  through the ensemble path when `--bavg` is set. The collapse reuses the
  Boltzmann populations of `rqm/population.md`.
- `tests/formation.rs` tests the scan and the values as pure functions <!-- rq-eb7da353 -->
  (sort-based reference selection, per-level definedness, missing
  references, stoichiometry, and the g_form = h_form − st_form identity),
  the section formatting, the corrected section's selection and
  definedness, the VALUE grammar and pressure resolution, and the
  concentration correction's stoichiometry and self-consistent factor;
  the ensemble collapse (grouping, member definedness, the
  population-weighted electronic and enthalpy means, the
  partition-function Gibbs against hand-computed splits, and the entropy
  term as H_ens − G_ens), the ensemble references, and the collapsed
  sections' formatting and concentration terms extend the same tests;
  CLI coverage extends `tests/cli.rs`.

## Out of Scope <!-- rq-39f86935 -->

- A separate monomer reference file or database: references come only
  from the run's own inputs.
- Formation energies for records without components (no composition, no
  reference stoichiometry) — they print `-`.
- Per-cluster reference overrides (pointing a code at a hand-picked
  conformer rather than the sort-selected one).
- Standard-state corrections to the thermochemistry model itself:
  `rqm/thermo.md` stays the ideal gas at 1 atm — `--conc` shifts only the
  formation levels and never re-derives a record's translational entropy —
  and liquid/solvent-phase reference states are not modeled.
- Vapor-depletion or Kelvin-equation self-consistency beyond `--cnt`'s
  (N−1)/N scaling: the given concentrations are taken as fixed inputs, not
  solved for.
- Boltzmann-population and other ensemble-derived properties beyond the
  formation sections: `rqm/population.md` computes populations on the run's
  final record set, like the formation values.
- A Boltzmann collapse of the main table: JKQC's `-bavg` averages every
  printed column over each composition; CJKQC's `--bavg` reshapes only the
  formation sections, and the main table always prints one row per record.
- The `-bavgout` flag name: the corrected section is averaged by the same
  `--bavg` at its own level, so no second flag exists.
- A separate formation entropy in J mol⁻¹ K⁻¹ and formation ZPE columns:
  the sections print the `st` energy term (T·S), matching the main table's
  `s` column, plus the `el`, `h`, and `g` levels.

## Gherkin Scenarios <!-- rq-077d9eed -->

```gherkin
Feature: Formation energies from monomer references

  Background:
    Given the default ThermoConditions of rqm/thermo.md

  # --- Reference scan ---

  @rq-34845056
  Scenario: A single-component count-one record is a monomer
    Given an ok record with components am = 1
    When collect_monomers is called with it
    Then the electronic reference for code "am" is that record's electronic energy

  @rq-56585302
  Scenario: Multi-component and multi-count records are not monomers
    Given ok records with components am = 2 and am = 1 sa = 1
    When collect_monomers is called with them
    Then no reference for "am" or "sa" is recorded from them

  @rq-64526ba5
  Scenario: Without --sort the lowest-energy conformer is the reference
    Given two ok records with components am = 1 and electronic energies -50.0 and -50.5
    When collect_monomers is called with them and the el sort key
    Then the electronic reference for "am" is -50.5

  @rq-64a72e10
  Scenario: --sort g makes the lowest-gibbs conformer the reference for every level
    Given conformer A with the lowest electronic energy and conformer B with the lowest gibbs free energy, both am = 1
    When collect_monomers is called with them and the g sort key
    Then the electronic reference is conformer B's energy
    And the enthalpy, entropy-term, and gibbs references are conformer B's values

  @rq-90e2ea3e
  Scenario: --sort filename takes the nameless monomer as the reference
    Given a nameless ok monomer with components am = 1 and electronic energy -50.0 and a named ok am = 1 monomer with electronic energy -50.5
    When collect_monomers is called with them and the filename sort key
    Then the electronic reference for "am" is -50.0

  @rq-feddee4c
  Scenario: --reverse does not change the reference
    Given a 1am cluster and two am monomers whose gibbs values are -50.0 and -50.5
    When CJKQC is invoked with the database and "--sort g --reverse --formation" and no --out
    Then the cluster's g formation value is computed against the -50.5 monomer's gibbs

  @rq-7da88087
  Scenario: The reference's undefined level leaves that level undefined
    Given a na monomer chosen by the sort key that has an energy but no frequencies
    When collect_monomers is called with the rg sort key
    Then the electronic reference is defined
    And the enthalpy, entropy-term, and gibbs references are absent

  @rq-536786ea
  Scenario: Failed monomer records never qualify
    Given a failed record with components am = 1 and an ok record with components am = 1
    When collect_monomers is called with them
    Then the references come from the ok record alone

  @rq-9063d573
  Scenario: A monomer without thermochemistry has no thermal reference
    Given an ok single-component record without vibrational frequencies
    When collect_monomers is called with it
    Then the electronic reference is defined
    And the enthalpy, entropy-term, and gibbs references are absent

  # --- Values ---

  @rq-44b86475
  Scenario: A 2am1sa cluster forms from its stoichiometric monomers
    Given references am = -50.0 and sa = -40.0 and a cluster record with components am = 2 sa = 1 and electronic energy -141.0
    When formation_values is called
    Then the formation energy is -1.0 Eh

  @rq-cfc7f6f3
  Scenario: A 2am1sa cluster has a stoichiometric formation enthalpy
    Given enthalpy references am = -49.9 and sa = -39.9 and a cluster record with components am = 2 sa = 1 and enthalpy -140.7
    When formation_values is called
    Then the formation enthalpy is -1.0 Eh

  @rq-91cdbab5
  Scenario: A 2am1sa cluster has a stoichiometric formation entropy term
    Given T*S references am = 0.01 and sa = 0.02 and a cluster record with components am = 2 sa = 1 and T*S value 0.045
    When formation_values is called
    Then the formation entropy term is 0.005 Eh
    And the formation free energy is the formation enthalpy minus the formation entropy term

  @rq-36de901e
  Scenario: A monomer's own formation values are zero
    Given a record with components am = 1 and the am references
    When formation_values is called
    Then the formation energy, formation enthalpy, formation entropy term, and formation free energy are 0

  @rq-1b5e6c7a
  Scenario: A missing code reference leaves that level undefined
    Given a cluster with components am = 1 na = 1 and a database holding only the am monomer
    When formation_values is called
    Then the electronic level is None

  @rq-650192f2
  Scenario: The electronic level is independent of the thermal levels
    Given a cluster whose only na monomer has an energy but no frequencies
    When formation_values is called
    Then the electronic level is Some
    And the enthalpy, entropy-term, and gibbs levels are None

  @rq-22dd2e0b
  Scenario: A thermal reference gap leaves the three thermal levels undefined together
    Given a cluster with components am = 1 na = 1, an am monomer with thermochemistry, and an na monomer whose enthalpy reference is absent
    When formation_values is called
    Then the electronic level is Some if both electronic references exist
    And the enthalpy, entropy-term, and gibbs levels are None

  @rq-b02bcba1
  Scenario: Records without components form at no level
    Given a record without components
    When formation_values is called
    Then every level is None

  # --- Printing ---

  @rq-2d5b9aba
  Scenario: The section identifies rows by composition token
    Given a database with a 2am1sa cluster and am and sa monomers
    When CJKQC is invoked with the database and "--formation" and no --out
    Then the section header names the columns name, el, h, st, g in that order
    And the cluster's row starts with the composition token 2am1sa

  @rq-ed9e179c
  Scenario: A record without components has a dash composition token
    Given a database with a single ok record without components
    When CJKQC is invoked with the database and "--formation" and no --out
    Then its section row prints - at all five columns

  @rq-27c4fe8e
  Scenario: The section aligns its columns like the main table
    Given a database with a 2am1sa cluster with defined formation values and am and sa monomers
    When CJKQC is invoked with the database and "--formation" and no --out
    Then the name cells left-align and the el, h, st, and g cells right-align, header included, each column as wide as its widest cell

  @rq-1b6b5757
  Scenario: The formation section follows the table and precedes the summary
    Given a database with a 2am cluster and am and sa monomers
    When CJKQC is invoked with the database and "--sort el --filename --el --formation --out kept.jsonl"
    Then stdout shows the main table, then the formation section, then "filtered 3 records into kept.jsonl"

  @rq-0f78a33b
  Scenario: --formation alone is a visible outcome
    Given a database with a cluster and its monomers
    When CJKQC is invoked with the database and "--formation" and no --out
    Then the formation section is printed and no database file is written

  @rq-043c29fe
  Scenario: Undefined levels print as dashes
    Given a database with a cluster referencing a missing monomer code
    When CJKQC is invoked with the database and "--formation" and no --out
    Then the cluster's formation row shows - for every level

  @rq-31a55ccc
  Scenario: Filtered-away monomers serve as no references
    Given a database with am and sa monomers and a 1am1sa cluster
    When CJKQC is invoked with the database, "--extract 1am1sa --formation" and no --out
    Then the section prints one row for the cluster with - for every level

  @rq-b46a6e05
  Scenario: A print-only run sees every loaded monomer
    Given a database with am and sa monomers and a 1am1sa cluster
    When CJKQC is invoked with the database and "--formation" and no --out
    Then the cluster's formation row shows defined values

  @rq-9c5b3a40
  Scenario: The section obeys the unit setting
    Given a cluster with formation energy -0.01 Eh, formation enthalpy 0.02 Eh, and formation entropy term 0.03 Eh
    When CJKQC is invoked with the database and "--formation --unit" and no --out
    Then the el, h, and st columns print their values in kcal/mol with 4 decimals

  @rq-39ca9fe7
  Scenario: The printed levels satisfy g = h - st
    Given a cluster with defined formation values and its monomers
    When CJKQC is invoked with the database and "--formation" and no --out
    Then the cluster's printed g cell equals its h cell minus its st cell within the printed precision

  @rq-e7f0ef7b
  Scenario: The thermal levels follow the run's temperature
    Given a cluster and its monomers with defined thermochemistry
    When the same database is run with "--formation" at --temp 298.15 and at --temp 350
    Then the two runs' h, st, and g columns differ
    And the two runs' el columns are equal

  # --- Corrected formation section ---

  @rq-39641660
  Scenario: The corrected section evaluates the attached corrections
    Given a 1am1sa cluster and am and sa monomers all carrying corrections
    When CJKQC is invoked with the database and "--formation-corr" and no --out
    Then the header names the columns name, elcorr, hcorr, stcorr, gcorr in that order and the cluster's row shows defined values

  @rq-cdb75b97
  Scenario: The corrected formation enthalpy subtracts the corrected monomer enthalpies
    Given a corrected 1am1sa cluster and corrected am and sa monomers with hcorr values
    When corrected_formation_values is called
    Then the corrected formation enthalpy is hcorr(cluster) - hcorr(am) - hcorr(sa)
    And the corrected formation enthalpy minus the corrected formation entropy term equals the corrected formation free energy

  @rq-4ca420a5
  Scenario: The corrected entropy term equals the raw entropy term
    Given a corrected 1am1sa cluster with defined thermochemistry and corrected am and sa monomers
    When the same database is run with "--formation --formation-corr" and no --out
    Then the cluster's stcorr cell equals its st cell
    And a cluster without a correction prints dashes in the stcorr column

  @rq-20ab3295
  Scenario: A record without a correction has dashes in the corrected section
    Given a 1am1sa cluster without a correction and corrected am and sa monomers
    When CJKQC is invoked with the database and "--formation-corr" and no --out
    Then the cluster's row prints the token 1am1sa with dashes at every level

  @rq-5ca4f95d
  Scenario: A reference monomer without a correction leaves every level undefined
    Given a 1am cluster with a correction whose am monomer has none
    When CJKQC is invoked with the database and "--formation-corr" and no --out
    Then the cluster's row shows - for every level

  @rq-b6409abd
  Scenario: The corrected section reuses the raw section's reference monomers
    Given two am monomers where the sort selects the one without a correction even though the other carries a lower corrected energy
    When CJKQC is invoked with the database and "--sort el --formation-corr" and no --out
    Then the cluster's corrected formation row shows - for every level

  @rq-34d743c7
  Scenario: The corrected section follows the raw section and precedes the summary
    Given a database with a corrected cluster and its corrected monomers
    When CJKQC is invoked with the database and "--formation --formation-corr --out kept.jsonl"
    Then stdout shows the main table, the formation section, the corrected formation section, then "filtered <n> records into kept.jsonl"

  @rq-09e99e7b
  Scenario: --formation-corr alone is a visible outcome
    Given a database with a corrected cluster and its corrected monomers
    When CJKQC is invoked with the database and "--formation-corr" and no --out
    Then the corrected section is printed and no database file is written

  @rq-6e0560c2
  Scenario: The corrected section obeys the unit setting
    Given a cluster whose corrected formation energy is -0.01 Eh
    When CJKQC is invoked with the database and "--formation-corr --unit" and no --out
    Then the elcorr column prints the value in kcal/mol with 4 decimals

  @rq-f99b56ad
  Scenario: Filtered-away monomers serve as no corrected references
    Given corrected am and sa monomers and a corrected 1am1sa cluster
    When CJKQC is invoked with the database, "--extract 1am1sa --formation-corr" and no --out
    Then the section prints one row for the cluster with - for every level

  # --- Concentrations ---

  @rq-e982f0e0
  Scenario: A plain value is a partial pressure in Pa
    When parse_conc_value is called with "0.00001"
    Then it returns 0.00001 in unit Pa

  @rq-d7a0c24f
  Scenario: A ppt value is a mole fraction of the standard atmosphere
    When parse_conc_value is called with "100ppt"
    Then it returns 100 in unit Ppt
    And the spec's pressure at 298.15 K is 100*10^-12*p° = 1.01325e-5 Pa

  @rq-183b0f2b
  Scenario: A ppb value is a mole fraction of the standard atmosphere
    When parse_conc_value is called with "10ppb"
    Then it returns 10 in unit Ppb
    And the spec's pressure at 298.15 K is 10*10^-9*p° = 1.01325e-3 Pa

  @rq-0f5b209d
  Scenario: A cmmc value converts through the ideal-gas law at the run's temperature
    When parse_conc_value is called with "1e10cmmc"
    Then it returns 1e10 in unit Cmmc
    And the spec's pressure at 298.15 K is 1e10*10^6*k_B*298.15 = 4.1164e-5 Pa
    And the spec's pressure at 350 K is 4.8323e-5 Pa

  @rq-9423306b
  Scenario: A value outside the grammar is a usage error
    Given a database with am and sa monomers
    When CJKQC is invoked with the database and "--formation --conc sa 100ppm" and no --out
    Then the exit code is 2

  @rq-890737af
  Scenario: A non-positive or non-finite value is a usage error
    Given a database with am and sa monomers
    When CJKQC is invoked with the database and "--formation --conc sa 0" and no --out
    Then the exit code is 2
    And the same holds for "-1e-7", "nan", and "inf"

  @rq-ae14b02d
  Scenario: The caret power syntax is not part of the grammar
    Given a database with am and sa monomers
    When CJKQC is invoked with the database and "--formation --conc sa 10^3ppt" and no --out
    Then the exit code is 2

  @rq-3384f0c4
  Scenario: A non-letter code is a usage error
    Given a database with am and sa monomers
    When CJKQC is invoked with the database and "--formation --conc 2sa 100ppt" and no --out
    Then the exit code is 2

  @rq-fecf8cb9
  Scenario: Naming one code twice is a usage error
    Given a database with am and sa monomers
    When CJKQC is invoked with the database and "--formation --conc sa 100ppt --conc sa 1ppb" and no --out
    Then the exit code is 2

  @rq-98440673
  Scenario: The named component shifts the g level by its chemical potential
    Given a database with a 1am1sa cluster whose formation free energy is -10.0 Eh under the am and sa monomers
    When CJKQC is invoked with the database and "--formation --conc sa 100ppt" and no --out
    Then the cluster's g formation value is -10.0 Eh + R T ln(10^10), about 13.6 kcal/mol higher
    And the cluster's st formation value is its standard-state value - R T ln(10^10)
    And the cluster's el and h formation values are unchanged
    And the cluster's g value still equals its h value minus its st value

  @rq-77502cf6
  Scenario: The concentration term moves g and st by equal and opposite amounts
    Given a database with a 1am1sa cluster with defined thermochemistry and its am and sa monomers
    When the same database is run with "--formation" and with "--formation --conc sa 100ppt"
    Then the concentration run's g cell is higher by exactly the amount its st cell is lower
    And both runs' h cells are equal

  @rq-85150fe0
  Scenario: Several --conc flags each cover their own component
    Given a database with a 2am1sa cluster whose formation free energy is g_form under the am and sa monomers
    When CJKQC is invoked with the database and "--formation --conc am 100ppt --conc sa 10ppb" and no --out
    Then the cluster's g formation value is g_form + 2 R T ln(10^10) + R T ln(10^8)

  @rq-bbf4e72d
  Scenario: A component without a spec stays at the standard state
    Given a 2am1sa record with references and a single spec for sa
    When formation_values is called
    Then the gibbs level is g_form - R T ln(c_sa/p°) with no am term

  @rq-1b039c1f
  Scenario: --cnt scales a fully covered cluster's correction by (N-1)/N
    Given a 3sa record with the sa reference and a sa spec
    When formation_values is called with self_consistent off and with self_consistent on
    Then the off gibbs level is g_form - 3 R T ln(c_sa/p°)
    And the on gibbs level is g_form - 2 R T ln(c_sa/p°)

  @rq-040dd0d3
  Scenario: --cnt keeps a monomer's formation values at zero
    Given a database with a 1sa monomer with defined thermochemistry
    When CJKQC is invoked with the database and "--formation --cnt --conc sa 100ppt" and no --out
    Then the monomer's g and st formation values are 0
    And without --cnt its g value is +R T ln(10^10), about 13.6 kcal/mol, and its st value is -R T ln(10^10)

  @rq-fd428ce5
  Scenario: A concentration above the standard state lowers the formation free energy
    Given a database with a 2sa cluster whose formation free energy is -5.0 Eh under the sa monomer
    When CJKQC is invoked with the database and "--formation --conc sa 202650" and no --out
    Then the cluster's g formation value is -5.0 Eh - 2 R T ln(2)

  @rq-b5d0f3d1
  Scenario: A spec at the standard pressure reproduces the uncorrected values
    Given a database with a 1am1sa cluster and its am and sa monomers
    When the same database is run with "--formation" and with "--formation --conc sa 101325 --conc am 101325"
    Then the two runs' formation sections are identical

  @rq-9b67220d
  Scenario: The correction never creates a defined value
    Given a database with am and sa monomers and a 1am1sa cluster
    When CJKQC is invoked with the database, "--extract 1am1sa --formation --conc sa 100ppt" and no --out
    Then the cluster's row shows - for every level

  @rq-c984bd29
  Scenario: Undefined thermochemistry stays undefined under --conc
    Given a sa monomer with defined thermochemistry and a 1sa cluster record without vibrational frequencies
    When CJKQC is invoked with the database and "--formation --conc sa 100ppt" and no --out
    Then the cluster's g formation value prints -

  @rq-e8ea3b7f
  Scenario: The corrected section shifts gcorr and stcorr the same way
    Given a 1am1sa cluster and am and sa monomers all carrying corrections
    When CJKQC is invoked with the database and "--formation-corr --conc sa 100ppt" and no --out
    Then the cluster's gcorr formation value is its standard-state value + R T ln(10^10)
    And the cluster's stcorr formation value is its standard-state value - R T ln(10^10)
    And the cluster's elcorr and hcorr formation values are unchanged

  @rq-985cc1a5
  Scenario: A spec for a code in no final record is a usage error
    Given a database with am and sa monomers and a 1am1sa cluster
    When CJKQC is invoked with the database and "--formation --conc w 100ppt" and no --out
    Then the exit code is 2

  @rq-38b48ce6
  Scenario: A spec whose monomers were filtered away is not a usage error
    Given a database with am and sa monomers and a 1am1sa cluster
    When CJKQC is invoked with the database, "--extract 1am1sa --formation --conc am 100ppt --conc sa 100ppt" and no --out
    Then the exit code is 0 and the cluster's row shows - for every level

  @rq-75ac7572
  Scenario: --conc without a formation section is an accepted no-op
    Given a database with one record
    When CJKQC is invoked with the database and "--filename --el --conc sa 100ppt" and no --out
    Then the table prints normally, identical to the run without --conc

  @rq-d6fc375d
  Scenario: --conc alone is not a visible outcome
    When CJKQC is invoked with a database and "--conc sa 100ppt" and no --out and no print flag
    Then the exit code is 2

  @rq-6f3e4732
  Scenario: The main table and --glob ignore the concentrations
    Given a database with a 2am1sa cluster and its am and sa monomers
    When CJKQC is invoked with the database and "--name --g --glob --formation --conc sa 100ppt" and no --out
    Then the main table's g column equals the run without --conc
    And the formation section's g value carries the concentration term

  @rq-ef6c9159
  Scenario: The ppt correction scales linearly with temperature
    Given a 1am1sa cluster with defined thermochemistry and a sa spec of 100ppt
    When the same database is run with "--formation --conc sa 100ppt" at --temp 298.15 and at --temp 350
    Then the shift from the standard-state value at 350 K is 350/298.15 times the shift at 298.15 K

  @rq-f623ed28
  Scenario: An absurd magnitude leaves the level undefined rather than infinite
    Given a database with an am monomer, an sa monomer, and a 2am1sa cluster with full thermochemistry
    When CJKQC is invoked with the database and "--formation --conc sa 1e-320" and no --out
    Then the run exits 0
    And every sa-covered row's g formation value prints -

  # --- Boltzmann averaging ---

  @rq-85dab208
  Scenario: --bavg prints one row per composition
    Given a database with two 2am1sa conformers, an am monomer, and an sa monomer
    When CJKQC is invoked with the database and "--formation --bavg" and no --out
    Then the section header names the columns name, el, h, st, g in that order
    And the section prints three rows: 2am1sa, 1am, and 1sa

  @rq-7d43ccc9
  Scenario: The collapsed g includes the conformational entropy term
    Given two 2am1sa conformers whose gibbs values differ by kB*T*ln(3) and whose electronic energies and enthalpies are equal, and the am and sa monomers
    When CJKQC is invoked with the database and "--formation --bavg" and no --out
    Then the cluster's el formation value equals either conformer's
    And the cluster's g formation value is k_B*T*ln(4/3) below the best conformer's
    And the cluster's st formation value carries the same k_B*T*ln(4/3) shift relative to the best conformer's
    And the cluster's h formation value equals either conformer's

  @rq-df7a2c88
  Scenario: The collapsed entropy term is the mean enthalpy minus the ensemble Gibbs
    Given two 2am1sa conformers with equal electronic energies and enthalpies whose gibbs values differ by kB*T*ln(3), and the am and sa monomers
    When CJKQC is invoked with the database and "--formation --bavg" and no --out
    Then the collapsed row's h formation value equals the population-weighted mean of the conformers' h formation values
    And the collapsed row's st formation value equals its h formation value minus its g formation value
    And the collapsed row's g formation value is the ensemble free energy

  @rq-f524bb50
  Scenario: The collapsed el is the population-weighted mean
    Given two 2am1sa conformers with equal gibbs values and electronic energies 0.02 Eh apart, and the am and sa monomers
    When CJKQC is invoked with the database and "--formation --bavg" and no --out
    Then the cluster's el formation value is the midpoint of the two conformers' el formation values

  @rq-f71670c3
  Scenario: Monomer rows print exactly zero
    Given a database with two am conformers with defined thermochemistry and a 2am cluster
    When CJKQC is invoked with the database and "--formation --bavg" and no --out
    Then the 1am row prints 0 at every level

  @rq-c39939f4
  Scenario: Ensemble references make the cluster less favorable than its best conformer
    Given two am monomers whose gibbs values differ by kB*T*ln(3) and whose electronic energies are equal, and a 2am cluster with defined thermochemistry
    When the same database is run with "--formation --sort g" and with "--formation --sort g --bavg"
    Then the two runs' cluster el formation values are equal
    And the averaged run's cluster g formation value is 2*k_B*T*ln(4/3) above the uncollapsed run's
    And the averaged run's 1am row prints 0 at every level

  @rq-e1615410
  Scenario: A single-point-only group prints dashes under --bavg
    Given two 2am1sa single-point records without frequencies and am and sa monomers with full thermochemistry
    When CJKQC is invoked with the database and "--formation --bavg" and no --out
    Then the 2am1sa row shows - for every level
    And without --bavg the same run prints the cluster's el formation value

  @rq-0df193f0
  Scenario: A monomer group without any Gibbs loses its reference at every level
    Given a 2am cluster with full thermochemistry, a single-point am monomer without frequencies, and an sa monomer with full thermochemistry
    When CJKQC is invoked with the database and "--formation --bavg" and no --out
    Then the cluster's row shows - for every level
    And without --bavg the cluster's el formation value is defined and its g is -

  @rq-1ad3c076
  Scenario: Records without components form one unnamed ensemble
    Given two records without components and an am monomer
    When CJKQC is invoked with the database and "--formation --bavg" and no --out
    Then the section prints exactly two rows: 1am and one all-dash row

  @rq-30bd57ee
  Scenario: --glob makes the collapse the identity
    Given a database with two 2am1sa conformers and am and sa monomers
    When the same database is run with "--glob --formation" and with "--glob --formation --bavg"
    Then the two runs' formation sections are identical

  @rq-f93d319b
  Scenario: Undefined members never poison the group
    Given three 2am1sa records, two with full thermochemistry whose gibbs values differ by kB*T*ln(3) and one without frequencies, and the am and sa monomers
    When CJKQC is invoked with the database and "--formation --bavg" and no --out
    Then the cluster's g formation value is k_B*T*ln(4/3) below the best conformer's

  @rq-482ab490
  Scenario: Failed records contribute nothing to the collapse
    Given a failed 2am1sa record and an ok 2am1sa record with full thermochemistry, and the am and sa monomers
    When CJKQC is invoked with the database and "--formation --bavg" and no --out
    Then the section prints one 2am1sa row with defined values

  @rq-deca4a24
  Scenario: The corrected section collapses at its own level
    Given two corrected 1am1sa conformers whose gcorr values differ by kB*T*ln(3) and whose correction energies and hcorr values are equal, and corrected am and sa monomers
    When CJKQC is invoked with the database and "--formation-corr --bavg" and no --out
    Then the cluster's elcorr formation value equals either conformer's
    And the cluster's gcorr formation value is k_B*T*ln(4/3) below the best conformer's
    And the cluster's hcorr formation value equals either conformer's
    And the cluster's stcorr formation value equals its hcorr value minus its gcorr value

  @rq-ade63c60
  Scenario: A corrected member without thermochemistry contributes nothing
    Given two corrected 1am1sa conformers, one without frequencies, and corrected am and sa monomers
    When the same database is run with "--formation-corr" and with "--formation-corr --bavg"
    Then the uncollapsed section prints two rows and the collapsed section one
    And the collapsed row's values equal the full conformer's uncollapsed row's values

  @rq-c959877c
  Scenario: Both sections collapse when both are printed
    Given a database with corrected 2am1sa conformers and corrected am and sa monomers
    When CJKQC is invoked with the database and "--formation --formation-corr --bavg" and no --out
    Then both sections print one row per composition

  @rq-f0980405
  Scenario: The collapsed row carries the concentration term
    Given a database with two 2sa conformers whose gibbs values differ by kB*T*ln(3) and an sa monomer
    When the same database is run with "--formation --bavg" and with "--formation --bavg --conc sa 100ppt"
    Then the conc run's collapsed g formation value is 2*R*T*ln(10^10) above the plain run's
    And the conc run's collapsed st formation value is 2*R*T*ln(10^10) below the plain run's
    And the two runs' collapsed h formation values are equal

  @rq-a74b25aa
  Scenario: --cnt scales the collapsed concentration term by (N-1)/N
    Given a database with two 2sa conformers whose gibbs values differ by kB*T*ln(3) and an sa monomer
    When the same database is run with "--formation --bavg --conc sa 100ppt" and with "--formation --bavg --cnt --conc sa 100ppt"
    Then the cnt run's collapsed g formation value is R*T*ln(10^10) below the conc run's, the term 2*(N-1)/N = 1

  @rq-de842863
  Scenario: --noname does not pool the collapse
    Given a database with 2am conformers and 1sa conformers and their monomers
    When CJKQC is invoked with the database and "--noname --formation --bavg" and no --out
    Then the section still prints one row per composition, not one global row

  @rq-7721f6ff
  Scenario: The collapse preserves first-appearance order
    Given a database loaded in the order 1am1sa, 2am, a second 1am1sa, 1sa
    When CJKQC is invoked with the database and "--formation --bavg" and no --out
    Then the section rows are 1am1sa, 2am, 1sa

  @rq-94cec13a
  Scenario: --bavg leaves the main table per record
    Given a database with three 2am1sa conformers and the am and sa monomers
    When CJKQC is invoked with the database and "--name --g --formation --bavg" and no --out
    Then the main table prints three rows
    And the formation section prints one row per composition

  @rq-9f5be6e0
  Scenario: --bavg without a formation section is an accepted no-op
    Given a database with one record
    When CJKQC is invoked with the database and "--filename --el --bavg" and no --out
    Then the table prints normally, identical to the run without --bavg

  @rq-b4bcf592
  Scenario: --bavg alone is not a visible outcome
    When CJKQC is invoked with a database and "--bavg" and no --out and no print flag
    Then the exit code is 2

  @rq-603dd405
  Scenario: --bavgout is a usage error
    Given a database with one record
    When CJKQC is invoked with the database and "--formation --bavgout" and no --out
    Then the exit code is 2
```
