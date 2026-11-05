//! Unit tests for the overwrite confirmation (`rqm/collection.md`,
//! Overwrite Confirmation), run with injected prompt and answer streams so
//! every decision branch is reachable without a terminal.

use std::io::{BufRead, BufReader, Cursor};
use std::path::Path;

use complied_jkqc::overwrite::{confirm_overwrite, overwrite_error, OverwriteDecision};

/// Runs one interactive confirmation against the given answer text,
/// returning the decision and everything written to the prompt sink.
fn confirm(path: &str, answer: &str) -> (OverwriteDecision, Vec<u8>) {
    let mut prompt = Vec::new();
    let mut input = BufReader::new(Cursor::new(answer.as_bytes().to_vec()));
    let decision = confirm_overwrite(Path::new(path), &mut prompt, &mut input, true);
    (decision, prompt)
}

#[test] // rq-ee9ac740
fn confirm_overwrite_proceeds_on_yes_answers() {
    for answer in ["y", "Y", "yes", "YES", " yes "] {
        let (decision, _) = confirm("db.jsonl", answer);
        assert_eq!(decision, OverwriteDecision::Proceed, "answer {answer:?}");
    }
}

#[test] // rq-83ccc28e
fn confirm_overwrite_declines_on_any_other_answer() {
    for answer in ["n", "", "no", "maybe"] {
        let (decision, _) = confirm("db.jsonl", answer);
        assert_eq!(decision, OverwriteDecision::Declined, "answer {answer:?}");
    }
}

#[test] // rq-42f7e417
fn confirm_overwrite_declines_at_end_of_input() {
    let (decision, prompt) = confirm("db.jsonl", "");
    assert_eq!(decision, OverwriteDecision::Declined);
    assert!(
        !prompt.is_empty(),
        "an end-of-input answer is still a asked question"
    );
}

#[test] // rq-466cd609
fn confirm_overwrite_refuses_without_a_terminal() {
    let mut prompt = Vec::new();
    let mut input = BufReader::new(Cursor::new(b"y\n".to_vec()));
    let decision = confirm_overwrite(Path::new("db.jsonl"), &mut prompt, &mut input, false);
    assert_eq!(decision, OverwriteDecision::Refused);
    assert!(prompt.is_empty(), "the prompt must stay untouched");
    assert_eq!(
        input.get_ref().position(),
        0,
        "the input stream must stay untouched"
    );
}

#[test] // rq-ed1ff9fd
fn the_interactive_question_names_the_target() {
    let (_, prompt) = confirm("db.jsonl", "y");
    let question = String::from_utf8(prompt).expect("utf-8 prompt");
    assert_eq!(question, "db.jsonl already exists; overwrite? [y/N]");
}

#[test] // rq-13aa8b07
fn a_declined_overwrite_maps_to_the_not_overwriting_error() {
    let message = overwrite_error(OverwriteDecision::Declined, Path::new("db.jsonl"));
    assert_eq!(
        message.as_deref(),
        Some("error: db.jsonl exists; not overwriting")
    );
}

#[test] // rq-0727ffaf
fn a_refused_overwrite_names_fout() {
    let message = overwrite_error(OverwriteDecision::Refused, Path::new("db.jsonl"));
    assert_eq!(
        message.as_deref(),
        Some("error: db.jsonl exists; not interactive; use --fout to force")
    );
}

/// A prompt sink that always fails, like a closed or broken stderr.
struct FailingPrompt;

impl std::io::Write for FailingPrompt {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("closed"))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Err(std::io::Error::other("closed"))
    }
}

#[test] // rq-59b68ca8
fn a_question_that_cannot_be_written_refuses_without_reading() {
    let mut prompt = FailingPrompt;
    let mut input = BufReader::new(Cursor::new(b"y\n".to_vec()));
    let decision = confirm_overwrite(Path::new("db.jsonl"), &mut prompt, &mut input, true);
    assert_eq!(decision, OverwriteDecision::Refused);
    // The answer was never consumed.
    let mut rest = String::new();
    input.read_line(&mut rest).expect("read remainder");
    assert_eq!(rest, "y\n");
}
