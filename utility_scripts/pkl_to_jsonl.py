#!/usr/bin/env python3
"""Convert a JKQC pandas pickle database into a CJKQC .jsonl database.

Requires pandas and ase, because the pickle stores ase.Atoms structures.
Only primary observables are written; thermochemistry is derived by CJKQC.
"""

import argparse
import json
import math
import re
from pathlib import Path

try:
    import pandas as pd
except ImportError as exc:
    raise SystemExit("pandas is required to read the pickle") from exc

try:
    import ase  # noqa: F401
except ImportError as exc:
    raise SystemExit("ase is required to read the stored structures") from exc


def column_map(frame):
    return {
        ".".join(column) if isinstance(column, tuple) else str(column): column
        for column in frame.columns
    }


def cell(row, columns, key):
    column = columns.get(key)
    if column is None:
        return None
    return row[column]


def missing(value):
    if value is None:
        return True
    if isinstance(value, float) and math.isnan(value):
        return True
    try:
        return bool(pd.isna(value))
    except (TypeError, ValueError):
        return False


def as_float(value):
    if missing(value) or isinstance(value, bool):
        return None
    try:
        number = float(value)
    except (TypeError, ValueError):
        return None
    return number if math.isfinite(number) else None


def as_int(value):
    number = as_float(value)
    if number is None:
        return None
    rounded = round(number)
    if abs(number - rounded) > 1e-9:
        return None
    return int(rounded)


def as_float_list(value):
    if missing(value) or isinstance(value, (str, bytes)):
        return None
    try:
        items = list(value)
    except TypeError:
        return None
    numbers = []
    for item in items:
        number = as_float(item)
        if number is None:
            return None
        numbers.append(number)
    return numbers or None


def as_vector3(value):
    numbers = as_float_list(value)
    if numbers is None or len(numbers) != 3:
        return None
    return numbers


def as_geometry(structure):
    if missing(structure):
        return None
    if not hasattr(structure, "get_chemical_symbols") or not hasattr(
        structure, "get_positions"
    ):
        return None
    try:
        symbols = [str(symbol) for symbol in structure.get_chemical_symbols()]
        positions = structure.get_positions()
    except Exception:
        return None
    coordinates = []
    for position in positions:
        vector = as_float_list(position)
        if vector is None or len(vector) != 3:
            return None
        coordinates.extend(vector)
    if not symbols or len(coordinates) != 3 * len(symbols):
        return None
    return {"symbols": symbols, "coordinates": coordinates}


def as_components(codes, counts):
    if missing(codes) or missing(counts):
        return None
    if isinstance(codes, str):
        codes = [codes]
    try:
        code_list = list(codes)
        count_list = list(counts)
    except TypeError:
        return None
    if len(code_list) != len(count_list):
        return None
    components = {}
    for code, count in zip(code_list, count_list):
        if not isinstance(code, str) or re.fullmatch(r"[A-Za-z]+", code) is None:
            return None
        number = as_int(count)
        if number is None or number < 1:
            return None
        components[code] = number
    return dict(sorted(components.items())) if components else None


def program_token(raw):
    if missing(raw):
        return None
    text = str(raw).strip().lower()
    if "gaussian" in text or text.startswith(("g16", "g09", "g03")):
        return "gaussian"
    if "orca" in text:
        return "orca"
    if "xtb" in text:
        return "xtb"
    if "mrcc" in text:
        return "mrcc"
    if text in {"xyz", "ase", "plain xyz"}:
        return "xyz"
    return None


def program_version(raw):
    if missing(raw):
        return None
    text = str(raw).strip()
    if not text:
        return None
    if "_" in text:
        return text.split("_", 1)[1]
    if text.lower() in {"gaussian", "orca", "xtb", "mrcc", "xyz", "ase"}:
        return None
    return text


def clean_command(raw):
    if missing(raw):
        return None
    text = str(raw).strip()
    if not text:
        return None
    if text.startswith("!"):
        text = text[1:].strip()
    text = " ".join(text.replace("_", " ").split())
    return text or None


def status_and_error(row, columns):
    termination = cell(row, columns, "log.termination")
    if missing(termination):
        return "ok", None
    if isinstance(termination, str):
        text = termination.strip().lower()
        if text.startswith(("1", "normal", "success", "converged", "ok")):
            return "ok", None
        return "failed", "source job did not terminate normally: %s" % termination
    number = as_int(termination)
    if number == 1:
        return "ok", None
    return "failed", "source job did not terminate normally: %s" % termination


def convert_row(row, columns):
    basename = cell(row, columns, "info.file_basename")
    folder = cell(row, columns, "info.folder_path")

    name = None
    if not missing(basename):
        name = Path(str(basename)).stem or None

    source_path = None
    if not missing(basename):
        source_path = str(Path(str(folder)) / str(basename)) if not missing(folder) else str(basename)

    status, error = status_and_error(row, columns)

    raw_program = cell(row, columns, "log.program")
    program = program_token(raw_program)
    version = cell(row, columns, "log.program_version")
    if missing(version):
        version = program_version(raw_program)
    else:
        version = str(version)

    command = cell(row, columns, "log.command")
    if missing(command):
        command = cell(row, columns, "log.method")
    command = clean_command(command)

    electronic_energy = as_float(cell(row, columns, "log.electronic_energy"))
    correction_energy = as_float(cell(row, columns, "out.electronic_energy"))
    scf_energy = as_float(cell(row, columns, "log.scf_energy"))
    charge = as_int(cell(row, columns, "log.charge"))
    multiplicity = as_int(cell(row, columns, "log.multiplicity"))
    geometry = as_geometry(cell(row, columns, "xyz.structure"))
    frequencies = as_float_list(cell(row, columns, "log.vibrational_frequencies"))
    gradient = as_float_list(cell(row, columns, "log.gradient"))
    dipole = as_vector3(cell(row, columns, "log.dipole_moments"))
    mulliken = as_float_list(cell(row, columns, "log.mulliken_charges"))
    symmetry = as_int(cell(row, columns, "log.rotational_symmetry_number"))
    components = as_components(
        cell(row, columns, "info.components"),
        cell(row, columns, "info.component_ratio"),
    )

    if symmetry is not None and symmetry < 1:
        symmetry = None
    if multiplicity is not None and multiplicity < 1:
        multiplicity = None

    correction = None
    if correction_energy is not None:
        correction = {"energy": correction_energy, "kind": "sp"}
        correction_program = cell(row, columns, "out.program")
        correction_token = program_token(correction_program)
        if correction_token is not None:
            correction["program"] = correction_token
        correction_version = cell(row, columns, "out.program_version")
        if missing(correction_version):
            correction_version = program_version(correction_program)
        if not missing(correction_version):
            correction["program_version"] = str(correction_version)
        correction_command = cell(row, columns, "out.command")
        if missing(correction_command):
            correction_command = cell(row, columns, "out.method")
        correction_command = clean_command(correction_command)
        if correction_command is not None:
            correction["commands"] = [correction_command]

    record = {"status": status}
    optional = (
        ("error", error),
        ("name", name),
        ("source_path", source_path),
        ("program", program),
        ("program_version", version),
        ("command", command),
        ("electronic_energy", electronic_energy),
        ("scf_energy", scf_energy),
        ("charge", charge),
        ("multiplicity", multiplicity),
        ("geometry", geometry),
        ("vibrational_frequencies", frequencies),
        ("gradient", gradient),
        ("dipole_moment", dipole),
        ("mulliken_charges", mulliken),
        ("rotational_symmetry_number", symmetry),
        ("components", components),
        ("correction", correction),
    )
    for key, value in optional:
        if value is not None:
            record[key] = value
    return record


def load_frame(path):
    obj = pd.read_pickle(path)
    if isinstance(obj, pd.DataFrame):
        return obj
    if isinstance(obj, dict):
        return pd.DataFrame(obj)
    if isinstance(obj, (list, tuple)):
        return pd.DataFrame(list(obj))
    raise SystemExit("the pickle does not contain a pandas DataFrame")


def main():
    parser = argparse.ArgumentParser(
        description="Convert a JKQC .pkl database into a CJKQC .jsonl database."
    )
    parser.add_argument("input", type=Path, help="the .pkl file to read")
    parser.add_argument(
        "-o",
        "--output",
        type=Path,
        help="the .jsonl file to write (default: input name with .jsonl)",
    )
    args = parser.parse_args()

    output = args.output or args.input.with_suffix(".jsonl")
    frame = load_frame(args.input)
    columns = column_map(frame)

    count = 0
    with output.open("w", encoding="utf-8") as handle:
        for _, row in frame.iterrows():
            record = convert_row(row, columns)
            handle.write(json.dumps(record, separators=(",", ":"), allow_nan=False))
            handle.write("\n")
            count += 1

    print(f"wrote {count} records to {output}")


if __name__ == "__main__":
    main()
