//! CJKQC — a spiritual successor to JKQC from JKCS2.1.
//!
//! Collection command line: `CJKQC <inputs>… --out <db>` builds a database
//! from QC outputs or merges record databases, per `rqm/collection.md`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use complied_jkqc::collect::{collect_inputs, CollectError, CollectOutcome};

// rq-70fc34c2 rq-899932a2
#[derive(Parser)]
#[command(
    name = "CJKQC",
    version,
    about = "Collect, organize, filter, and post-process quantum-chemistry output for molecular clusters"
)]
struct Cli {
    /// Database to write: build replaces it, merge combines inputs into it.
    #[arg(short = 'o', long = "out")]
    out: PathBuf,

    /// Replace an existing --out without the warning.
    #[arg(long = "fout")]
    fout: bool,

    /// Input files: QC program outputs (build) or record databases (merge).
    #[arg(required = true)]
    inputs: Vec<PathBuf>,
}

// rq-a4e0ba89 rq-a2681a7a
fn main() -> ExitCode {
    let cli = Cli::parse();
    let replacing = cli.out.exists();
    let inputs: Vec<&std::path::Path> = cli.inputs.iter().map(PathBuf::as_path).collect();
    match collect_inputs(&inputs, &cli.out) {
        Ok(CollectOutcome::Build {
            written,
            failed,
            truncated,
        }) => {
            warn_if_replacing(replacing, cli.fout, &cli.out);
            println!(
                "collected {written} records into {} ({failed} failed, {truncated} truncated)",
                cli.out.display()
            );
            ExitCode::SUCCESS
        }
        Ok(CollectOutcome::Merge { written, problems }) => {
            warn_if_replacing(replacing, cli.fout, &cli.out);
            for problem in &problems {
                eprintln!(
                    "warning: {} line {}: {}",
                    problem.input.display(),
                    problem.line,
                    problem.reason
                );
            }
            println!("merged {written} records into {}", cli.out.display());
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
fn warn_if_replacing(replacing: bool, forced: bool, out: &std::path::Path) {
    if replacing && !forced {
        eprintln!("warning: replacing existing database {}", out.display());
    }
}
