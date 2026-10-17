//! Plain xyz geometry files: an atom-count line, a comment line, and one
//! row per atom, with frames concatenated back to back. The parser yields
//! exactly one record for the last complete frame — its geometry in
//! Ångström plus the `Electronic_Energy:` label from that frame's comment
//! when present. Recognition is structural (no banner); the standard
//! registry registers this parser last.
//!
//! (rq-bfac850d)

use crate::parser::extract::{
    parse_fortran_f64, read_geometry_table, LengthUnits, SymbolColumn, TableSpec,
};
use crate::parser::{FileWindows, Parser, Program};
use crate::record::{Geometry, Record, RecordStatus};

/// Plain xyz reader: `program()` is `Program::XYZ`.
///
/// (rq-aa33eaea)
pub struct XyzParser;

/// The positive integer an xyz count line holds, if the line is one.
fn count_line(line: &str) -> Option<usize> {
    let trimmed = line.trim();
    if trimmed.is_empty() || !trimmed.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    trimmed.parse::<usize>().ok().filter(|count| *count >= 1)
}

/// Whether the fields are a well-formed atom row: an all-alphabetic
/// element symbol followed by at least three parseable numeric fields.
fn is_row(fields: &[&str]) -> bool {
    fields.len() >= 4
        && !fields[0].is_empty()
        && fields[0].bytes().all(|byte| byte.is_ascii_alphabetic())
        && fields[1..4]
            .iter()
            .all(|field| parse_fortran_f64(field).is_some())
}

fn is_row_line(line: &str) -> bool {
    is_row(&line.split_whitespace().collect::<Vec<&str>>())
}

/// The geometry of the `count` rows that follow a frame's comment line.
fn frame_geometry(lines: &[&str], count: usize) -> Option<Geometry> {
    let spec = TableSpec {
        symbol_column: SymbolColumn::Symbol(0),
        coordinate_columns: [1, 2, 3],
        units: LengthUnits::Angstrom,
        expected_rows: Some(count),
    };
    read_geometry_table(lines.iter().copied(), &spec)
}

/// The `Electronic_Energy:` label's value in a comment line; every other
/// comment token is opaque.
// rq-e2eda6d5 rq-8692e310 rq-44ba9967
fn comment_energy(comment: &str) -> Option<f64> {
    let tokens: Vec<&str> = comment.split_whitespace().collect();
    let position = tokens.iter().position(|token| {
        token
            .trim_matches(':')
            .eq_ignore_ascii_case("electronic_energy")
    })?;
    tokens
        .get(position + 1)
        .and_then(|value| parse_fortran_f64(value))
}

// rq-aa33eaea
impl Parser for XyzParser {
    fn program(&self) -> Program {
        Program::XYZ
    }

    // rq-535bd7b8
    fn detect(&self, head: &str) -> bool {
        let mut lines = head.lines();
        let Some(first) = lines.next() else {
            return false;
        };
        let Some(_comment) = lines.next() else {
            return false;
        };
        count_line(first).is_some() && lines.next().is_some_and(is_row_line)
    }

    fn parse(&self, windows: &FileWindows) -> Record {
        // The frame scan reads the visible content that ends at the file's
        // end: the tail window (the entire content when covered). A frame
        // whose count line lies in an elided middle is not reconstructed;
        // geometry comes from the last fully visible complete frame.
        let lines: Vec<&str> = windows.tail.lines().collect();
        let mut geometry: Option<Geometry> = None;
        let mut comment: Option<&str> = None;
        let mut status = RecordStatus::Ok;
        let mut error: Option<String> = None;

        let mut index = 0;
        while index < lines.len() {
            let Some(count) = count_line(lines[index]) else {
                index += 1;
                continue;
            };
            let frame_comment = lines.get(index + 1).copied().unwrap_or("");
            let start = index + 2;
            let mut end = start;
            while end < lines.len() && is_row_line(lines[end]) {
                end += 1;
            }
            let found = end - start;

            if found >= count {
                match frame_geometry(&lines[start..start + count], count) {
                    // rq-48bb5800 rq-ebaddd9d
                    Some(frame) => {
                        geometry = Some(frame);
                        comment = Some(frame_comment);
                        index = start + count;
                    }
                    // rq-509b31ed
                    None => {
                        status = RecordStatus::Failed;
                        error = Some(format!(
                            "malformed xyz frame at line {}: a row names no known element",
                            index + 1
                        ));
                        break;
                    }
                }
            } else {
                // rq-3265a4b3
                status = RecordStatus::Truncated;
                error = Some(format!(
                    "incomplete xyz frame at line {}: header declares {count} atoms, only {found} rows follow",
                    index + 1
                ));
                break;
            }
        }

        if geometry.is_none() && matches!(status, RecordStatus::Ok) {
            status = RecordStatus::Truncated;
            error = Some("no complete xyz frame is visible".to_string());
        }

        Record {
            status,
            error,
            electronic_energy: comment.and_then(comment_energy),
            geometry,
            ..Record::default()
        }
    }
}
