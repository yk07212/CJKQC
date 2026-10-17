//! Integration tests for the CJKQC command scaffold.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

// rq-e61c2633
fn cjkqc(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_CJKQC"))
        .args(args)
        .output()
        .expect("failed to start CJKQC")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test] // rq-f9a3759e
fn help_prints_usage() {
    let out = cjkqc(&["--help"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("Usage: CJKQC"));
}

#[test] // rq-e1c51ff7
fn short_help_flag_behaves_like_help() {
    let out = cjkqc(&["-h"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("Usage: CJKQC"));
}

#[test] // rq-1bbcf87c
fn version_prints_the_crate_version() {
    let out = cjkqc(&["--version"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("CJKQC"));
    assert!(text.contains(env!("CARGO_PKG_VERSION")));
}

#[test] // rq-843d152e
fn short_version_flag_behaves_like_version() {
    let out = cjkqc(&["-V"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains(env!("CARGO_PKG_VERSION")));
}

#[test] // rq-72e0ded9
fn help_takes_precedence_over_extra_arguments() {
    let out = cjkqc(&["--help", "extra"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("Usage: CJKQC"));
}

#[test] // rq-953a8521
fn version_takes_precedence_over_extra_arguments() {
    let out = cjkqc(&["--version", "extra"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains(env!("CARGO_PKG_VERSION")));
}

#[test] // rq-6b872f89
fn no_arguments_is_a_usage_error() {
    let out = cjkqc(&[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("no functionality is implemented yet"));
}

#[test] // rq-be35a7b3
fn positional_arguments_are_a_usage_error() {
    let out = cjkqc(&["output.log", "output2.out"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("no functionality is implemented yet"));
}

#[test] // rq-e003541d
fn jkqc_style_invocation_is_a_usage_error() {
    let out = cjkqc(&["output.log", "-el"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(!stderr(&out).is_empty());
}

#[test] // rq-bd59ba92
fn help_advertises_no_domain_flags() {
    let out = cjkqc(&["--help"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("--help"));
    assert!(text.contains("--version"));
    let option_lines = text
        .lines()
        .filter(|line| line.trim_start().starts_with('-'))
        .count();
    assert_eq!(option_lines, 2, "help must list only --help/--version");
}

#[test] // rq-2486bcc8
fn build_produces_the_exact_executable_name() {
    let path = Path::new(env!("CARGO_BIN_EXE_CJKQC"));
    let expected = format!("CJKQC{}", std::env::consts::EXE_SUFFIX);
    assert_eq!(
        path.file_name().and_then(|name| name.to_str()),
        Some(expected.as_str())
    );
    assert!(path.is_file());
}

#[test] // rq-22fd07a1
fn no_placeholder_canvas_remains() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let self_path = manifest.join(file!());
    let mut files = vec![manifest.join("Cargo.toml")];
    for dir in ["src", "tests"] {
        for entry in fs::read_dir(manifest.join(dir))
            .unwrap_or_else(|error| panic!("failed to list {dir}/: {error}"))
        {
            let path = entry
                .unwrap_or_else(|error| panic!("failed to read a {dir}/ entry: {error}"))
                .path();
            if path.extension().is_some_and(|ext| ext == "rs") && path != self_path {
                files.push(path);
            }
        }
    }
    for file in files {
        let content = fs::read_to_string(&file)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", file.display()));
        assert!(
            !content.contains("canvas"),
            "{} still references the template placeholder",
            file.display()
        );
    }
}
