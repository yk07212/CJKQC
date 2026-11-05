//! The movie export behind `--movie` (see `rqm/movie.md`): the run's
//! final record set written as a multi-frame xyz file named `movie.xyz`,
//! one frame per record that is `ok` and carries a geometry, in run
//! order. The frame text is built from the stored geometry and the
//! stored observables only; the file itself is installed with the
//! shared atomic replacement write (`src/atomic.rs`,
//! `rqm/database.md`).
//!
//! (rq-6fb174a0)

use std::fmt;
use std::path::Path;

use crate::record::{Record, RecordStatus};

/// The write failure.
///
/// (rq-e5292dca)
#[derive(Debug, Clone, PartialEq)]
pub enum MovieError {
    /// A filesystem operation failed; the reason is worded like the
    /// database writer's `DbError` (`rqm/database.md`).
    Io(String),
}

impl fmt::Display for MovieError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MovieError::Io(reason) => write!(f, "movie I/O failure: {reason}"),
        }
    }
}

impl std::error::Error for MovieError {}

/// The frame-eligible subset of the given record set: status `ok` with a
/// geometry present, in input order, with the count of skipped records
/// for the stderr note (`no geometry or not ok`).
///
/// (rq-78af10b3)
pub fn movie_frames(records: &[Record]) -> (Vec<&Record>, usize) {
    let mut frames = Vec::new();
    let mut excluded = 0;
    for record in records {
        if record.status == RecordStatus::Ok && record.geometry.is_some() {
            frames.push(record);
        } else {
            excluded += 1;
        }
    }
    (frames, excluded)
}

/// One frame's full text: the atom-count line, the comment line, and the
/// atom lines, each ended by a newline. The comment names the record
/// (`-` when nameless) and, when the record stores an electronic energy,
/// appends `E=` with that energy in hartree with 10 decimals — the `el`
/// column's format (`rqm/filtering.md`). Each atom line is the stored
/// title-case symbol and the three coordinates in Å with 6 decimals.
/// Line breaks in the name are replaced with spaces so the comment stays
/// one line (rqm/movie.md, Frame Format). A record without a geometry,
/// or whose geometry's coordinate count disagrees with its atom count,
/// yields an empty string; `movie_frames` never selects one.
///
/// (rq-4418be27)
pub fn frame_text(record: &Record) -> String {
    let Some(geometry) = &record.geometry else {
        return String::new();
    };
    if geometry.coordinates.len() != 3 * geometry.symbols.len() {
        return String::new();
    }
    let mut text = String::new();
    text.push_str(&geometry.symbols.len().to_string());
    text.push('\n');
    let name = record
        .name
        .as_deref()
        .unwrap_or("-")
        .replace(['\n', '\r'], " ");
    text.push_str(&name);
    if let Some(energy) = record.electronic_energy {
        text.push_str(&format!(" E={energy:.10}"));
    }
    text.push('\n');
    for (index, symbol) in geometry.symbols.iter().enumerate() {
        let x = geometry.coordinates[3 * index];
        let y = geometry.coordinates[3 * index + 1];
        let z = geometry.coordinates[3 * index + 2];
        text.push_str(&format!("{symbol} {x:.6} {y:.6} {z:.6}\n"));
    }
    text
}

/// Writes the frames as the complete new content of `path`, atomically
/// via the shared atomic writer (`src/atomic.rs`): the content lands in
/// a temporary file in the target's parent directory, is flushed and
/// synced, then renamed over the target, so a failed write leaves any
/// previous file intact. An empty slice writes an empty file.
///
/// (rq-e5ec2a43)
pub fn write_movie_xyz(path: &Path, frames: &[&Record]) -> Result<(), MovieError> {
    let mut contents = String::new();
    for frame in frames {
        contents.push_str(&frame_text(frame));
    }
    crate::atomic::write_atomically(path, &contents)
        .map_err(|error| MovieError::Io(format!("{}: {error}", path.display())))
}
