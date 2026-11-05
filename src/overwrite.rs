//! The overwrite confirmation behind `--out` and `movie.xyz` (see
//! `rqm/collection.md`, Overwrite Confirmation): an existing target is
//! replaced only after a yes/no answer — or `--fout` — and a run without
//! an interactive terminal refuses rather than guessing. The asking is
//! CLI logic: the question goes to the caller's prompt sink and the
//! answer comes from the caller's reader, so tests inject both.
//!
//! (rq-eff8dce8)

use std::io::{BufRead, Write};
use std::path::Path;

/// The confirmation verdict for one target.
///
/// (rq-b0fad254)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverwriteDecision {
    /// The answer was yes: the replacement proceeds.
    Proceed,
    /// The answer was no (or there was no answer): the run aborts.
    Declined,
    /// There was no terminal to ask: the run refuses unless forced.
    Refused,
}

/// The answer grammar: the trimmed line equals `y` or `yes`,
/// case-insensitively.
///
/// (rq-298c8373)
pub fn parse_overwrite_answer(line: &str) -> bool {
    matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

/// The confirmation of the Overwrite Confirmation rule for one target.
/// Interactive: the question `<path> already exists; overwrite? [y/N]` is
/// written to `prompt` (the CLI's stderr) and one answer line is read
/// from `input`, trimmed, and parsed; `Proceed` only for a yes answer,
/// `Declined` otherwise, end-of-input included. A question that cannot
/// be written to `prompt` is treated like the non-interactive case: the
/// verdict is `Refused` and `input` is untouched. Non-interactive:
/// neither stream is touched and the verdict is `Refused`.
///
/// (rq-59b68ca8)
pub fn confirm_overwrite(
    path: &Path,
    prompt: &mut dyn Write,
    input: &mut dyn BufRead,
    interactive: bool,
) -> OverwriteDecision {
    if !interactive {
        return OverwriteDecision::Refused;
    }
    let question = format!("{} already exists; overwrite? [y/N]", path.display());
    if write!(prompt, "{question}").is_err() || prompt.flush().is_err() {
        // The question could not be shown; asking anyway would block on
        // an answer the user never saw a prompt for
        // (rqm/collection.md, Overwrite Confirmation).
        return OverwriteDecision::Refused;
    }
    let mut line = String::new();
    match input.read_line(&mut line) {
        Ok(count) if count > 0 && parse_overwrite_answer(&line) => OverwriteDecision::Proceed,
        Ok(_) => OverwriteDecision::Declined,
        Err(_) => OverwriteDecision::Declined,
    }
}

/// The stderr error line for a refused replacement:
/// `error: <path> exists; not overwriting` for `Declined` and
/// `error: <path> exists; not interactive; use --fout to force` for
/// `Refused`; `None` for `Proceed`.
///
/// (rq-7e0e7fa6)
pub fn overwrite_error(decision: OverwriteDecision, path: &Path) -> Option<String> {
    let path = path.display();
    match decision {
        OverwriteDecision::Proceed => None,
        OverwriteDecision::Declined => Some(format!("error: {path} exists; not overwriting")),
        OverwriteDecision::Refused => Some(format!(
            "error: {path} exists; not interactive; use --fout to force"
        )),
    }
}
