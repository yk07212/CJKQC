#!/usr/bin/env python3
"""Benchmark the Rust --arbalign command against ArbAlignCPP --SORT --MPA.

Implements the end-to-end benchmark and hard parity gate of rq-975c0536.

The corpus is a directory of .xyz files of one composition. The script:

1. builds a JSONL database from the corpus for the Rust command;
2. times ArbAlignCPP `--SORT list.txt THRESHOLD --MPA` end-to-end;
3. times the Rust `CJKQC db.jsonl --arbalign THRESHOLD --filename` end-to-end;
4. compares the survivor sets (and reports order equality);
5. reports the Rust sweep-only diagnostic from the ignored `arbalign_bench`
   test, then enforces the speed gate.

Usage:
    utility_scripts/arbalign_bench.py --cpp /path/to/ArbAlignCPP
"""

import argparse
import json
import os
import pathlib
import re
import statistics
import subprocess
import sys
import tempfile
import time


def read_xyz(path):
    lines = path.read_text().splitlines()
    if not lines:
        raise SystemExit(f"{path}: empty xyz")
    count = int(lines[0].strip())
    if len(lines) < count + 2:
        raise SystemExit(f"{path}: incomplete xyz")
    symbols = []
    coordinates = []
    for line in lines[2 : 2 + count]:
        fields = line.split()
        if len(fields) < 4:
            raise SystemExit(f"{path}: malformed atom row: {line!r}")
        symbols.append(fields[0])
        coordinates.extend(float(value) for value in fields[1:4])
    return symbols, coordinates


def write_database(path, corpus):
    with path.open("w") as database:
        for xyz in corpus:
            symbols, coordinates = read_xyz(xyz)
            record = {
                "status": "ok",
                "name": xyz.name,
                "geometry": {"symbols": symbols, "coordinates": coordinates},
            }
            database.write(json.dumps(record, separators=(",", ":")) + "\n")


def timed(command, cwd, runs=1):
    samples = []
    last_stdout = ""
    for _ in range(runs):
        start = time.perf_counter()
        completed = subprocess.run(
            command,
            cwd=cwd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        elapsed = time.perf_counter() - start
        if completed.returncode != 0:
            sys.stderr.write(completed.stdout)
            sys.stderr.write(completed.stderr)
            raise SystemExit(
                f"command failed ({completed.returncode}): {' '.join(command)}"
            )
        samples.append(elapsed)
        last_stdout = completed.stdout
    return statistics.median(samples), samples, last_stdout


def cpp_survivors(results_file):
    return [
        pathlib.Path(line.strip()).name
        for line in results_file.read_text().splitlines()
        if line.strip()
    ]


def rust_survivors(table):
    rows = [line.split() for line in table.splitlines() if line.strip()]
    if not rows or rows[0][0] != "filename":
        raise SystemExit(f"unexpected Rust table:\n{table}")
    return [row[0] for row in rows[1:]]


def rust_sweep(repo, database, threshold, threads, runs):
    env = os.environ.copy()
    env["ARBALIGN_BENCH_JSONL"] = str(database)
    env["ARBALIGN_BENCH_THRESHOLD"] = str(threshold)
    env["ARBALIGN_BENCH_THREADS"] = str(threads)
    env["ARBALIGN_BENCH_RUNS"] = str(runs)
    completed = subprocess.run(
        [
            "cargo",
            "test",
            "--release",
            "--test",
            "arbalign_bench",
            "--",
            "--ignored",
            "--nocapture",
        ],
        cwd=repo,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if completed.returncode != 0:
        sys.stderr.write(completed.stdout)
        sys.stderr.write(completed.stderr)
        raise SystemExit("Rust arbalign sweep diagnostic failed")
    match = re.search(r"ARBALIGN_SWEEP_NS=(\d+)", completed.stdout)
    if not match:
        sys.stderr.write(completed.stdout)
        raise SystemExit("Rust benchmark printed no ARBALIGN_SWEEP_NS line")
    return int(match.group(1))


def print_samples(name, samples):
    formatted = ", ".join(f"{sample * 1e3:.1f} ms" for sample in samples)
    print(f"{name} samples: {formatted}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cpp", required=True, help="ArbAlignCPP binary")
    parser.add_argument(
        "--corpus",
        default="tests/arbalign_fixtures/bench_corpus",
        help="directory of same-composition .xyz files",
    )
    parser.add_argument("--threshold", type=float, default=0.3)
    parser.add_argument("--threads", type=int, default=1)
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument(
        "--no-gate",
        action="store_true",
        help="report timings without failing when Rust is slower",
    )
    args = parser.parse_args()
    if args.threads < 1:
        raise SystemExit("--threads must be at least 1")
    if args.runs < 1:
        raise SystemExit("--runs must be at least 1")

    repo = pathlib.Path(__file__).resolve().parent.parent
    corpus_dir = (repo / args.corpus).resolve()
    corpus = sorted(corpus_dir.glob("*.xyz"))
    if not corpus:
        raise SystemExit(f"no .xyz files in {corpus_dir}")
    cpp = pathlib.Path(args.cpp).resolve()
    if not cpp.is_file():
        raise SystemExit(f"ArbAlignCPP binary not found: {cpp}")

    subprocess.run(
        ["cargo", "build", "--release"],
        cwd=repo,
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    rust_binary = repo / "target" / "release" / "CJKQC"

    with tempfile.TemporaryDirectory(prefix="cjkqc-arbalign-bench-") as temp:
        temp = pathlib.Path(temp)
        list_file = temp / "list.txt"
        list_file.write_text("".join(f"{xyz}\n" for xyz in corpus))
        database = temp / "bench.jsonl"
        write_database(database, corpus)

        cpp_time, cpp_samples, _ = timed(
            [
                str(cpp),
                "--SORT",
                str(list_file),
                str(args.threshold),
                "--MPA",
                "--MI",
                "--THREADS",
                str(args.threads),
            ],
            cwd=temp,
            runs=args.runs,
        )
        cpp_set = cpp_survivors(temp / "results.txt")

        rust_time, rust_samples, rust_table = timed(
            [
                str(rust_binary),
                str(database),
                "--arbalign",
                str(args.threshold),
                "--parallel",
                str(args.threads),
                "--filename",
            ],
            cwd=temp,
            runs=args.runs,
        )
        rust_set = rust_survivors(rust_table)

        rust_sweep_ns = rust_sweep(
            repo, database, args.threshold, args.threads, args.runs
        )

    print(
        f"corpus: {len(corpus)} structures, threshold {args.threshold}, "
        f"threads {args.threads}"
    )
    print_samples("ArbAlignCPP end-to-end", cpp_samples)
    print_samples("Rust end-to-end", rust_samples)
    print(f"ArbAlignCPP median: {cpp_time * 1e3:.1f} ms")
    print(f"Rust median:       {rust_time * 1e3:.1f} ms")
    print(f"Rust sweep-only:   {rust_sweep_ns / 1e6:.3f} ms")
    print(
        f"survivors: cpp {len(cpp_set)}, rust {len(rust_set)}; "
        f"same set: {set(cpp_set) == set(rust_set)}; "
        f"same order: {cpp_set == rust_set}"
    )

    code = 0
    if set(cpp_set) != set(rust_set):
        print("parity: survivor sets differ")
        print("only cpp:", sorted(set(cpp_set) - set(rust_set)))
        print("only rust:", sorted(set(rust_set) - set(cpp_set)))
        code = 1
    else:
        print("parity: survivor sets agree")

    if rust_time <= cpp_time:
        print("speed gate: Rust is not slower")
    else:
        print(f"speed gate: Rust is {rust_time / cpp_time:.2f}x slower")
        if not args.no_gate:
            code = 1
    return code


if __name__ == "__main__":
    sys.exit(main())
