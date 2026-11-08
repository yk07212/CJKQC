//! Shared extraction helpers: every per-format parser composes from these
//! rather than duplicating logic — the Fortran-style float parser, the
//! atomic-number-to-symbol map, the columnar geometry-table reader shared
//! by the plain-xyz, Gaussian, ORCA, and MRCC layouts, and the
//! vibrational-mode filter that keeps genuine modes and drops the
//! translational/rotational trivial ones.
//!
//! (rq-d59dac72)

use crate::record::Geometry;

/// One line of a window that carries a literal marker text, located by
/// the SIMD whole-window byte search of the gather rule: a seam fallback
/// that cannot anchor to a located header finds its candidate rows by
/// scanning the marker text and field-splits the candidates only — never
/// every line of the window.
///
/// (rq-15b1763d)
pub struct MarkerLine<'a> {
    /// The candidate line's text, its line-ending bytes withheld.
    pub line: &'a str,
    /// The byte offset at which this line begins within the window.
    pub line_start: usize,
    /// The byte offset at which the next physical line begins: the
    /// adjacency key of the consecutive-row section rules — a candidate
    /// that does not begin exactly where the previous candidate's next
    /// line begins has at least one line before it, so the section a
    /// line walk would have ended at its first non-row line ends here.
    pub next_start: usize,
}

/// Every line of `window` that carries `marker`, in window order, once
/// each: a line holding the marker twice is yielded once. The lines are
/// the ones a per-line walk would have tested, so a gather over the
/// yield — verifying the row shape on each — answers exactly what the
/// walk answered.
///
/// (rq-15b1763d)
pub fn marker_lines<'a>(window: &'a str, marker: &'a str) -> impl Iterator<Item = MarkerLine<'a>> {
    let bytes = window.as_bytes();
    let mut hits = memchr::memmem::find_iter(bytes, marker.as_bytes());
    let mut last_line_start: Option<usize> = None;
    std::iter::from_fn(move || {
        for hit in hits.by_ref() {
            let line_start = match memchr::memrchr(b'\n', &bytes[..hit]) {
                Some(newline) => newline + 1,
                None => 0,
            };
            if last_line_start == Some(line_start) {
                // a second marker hit on a line already yielded
                continue;
            }
            last_line_start = Some(line_start);
            let newline = memchr::memchr(b'\n', &bytes[hit..]).map(|offset| hit + offset);
            let line_end = newline.unwrap_or(bytes.len());
            let next_start = match newline {
                Some(end) => end + 1,
                None => bytes.len(),
            };
            // `str::lines` withholds a trailing carriage return; the
            // located line text matches its output exactly.
            let mut content_end = line_end;
            if content_end > line_start && bytes[content_end - 1] == b'\r' {
                content_end -= 1;
            }
            return Some(MarkerLine {
                line: &window[line_start..content_end],
                line_start,
                next_start,
            });
        }
        None
    })
}

/// The bohr-to-Ångström conversion constant (CODATA 2018), defined once:
/// every Bohr-unit extraction converts through it.
///
/// (rq-0a70b97d)
pub const BOHR_TO_ANGSTROM: f64 = 0.529177210903;

/// The magnitude boundary that recognizes the printed translational and
/// rotational trivial modes of a full Cartesian Hessian: an entry survives
/// `vibrational_modes` exactly when its absolute value exceeds it. Programs
/// print those modes as `0.00` or `-0.00` (magnitude below 0.005), so a cut
/// of 0.01 cm⁻¹ drops them while keeping any genuine soft or imaginary
/// mode. This is a printed-zero recognizer, deliberately distinct from the
/// derivation's 1.0 cm⁻¹ participation test in `src/thermo.rs`: the record
/// archives every mode above this cut and the derivation re-filters at its
/// own boundary.
///
/// (rq-c37111b1)
pub const TRIVIAL_MODE_CUTOFF: f64 = 0.01;

/// Title-case element symbols for atomic numbers 1–118, in order.
///
/// (rq-295538b5 rq-0a70b97d)
const ELEMENT_SYMBOLS: [&str; 118] = [
    "H", "He", "Li", "Be", "B", "C", "N", "O", "F", "Ne", "Na", "Mg", "Al", "Si", "P", "S", "Cl",
    "Ar", "K", "Ca", "Sc", "Ti", "V", "Cr", "Mn", "Fe", "Co", "Ni", "Cu", "Zn", "Ga", "Ge", "As",
    "Se", "Br", "Kr", "Rb", "Sr", "Y", "Zr", "Nb", "Mo", "Tc", "Ru", "Rh", "Pd", "Ag", "Cd", "In",
    "Sn", "Sb", "Te", "I", "Xe", "Cs", "Ba", "La", "Ce", "Pr", "Nd", "Pm", "Sm", "Eu", "Gd", "Tb",
    "Dy", "Ho", "Er", "Tm", "Yb", "Lu", "Hf", "Ta", "W", "Re", "Os", "Ir", "Pt", "Au", "Hg", "Tl",
    "Pb", "Bi", "Po", "At", "Rn", "Fr", "Ra", "Ac", "Th", "Pa", "U", "Np", "Pu", "Am", "Cm", "Bk",
    "Cf", "Es", "Fm", "Md", "No", "Lr", "Rf", "Db", "Sg", "Bh", "Hs", "Mt", "Ds", "Rg", "Cn", "Nh",
    "Fl", "Mc", "Lv", "Ts", "Og",
];

/// Maps an atomic number 1–118 to its title-case element symbol
/// (`1` → `"H"`, `17` → `"Cl"`, `118` → `"Og"`); `None` outside that
/// range.
///
/// (rq-295538b5)
pub fn element_symbol(z: u32) -> Option<String> {
    if z == 0 || z as usize > ELEMENT_SYMBOLS.len() {
        return None;
    }
    Some(ELEMENT_SYMBOLS[(z - 1) as usize].to_string())
}

/// The canonical (title-case) symbol for a case-insensitive element
/// symbol field; `None` when the field names no known element.
///
/// (rq-0a70b97d)
fn element_from_symbol(field: &str) -> Option<&'static str> {
    ELEMENT_SYMBOLS
        .iter()
        .find(|symbol| symbol.eq_ignore_ascii_case(field))
        .copied()
}

/// Parses a decimal number accepting `e`, `E`, `d`, and `D` exponents
/// (Fortran-style output, e.g. `1.2345678D-05`). Returns `None` for
/// non-numeric or non-finite fields.
///
/// (rq-39759c8a)
pub fn parse_fortran_f64(field: &str) -> Option<f64> {
    if !field.contains(['d', 'D']) {
        return field.parse::<f64>().ok().filter(|value| value.is_finite());
    }
    let normalized: String = field
        .chars()
        .map(|character| match character {
            'd' => 'e',
            'D' => 'E',
            other => other,
        })
        .collect();
    normalized
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

/// Which column of a geometry table carries the element, and how it is
/// written: a column of element symbols (case-insensitive, normalized to
/// title case) or a column of atomic numbers.
///
/// (rq-0a70b97d)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolColumn {
    /// A column of element symbols.
    Symbol(usize),
    /// A column of atomic numbers, mapped via `element_symbol`.
    AtomicNumber(usize),
}

/// The length units a geometry table is written in.
///
/// (rq-0a70b97d)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthUnits {
    /// Ångström — the record's canonical unit.
    Angstrom,
    /// Bohr — converted to Ångström with `BOHR_TO_ANGSTROM`.
    Bohr,
}

/// Where the columns of a columnar geometry table live and how to read
/// them.
///
/// (rq-0a70b97d)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableSpec {
    /// The column carrying the element.
    pub symbol_column: SymbolColumn,
    /// The x, y, z column indices.
    pub coordinate_columns: [usize; 3],
    /// The table's length units.
    pub units: LengthUnits,
    /// The exact row count the table must hold, when known.
    pub expected_rows: Option<usize>,
}

/// Reads a columnar geometry block (the xyz-style table shared by the
/// plain-xyz, Gaussian, ORCA, and MRCC layouts) from the lines that
/// follow the block's header, returning the geometry in the record's
/// canonical units.
///
/// A row is the line split on whitespace; it conforms when every
/// referenced column exists, every referenced numeric field parses via
/// `parse_fortran_f64`, and the symbol field is a known element; columns
/// beyond the referenced ones are ignored. The first line that is not a
/// row at all — a missing column or an unparsable coordinate — ends the
/// table. Returns `None` when the table holds zero rows, when
/// `expected_rows` is set and the row count found differs from it, or
/// when the symbol column holds an unknown element: a table cut by a
/// window boundary yields `None`, never a short geometry — the
/// seam-honesty rule.
///
/// (rq-0a70b97d)
pub fn read_geometry_table<'a, I>(lines: I, spec: &TableSpec) -> Option<Geometry>
where
    I: IntoIterator<Item = &'a str>,
{
    let symbol_column = match spec.symbol_column {
        SymbolColumn::Symbol(column) | SymbolColumn::AtomicNumber(column) => column,
    };
    let mut highest_column = symbol_column;
    for &column in &spec.coordinate_columns {
        highest_column = highest_column.max(column);
    }

    let mut symbols: Vec<String> = Vec::new();
    let mut coordinates: Vec<f64> = Vec::new();

    for line in lines {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() <= highest_column {
            // a referenced column is missing: the table ends here
            break;
        }

        let mut row = [0.0; 3];
        let mut numeric = true;
        for (index, &column) in spec.coordinate_columns.iter().enumerate() {
            match parse_fortran_f64(fields[column]) {
                Some(number) => row[index] = number,
                None => {
                    numeric = false;
                    break;
                }
            }
        }
        if !numeric {
            // a coordinate is not a number: the table ends here
            break;
        }

        let symbol = match spec.symbol_column {
            SymbolColumn::Symbol(column) => element_from_symbol(fields[column]).map(String::from),
            SymbolColumn::AtomicNumber(column) => {
                fields[column].parse::<u32>().ok().and_then(element_symbol)
            }
        };
        let Some(symbol) = symbol else {
            // row-shaped but the symbol column names no element: a cut
            // or corrupted table, so the whole read fails
            return None;
        };

        symbols.push(symbol);
        coordinates.extend_from_slice(&row);
    }

    if symbols.is_empty() {
        return None;
    }
    if let Some(expected) = spec.expected_rows {
        if symbols.len() != expected {
            return None;
        }
    }

    if matches!(spec.units, LengthUnits::Bohr) {
        for coordinate in &mut coordinates {
            *coordinate *= BOHR_TO_ANGSTROM;
        }
    }
    Some(Geometry {
        symbols,
        coordinates,
    })
}

/// Keeps the genuine vibrational modes of a parsed frequency list and
/// drops the translational and rotational trivial modes: an entry
/// survives exactly when its absolute value exceeds
/// `TRIVIAL_MODE_CUTOFF`, in input order and with the sign preserved.
/// Programs print the trivial modes as `0.00` or `-0.00`, so they fall to
/// the cut whatever their printed sign or position; a genuine soft or
/// imaginary mode is never dropped, and no ordering assumption is needed —
/// which matters because xtb sorts eigenvalues ascending and prints
/// imaginary modes before the zeros. The Gaussian, ORCA, and XTB parsers
/// pass their parsed frequency lists through it before storing, so the
/// convention holds program-independently.
///
/// (rq-c37111b1)
pub fn vibrational_modes(values: &[f64]) -> Vec<f64> {
    values
        .iter()
        .filter(|value| value.abs() > TRIVIAL_MODE_CUTOFF)
        .copied()
        .collect()
}
