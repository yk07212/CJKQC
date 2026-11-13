//! Unit tests for the movie export (`rqm/movie.md`): frame eligibility
//! and order, the frame text format, and the atomic write.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use compiled_jkqc::movie::{frame_text, movie_frames, write_movie_xyz};
use compiled_jkqc::{Geometry, Record, RecordStatus};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "cjkqc-movie-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// A record with a geometry: one `("symbol", x, y, z)` triple per atom.
fn geometry_record(
    name: Option<&str>,
    energy: Option<f64>,
    atoms: &[(&str, f64, f64, f64)],
) -> Record {
    let mut symbols = Vec::new();
    let mut coordinates = Vec::new();
    for (symbol, x, y, z) in atoms {
        symbols.push((*symbol).to_string());
        coordinates.extend([*x, *y, *z]);
    }
    Record {
        name: name.map(str::to_string),
        electronic_energy: energy,
        geometry: Some(Geometry {
            symbols,
            coordinates,
        }),
        ..Record::default()
    }
}

#[test] // rq-78af10b3
fn movie_frames_keep_ok_records_with_a_geometry_in_order() {
    let first = geometry_record(Some("a"), Some(-1.0), &[("N", 0.0, 0.0, 0.0)]);
    let no_geometry = Record {
        name: Some("b".to_string()),
        ..Record::default()
    };
    let failed = Record {
        status: RecordStatus::Failed,
        name: Some("c".to_string()),
        geometry: Some(Geometry {
            symbols: vec!["N".to_string()],
            coordinates: vec![0.0, 0.0, 0.0],
        }),
        ..Record::default()
    };
    let records = vec![first, no_geometry, failed];
    let (frames, excluded) = movie_frames(&records);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].name.as_deref(), Some("a"));
    assert_eq!(excluded, 2);
}

#[test] // rq-9e89b02f
fn the_comment_line_names_the_record_and_its_electronic_energy() {
    let record = geometry_record(Some("1dma"), Some(-38.51), &[("N", 0.0, 0.0, 0.0)]);
    let text = frame_text(&record);
    let comment = text.lines().nth(1).expect("comment line");
    assert_eq!(comment, "1dma E=-38.5100000000");
}

#[test] // rq-fd2671de
fn a_record_without_a_name_has_a_dash_name() {
    let record = geometry_record(None, Some(-1.0), &[("N", 0.0, 0.0, 0.0)]);
    let text = frame_text(&record);
    let comment = text.lines().nth(1).expect("comment line");
    assert_eq!(comment, "- E=-1.0000000000");
}

#[test] // rq-9b8707e4
fn a_record_without_an_electronic_energy_omits_the_e_part() {
    let record = geometry_record(Some("1dma"), None, &[("N", 0.0, 0.0, 0.0)]);
    let text = frame_text(&record);
    let comment = text.lines().nth(1).expect("comment line");
    assert_eq!(comment, "1dma");
}

#[test] // rq-a4493f29
fn a_record_with_neither_name_nor_energy_comments_a_dash() {
    let record = geometry_record(None, None, &[("N", 0.0, 0.0, 0.0)]);
    let text = frame_text(&record);
    let comment = text.lines().nth(1).expect("comment line");
    assert_eq!(comment, "-");
}

#[test] // rq-92d5adc8
fn atom_lines_carry_the_stored_geometry() {
    let record = geometry_record(
        Some("nh"),
        None,
        &[("N", 0.0, 0.0, 0.0), ("H", 0.0, 0.0, 1.01)],
    );
    let text = frame_text(&record);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0], "2");
    assert_eq!(lines[2], "N 0.000000 0.000000 0.000000");
    assert_eq!(lines[3], "H 0.000000 0.000000 1.010000");
}

#[test] // rq-e5ec2a43
fn write_movie_xyz_replaces_and_writes_empty_files_atomically() {
    let dir = temp_dir();
    let path = dir.join("movie.xyz");
    // An empty slice writes an empty file.
    write_movie_xyz(&path, &[]).expect("empty write");
    assert_eq!(fs::read(&path).expect("read"), b"");

    // A second write replaces the first content.
    let first = geometry_record(Some("a"), Some(-1.0), &[("N", 0.0, 0.0, 0.0)]);
    let second = geometry_record(
        Some("b"),
        Some(-2.0),
        &[("O", 0.1, 0.2, 0.3), ("H", 0.4, 0.5, 0.6)],
    );
    let frames = vec![&first, &second];
    write_movie_xyz(&path, &frames).expect("replacement write");
    let written = fs::read_to_string(&path).expect("read");
    let expected: String = frames.iter().map(|frame| frame_text(frame)).collect();
    assert_eq!(written, expected);
}

#[test] // rq-e5292dca
fn a_failed_write_is_a_movie_error() {
    let dir = temp_dir();
    let record = geometry_record(Some("a"), None, &[("N", 0.0, 0.0, 0.0)]);
    let error = write_movie_xyz(&dir.join("no-such-dir/movie.xyz"), &[&record])
        .expect_err("missing parent directory");
    assert!(
        matches!(error, compiled_jkqc::MovieError::Io(_)),
        "{error:?}"
    );
}

#[test] // rq-545321a5
fn frame_text_yields_an_empty_string_for_a_mismatched_geometry() {
    let record = Record {
        name: Some("bad".to_string()),
        geometry: Some(Geometry {
            symbols: vec!["O".to_string(), "H".to_string()],
            coordinates: vec![0.0, 0.0, 0.0],
        }),
        ..Record::default()
    };
    assert_eq!(frame_text(&record), "");
}

#[test] // rq-79e2785e
fn a_names_line_breaks_do_not_split_the_comment_line() {
    let record = geometry_record(Some("wa\nter"), Some(-1.0), &[("O", 0.0, 0.0, 0.0)]);
    let text = frame_text(&record);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines,
        [
            "1",
            "wa ter E=-1.0000000000",
            "O 0.000000 0.000000 0.000000"
        ]
    );

    // A carriage return is flattened the same way, keeping one comment line.
    let crlf = geometry_record(Some("a\r\nb"), None, &[("O", 0.0, 0.0, 0.0)]);
    let text = frame_text(&crlf);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines, ["1", "a  b", "O 0.000000 0.000000 0.000000"]);
}
