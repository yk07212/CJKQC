//! Composition resolution from JKQC names (see `rqm/composition.md`).
//!
//! A record's `name` may follow the JKQC cluster-naming convention:
//! `composition ("-" label)?` where the composition is one or more
//! `count code` pairs — ASCII digit counts of value at least 1 and ASCII
//! letter codes matched case-sensitively (`Am` and `am` are distinct). The
//! label after the first hyphen is never parsed and never stored. Resolution
//! fills `Record::components` from the name unless the record already
//! carries a composition.
//!
//! (rq-1308259f rq-8e661b0d)

use std::fmt;

use crate::record::{Components, Record};

/// One `--rename` occurrence: rename component code `old` to `new`.
///
/// (rq-d9d834d3)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameSpec {
    /// The component code whose count moves.
    pub old: String,
    /// The component code that receives the count.
    pub new: String,
}

/// A rename that cannot be applied.
///
/// (rq-3eb85fbe)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameError {
    /// Merging the renamed count into `code` would exceed `u32::MAX`.
    CountOverflow {
        /// The code whose merged count is unrepresentable.
        code: String,
    },
}

impl fmt::Display for RenameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenameError::CountOverflow { code } => {
                write!(
                    f,
                    "renaming components would overflow the count for `{code}`"
                )
            }
        }
    }
}

impl std::error::Error for RenameError {}

/// Parses a JKQC-style name into its composition, or `None` when the name
/// does not match the grammar. Never panics: `None` is a legitimate answer
/// for every input. Repeated codes sum.
///
/// (rq-eabf1669)
pub fn parse_jkqc_name(name: &str) -> Option<Components> {
    // The label is everything after the first hyphen: dropped whole.
    let head = name.split('-').next()?;
    let bytes = head.as_bytes();
    let mut components = Components::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let count_start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        if cursor == count_start {
            return None;
        }
        let count: u32 = head[count_start..cursor].parse().ok()?;
        if count < 1 {
            return None;
        }
        let code_start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_alphabetic() {
            cursor += 1;
        }
        if cursor == code_start {
            return None;
        }
        let code = &head[code_start..cursor];
        let slot = components.entry(code.to_string()).or_insert(0);
        *slot = slot.checked_add(count)?;
    }
    if components.is_empty() {
        return None;
    }
    Some(components)
}

/// Fills `record.components` from `record.name` when the record carries no
/// composition, the name is present, and the name resolves. A composition
/// already on the record is never replaced or merged; runs on records of any
/// status.
///
/// (rq-230f406d)
pub fn resolve_components(record: &mut Record) {
    if record.components.is_some() {
        return;
    }
    if let Some(name) = &record.name {
        record.components = parse_jkqc_name(name);
    }
}

/// Renames `old` to `new` in one composition map (`rqm/composition.md`,
/// Renaming Component Codes). An absent `old` and `old == new` leave the map
/// unchanged; otherwise `old`'s count is checked-added to `new`'s count.
///
/// (rq-67de9cfd rq-958da001)
pub fn rename_component(
    components: &mut Components,
    old: &str,
    new: &str,
) -> Result<(), RenameError> {
    if old == new {
        return Ok(());
    }
    let Some(count) = components.remove(old) else {
        return Ok(());
    };
    match components.get_mut(new) {
        Some(slot) => match slot.checked_add(count) {
            Some(total) => {
                *slot = total;
                Ok(())
            }
            None => {
                // Put the original entry back: a refused rename changes
                // nothing.
                components.insert(old.to_string(), count);
                Err(RenameError::CountOverflow {
                    code: new.to_string(),
                })
            }
        },
        None => {
            components.insert(new.to_string(), count);
            Ok(())
        }
    }
}
