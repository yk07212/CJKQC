//! Ion TOKEN declarations and ACDC display names (see
//! `rqm/formation.md` Ionic References / Ion Tokens).
//!
//! This module is the neutral composition layer: it parses `--ion CODE
//! TOKEN`, validates the declarations, and turns stored component codes
//! into ACDC display names. It depends only on the record's `Components`,
//! so the table printer can use it without importing the formation module.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::record::Components;

/// The polarity an ion token declares: `Added` for the `P` marker, a
/// positive ion, and `Removed` for the `H` marker, a negative ion
/// (`rqm/formation.md` Ion Tokens).
///
/// (rq-06569a42)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProtonKind {
    /// Added protons, the `P` marker.
    Added,
    /// Removed protons, the `H` marker.
    Removed,
}

/// A parsed ACDC ion token: the neutral molecules plus added or removed
/// protons (`rqm/formation.md` Ion Tokens). `neutral` is empty for a bare
/// `1P` or `2H`.
///
/// (rq-81af40f0)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IonToken {
    /// The non-marker composition pairs.
    pub neutral: Components,
    /// The total proton count, always at least 1.
    pub protons: u32,
    /// Whether the protons are added or removed.
    pub kind: ProtonKind,
}

/// One `--ion` occurrence: declare component code `code` as the ion
/// spelled by `token` (`rqm/formation.md` Ion Tokens).
///
/// (rq-fcd56d7e)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IonSpec {
    /// The stored component code the declaration covers.
    pub code: String,
    /// The ACDC composition of the ion.
    pub token: IonToken,
}

/// The run's ion settings in command order (`rqm/formation.md` Ionic
/// References). `Default` is empty.
///
/// (rq-c993d09a)
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IonDeclarations {
    /// The parsed `--ion` specs in command order.
    pub specs: Vec<IonSpec>,
}

impl IonDeclarations {
    /// The first spec of the given polarity: that polarity's reference
    /// ion (`rqm/formation.md` Reference Ions and Values).
    ///
    /// (rq-c993d09a)
    pub fn reference(&self, kind: ProtonKind) -> Option<&IonSpec> {
        self.specs.iter().find(|spec| spec.token.kind == kind)
    }

    /// The specs of one polarity in command order.
    // rq-c993d09a
    pub(crate) fn of_kind(&self, kind: ProtonKind) -> Vec<&IonSpec> {
        self.specs
            .iter()
            .filter(|spec| spec.token.kind == kind)
            .collect()
    }
}

/// Why a TOKEN is invalid (`rqm/formation.md` Ion Tokens).
///
/// (rq-836b1614)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IonTokenError {
    /// The token is empty.
    Empty,
    /// A count is zero or beyond `u32::MAX`, or a count is missing.
    BadCount,
    /// A pair has no letter code.
    NonLetterCode,
    /// The token contains a hyphen and therefore a label.
    LabelNotAllowed,
    /// No `P` or `H` marker appears.
    NoProtonMarker,
    /// Both `P` and `H` markers appear.
    MixedProtonMarkers,
}

impl fmt::Display for IonTokenError {
    // rq-836b1614
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IonTokenError::Empty => write!(f, "the token is empty"),
            IonTokenError::BadCount => write!(f, "a count is missing, zero, or too large"),
            IonTokenError::NonLetterCode => write!(f, "a composition pair has no letter code"),
            IonTokenError::LabelNotAllowed => write!(f, "a token cannot carry a hyphenated label"),
            IonTokenError::NoProtonMarker => write!(f, "a token needs a P or H proton marker"),
            IonTokenError::MixedProtonMarkers => {
                write!(f, "a token cannot mix the P and H proton markers")
            }
        }
    }
}

impl std::error::Error for IonTokenError {}

/// Why a declaration set is invalid (`rqm/formation.md` Ion Tokens).
///
/// (rq-6731c06b)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IonDeclarationError {
    /// The same CODE is declared twice.
    DuplicateCode {
        /// The repeated component code.
        code: String,
    },
    /// A token's neutral part names another declared ion code.
    NeutralCodeIsIon {
        /// The neutral code that is also declared as an ion.
        code: String,
    },
}

impl fmt::Display for IonDeclarationError {
    // rq-6731c06b
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IonDeclarationError::DuplicateCode { code } => {
                write!(f, "--ion names the code `{code}` twice")
            }
            IonDeclarationError::NeutralCodeIsIon { code } => {
                write!(
                    f,
                    "--ion token's neutral code `{code}` is itself a declared ion"
                )
            }
        }
    }
}

impl std::error::Error for IonDeclarationError {}

/// Parses an ACDC ion TOKEN (`rqm/formation.md` Ion Tokens): count-code
/// pairs with exactly one proton marker kind, `P` (added) or `H`
/// (removed), and no label. Repeated neutral codes and repeated marker
/// pairs sum. A bare `1P` or `2H` is valid and has no neutral molecules.
///
/// (rq-0fd47d97)
pub fn parse_acdc_token(text: &str) -> Result<IonToken, IonTokenError> {
    if text.is_empty() {
        return Err(IonTokenError::Empty);
    }
    if text.contains('-') {
        return Err(IonTokenError::LabelNotAllowed);
    }
    let mut neutral = Components::new();
    let mut kind: Option<ProtonKind> = None;
    let mut protons = 0u32;
    let bytes = text.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let count_start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        if cursor == count_start {
            return Err(IonTokenError::BadCount);
        }
        let count: u32 = text[count_start..cursor]
            .parse()
            .map_err(|_| IonTokenError::BadCount)?;
        if count == 0 {
            return Err(IonTokenError::BadCount);
        }
        let code_start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_alphabetic() {
            cursor += 1;
        }
        if cursor == code_start {
            return Err(IonTokenError::NonLetterCode);
        }
        let code = &text[code_start..cursor];
        match code {
            "P" | "H" => {
                let marker_kind = if code == "P" {
                    ProtonKind::Added
                } else {
                    ProtonKind::Removed
                };
                if kind.is_some_and(|kind| kind != marker_kind) {
                    return Err(IonTokenError::MixedProtonMarkers);
                }
                kind = Some(marker_kind);
                protons = protons.checked_add(count).ok_or(IonTokenError::BadCount)?;
            }
            _ => {
                let slot = neutral.entry(code.to_string()).or_insert(0);
                *slot = slot.checked_add(count).ok_or(IonTokenError::BadCount)?;
            }
        }
    }
    let Some(kind) = kind else {
        return Err(IonTokenError::NoProtonMarker);
    };
    Ok(IonToken {
        neutral,
        protons,
        kind,
    })
}

/// The declaration checks of Ionic References: each CODE is unique and no
/// TOKEN's neutral part names another declared ion code.
///
/// (rq-6873e99b)
pub fn validate_ion_declarations(ions: &IonDeclarations) -> Result<(), IonDeclarationError> {
    let mut codes = BTreeSet::new();
    for spec in &ions.specs {
        if !codes.insert(spec.code.as_str()) {
            return Err(IonDeclarationError::DuplicateCode {
                code: spec.code.clone(),
            });
        }
    }
    for spec in &ions.specs {
        for code in spec.token.neutral.keys() {
            if codes.contains(code.as_str()) {
                return Err(IonDeclarationError::NeutralCodeIsIon { code: code.clone() });
            }
        }
    }
    Ok(())
}

/// The ACDC display token of ACDC Names: each declared ion code is replaced
/// by its TOKEN times the code's count, neutral pairs print alphabetically,
/// then the removed-proton total as `{n}H` and the added-proton total as
/// `{n}P`. `None` for absent or empty components. With empty declarations
/// it prints the same pairs as the ordinary composition token.
///
/// (rq-c19ab727)
pub fn acdc_composition_token(
    components: Option<&Components>,
    ions: &IonDeclarations,
) -> Option<String> {
    let components = components?;
    if components.is_empty() {
        return None;
    }
    let mut neutral: BTreeMap<String, u128> = components
        .iter()
        .map(|(code, count)| (code.clone(), u128::from(*count)))
        .collect();
    let mut removed = 0u128;
    let mut added = 0u128;
    for spec in &ions.specs {
        let Some(count) = neutral.remove(&spec.code) else {
            continue;
        };
        for (code, token_count) in &spec.token.neutral {
            *neutral.entry(code.clone()).or_insert(0) += count * u128::from(*token_count);
        }
        let protons = count * u128::from(spec.token.protons);
        match spec.token.kind {
            ProtonKind::Added => added += protons,
            ProtonKind::Removed => removed += protons,
        }
    }
    let mut token = String::new();
    for (code, count) in neutral {
        token.push_str(&format!("{count}{code}"));
    }
    if removed > 0 {
        token.push_str(&format!("{removed}H"));
    }
    if added > 0 {
        token.push_str(&format!("{added}P"));
    }
    Some(token)
}
