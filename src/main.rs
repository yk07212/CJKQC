//! CJKQC — a spiritual successor to JKQC from JKCS2.1.
//!
//! One command line does collection (`rqm/collection.md`) and the
//! manipulation pipeline with JKQC-style per-column printing
//! (`rqm/filtering.md`). Flags mirror JKQC's spelling with `--` because
//! clap does not support single-dash long flags.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{ArgMatches, CommandFactory, FromArgMatches, Parser};

use complied_jkqc::collect::{gather_inputs_with, CollectError, CollectOutcome, Gathered};
use complied_jkqc::correction::{attach_corrections, CorrectionError};
use complied_jkqc::db::{self, DbError, LoadProblem};
use complied_jkqc::filter::{
    apply_filters, parse_extract_query, print_table, CmpOp, Column, Cutoff, ExtractMode,
    FilterSpec, Grouping, Mode, Property, SortKey, Unit,
};
use complied_jkqc::formation::{
    collect_monomers, corrected_monomer_references, parse_conc_value, print_formation_corr_section,
    print_formation_section, ConcConditions, ConcSpec,
};
use complied_jkqc::metrics::parse_uniq_mode;
use complied_jkqc::parser::WindowBudget;
use complied_jkqc::population::{parse_pop_level, PopLevel};
use complied_jkqc::reacted::ReactedMode;
use complied_jkqc::record::{Record, RecordStatus};
use complied_jkqc::thermo::{ThermoConditions, ThermoMemo};

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

    /// Replace an existing --out without the warning.
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

    /// Attach single-point corrections from the named database, matching
    /// correction records to run records by exact case-sensitive name
    /// (a manipulation flag).
    #[arg(long = "add-correction", value_name = "PATH")]
    add_correction: Vec<PathBuf>,

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
    /// Print the record name column.
    #[arg(long = "name")]
    name: bool,
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
    /// Print the net charge (elementary charges).
    #[arg(long = "charge")]
    charge: bool,
    /// Print the spin multiplicity column.
    #[arg(long = "multiplicity")]
    multiplicity: bool,
    /// Print the geometry atom count.
    #[arg(long = "natoms")]
    natoms: bool,
    /// Print the lowest stored vibrational frequency (cm^-1, 1 decimal;
    /// raw values, so imaginary modes print negative).
    #[arg(long = "lf")]
    lf: bool,
    /// Print the vibrational frequency count.
    #[arg(long = "nfreq")]
    nfreq: bool,
    /// Print the negative (imaginary) frequency count.
    #[arg(long = "nimag")]
    nimag: bool,
    /// Print the resolved composition as 2am+3sa-style pairs.
    #[arg(long = "components")]
    components: bool,
    /// Print the composition token: 2am3sa-style alphabetical pairs
    /// without separators.
    #[arg(long = "ct")]
    ct: bool,

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
        head_bytes: cli.head_bytes.unwrap_or(complied_jkqc::DEFAULT_HEAD_BYTES),
        tail_bytes: cli.tail_bytes.unwrap_or(complied_jkqc::DEFAULT_TAIL_BYTES),
    };
    if budget.head_bytes == 0 || budget.tail_bytes == 0 {
        return usage("--head-bytes and --tail-bytes must be at least 1");
    }
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
            .any(|id| *id != "add_correction");
    // Repeating --add-correction is a usage error (`rqm/correction.md`).
    if cli.add_correction.len() > 1 {
        return usage("cannot repeat --add-correction; give one correction database");
    }

    let set_print = |column: Column| match column {
        Column::Name => cli.name,
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
        Column::Charge => cli.charge,
        Column::Multiplicity => cli.multiplicity,
        Column::Natoms => cli.natoms,
        Column::Lf => cli.lf,
        Column::Nfreq => cli.nfreq,
        Column::Nimag => cli.nimag,
        Column::Components => cli.components,
        Column::Ct => cli.ct,
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
    // `--formation` and `--formation-corr` print sections instead of
    // columns but still count as print flags for the visible-outcome rule
    // (`rqm/formation.md`).
    let has_print = !columns.is_empty() || cli.formation || cli.formation_corr;

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
        if has_print {
            let gathered = match gather_inputs_with(&inputs, &budget) {
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
            // Every --conc code must appear in the record set the
            // formation sections evaluate (`rqm/formation.md`).
            // rq-10341821
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
            // (rqm/formation.md).
            if cli.formation {
                let references = collect_monomers(&records, &memo, SortKey::El);
                println!(
                    "{}",
                    print_formation_section(&records, &references, &memo, unit, &concs)
                );
            }
            // The corrected section prints after the raw section, from
            // the stored corrections (`rqm/formation.md`).
            if cli.formation_corr {
                let references = corrected_monomer_references(&records, &memo, SortKey::El);
                println!(
                    "{}",
                    print_formation_corr_section(&records, &references, &memo, unit, &concs)
                );
            }
        }
        return match &cli.out {
            Some(out) => run_collection(&inputs, out, cli.fout, &budget),
            None => ExitCode::SUCCESS,
        };
    }

    // A run whose only enrichment is attachment, printing nothing:
    // collection semantics — every record written, the collection summary
    // line with the corrected suffix (`rqm/correction.md`).
    if cli.add_correction.len() == 1 && !has_other_manipulation && !has_print {
        return run_correcting_collection(
            &inputs,
            cli.add_correction[0].as_path(),
            cli.out.as_deref(),
            cli.fout,
            &budget,
        );
    }

    // Manipulation run: gather, filter, print, write.
    let (mut records, problems) = match gather_inputs_with(&inputs, &budget) {
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

    // Every --conc code must appear in the record set the formation
    // sections evaluate — the pipeline survivors (`rqm/formation.md`).
    // rq-10341821
    if cli.formation || cli.formation_corr {
        if let Err(error) = validate_conc_codes(&concs, &records) {
            return usage(&error);
        }
    }

    if !columns.is_empty() {
        println!("{}", print_table(&records, &columns, &memo, unit, grouping));
    }
    // The formation sections print after the table, before the summary,
    // over the pipeline survivors (`rqm/formation.md`).
    if cli.formation {
        let sort_key = spec.sort.unwrap_or(SortKey::El);
        let references = collect_monomers(&records, &memo, sort_key);
        println!(
            "{}",
            print_formation_section(&records, &references, &memo, unit, &concs)
        );
    }
    // The corrected section prints after the raw one, from the same
    // reference selection (`rqm/formation.md`).
    if cli.formation_corr {
        let sort_key = spec.sort.unwrap_or(SortKey::El);
        let references = corrected_monomer_references(&records, &memo, sort_key);
        println!(
            "{}",
            print_formation_corr_section(&records, &references, &memo, unit, &concs)
        );
    }

    if let Some(out) = &cli.out {
        let replacing = out.exists();
        if let Err(error) = write_records(out, &records) {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
        warn_if_replacing(replacing, cli.fout, out);
        // The filtered summary reports the enrichment when the run
        // carried --add-correction (`rqm/correction.md`).
        // rq-58d85e68
        let suffix = if cli.add_correction.is_empty() {
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

    if problems.is_empty() && correction_problems.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

// rq-a2681a7a — collection behaves exactly as `rqm/collection.md`.
fn run_collection(inputs: &[&Path], out: &Path, fout: bool, budget: &WindowBudget) -> ExitCode {
    let replacing = out.exists();
    match complied_jkqc::collect_inputs_with(inputs, out, budget) {
        Ok(CollectOutcome::Build {
            written,
            failed,
            truncated,
        }) => {
            warn_if_replacing(replacing, fout, out);
            println!(
                "collected {written} records into {} ({failed} failed, {truncated} truncated)",
                out.display()
            );
            ExitCode::SUCCESS
        }
        Ok(CollectOutcome::Merge { written, problems }) => {
            warn_if_replacing(replacing, fout, out);
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
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error @ CollectError::Mixed { .. }) => {
            eprintln!("error: {error}");
            ExitCode::from(2)
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

// rq-a2681a7a
fn warn_if_replacing(replacing: bool, forced: bool, out: &Path) {
    if replacing && !forced {
        eprintln!("warning: replacing existing database {}", out.display());
    }
}

/// A correcting collection run (`rqm/correction.md`): the plain collection
/// flow with attachment inserted before the write — every record is
/// written, and the collection summary line carries the corrected suffix.
///
/// (rq-58d85e68 rq-66e4cb89)
fn run_correcting_collection(
    inputs: &[&Path],
    correction_path: &Path,
    out: Option<&Path>,
    fout: bool,
    budget: &WindowBudget,
) -> ExitCode {
    // Unreachable via the visible-outcome rule, which demands --out on a
    // run with no print flags and no other manipulation — but fail loudly
    // rather than silently succeeding if that rule is ever relaxed
    // (`rqm/correction.md`).
    // rq-3fd0b9bb
    let Some(out) = out else {
        eprintln!("error: a correcting collection run needs --out");
        return ExitCode::from(2);
    };
    let (records, problems, is_build) = match gather_inputs_with(inputs, budget) {
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
    let Some((records, correction_problems)) = attach_from_path(&records, correction_path) else {
        return ExitCode::FAILURE;
    };
    let replacing = out.exists();
    if let Err(error) = write_records(out, &records) {
        eprintln!("error: {error}");
        return ExitCode::FAILURE;
    }
    warn_if_replacing(replacing, fout, out);
    // The collection summary reports the enrichment (`rqm/correction.md`).
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
    if problems.is_empty() && correction_problems.is_empty() {
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
        if let Some(reason) = complied_jkqc::correction::skip_reason(record) {
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
    /// concentration conditions, `--formation` and `--formation-corr`
    /// into the formation sections, and the rest never affect the
    /// outcome rule.
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
        "unit",
        "formation",
        "formation_corr",
        "conc",
        "cnt",
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
