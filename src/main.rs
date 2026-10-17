//! CJKQC — a spiritual successor to JKQC from JKCS2.1.
//!
//! Command-line shell for CJKQC. The scaffold provides only the built-in
//! `--help`/`--version`; every other invocation is a usage error.

use clap::Parser;

// rq-c3792c41 rq-899932a2
#[derive(Parser)]
#[command(
    name = "CJKQC",
    version,
    about = "Collect, organize, filter, and post-process quantum-chemistry output for molecular clusters"
)]
struct Cli {}

// rq-899932a2
fn unimplemented_stub() -> ! {
    eprintln!("error: no functionality is implemented yet");
    eprintln!("For more information, try '--help'.");
    std::process::exit(2);
}

// rq-c3792c41
fn main() {
    match Cli::try_parse() {
        Ok(_) => unimplemented_stub(),
        Err(err) if err.use_stderr() => unimplemented_stub(),
        Err(err) => err.exit(),
    }
}
