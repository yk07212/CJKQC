//! CJKQC — a spiritual successor to JKQC from JKCS2.1.
//!
//! One command line does collection (`rqm/collection.md`) and the
//! manipulation pipeline with JKQC-style per-column printing
//! (`rqm/filtering.md`). Flags mirror JKQC's spelling with `--` because
//! clap does not support single-dash long flags.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{ArgMatches, CommandFactory, FromArgMatches, Parser};

use compiled_jkqc::collect::{gather_inputs_with, CollectError, CollectOutcome, Gathered};
use compiled_jkqc::correction::{attach_corrections, CorrectionError};
use compiled_jkqc::db::{self, DbError, LoadProblem};
use compiled_jkqc::extrapolate::{
    correction_for, is_candidate, parse_extrapolation, skip_reason, source_paths,
    ExtrapolationError, ExtrapolationSpec,
};
use compiled_jkqc::filter::{
    apply_filters, parse_extract_query, print_table, CmpOp, Column, Cutoff, ExtractMode,
    FilterSpec, Grouping, Mode, Property, SortKey, Unit,
};
use compiled_jkqc::formation::{
    collapse_ensembles, collect_monomers, corrected_collapse_ensembles,
    corrected_ensemble_references, corrected_monomer_references, ensemble_references,
    parse_conc_value, print_formation_corr_ensembles, print_formation_corr_section,
    print_formation_ensembles, print_formation_section, ConcConditions, ConcSpec, ConcUnit,
};
use compiled_jkqc::hydration::{
    corrected_hydration_states, corrected_hydration_states_from_ensembles, hydration_states,
    hydration_states_from_ensembles, print_hydration_section, saturation_pressure_water,
    HydrationGroup, SolventConditions,
};
use compiled_jkqc::metrics::parse_uniq_mode;
use compiled_jkqc::movie::{movie_frames, write_movie_xyz};
use compiled_jkqc::overwrite::{confirm_overwrite, overwrite_error};
use compiled_jkqc::parser::WindowBudget;
use compiled_jkqc::population::{parse_pop_level, PopLevel};
use compiled_jkqc::reacted::ReactedMode;
use compiled_jkqc::record::{Record, RecordStatus};
use compiled_jkqc::thermo::{ThermoConditions, ThermoMemo};

/// The cutoff flags: clap arg id, comparison, and mode
/// (`rqm/filtering.md`).
const CUTOFF_FLAGS: &[(&str, CmpOp, Mode)] = &[
    ("cut", CmpOp::Le, Mode::Absolute),
    ("filter_lt", CmpOp::Lt, Mode::Absolute),
    ("pass", CmpOp::Gt, Mode::Absolute),
    ("filter_ge", CmpOp::Ge, Mode::Absolute),
    ("is", CmpOp::Eq, Mode::Absolute),
    ("isnot", CmpOp::Ne, Mode::Absolute),
    ("cutr", CmpOp::Le, Mode::Relative),
    ("rel_filter_lt", CmpOp::Lt, Mode::Relative),
    ("passr", CmpOp::Gt, Mode::Relative),
    ("rel_filter_ge", CmpOp::Ge, Mode::Relative),
];

// rq-a4e0ba89 rq-9b4dda85
#[derive(Parser)]
#[command(
    name = "CJKQC",
    version,
    about = "Collect, organize, filter, and post-process quantum-chemistry output for molecular clusters",
    after_help = "Cutoff/sort/uniqueness PROPERTY: el (electronic energy), g (Gibbs at --temp), elcorr (correction energy), gcorr (Gibbs from the correction), rg (radius of gyration), lf (lowest frequency), dip (dipole magnitude), mass, charge, multiplicity, natoms, nfreq, nimag.\nEnergy units: absolute cutoffs compare hartree, relative cutoffs kcal/mol; --unit switches printing to kcal/mol."
)]
struct Cli {
    /// Database to write; required unless a manipulation or print flag
    /// makes it optional.
    #[arg(short = 'o', long = "out")]
    out: Option<PathBuf>,

    /// Force the replacement of an existing --out (or movie.xyz) without
    /// the confirmation question (rqm/collection.md).
    #[arg(long = "fout")]
    fout: bool,

    /// Keep only records whose composition matches the query.
    #[arg(long = "extract")]
    extract: Vec<String>,

    /// Drop records whose composition matches the query.
    #[arg(long = "nextract")]
    nextract: Vec<String>,

    /// Uniqueness: `dup` or a `metric[decimals]` comma list
    /// (el2.4,g,rg).
    #[arg(long = "uniq")]
    uniq: Option<String>,

    /// Keep the composition group's majority fragment pattern.
    #[arg(long = "reacted")]
    reacted: bool,

    /// Keep only records deviating from the fragment consensus.
    #[arg(long = "reacted2")]
    reacted2: bool,

    /// Sort by el, g (Gibbs at --temp), elcorr, gcorr, rg, mass, or name.
    #[arg(long = "sort")]
    sort: Option<String>,

    /// Reverse the sort.
    #[arg(long = "reverse")]
    reverse: bool,

    /// Keep the first N records of each group.
    #[arg(long = "select")]
    select: Option<usize>,

    /// Keep each group's lowest Gibbs free energy record.
    #[arg(long = "glob")]
    glob: bool,

    /// Shuffle the final result.
    #[arg(long = "shuffle")]
    shuffle: bool,

    /// Seed for --shuffle.
    #[arg(long = "seed")]
    seed: Option<u64>,

    /// Treat the whole database as one group; extract compares names.
    #[arg(long = "noname")]
    noname: bool,

    // --- settings flags: configure derivations and units (rqm/thermo.md) ---
    /// Temperature in K for thermochemistry (default 298.15).
    #[arg(long = "temp", allow_hyphen_values = true)]
    temp: Option<f64>,

    /// Quasi-harmonic reference frequency in cm^-1 (default 100; 0 = harmonic).
    #[arg(long = "fc", allow_hyphen_values = true)]
    fc: Option<f64>,

    /// Vibrational frequency scale factor (default 1.0).
    #[arg(long = "fscale", allow_hyphen_values = true)]
    fscale: Option<f64>,

    /// Flip imaginary frequencies to real inside the thermochemistry.
    #[arg(long = "makereal")]
    makereal: bool,

    /// Bytes of each input file's head window (default 262144). Raise
    /// --head-bytes/--tail-bytes when a record's error says a table was
    /// cut by the window seam.
    #[arg(long = "head-bytes", value_name = "BYTES")]
    head_bytes: Option<usize>,

    /// Bytes of each input file's tail window (default 4194304).
    #[arg(long = "tail-bytes", value_name = "BYTES")]
    tail_bytes: Option<usize>,

    /// Worker threads parsing build inputs (default 1, sequential).
    /// Any count writes the same database the sequential run writes.
    #[arg(long = "parallel", value_name = "N", default_value_t = 1)]
    parallel: usize,

    /// Skip imaginary frequencies in the thermochemistry (mutually
    /// exclusive with --makereal).
    #[arg(long = "ignoreimag", conflicts_with = "makereal")]
    ignoreimag: bool,

    /// Print energies in kcal/mol instead of hartree; takes no value.
    #[arg(long = "unit", num_args = 0..=1)]
    unit: Option<Option<String>>,

    /// Print the formation-energy section after the table.
    #[arg(long = "formation")]
    formation: bool,

    /// Print the corrected formation-energy section after the table
    /// (needs corrections, attached or stored).
    #[arg(long = "formation-corr")]
    formation_corr: bool,

    /// Print the hydration distribution section after the formation
    /// sections (rqm/hydration.md): the Boltzmann distribution of the
    /// water count within each core composition. The solvent code is `w`.
    #[arg(long = "hydration")]
    hydration: bool,

    /// Print the corrected hydration distribution section after the raw
    /// one (needs corrections, attached or stored).
    #[arg(long = "hydration-corr")]
    hydration_corr: bool,

    /// Print the solvation distribution section for the named solvent
    /// CODE instead of water (rqm/hydration.md).
    #[arg(long = "solvation", value_name = "CODE")]
    solvation: Option<String>,

    /// Print the corrected solvation distribution section for CODE.
    #[arg(long = "solvation-corr", value_name = "CODE")]
    solvation_corr: Option<String>,

    /// Evaluate the formation free energies at vapors of the named
    /// component at the given concentration (repeatable; VALUE in Pa, or
    /// a ppt/ppb mole fraction, or cmmc molecules/cm^3). A settings flag
    /// (rqm/formation.md).
    #[arg(long = "conc", num_args = 2, value_names = ["CODE", "VALUE"], allow_hyphen_values = true)]
    conc: Vec<String>,

    /// Scale each named component's concentration term by (N-1)/N, so a
    /// monomer's formation free energy stays zero (self-consistent dG).
    #[arg(long = "cnt")]
    cnt: bool,

    /// Collapse each printed formation section into one
    /// Boltzmann-averaged row per composition
    /// (rqm/formation.md Boltzmann-Averaged Sections). A settings flag
    /// like --conc: without a formation section it is an accepted no-op.
    #[arg(long = "bavg")]
    bavg: bool,

    /// Relative humidity of the vapor for the hydration sections
    /// (rqm/hydration.md): 0.5 is 50 %. A settings flag; an accepted
    /// no-op without a hydration section.
    #[arg(long = "rh", allow_hyphen_values = true)]
    rh: Option<f64>,

    /// The solvent's partial pressure for the hydration sections, VALUE
    /// in the --conc grammar (rqm/formation.md); overrides --rh. A
    /// settings flag; an accepted no-op without a hydration section.
    #[arg(long = "psolvent", value_name = "VALUE", allow_hyphen_values = true)]
    psolvent: Option<String>,

    /// Attach single-point corrections from the named database, matching
    /// correction records to run records by exact case-sensitive name
    /// (a manipulation flag).
    #[arg(long = "add-correction", value_name = "PATH")]
    add_correction: Vec<PathBuf>,

    /// Attach an extrapolated electronic-energy correction built from
    /// the named source databases, matching their records to run records
    /// by exact case-sensitive name (a manipulation flag). The mode
    /// token leads: `2p LOW HIGH N ALPHA BETA`, `2s LOW HIGH F`, or the
    /// combined `2p LOWPAIR HIGHPAIR N ALPHA BETA F` with comma-joined
    /// path pairs. Like --pop for its single value, the flag eats the
    /// values that follow it until the next --flag, so place it after
    /// the positional inputs.
    // rq-24294fd5
    #[arg(
        long = "extrapolate",
        num_args = 1..,
        value_name = "MODE VALUES",
        allow_negative_numbers = true
    )]
    extrapolate: Vec<String>,

    // --- cutoff flags: `--<flag> PROPERTY VALUE` (repeatable) ---
    /// Absolute cutoff: keep records whose PROPERTY <= VALUE (energies
    /// in hartree).
    #[arg(long = "cut", num_args = 2, value_names = ["PROPERTY", "VALUE"], allow_hyphen_values = true)]
    cut: Vec<String>,
    /// Relative cutoff: keep records whose PROPERTY is at most VALUE
    /// above the group minimum (energies in kcal/mol).
    #[arg(long = "cutr", num_args = 2, value_names = ["PROPERTY", "VALUE"], allow_hyphen_values = true)]
    cutr: Vec<String>,
    /// Absolute cutoff: keep records whose PROPERTY > VALUE (energies in
    /// hartree).
    #[arg(long = "pass", num_args = 2, value_names = ["PROPERTY", "VALUE"], allow_hyphen_values = true)]
    pass: Vec<String>,
    /// Relative cutoff: keep records whose PROPERTY is more than VALUE
    /// above the group minimum (energies in kcal/mol).
    #[arg(long = "passr", num_args = 2, value_names = ["PROPERTY", "VALUE"], allow_hyphen_values = true)]
    passr: Vec<String>,
    /// Absolute cutoff: keep records whose PROPERTY == VALUE.
    #[arg(long = "is", num_args = 2, value_names = ["PROPERTY", "VALUE"], allow_hyphen_values = true)]
    is: Vec<String>,
    /// Absolute cutoff: keep records whose PROPERTY != VALUE; records
    /// with an undefined PROPERTY pass.
    #[arg(long = "isnot", num_args = 2, value_names = ["PROPERTY", "VALUE"], allow_hyphen_values = true)]
    isnot: Vec<String>,
    /// Absolute cutoff: keep records whose PROPERTY < VALUE (energies in
    /// hartree).
    #[arg(long = "filter-lt", num_args = 2, value_names = ["PROPERTY", "VALUE"], allow_hyphen_values = true)]
    filter_lt: Vec<String>,
    /// Absolute cutoff: keep records whose PROPERTY >= VALUE (energies
    /// in hartree).
    #[arg(long = "filter-ge", num_args = 2, value_names = ["PROPERTY", "VALUE"], allow_hyphen_values = true)]
    filter_ge: Vec<String>,
    /// Relative cutoff: keep records whose PROPERTY is less than VALUE
    /// above the group minimum (energies in kcal/mol).
    #[arg(long = "rel-filter-lt", num_args = 2, value_names = ["PROPERTY", "VALUE"], allow_hyphen_values = true)]
    rel_filter_lt: Vec<String>,
    /// Relative cutoff: keep records whose PROPERTY is at least VALUE
    /// above the group minimum (energies in kcal/mol).
    #[arg(long = "rel-filter-ge", num_args = 2, value_names = ["PROPERTY", "VALUE"], allow_hyphen_values = true)]
    rel_filter_ge: Vec<String>,

    // --- print flags: one per column (rqm/filtering.md) ---
    /// Print the filename column: the record's name, the stem of the
    /// source file name.
    #[arg(long = "filename")]
    filename: bool,
    /// Print the name column: the composition token, 2am3sa-style
    /// alphabetical pairs without separators.
    #[arg(long = "name")]
    name: bool,
    /// Print the path column: the record's source_path, the QC output
    /// file the record was parsed from.
    #[arg(long = "path")]
    path: bool,
    /// Print the job status column (ok, failed, truncated).
    #[arg(long = "status")]
    status: bool,
    /// Print the program token and version column (orca, gaussian, xtb,
    /// mrcc, xyz).
    #[arg(long = "program")]
    program: bool,
    /// Print the quoted command line column.
    #[arg(long = "command")]
    command: bool,
    /// Print the correction provenance column: kind, program and version,
    /// quoted commands.
    #[arg(long = "correction")]
    correction: bool,
    /// Print the electronic energy (hartree; kcal/mol with --unit).
    #[arg(long = "el")]
    el: bool,
    /// Print the harmonic zero-point energy (thermo.md model, derived).
    #[arg(long = "zpe")]
    zpe: bool,
    /// Print the enthalpy H at --temp (thermo.md model, derived).
    #[arg(long = "h")]
    h: bool,
    /// Print the T*S entropy term at --temp (thermo.md model, derived).
    #[arg(long = "s")]
    s: bool,
    /// Print the Gibbs free energy at --temp (thermo.md model, derived).
    #[arg(long = "g")]
    g: bool,
    /// Print the attached correction energy (needs corrections).
    #[arg(long = "elcorr")]
    elcorr: bool,
    /// Print the enthalpy from the attached correction energy.
    #[arg(long = "hcorr")]
    hcorr: bool,
    /// Print the Gibbs free energy from the attached correction energy.
    #[arg(long = "gcorr")]
    gcorr: bool,
    /// Print a Boltzmann population column (rqm/population.md): the
    /// fraction of the thermal ensemble each printed structure holds,
    /// computed per composition group. LEVEL picks the energy basis: g
    /// (Gibbs, the default), el (electronic), elcorr, or gcorr — JKQC's
    /// -pop and -popEL. Repeatable, one column per requested level. A
    /// space-separated value is eaten by the flag even when it is your
    /// database path (CJKQC --pop db.jsonl fails): use --pop=LEVEL there,
    /// or place --pop after the inputs.
    #[arg(long = "pop", num_args = 0..=1, value_name = "LEVEL", default_missing_value = "g")]
    pop: Vec<String>,
    /// Print the mass-weighted radius of gyration (Angstrom).
    #[arg(long = "rg")]
    rg: bool,
    /// Print the total mass (u).
    #[arg(long = "mass")]
    mass: bool,
    /// Print the dipole moment magnitude (atomic units).
    #[arg(long = "dip")]
    dip: bool,
    /// Print the dipole moment vector (atomic units, x,y,z
    /// comma-separated).
    #[arg(long = "dips")]
    dips: bool,
    /// Print the net charge (elementary charges).
    #[arg(long = "charge")]
    charge: bool,
    /// Print the spin multiplicity column.
    #[arg(long = "multiplicity")]
    multiplicity: bool,
    /// Print the rotational symmetry number.
    #[arg(long = "rsn")]
    rsn: bool,
    /// Print the geometry atom count.
    #[arg(long = "natoms")]
    natoms: bool,
    /// Print the lowest stored vibrational frequency (cm^-1, 1 decimal;
    /// raw values, so imaginary modes print negative).
    #[arg(long = "lf")]
    lf: bool,
    /// Print all stored vibrational frequencies (cm^-1, 1 decimal each,
    /// comma-separated; raw values, so imaginary modes print negative).
    #[arg(long = "freqs")]
    freqs: bool,
    /// Print the vibrational frequency count.
    #[arg(long = "nfreq")]
    nfreq: bool,
    /// Print the negative (imaginary) frequency count.
    #[arg(long = "nimag")]
    nimag: bool,
    /// Print the rotational constants (cm^-1, descending, 6 decimals
    /// each, comma-separated).
    #[arg(long = "rots")]
    rots: bool,
    /// Print the resolved composition as 2am+3sa-style pairs.
    #[arg(long = "components")]
    components: bool,

    /// Write the final structures as xyz frames into movie.xyz
    /// (rqm/movie.md): one frame per record that is ok and carries a
    /// geometry, in the order the table prints. A print flag like
    /// --formation: it is a visible outcome on its own and manipulates
    /// nothing.
    #[arg(long = "movie")]
    movie: bool,

    /// Input files: QC program outputs (build) or record databases (merge).
    #[arg(required = true)]
    inputs: Vec<PathBuf>,
}

// rq-a4e0ba89 rq-a2681a7a rq-9b4dda85
fn main() -> ExitCode {
    let matches = match Cli::command().try_get_matches_from(std::env::args_os()) {
        Ok(matches) => matches,
        Err(error) => error.exit(),
    };
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(error) => error.exit(),
    };
    run(cli, &matches)
}

fn run(cli: Cli, matches: &ArgMatches) -> ExitCode {
    // Cutoffs chain in command order: merge the ten flags by clap's
    // occurrence index.
    let mut ordered: Vec<(usize, Cutoff)> = Vec::new();
    for (id, op, mode) in CUTOFF_FLAGS {
        let Some(values) = matches.get_many::<String>(id) else {
            continue;
        };
        let values: Vec<&String> = values.collect();
        let indices: Vec<usize> = matches
            .indices_of(id)
            .map(|indices| indices.collect())
            .unwrap_or_default();
        for (position, pair) in values.chunks(2).enumerate() {
            let [property, value] = pair else { break };
            let Some(property) = Property::parse(property) else {
                return usage(&format!("unknown cutoff property `{property}`"));
            };
            let Ok(value) = value.parse::<f64>() else {
                return usage(&format!(
                    "`{value}` is not a number{}",
                    forgotten_value_hint(value)
                ));
            };
            let order = indices.get(position).copied().unwrap_or(usize::MAX);
            ordered.push((
                order,
                Cutoff {
                    op: *op,
                    mode: *mode,
                    property,
                    value,
                },
            ));
        }
    }
    ordered.sort_by_key(|(order, _)| *order);
    let cutoffs: Vec<Cutoff> = ordered.into_iter().map(|(_, cutoff)| cutoff).collect();

    if !cli.extract.is_empty() && !cli.nextract.is_empty() {
        return usage("cannot combine --extract with --nextract");
    }
    // Under --noname extract compares names verbatim; the composition
    // grammar (and its validation) does not apply (rqm/filtering.md).
    if !cli.noname {
        for query in cli.extract.iter().chain(&cli.nextract) {
            if let Err(error) = parse_extract_query(query) {
                return usage(&format!("extract query: {error}"));
            }
        }
    }
    let uniq = match &cli.uniq {
        Some(value) => match parse_uniq_mode(value) {
            Ok(mode) => Some(mode),
            Err(error) => return usage(&error),
        },
        None => None,
    };
    let sort = match &cli.sort {
        Some(value) => match SortKey::parse(value) {
            Some(key) => Some(key),
            None => return usage(&format!("unknown sort key `{value}`")),
        },
        None => None,
    };
    if cli.reacted && cli.reacted2 {
        return usage("cannot combine --reacted with --reacted2");
    }

    // The --pop levels parse up front: bare --pop arrives as the default
    // level `g` and an unknown level is a usage error
    // (`rqm/population.md` Bases and Flags). Repeats dedupe into the
    // columns' canonical order.
    // rq-cd221e62 rq-35d66314
    let mut pop_levels: Vec<PopLevel> = Vec::new();
    for text in &cli.pop {
        let Some(parsed) = parse_pop_level(text) else {
            return usage(&format!(
                "unknown population level `{text}` (expected el, g, elcorr, or gcorr)"
            ));
        };
        pop_levels.push(parsed);
    }
    let pop_levels: Vec<PopLevel> = PopLevel::CANONICAL
        .iter()
        .copied()
        .filter(|level| pop_levels.contains(level))
        .collect();

    // Settings flags configure the derivations; invalid values are usage
    // errors (`rqm/thermo.md`).
    let temperature = cli.temp.unwrap_or(298.15);
    if !temperature.is_finite() || temperature <= 0.0 {
        return usage("--temp must be a temperature above 0 K");
    }
    let freq_cutoff = cli.fc.unwrap_or(100.0);
    if !freq_cutoff.is_finite() || freq_cutoff < 0.0 {
        return usage("--fc must be a reference frequency of at least 0 cm^-1");
    }
    let freq_scale = cli.fscale.unwrap_or(1.0);
    if !freq_scale.is_finite() || freq_scale <= 0.0 {
        return usage("--fscale must be a scale factor above 0");
    }
    let conditions = ThermoConditions {
        temperature,
        freq_cutoff,
        freq_scale,
        makereal: cli.makereal,
        ignoreimag: cli.ignoreimag,
    };
    // The window budget for reading build inputs: the defaults widened
    // by --head-bytes/--tail-bytes; a zero window reads nothing and is a
    // usage error.
    let budget = WindowBudget {
        head_bytes: cli.head_bytes.unwrap_or(compiled_jkqc::DEFAULT_HEAD_BYTES),
        tail_bytes: cli.tail_bytes.unwrap_or(compiled_jkqc::DEFAULT_TAIL_BYTES),
    };
    if budget.head_bytes == 0 || budget.tail_bytes == 0 {
        return usage("--head-bytes and --tail-bytes must be at least 1");
    }
    // The worker count for build-mode collection (`rqm/collection.md`,
    // Parallel Collection): a count below 1 is a usage error, exactly
    // like a zero window budget.
    // rq-27e5a6b2
    if cli.parallel == 0 {
        return usage("--parallel must be at least 1");
    }
    let workers = cli.parallel;
    // One derivation memo for the whole run: every Gibbs-valued stage and
    // every energy column shares it, so each record's thermochemistry is
    // derived at most once (`rqm/thermo.md` Performance).
    // rq-8b44ad8d
    let memo = ThermoMemo::new(conditions);
    // A plain `--unit` selects kcal/mol; a value after it is a usage error
    // (the flag takes none, `rqm/thermo.md`).
    let unit = match cli.unit {
        None => Unit::Au,
        Some(None) => Unit::Kcal,
        Some(Some(value)) => {
            return usage(&format!(
                "--unit takes no value (got `{value}`); add it alone for kcal/mol"
            ));
        }
    };
    // The run's grouping: the population columns pool per composition
    // group, or into one global pool under --noname
    // (`rqm/population.md` Computation).
    // rq-b7d07857
    let grouping = if cli.noname {
        Grouping::Unit
    } else {
        Grouping::Composition
    };
    // The `--conc` specs parse and validate up front: the VALUE grammar,
    // letter-only codes, and one spec per code are usage errors
    // (`rqm/formation.md` Formation at Given Concentrations). Settings
    // flags: they configure the formation derivations and never count as
    // manipulations or print flags.
    // rq-10341821
    let mut concs = ConcConditions {
        specs: Vec::new(),
        self_consistent: cli.cnt,
    };
    for pair in cli.conc.chunks(2) {
        let [code, value] = pair else { break };
        if code.is_empty() || !code.bytes().all(|byte| byte.is_ascii_alphabetic()) {
            return usage(&format!("--conc code `{code}` must be letter-only"));
        }
        if concs.specs.iter().any(|spec| &spec.code == code) {
            return usage(&format!("--conc names the code `{code}` twice"));
        }
        let Some((magnitude, conc_unit)) = parse_conc_value(value) else {
            return usage(&format!(
                "`--conc {code} {value}`: the value must be a positive Pa pressure, optionally suffixed ppt, ppb, or cmmc{}",
                forgotten_value_hint(value)
            ));
        };
        concs.specs.push(ConcSpec {
            code: code.clone(),
            magnitude,
            unit: conc_unit,
        });
    }

    // --psolvent VALUE parses through the --conc grammar whether or not a
    // hydration section is requested, like --conc's own values
    // (`rqm/hydration.md` Flag Surface).
    // rq-bcb7acc5
    let psolvent = match &cli.psolvent {
        Some(value) => {
            let Some((magnitude, unit)) = parse_conc_value(value) else {
                return usage(&format!(
                    "`--psolvent {value}`: the value must be a positive Pa pressure, optionally suffixed ppt, ppb, or cmmc{}",
                    forgotten_value_hint(value)
                ));
            };
            // The pressure resolves once the temperature is known; the
            // magnitude-unit pair is a value in the --conc grammar, not
            // a spec for any component (`rqm/hydration.md` Solvent
            // Pressure).
            Some((magnitude, unit))
        }
        None => None,
    };
    // --rh VALUE must be a finite fraction above zero; supersaturation
    // (values above 1) is accepted (`rqm/hydration.md` Flag Surface).
    // rq-bcb7acc5
    if let Some(rh) = cli.rh {
        if !rh.is_finite() || rh <= 0.0 {
            return usage("--rh must be a fraction above zero");
        }
    }

    // The hydration solvent flags resolve up front (`rqm/hydration.md`
    // Flag Surface): the `--hydration` spellings fix the code to `w`, the
    // `--solvation` spellings take it, and all flags given together must
    // agree on one code — a second, different code is a usage error,
    // while repeated spellings of the same code are redundant.
    // rq-bcb7acc5
    let mut solvent_flags: Vec<(&'static str, String, bool)> = Vec::new();
    if cli.hydration {
        solvent_flags.push(("--hydration", "w".to_string(), false));
    }
    if cli.hydration_corr {
        solvent_flags.push(("--hydration-corr", "w".to_string(), true));
    }
    if let Some(code) = &cli.solvation {
        solvent_flags.push(("--solvation", code.clone(), false));
    }
    if let Some(code) = &cli.solvation_corr {
        solvent_flags.push(("--solvation-corr", code.clone(), true));
    }
    let hydration_raw = solvent_flags.iter().any(|(_, _, corrected)| !corrected);
    let hydration_corr = solvent_flags.iter().any(|(_, _, corrected)| *corrected);
    let solvent: Option<SolventConditions> = if solvent_flags.is_empty() {
        None
    } else {
        // The code is a non-empty, letter-only code, case-significant
        // (`rqm/record.md` component validation).
        // rq-bcb7acc5
        for (flag, code, _) in &solvent_flags {
            if code.is_empty() || !code.bytes().all(|byte| byte.is_ascii_alphabetic()) {
                return usage(&format!("{flag} solvent code `{code}` must be letter-only"));
            }
        }
        let (_, first, _) = &solvent_flags[0];
        if let Some((flag, code, _)) = solvent_flags
            .iter()
            .find(|(_, code, _)| code.as_str() != first.as_str())
        {
            return usage(&format!(
                "{flag} names the solvent code `{code}`, but another solvent flag names `{first}`; one solvent per run"
            ));
        }
        let code = first.clone();
        // --rh is water physics: with any solvent code other than `w` it
        // is a usage error (`rqm/hydration.md` Flag Surface).
        // rq-bcb7acc5
        if cli.rh.is_some() && code != "w" {
            return usage(&format!(
                "--rh is water physics and cannot combine with the solvent code `{code}`"
            ));
        }
        // Resolution order: --psolvent wins over --rh; with neither, the
        // default is the saturated vapor — valid for water only
        // (`rqm/hydration.md` Solvent Pressure).
        // rq-d36686ec
        let pressure = match &psolvent {
            // The psolvent value paired with the resolved code is the
            // implied spec under construction; its pressure resolves
            // through the --conc grammar's conversion
            // (`rqm/hydration.md` Solvent Pressure).
            Some((magnitude, unit)) => ConcSpec {
                code: code.clone(),
                magnitude: *magnitude,
                unit: *unit,
            }
            .pressure(temperature),
            None => match cli.rh {
                Some(rh) => rh * saturation_pressure_water(temperature),
                None => {
                    if code != "w" {
                        return usage(&format!(
                            "the solvent code `{code}` needs --psolvent; only water carries the saturation-vapor correlation"
                        ));
                    }
                    saturation_pressure_water(temperature)
                }
            },
        };
        // The pressure must resolve to a finite positive number before
        // the run starts (Solvent Pressure).
        // rq-d36686ec
        if !pressure.is_finite() || pressure <= 0.0 {
            return usage(
                "the resolved solvent pressure is not a finite positive number (check --rh, --psolvent, and --temp)",
            );
        }
        Some(SolventConditions { code, pressure })
    };
    // The resolved pressure enters the run as an implied concentration
    // spec: a hydration section behaves as if `--conc CODE p` had been
    // given, so the formation sections' solvent term shifts by it, --cnt
    // scales it like any spec, and the distribution values carry it
    // (`rqm/hydration.md` Solvent Pressure). Naming the solvent code in
    // an explicit --conc is a usage error; specs for other components
    // ride along untouched.
    // rq-d36686ec
    if let Some(solvent) = &solvent {
        if concs.specs.iter().any(|spec| spec.code == solvent.code) {
            return usage(&format!(
                "--conc already names the solvent code `{}`",
                solvent.code
            ));
        }
        concs.specs.push(ConcSpec {
            code: solvent.code.clone(),
            magnitude: solvent.pressure,
            unit: ConcUnit::Pa,
        });
    }
    let hydration_requested = hydration_raw || hydration_corr;

    let extract = if !cli.extract.is_empty() {
        Some((cli.extract.clone(), ExtractMode::Keep))
    } else if !cli.nextract.is_empty() {
        Some((cli.nextract.clone(), ExtractMode::Drop))
    } else {
        None
    };
    // A manipulation flag, when set, must also flow into `has_manipulation`;
    // `manipulation_ids` is the single source of truth shared with the
    // consistency test, so a flag that is declared but never read is caught.
    let active_manipulations = manipulation_ids(&cli);
    let has_manipulation = !cutoffs.is_empty() || !active_manipulations.is_empty();
    // `--add-correction` is a manipulation flag for the visible-outcome
    // rule, but a run whose only enrichment is attachment and that prints
    // nothing still completes like a collection run, with the collection
    // summary line (`rqm/correction.md`).
    let has_other_manipulation = !cutoffs.is_empty()
        || active_manipulations
            .iter()
            .any(|id| *id != "add_correction" && *id != "extrapolate");
    // Repeating --add-correction is a usage error (`rqm/correction.md`).
    if cli.add_correction.len() > 1 {
        return usage("cannot repeat --add-correction; give one correction database");
    }
    // One --extrapolate per run (`rqm/extrapolation.md`). clap flattens a
    // repeated flag's values into one list, so a repeat is detected by
    // occurrence count before the grammar can misread the merged values
    // as one valid invocation.
    // rq-24294fd5
    if matches
        .get_occurrences::<String>("extrapolate")
        .is_some_and(|mut occurrences| occurrences.nth(1).is_some())
    {
        return usage("cannot repeat --extrapolate; give one extrapolation");
    }
    // One --extrapolate per run, never alongside --add-correction; the
    // grammar and parameter bounds are usage errors reported before any
    // input is read (`rqm/extrapolation.md`).
    // rq-24294fd5
    if !cli.add_correction.is_empty() && !cli.extrapolate.is_empty() {
        return usage(
            "cannot combine --add-correction with --extrapolate; a record carries one correction",
        );
    }
    let extrapolation = if cli.extrapolate.is_empty() {
        None
    } else {
        match parse_extrapolation(&cli.extrapolate) {
            Ok(spec) => Some(spec),
            Err(message) => return usage(&message),
        }
    };

    let set_print = |column: Column| match column {
        Column::Filename => cli.filename,
        Column::Name => cli.name,
        Column::Path => cli.path,
        Column::Status => cli.status,
        Column::Program => cli.program,
        Column::Command => cli.command,
        Column::Correction => cli.correction,
        Column::El => cli.el,
        Column::Zpe => cli.zpe,
        Column::H => cli.h,
        Column::S => cli.s,
        Column::G => cli.g,
        Column::ElCorr => cli.elcorr,
        Column::HCorr => cli.hcorr,
        Column::GCorr => cli.gcorr,
        // Population columns are driven by the --pop levels, never by a
        // boolean (`rqm/population.md` Bases and Flags).
        Column::Pop | Column::PopEl | Column::PopElCorr | Column::PopGCorr => false,
        Column::Rg => cli.rg,
        Column::Mass => cli.mass,
        Column::Dip => cli.dip,
        Column::Dips => cli.dips,
        Column::Charge => cli.charge,
        Column::Multiplicity => cli.multiplicity,
        Column::Rsn => cli.rsn,
        Column::Natoms => cli.natoms,
        Column::Lf => cli.lf,
        Column::Freqs => cli.freqs,
        Column::Nfreq => cli.nfreq,
        Column::Nimag => cli.nimag,
        Column::RotS => cli.rots,
        Column::Components => cli.components,
    };
    let columns: Vec<Column> = Column::ALL
        .iter()
        .copied()
        .filter(|column| match column.pop_level() {
            // The population columns join through the --pop levels
            // (`rqm/population.md` Bases and Flags).
            // rq-4d3ba8f2 rq-9c420929 rq-0c93ce7a
            Some(level) => pop_levels.contains(&level),
            None => set_print(*column),
        })
        .collect();
    // `--formation`, `--formation-corr`, `--movie`, and the hydration
    // flags print sections or write files instead of columns but still
    // count as print flags for the visible-outcome rule
    // (`rqm/formation.md`, `rqm/hydration.md`, `rqm/movie.md`).
    // rq-2dbfbb9a
    let has_print = !columns.is_empty()
        || cli.formation
        || cli.formation_corr
        || hydration_requested
        || cli.movie;

    // A run must have a visible outcome (`rqm/filtering.md`); `has_manipulation`
    // and `has_print` are the single source of truth for the rule.
    if cli.out.is_none() && !has_manipulation && !has_print {
        return missing_out_usage();
    }
    if cli.out.is_none() && has_manipulation && !has_print {
        return usage("a manipulation needs --out or at least one print flag");
    }

    let inputs: Vec<&Path> = cli.inputs.iter().map(PathBuf::as_path).collect();

    // A plain collection run writes exactly what collection writes (the
    // verbatim merge path); print flags show the records as loaded before
    // the collection summary, and a print-only run writes nothing at all.
    // (Print plus --out reads the inputs twice — once to print, once to
    // write verbatim — until milestone 7 unifies the plumbing.)
    if !has_manipulation {
        let mut gathered_records: Option<Vec<Record>> = None;
        if has_print {
            let gathered = match gather_inputs_with(&inputs, &budget, workers) {
                Ok(gathered) => gathered,
                Err(error) => {
                    eprintln!("error: {error}");
                    return collect_exit_code(&error);
                }
            };
            let records = match gathered {
                Gathered::Build { records } => records,
                Gathered::Merge { records, .. } => records,
            };
            // The hydration section's solvent code must appear in the
            // record set (`rqm/hydration.md` Flag Surface), validated
            // first so a combined `--formation --hydration` run names
            // the solvent, not the implied `--conc` spec that carries
            // it (`rqm/hydration.md` Solvent Pressure). Every --conc
            // code must appear in the record set the formation
            // sections evaluate (`rqm/formation.md`).
            // rq-10341821 rq-bcb7acc5
            if let Some(solvent) = &solvent {
                if let Err(error) = validate_solvent_code(solvent, &records) {
                    return usage(&error);
                }
            }
            if cli.formation || cli.formation_corr {
                if let Err(error) = validate_conc_codes(&concs, &records) {
                    return usage(&error);
                }
            }
            if !columns.is_empty() {
                println!("{}", print_table(&records, &columns, &memo, unit, grouping));
            }
            // A print-only run has no --sort (that would be a
            // manipulation), so the references use the el ordering
            // (rqm/formation.md). Under --bavg the section collapses
            // into one ensemble per composition first
            // (rqm/formation.md Boltzmann-Averaged Sections).
            // rq-dd65e289
            print_formation_sections(
                &records,
                &memo,
                &concs,
                unit,
                cli.formation,
                cli.formation_corr,
                cli.bavg,
                SortKey::El,
            );
            // The hydration sections print after the formation sections,
            // over the same record set, raw before corrected
            // (`rqm/hydration.md` Printing); a print-only run has no
            // --sort, so the references use the el ordering like the
            // formation sections.
            // rq-5e154b92
            if let Some(solvent) = &solvent {
                print_hydration_sections(
                    &records,
                    &memo,
                    &concs,
                    &solvent.code,
                    hydration_raw,
                    hydration_corr,
                    cli.bavg,
                    SortKey::El,
                );
            }
            gathered_records = Some(records);
        }
        // The --out write precedes the movie write (`rqm/movie.md`); a
        // failed --out write ends the run before the movie is written.
        let (mut code, out_written) = match &cli.out {
            Some(out) => run_collection(&inputs, out, cli.fout, &budget, workers),
            None => (ExitCode::SUCCESS, false),
        };
        if cli.movie && (cli.out.is_none() || out_written) {
            let records = gathered_records
                .as_deref()
                .expect("--movie is a print flag, so a print-only run gathers its records");
            if write_movie_block(records, cli.fout) == ExitCode::FAILURE {
                code = ExitCode::FAILURE;
            }
        }
        return code;
    }

    // A run whose only enrichment is attachment, printing nothing:
    // collection semantics — every record written, the collection summary
    // line with the corrected suffix (`rqm/correction.md`).
    if cli.add_correction.len() == 1 && !has_other_manipulation && !has_print {
        let correction_path = cli.add_correction[0].clone();
        return run_enriching_collection(
            &inputs,
            cli.out.as_deref(),
            cli.fout,
            &budget,
            workers,
            |records| {
                attach_from_path(records, &correction_path)
                    .map(|(enriched, problems)| (enriched, !problems.is_empty()))
            },
        );
    }

    // A run whose only enrichment is extrapolation, printing nothing:
    // collection semantics — every record written, the collected or
    // merged summary line with the corrected suffix
    // (`rqm/extrapolation.md`).
    // rq-c7255c36
    if !cli.extrapolate.is_empty() && !has_other_manipulation && !has_print {
        let spec = extrapolation.as_ref().expect("extrapolation parsed");
        return run_enriching_collection(
            &inputs,
            cli.out.as_deref(),
            cli.fout,
            &budget,
            workers,
            |records| extrapolate_from_paths(records, spec),
        );
    }

    // Manipulation run: gather, filter, print, write.
    let (mut records, problems) = match gather_inputs_with(&inputs, &budget, workers) {
        Ok(Gathered::Build { records }) => (records, Vec::new()),
        Ok(Gathered::Merge { records, problems }) => (records, problems),
        Err(error) => {
            eprintln!("error: {error}");
            return collect_exit_code(&error);
        }
    };
    for problem in &problems {
        eprintln!(
            "warning: {} line {}: {}",
            problem.input.display(),
            problem.line,
            problem.reason
        );
    }

    // Attachment precedes every pipeline stage, so cutoffs, sorts, and
    // the formation sections see the corrected values
    // (`rqm/correction.md`).
    // rq-587473fd
    let mut correction_problems: Vec<LoadProblem> = Vec::new();
    if let Some(path) = cli.add_correction.first() {
        let Some((enriched, loaded)) = attach_from_path(&records, path) else {
            return ExitCode::FAILURE;
        };
        records = enriched;
        correction_problems = loaded;
    }

    // Extrapolation, like attachment, precedes every pipeline stage, so
    // cutoffs, sorts, and the corrected sections see the extrapolated
    // values (`rqm/extrapolation.md`).
    // rq-a757e0f8
    let mut extrapolation_problems = false;
    if let Some(spec) = &extrapolation {
        let Some((enriched, problems)) = extrapolate_from_paths(&records, spec) else {
            return ExitCode::FAILURE;
        };
        records = enriched;
        extrapolation_problems = problems;
    }

    let excluded = records
        .iter()
        .filter(|record| record.status != RecordStatus::Ok)
        .count();
    if excluded > 0 {
        eprintln!("excluded {excluded} records with status failed or truncated");
    }

    let spec = FilterSpec {
        extract,
        cutoffs,
        uniq,
        reacted: match (cli.reacted, cli.reacted2) {
            (true, _) => Some(ReactedMode::Consensus),
            (_, true) => Some(ReactedMode::Deviants),
            _ => None,
        },
        sort,
        reverse: cli.reverse,
        select: cli.select,
        glob: cli.glob,
        shuffle: cli.shuffle,
        seed: cli.seed.unwrap_or(0),
        grouping,
    };
    records = match apply_filters(&records, &spec, &memo) {
        Ok(records) => records,
        Err(error) => {
            eprintln!("error: extract query: {error}");
            return ExitCode::from(2);
        }
    };

    // The hydration section's solvent code must appear in the record
    // set (`rqm/hydration.md` Flag Surface), validated first so a
    // combined `--formation --hydration` run names the solvent, not
    // the implied `--conc` spec that carries it (`rqm/hydration.md`
    // Solvent Pressure). Every --conc code must appear in the record
    // set the formation sections evaluate — the pipeline survivors
    // (`rqm/formation.md`).
    // rq-10341821 rq-bcb7acc5
    if let Some(solvent) = &solvent {
        if let Err(error) = validate_solvent_code(solvent, &records) {
            return usage(&error);
        }
    }
    if cli.formation || cli.formation_corr {
        if let Err(error) = validate_conc_codes(&concs, &records) {
            return usage(&error);
        }
    }

    if !columns.is_empty() {
        println!("{}", print_table(&records, &columns, &memo, unit, grouping));
    }
    // The formation sections print after the table, before the summary,
    // over the pipeline survivors (`rqm/formation.md`), collapsing into
    // ensembles under --bavg (rqm/formation.md Boltzmann-Averaged
    // Sections).
    // rq-dd65e289
    let sort_key = spec.sort.unwrap_or(SortKey::El);
    print_formation_sections(
        &records,
        &memo,
        &concs,
        unit,
        cli.formation,
        cli.formation_corr,
        cli.bavg,
        sort_key,
    );
    // The hydration sections print after the formation sections, over
    // the pipeline survivors, raw before corrected
    // (`rqm/hydration.md` Printing), with the run's sort ordering.
    // rq-5e154b92
    if let Some(solvent) = &solvent {
        print_hydration_sections(
            &records,
            &memo,
            &concs,
            &solvent.code,
            hydration_raw,
            hydration_corr,
            cli.bavg,
            sort_key,
        );
    }

    // The --out write precedes the movie write (`rqm/movie.md`); the
    // confirmation gate fires immediately before the atomic replacement
    // (`rqm/collection.md`, Overwrite Confirmation), and a no answer or a
    // refusal ends the run before the movie is considered.
    if let Some(out) = &cli.out {
        if !confirm_target(out, cli.fout) {
            return ExitCode::FAILURE;
        }
        if let Err(error) = write_records(out, &records) {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
        // The filtered summary reports the enrichment when the run
        // carried --add-correction or --extrapolate
        // (`rqm/correction.md`, `rqm/extrapolation.md`).
        // rq-58d85e68
        let suffix = if cli.add_correction.is_empty() && cli.extrapolate.is_empty() {
            String::new()
        } else {
            let corrected = records
                .iter()
                .filter(|record| record.correction.is_some())
                .count();
            format!(" ({} corrected)", corrected)
        };
        println!(
            "filtered {} records into {}{}",
            records.len(),
            out.display(),
            suffix
        );
    }

    // The movie writes after the --out write (`rqm/movie.md`), always the
    // run's last stdout line; its failure fails the run.
    // rq-6fb174a0
    let mut code =
        if problems.is_empty() && correction_problems.is_empty() && !extrapolation_problems {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    if cli.movie && write_movie_block(&records, cli.fout) == ExitCode::FAILURE {
        code = ExitCode::FAILURE;
    }
    code
}

// rq-a2681a7a — collection behaves exactly as `rqm/collection.md`.
/// Returns the exit code and whether the output database was written, so
/// the caller knows whether a movie write may still follow
/// (`rqm/movie.md`). The confirmation hook asks immediately before the
/// atomic replacement inside the collection pipeline
/// (`rqm/collection.md`, Overwrite Confirmation).
fn run_collection(
    inputs: &[&Path],
    out: &Path,
    fout: bool,
    budget: &WindowBudget,
    workers: usize,
) -> (ExitCode, bool) {
    let mut confirm = || confirm_target(out, fout);
    match compiled_jkqc::collect_inputs_with(inputs, out, budget, workers, &mut confirm) {
        Ok(CollectOutcome::Build {
            written,
            failed,
            truncated,
        }) => {
            println!(
                "collected {written} records into {} ({failed} failed, {truncated} truncated)",
                out.display()
            );
            (ExitCode::SUCCESS, true)
        }
        Ok(CollectOutcome::Merge { written, problems }) => {
            for problem in &problems {
                eprintln!(
                    "warning: {} line {}: {}",
                    problem.input.display(),
                    problem.line,
                    problem.reason
                );
            }
            println!("merged {written} records into {}", out.display());
            if problems.is_empty() {
                (ExitCode::SUCCESS, true)
            } else {
                (ExitCode::FAILURE, true)
            }
        }
        Err(error @ CollectError::Mixed { .. }) => {
            eprintln!("error: {error}");
            (ExitCode::from(2), false)
        }
        // The hook printed the reason; the run simply fails
        // (`rqm/collection.md`, Overwrite Confirmation).
        Err(CollectError::NotConfirmed) => (ExitCode::FAILURE, false),
        Err(error) => {
            eprintln!("error: {error}");
            (ExitCode::FAILURE, false)
        }
    }
}

/// An enriching collection run (`rqm/correction.md`,
/// `rqm/extrapolation.md`): the plain collection flow with the
/// enrichment inserted before the write — every record is written, and
/// the collection summary line carries the corrected suffix. `enrich`
/// maps the gathered records to their enriched copies and whether it
/// reported problems; a `None` enrichment (a fatal error, already
/// printed) ends the run before anything is written.
///
/// (rq-58d85e68 rq-66e4cb89 rq-c7255c36)
fn run_enriching_collection(
    inputs: &[&Path],
    out: Option<&Path>,
    fout: bool,
    budget: &WindowBudget,
    workers: usize,
    enrich: impl FnOnce(&[Record]) -> Option<(Vec<Record>, bool)>,
) -> ExitCode {
    // Unreachable via the visible-outcome rule, which demands --out on a
    // run with no print flags and no other manipulation — but fail loudly
    // rather than silently succeeding if that rule is ever relaxed
    // (`rqm/correction.md`, `rqm/extrapolation.md`).
    // rq-3fd0b9bb
    let Some(out) = out else {
        eprintln!("error: an enriching collection run needs --out");
        return ExitCode::from(2);
    };
    let (records, problems, is_build) = match gather_inputs_with(inputs, budget, workers) {
        Ok(Gathered::Build { records }) => (records, Vec::new(), true),
        Ok(Gathered::Merge { records, problems }) => (records, problems, false),
        Err(error) => {
            eprintln!("error: {error}");
            return collect_exit_code(&error);
        }
    };
    for problem in &problems {
        eprintln!(
            "warning: {} line {}: {}",
            problem.input.display(),
            problem.line,
            problem.reason
        );
    }
    let Some((records, enrichment_problems)) = enrich(&records) else {
        return ExitCode::FAILURE;
    };
    // The overwrite confirmation gate precedes the write, so the question
    // fires immediately before the atomic replacement
    // (`rqm/collection.md`, Overwrite Confirmation).
    if !confirm_target(out, fout) {
        return ExitCode::FAILURE;
    }
    if let Err(error) = write_records(out, &records) {
        eprintln!("error: {error}");
        return ExitCode::FAILURE;
    }
    // The collection summary reports the enrichment
    // (`rqm/correction.md`, `rqm/extrapolation.md`).
    // rq-58d85e68
    let corrected = records
        .iter()
        .filter(|record| record.correction.is_some())
        .count();
    if is_build {
        let failed = records
            .iter()
            .filter(|record| record.status == RecordStatus::Failed)
            .count();
        let truncated = records
            .iter()
            .filter(|record| record.status == RecordStatus::Truncated)
            .count();
        println!(
            "collected {} records into {} ({} failed, {} truncated, {} corrected)",
            records.len(),
            out.display(),
            failed,
            truncated,
            corrected
        );
    } else {
        println!(
            "merged {} records into {} ({} corrected)",
            records.len(),
            out.display(),
            corrected
        );
    }
    if problems.is_empty() && !enrichment_problems {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Loads the correction database — always through `load_database`, never
/// classified like the positional inputs (`rqm/correction.md`) — and
/// attaches its records to the run's records, reporting the load warnings
/// and the attach warnings. A fatal failure (an unreadable database,
/// duplicate candidate names) prints its error and returns `None`.
///
/// (rq-a9dae115)
fn attach_from_path(records: &[Record], path: &Path) -> Option<(Vec<Record>, Vec<LoadProblem>)> {
    let (corrections, problems) = match load_corrections(path) {
        Ok(loaded) => loaded,
        Err(error) => {
            eprintln!("error: {error}");
            return None;
        }
    };
    for problem in &problems {
        eprintln!(
            "warning: {} line {}: {}",
            path.display(),
            problem.line,
            problem.reason
        );
    }
    // A correction database that loads no record at all while reporting
    // load problems is fatal: the error is reported and nothing is
    // written (`rqm/correction.md`).
    // rq-9fd40c4e
    if corrections.is_empty() && !problems.is_empty() {
        eprintln!(
            "error: {}: no valid record loads from the correction database",
            path.display()
        );
        return None;
    }
    let attachment = match attach_corrections(records, &corrections) {
        Ok(attachment) => attachment,
        Err(error) => {
            eprintln!("error: {error}");
            return None;
        }
    };
    // A non-ok correction record, or an ok one without an electronic
    // energy, is skipped and warned individually, naming its name
    // (`rqm/correction.md`). The skip reason comes from the same helper
    // the attach pass counts with.
    // rq-e59918d8 rq-a83c76ac
    for record in &corrections {
        if let Some(reason) = compiled_jkqc::correction::skip_reason(record) {
            let name = record.name.as_deref().unwrap_or("(unnamed)");
            eprintln!("warning: skipped correction record \"{name}\": {reason}");
        }
    }
    // The unmatched candidates are reported as one aggregate warning
    // (`rqm/correction.md`).
    // rq-6f572182
    if attachment.unmatched > 0 {
        let noun = if attachment.unmatched == 1 {
            "record"
        } else {
            "records"
        };
        eprintln!(
            "warning: {} unmatched correction {}",
            attachment.unmatched, noun
        );
    }
    Some((attachment.records, problems))
}

/// Loads every extrapolation source — always through `load_database`,
/// never classified like the positional inputs (`rqm/extrapolation.md`)
/// — runs the extrapolation pass over the run's records, and prints the
/// per-record warnings. A fatal failure (an unreadable source, duplicate
/// candidate names, mixed programs or versions, repeated calculations)
/// prints its error and returns `None`.
///
/// (rq-c68c02b0)
fn extrapolate_from_paths(
    records: &[Record],
    spec: &ExtrapolationSpec,
) -> Option<(Vec<Record>, bool)> {
    let paths = source_paths(spec);
    let needs_scf = spec.needs_scf();
    let mut source_records: Vec<Vec<Record>> = Vec::with_capacity(paths.len());
    let mut any_problems = false;
    for path in &paths {
        let (loaded, problems) = match db::load_database(path) {
            Ok(outcome) => (outcome.records, outcome.problems),
            Err(error) => {
                eprintln!("error: {error}");
                return None;
            }
        };
        for problem in &problems {
            eprintln!(
                "warning: {} line {}: {}",
                path.display(),
                problem.line,
                problem.reason
            );
        }
        // A source that loads no record at all while reporting load
        // problems is fatal: the error is reported and nothing is
        // written (`rqm/extrapolation.md`).
        if loaded.is_empty() && !problems.is_empty() {
            eprintln!(
                "error: {}: no valid record loads from the extrapolation source database",
                path.display()
            );
            return None;
        }
        any_problems |= !problems.is_empty();
        source_records.push(loaded);
    }
    let slices: Vec<&[Record]> = source_records.iter().map(Vec::as_slice).collect();
    let extrapolation =
        match compiled_jkqc::extrapolate::extrapolate_corrections(records, &slices, spec) {
            Ok(extrapolation) => extrapolation,
            Err(error) => {
                print_extrapolation_error(&error, &paths);
                return None;
            }
        };
    // A non-candidate source record is skipped and warned individually,
    // naming its name (`(unnamed)` when absent)
    // (`rqm/extrapolation.md`). The skip reason comes from the same
    // helper the pass counts with.
    // rq-c68c02b0
    for source in &source_records {
        for record in source {
            if let Some(reason) = skip_reason(record, needs_scf) {
                let name = record.name.as_deref().unwrap_or("(unnamed)");
                eprintln!("warning: skipped extrapolation record \"{name}\": {reason}");
            }
        }
    }
    // A run record whose name is not fully covered is skipped with a
    // warning naming the name and the first source database missing it;
    // a covered name whose result is not finite warns like any skip.
    // (`rqm/extrapolation.md`)
    // rq-925967be rq-a4fc7526
    for record in records {
        let Some(name) = record.name.as_deref() else {
            eprintln!(
                "warning: record \"(unnamed)\" is uncovered: no extrapolation candidate in {}",
                paths[0].display()
            );
            continue;
        };
        let first_missing = source_records
            .iter()
            .zip(&paths)
            .find_map(|(source, path)| {
                (!source.iter().any(|candidate| {
                    is_candidate(candidate, needs_scf) && candidate.name.as_deref() == Some(name)
                }))
                .then_some(path.clone())
            });
        match first_missing {
            Some(path) => eprintln!(
                "warning: record \"{name}\" is uncovered: no extrapolation candidate in {}",
                path.display()
            ),
            None => {
                if correction_for(&slices, spec, name).is_none() {
                    eprintln!(
                        "warning: record \"{name}\": the extrapolated energy is not finite; \
                         no correction attached"
                    );
                }
            }
        }
    }
    // The unmatched candidates are reported as one aggregate warning
    // (`rqm/extrapolation.md`).
    // rq-3bad508e
    if extrapolation.unmatched > 0 {
        let noun = if extrapolation.unmatched == 1 {
            "record"
        } else {
            "records"
        };
        eprintln!(
            "warning: {} unmatched extrapolation {}",
            extrapolation.unmatched, noun
        );
    }
    Some((extrapolation.records, any_problems))
}

/// Prints a fatal extrapolation error, naming the databases the
/// error's source indices refer to (`rqm/extrapolation.md`).
// rq-73d8ed95
fn print_extrapolation_error(error: &ExtrapolationError, paths: &[PathBuf]) {
    match error {
        ExtrapolationError::SourceCount { .. } => eprintln!("error: {error}"),
        ExtrapolationError::DuplicateName { source, name } => eprintln!(
            "error: {}: extrapolation database holds multiple records named \"{name}\"",
            paths[*source].display()
        ),
        ExtrapolationError::ProgramMismatch {
            name,
            source_a,
            source_b,
            program_a,
            program_b,
        } => eprintln!(
            "error: extrapolation contributors for \"{name}\" mix programs: \
             {program_a} ({}) and {program_b} ({})",
            paths[*source_a].display(),
            paths[*source_b].display()
        ),
        ExtrapolationError::VersionMismatch {
            name,
            source_a,
            source_b,
            version_a,
            version_b,
        } => eprintln!(
            "error: extrapolation contributors for \"{name}\" mix program versions: \
             {version_a} ({}) and {version_b} ({})",
            paths[*source_a].display(),
            paths[*source_b].display()
        ),
        ExtrapolationError::DuplicateEnergy {
            name,
            source_a,
            source_b,
        } => eprintln!(
            "error: extrapolation contributors for \"{name}\" carry exactly equal \
             electronic energies ({} and {})",
            paths[*source_a].display(),
            paths[*source_b].display()
        ),
    }
}

/// Loads the correction database through `load_database`, mapping the
/// database error onto `CorrectionError`. `load_database` reports only
/// filesystem-level failures as errors, so the catch-all arm below is
/// unreachable today; it maps defensively rather than panicking.
// rq-874abd6a
fn load_corrections(path: &Path) -> Result<(Vec<Record>, Vec<LoadProblem>), CorrectionError> {
    match db::load_database(path) {
        Ok(outcome) => Ok((outcome.records, outcome.problems)),
        Err(DbError::Io { kind, reason }) => Err(CorrectionError::Io { kind, reason }),
        Err(error) => Err(CorrectionError::Io {
            kind: std::io::ErrorKind::Other,
            reason: error.to_string(),
        }),
    }
}

/// Validates, serializes, and writes the records as the complete new
/// content of the output database (shared by the manipulation and
/// correcting-collection runs).
// rq-587473fd
fn write_records(out: &Path, records: &[Record]) -> Result<(), String> {
    let mut lines = Vec::with_capacity(records.len());
    for record in records {
        if let Err(invalidity) = record.validate() {
            return Err(invalidity.to_string());
        }
        match serde_json::to_string(record) {
            Ok(line) => lines.push(line),
            Err(error) => return Err(error.to_string()),
        }
    }
    db::write_database(out, &lines).map_err(|error| error.to_string())
}

/// The overwrite confirmation gate (`rqm/collection.md`, Overwrite
/// Confirmation): an existing target is replaced only after a yes answer,
/// `--fout` forces it, and a run without an interactive terminal refuses.
/// A decline or refusal prints its error line and returns false; the
/// caller aborts with exit 1. A target that does not exist is written the
/// same way with or without `--fout`.
// rq-eff8dce8
fn confirm_target(path: &Path, forced: bool) -> bool {
    if forced || !path.exists() {
        return true;
    }
    let interactive = std::io::stdin().is_terminal();
    let decision = confirm_overwrite(
        path,
        &mut std::io::stderr().lock(),
        &mut std::io::stdin().lock(),
        interactive,
    );
    match overwrite_error(decision, path) {
        Some(message) => {
            eprintln!("{message}");
            false
        }
        None => true,
    }
}

/// The movie export of the run's final record set (`rqm/movie.md`): the
/// frame-eligible subset in run order, the skip note, the overwrite
/// confirmation (`rqm/collection.md`), the atomic write, and the summary
/// line — always the run's last stdout line.
// rq-6fb174a0
fn write_movie_block(records: &[Record], fout: bool) -> ExitCode {
    let path = Path::new("movie.xyz");
    let (frames, excluded) = movie_frames(records);
    // The skip note counts the final-set records that could not become a
    // frame (`rqm/movie.md`).
    // rq-21313a28
    if excluded > 0 {
        eprintln!("excluded {excluded} records from movie.xyz (no geometry or not ok)");
    }
    if !confirm_target(path, fout) {
        return ExitCode::FAILURE;
    }
    match write_movie_xyz(path, &frames) {
        Ok(()) => {
            // The summary line names the frames actually written
            // (`rqm/movie.md`).
            // rq-4aab0ef8
            println!("wrote {} frames into movie.xyz", frames.len());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn usage(message: &str) -> ExitCode {
    eprintln!("error: {message}");
    ExitCode::from(2)
}

/// The hint for a two-value flag (`--cut PROPERTY VALUE`, `--conc CODE
/// VALUE`) whose second word is itself a flag: the classic forgotten-
/// VALUE mistake, where `allow_hyphen_values` let the next flag be eaten.
fn forgotten_value_hint(value: &str) -> &'static str {
    if value.starts_with("--") {
        " (did you forget the VALUE?)"
    } else {
        ""
    }
}

/// Every `--conc` code must appear as a component of at least one record
/// in the section's record set, else the run is a usage error; a code
/// whose monomers were filtered away may still appear inside cluster
/// compositions and simply forms no reference (`rqm/formation.md`).
///
/// (rq-10341821)
fn validate_conc_codes(concs: &ConcConditions, records: &[Record]) -> Result<(), String> {
    for spec in &concs.specs {
        let present = records.iter().any(|record| {
            record
                .components
                .as_ref()
                .is_some_and(|components| components.contains_key(&spec.code))
        });
        if !present {
            return Err(format!(
                "--conc code `{}` appears in no record's composition",
                spec.code
            ));
        }
    }
    Ok(())
}

/// The hydration section's solvent code must appear as a component of at
/// least one record in the section's record set — the run's final record
/// set — or the run is a usage error (the `--conc` code-validation rule
/// of `rqm/formation.md`, `rqm/hydration.md` Flag Surface).
///
/// (rq-bcb7acc5)
fn validate_solvent_code(solvent: &SolventConditions, records: &[Record]) -> Result<(), String> {
    let present = records.iter().any(|record| {
        record
            .components
            .as_ref()
            .is_some_and(|components| components.contains_key(&solvent.code))
    });
    if present {
        Ok(())
    } else {
        Err(format!(
            "the solvent code `{}` appears in no record's composition",
            solvent.code
        ))
    }
}

/// The formation sections: raw before corrected, over the collapsed
/// ensembles when `bavg` is set (`rqm/formation.md`, Boltzmann-Averaged
/// Sections). A print-only run passes the el ordering — no --sort is a
/// manipulation — a pipeline run the run's sort ordering.
// rq-dd65e289
#[allow(clippy::too_many_arguments)]
fn print_formation_sections(
    records: &[Record],
    memo: &ThermoMemo,
    concs: &ConcConditions,
    unit: Unit,
    raw: bool,
    corr: bool,
    bavg: bool,
    sort_key: SortKey,
) {
    if raw {
        if bavg {
            let ensembles = collapse_ensembles(records, memo);
            let references = ensemble_references(&ensembles);
            println!(
                "{}",
                print_formation_ensembles(&ensembles, &references, memo, unit, concs)
            );
        } else {
            let references = collect_monomers(records, memo, sort_key);
            println!(
                "{}",
                print_formation_section(records, &references, memo, unit, concs)
            );
        }
    }
    if corr {
        if bavg {
            let ensembles = corrected_collapse_ensembles(records, memo);
            let references = corrected_ensemble_references(&ensembles);
            println!(
                "{}",
                print_formation_corr_ensembles(&ensembles, &references, memo, unit, concs)
            );
        } else {
            let references = corrected_monomer_references(records, memo, sort_key);
            println!(
                "{}",
                print_formation_corr_section(records, &references, memo, unit, concs)
            );
        }
    }
}

/// The hydration sections: raw before corrected, over the collapsed
/// ensembles when `bavg` is set (`rqm/hydration.md` Printing, Hydration
/// States). The uncollapsed sections read the formation free energies
/// under the given sort ordering; the averaged ones the ensembles'
/// formation values. A section with no core groups prints nothing.
// rq-5e154b92
#[allow(clippy::too_many_arguments)]
fn print_hydration_sections(
    records: &[Record],
    memo: &ThermoMemo,
    concs: &ConcConditions,
    solvent: &str,
    raw: bool,
    corr: bool,
    bavg: bool,
    sort_key: SortKey,
) {
    let print_block = |groups: &[HydrationGroup]| {
        let section = print_hydration_section(groups, solvent, memo.temperature());
        if !section.is_empty() {
            println!("{section}");
        }
    };
    if bavg {
        if raw {
            let ensembles = collapse_ensembles(records, memo);
            let groups = hydration_states_from_ensembles(&ensembles, memo, concs, solvent);
            print_block(&groups);
        }
        if corr {
            let ensembles = corrected_collapse_ensembles(records, memo);
            let groups =
                corrected_hydration_states_from_ensembles(&ensembles, memo, concs, solvent);
            print_block(&groups);
        }
    } else {
        if raw {
            let groups = hydration_states(records, sort_key, memo, concs, solvent);
            print_block(&groups);
        }
        if corr {
            let groups = corrected_hydration_states(records, sort_key, memo, concs, solvent);
            print_block(&groups);
        }
    }
}

/// The ids of manipulation flags actually set on `cli` (`rqm/filtering.md`).
/// `run` derives `has_manipulation` from this and the consistency test
/// checks it against the declared flags, so every manipulation flag must be
/// listed here — the single source of truth that keeps the two in sync.
fn manipulation_ids(cli: &Cli) -> Vec<&'static str> {
    let mut ids = Vec::new();
    for (id, set) in [
        ("extract", !cli.extract.is_empty()),
        ("nextract", !cli.nextract.is_empty()),
        ("uniq", cli.uniq.is_some()),
        ("reacted", cli.reacted),
        ("reacted2", cli.reacted2),
        ("sort", cli.sort.is_some()),
        ("reverse", cli.reverse),
        ("select", cli.select.is_some()),
        ("glob", cli.glob),
        ("shuffle", cli.shuffle),
        ("noname", cli.noname),
        ("add_correction", !cli.add_correction.is_empty()),
        ("extrapolate", !cli.extrapolate.is_empty()),
    ] {
        if set {
            ids.push(id);
        }
    }
    ids
}

/// Collection-style exit codes for gather failures: a mixed invocation is
/// a usage error (exit 2), everything else is a runtime failure (exit 1).
fn collect_exit_code(error: &CollectError) -> ExitCode {
    match error {
        CollectError::Mixed { .. } => ExitCode::from(2),
        _ => ExitCode::FAILURE,
    }
}

/// clap-style diagnostic for a missing `--out` on a plain collection run
/// (`rqm/collection.md`); keeps the wording clap would produce.
fn missing_out_usage() -> ExitCode {
    eprintln!(
        "error: the following required arguments were not provided:\n  \
         --out <OUT>\n\nUsage: CJKQC --out <OUT> <INPUTS>...\n\n\
         For more information, try '--help'."
    );
    ExitCode::from(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Flags exempt from the manipulation/print accounting: settings flags
    /// are read into the conditions, window budget, unit, and the
    /// concentration and solvent conditions, `--formation`,
    /// `--formation-corr`, and the hydration flags into the printed
    /// sections, and the rest never affect the outcome rule.
    const EXEMPT_IDS: &[&str] = &[
        "out",
        "fout",
        "seed",
        "help",
        "version",
        "temp",
        "fc",
        "fscale",
        "makereal",
        "ignoreimag",
        "head_bytes",
        "tail_bytes",
        "parallel",
        "unit",
        "formation",
        "formation_corr",
        "hydration",
        "hydration_corr",
        "solvation",
        "solvation_corr",
        "rh",
        "psolvent",
        "conc",
        "cnt",
        "bavg",
        "movie",
    ];

    /// Every flag on `Cli` must be wired into `run`: a cutoff flag, a print
    /// column, or a manipulation flag reported by `manipulation_ids` (the
    /// same function `run` uses to decide `has_manipulation`), so a flag
    /// declared but never read cannot slip through (`rqm/filtering.md`).
    #[test]
    fn every_cli_flag_is_accounted_for() {
        let command = Cli::command();
        let all_manipulations = Cli::parse_from([
            "CJKQC",
            "input",
            "--extract",
            "1am",
            "--nextract",
            "1am",
            "--uniq",
            "dup",
            "--reacted",
            "--reacted2",
            "--sort",
            "el",
            "--reverse",
            "--select",
            "1",
            "--glob",
            "--shuffle",
            "--noname",
            "--add-correction",
            "corr.jsonl",
            "--extrapolate",
            "2s",
            "low.jsonl",
            "high.jsonl",
            "1.5",
        ]);
        let active = manipulation_ids(&all_manipulations);
        for arg in command.get_arguments() {
            let id = arg.get_id().as_str();
            if EXEMPT_IDS.contains(&id) || id == "inputs" {
                continue;
            }
            assert!(
                arg.get_long().is_some(),
                "flag `{id}` unexpectedly has no long form"
            );
            let known = CUTOFF_FLAGS.iter().any(|(flag, _, _)| *flag == id)
                || active.contains(&id)
                || Column::ALL
                    .iter()
                    .any(|column| column.label() == arg.get_long().unwrap());
            assert!(known, "flag `{id}` is not wired into run()");
        }
    }
}
