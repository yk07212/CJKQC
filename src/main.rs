//! CJKQC — a spiritual successor to JKQC from JKCS2.1.
//!
//! One command line does collection (`rqm/collection.md`) and the
//! manipulation pipeline with JKQC-style per-column printing
//! (`rqm/filtering.md`). Flags mirror JKQC's spelling with `--` because
//! clap does not support single-dash long flags.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{ArgMatches, CommandFactory, FromArgMatches, Parser};

use complied_jkqc::collect::{gather_inputs, CollectError, CollectOutcome, Gathered};
use complied_jkqc::db;
use complied_jkqc::filter::{
    apply_filters, parse_extract_query, print_table, CmpOp, Column, Cutoff, ExtractMode,
    FilterSpec, Grouping, Mode, Property, SortKey, Unit,
};
use complied_jkqc::formation::{collect_monomers, print_formation_section};
use complied_jkqc::metrics::parse_uniq_mode;
use complied_jkqc::reacted::ReactedMode;
use complied_jkqc::record::RecordStatus;
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
    about = "Collect, organize, filter, and post-process quantum-chemistry output for molecular clusters"
)]
struct Cli {
    /// Database to write; required unless a manipulation or print flag
    /// makes it optional (enforced in `run`, see `rqm/filtering.md`).
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

    /// Uniqueness: `dup` or a `metric[decimals]` comma list (el2.4,rg).
    #[arg(long = "uniq")]
    uniq: Option<String>,

    /// Keep the composition group's majority fragment pattern.
    #[arg(long = "reacted")]
    reacted: bool,

    /// Keep only records deviating from the fragment consensus.
    #[arg(long = "reacted2")]
    reacted2: bool,

    /// Sort by el, rg, mass, or name.
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

    /// Print energies in kcal/mol instead of hartree; takes no value.
    #[arg(long = "unit", num_args = 0..=1)]
    unit: Option<Option<String>>,

    /// Print the formation-energy section after the table.
    #[arg(long = "formation")]
    formation: bool,

    // --- cutoff flags: `--<flag> PROPERTY VALUE` (repeatable) ---
    #[arg(long = "cut", num_args = 2, value_names = ["PROPERTY", "VALUE"], allow_hyphen_values = true)]
    cut: Vec<String>,
    #[arg(long = "cutr", num_args = 2, value_names = ["PROPERTY", "VALUE"], allow_hyphen_values = true)]
    cutr: Vec<String>,
    #[arg(long = "pass", num_args = 2, value_names = ["PROPERTY", "VALUE"], allow_hyphen_values = true)]
    pass: Vec<String>,
    #[arg(long = "passr", num_args = 2, value_names = ["PROPERTY", "VALUE"], allow_hyphen_values = true)]
    passr: Vec<String>,
    #[arg(long = "is", num_args = 2, value_names = ["PROPERTY", "VALUE"], allow_hyphen_values = true)]
    is: Vec<String>,
    #[arg(long = "isnot", num_args = 2, value_names = ["PROPERTY", "VALUE"], allow_hyphen_values = true)]
    isnot: Vec<String>,
    #[arg(long = "filter-lt", num_args = 2, value_names = ["PROPERTY", "VALUE"], allow_hyphen_values = true)]
    filter_lt: Vec<String>,
    #[arg(long = "filter-ge", num_args = 2, value_names = ["PROPERTY", "VALUE"], allow_hyphen_values = true)]
    filter_ge: Vec<String>,
    #[arg(long = "rel-filter-lt", num_args = 2, value_names = ["PROPERTY", "VALUE"], allow_hyphen_values = true)]
    rel_filter_lt: Vec<String>,
    #[arg(long = "rel-filter-ge", num_args = 2, value_names = ["PROPERTY", "VALUE"], allow_hyphen_values = true)]
    rel_filter_ge: Vec<String>,

    // --- print flags: one per column (rqm/filtering.md) ---
    #[arg(long = "name")]
    name: bool,
    #[arg(long = "status")]
    status: bool,
    #[arg(long = "program")]
    program: bool,
    #[arg(long = "method")]
    method: bool,
    #[arg(long = "el")]
    el: bool,
    #[arg(long = "zpe")]
    zpe: bool,
    #[arg(long = "h")]
    h: bool,
    #[arg(long = "s")]
    s: bool,
    #[arg(long = "g")]
    g: bool,
    #[arg(long = "rg")]
    rg: bool,
    #[arg(long = "mass")]
    mass: bool,
    #[arg(long = "dip")]
    dip: bool,
    #[arg(long = "charge")]
    charge: bool,
    #[arg(long = "multiplicity")]
    multiplicity: bool,
    #[arg(long = "natoms")]
    natoms: bool,
    #[arg(long = "nfreq")]
    nfreq: bool,
    #[arg(long = "nimag")]
    nimag: bool,
    #[arg(long = "components")]
    components: bool,

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
                return usage(&format!("`{value}` is not a number"));
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
    };
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

    let set_print = |column: Column| match column {
        Column::Name => cli.name,
        Column::Status => cli.status,
        Column::Program => cli.program,
        Column::Method => cli.method,
        Column::El => cli.el,
        Column::Zpe => cli.zpe,
        Column::H => cli.h,
        Column::S => cli.s,
        Column::G => cli.g,
        Column::Rg => cli.rg,
        Column::Mass => cli.mass,
        Column::Dip => cli.dip,
        Column::Charge => cli.charge,
        Column::Multiplicity => cli.multiplicity,
        Column::Natoms => cli.natoms,
        Column::Nfreq => cli.nfreq,
        Column::Nimag => cli.nimag,
        Column::Components => cli.components,
    };
    let columns: Vec<Column> = Column::ALL
        .iter()
        .copied()
        .filter(|column| set_print(*column))
        .collect();
    // `--formation` prints a section instead of a column but still counts
    // as a print flag for the visible-outcome rule (`rqm/formation.md`).
    let has_print = !columns.is_empty() || cli.formation;

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
            let gathered = match gather_inputs(&inputs) {
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
            if !columns.is_empty() {
                println!("{}", print_table(&records, &columns, &memo, unit));
            }
            // A print-only run has no --sort (that would be a
            // manipulation), so the references use the el ordering
            // (rqm/formation.md).
            if cli.formation {
                let references = collect_monomers(&records, &memo, SortKey::El);
                println!(
                    "{}",
                    print_formation_section(&records, &references, &memo, unit)
                );
            }
        }
        return match &cli.out {
            Some(out) => run_collection(&inputs, out, cli.fout),
            None => ExitCode::SUCCESS,
        };
    }

    // Manipulation run: gather, filter, print, write.
    let (mut records, problems) = match gather_inputs(&inputs) {
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
        grouping: if cli.noname {
            Grouping::Unit
        } else {
            Grouping::Composition
        },
    };
    records = match apply_filters(&records, &spec, &memo) {
        Ok(records) => records,
        Err(error) => {
            eprintln!("error: extract query: {error}");
            return ExitCode::from(2);
        }
    };

    if !columns.is_empty() {
        println!("{}", print_table(&records, &columns, &memo, unit));
    }
    // The formation section prints after the table, before the summary,
    // over the pipeline survivors (rqm/formation.md).
    if cli.formation {
        let sort_key = spec.sort.unwrap_or(SortKey::El);
        let references = collect_monomers(&records, &memo, sort_key);
        println!(
            "{}",
            print_formation_section(&records, &references, &memo, unit)
        );
    }

    if let Some(out) = &cli.out {
        let replacing = out.exists();
        let mut lines = Vec::with_capacity(records.len());
        for record in &records {
            if let Err(invalidity) = record.validate() {
                eprintln!("error: {invalidity}");
                return ExitCode::FAILURE;
            }
            match serde_json::to_string(record) {
                Ok(line) => lines.push(line),
                Err(error) => {
                    eprintln!("error: {error}");
                    return ExitCode::FAILURE;
                }
            }
        }
        if let Err(error) = db::write_database(out, &lines) {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
        warn_if_replacing(replacing, cli.fout, out);
        println!("filtered {} records into {}", records.len(), out.display());
    }

    if problems.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

// rq-a2681a7a — collection behaves exactly as `rqm/collection.md`.
fn run_collection(inputs: &[&Path], out: &Path, fout: bool) -> ExitCode {
    let replacing = out.exists();
    match complied_jkqc::collect_inputs(inputs, out) {
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

fn usage(message: &str) -> ExitCode {
    eprintln!("error: {message}");
    ExitCode::from(2)
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
    /// are read into the conditions and unit, `--formation` into the
    /// formation section, and the rest never affect the outcome rule.
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
        "unit",
        "formation",
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
