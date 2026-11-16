//! Unit tests for the movie export (`rqm/movie.md`): frame eligibility
//! and order, the frame text format, and the atomic write.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use compiled_jkqc::movie::{
    frame_text, movie_frames, select_xyz_records, write_movie_xyz, write_xyz_files, xyz_file_name,
};
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

#[test] // rq-7dda644d
fn xyz_file_name_uses_the_record_name_and_rejects_unsafe_names() {
    let safe = geometry_record(Some("1sa-3xstr"), None, &[("N", 0.0, 0.0, 0.0)]);
    assert_eq!(xyz_file_name(&safe).as_deref(), Some("1sa-3xstr.xyz"));

    let nameless = geometry_record(None, None, &[("N", 0.0, 0.0, 0.0)]);
    assert_eq!(xyz_file_name(&nameless), None);

    for name in [
        "",
        ".",
        "..",
        "../escape",
        "a/b",
        "a\\b",
        "a\0b",
        "bad\nname",
        "bad\tname",
        "bad\u{7f}name",
    ] {
        let unsafe_name = geometry_record(Some(name), None, &[("N", 0.0, 0.0, 0.0)]);
        assert_eq!(xyz_file_name(&unsafe_name), None, "name {name:?}");
    }

    let failed = Record {
        status: RecordStatus::Failed,
        name: Some("failed".to_string()),
        error: Some("boom".to_string()),
        geometry: Some(Geometry {
            symbols: vec!["N".to_string()],
            coordinates: vec![0.0, 0.0, 0.0],
        }),
        ..Record::default()
    };
    assert_eq!(xyz_file_name(&failed), None);

    let no_geometry = Record {
        name: Some("no-geometry".to_string()),
        ..Record::default()
    };
    assert_eq!(xyz_file_name(&no_geometry), None);
}

#[test] // rq-f48c7dd8
fn select_xyz_records_keeps_the_eligible_subset_in_order_and_counts_skips() {
    let first = geometry_record(Some("a"), Some(-1.0), &[("N", 0.0, 0.0, 0.0)]);
    let nameless = geometry_record(None, Some(-2.0), &[("N", 0.0, 0.0, 0.0)]);
    let no_geometry = Record {
        name: Some("b".to_string()),
        ..Record::default()
    };
    let failed = Record {
        status: RecordStatus::Failed,
        name: Some("c".to_string()),
        error: Some("boom".to_string()),
        geometry: Some(Geometry {
            symbols: vec!["N".to_string()],
            coordinates: vec![0.0, 0.0, 0.0],
        }),
        ..Record::default()
    };
    let second = geometry_record(Some("d"), Some(-3.0), &[("N", 0.0, 0.0, 0.0)]);
    let records = vec![first, nameless, no_geometry, failed, second];
    let (selected, skipped) = select_xyz_records(&records).expect("unique names");
    let names: Vec<(&str, &str)> = selected
        .iter()
        .map(|(record, target)| (record.name.as_deref().unwrap_or_default(), target.as_str()))
        .collect();
    assert_eq!(names, [("a", "a.xyz"), ("d", "d.xyz")]);
    assert_eq!(skipped, 3);
}

#[test] // rq-4a8815f8
fn select_xyz_records_rejects_equal_names_before_writing() {
    let first = geometry_record(Some("1dma"), Some(-1.0), &[("N", 0.0, 0.0, 0.0)]);
    let second = geometry_record(Some("1dma"), Some(-2.0), &[("N", 0.0, 0.0, 0.0)]);
    let error = select_xyz_records(&[first, second]).expect_err("duplicate target");
    assert_eq!(
        error,
        compiled_jkqc::XyzError::DuplicateName("1dma.xyz".to_string())
    );
    assert_eq!(
        error.to_string(),
        "duplicate xyz file name 1dma.xyz; nothing written"
    );
}

#[test] // rq-26470d18
fn select_xyz_records_rejects_names_differing_only_by_case() {
    let first = geometry_record(Some("Water"), Some(-1.0), &[("O", 0.0, 0.0, 0.0)]);
    let second = geometry_record(Some("water"), Some(-2.0), &[("O", 0.0, 0.0, 0.0)]);
    let error = select_xyz_records(&[first, second]).expect_err("case duplicate target");
    assert_eq!(
        error,
        compiled_jkqc::XyzError::DuplicateName("water.xyz".to_string())
    );
}

#[test] // rq-42664cae
fn write_xyz_files_writes_each_record_and_an_empty_slice_writes_nothing() {
    let dir = temp_dir();
    let first = geometry_record(Some("a"), Some(-1.0), &[("N", 0.0, 0.0, 0.0)]);
    let second = geometry_record(
        Some("1sa-3xstr"),
        Some(-2.0),
        &[("O", 0.1, 0.2, 0.3), ("H", 0.4, 0.5, 0.6)],
    );
    let files = [
        (&first, xyz_file_name(&first).expect("a is eligible")),
        (
            &second,
            xyz_file_name(&second).expect("1sa-3xstr is eligible"),
        ),
    ];
    write_xyz_files(&dir, &files).expect("write files");
    assert_eq!(
        fs::read_to_string(dir.join("a.xyz")).expect("read a.xyz"),
        frame_text(&first)
    );
    assert_eq!(
        fs::read_to_string(dir.join("1sa-3xstr.xyz")).expect("read 1sa-3xstr.xyz"),
        frame_text(&second)
    );

    let empty = temp_dir();
    write_xyz_files(&empty, &[]).expect("empty export");
    assert_eq!(fs::read_dir(&empty).expect("read dir").count(), 0);

    let record = geometry_record(Some("a"), None, &[("N", 0.0, 0.0, 0.0)]);
    let files = [(&record, xyz_file_name(&record).expect("a is eligible"))];
    let error =
        write_xyz_files(&dir.join("no-such-dir"), &files).expect_err("missing parent directory");
    assert!(matches!(error, compiled_jkqc::XyzError::Io(_)), "{error:?}");
}
