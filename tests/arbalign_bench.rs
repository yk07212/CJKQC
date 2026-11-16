//! Release-only benchmark for the arbalign pairwise sweep
//! (`rqm/arbalign.md` Performance Constraints). The default test run skips
//! it; `utility_scripts/arbalign_bench.py` drives it against ArbAlignCPP.

use std::path::Path;
use std::time::Instant;

use compiled_jkqc::arbalign::{arbalign_mask, prepare_arbalign};
use compiled_jkqc::db::load_database;

#[test] // rq-975c0536
#[ignore = "release-only benchmark; run via utility_scripts/arbalign_bench.py"]
fn arbalign_sweep_benchmark() {
    let path = std::env::var("ARBALIGN_BENCH_JSONL")
        .expect("ARBALIGN_BENCH_JSONL must name the benchmark database");
    let threshold: f64 = std::env::var("ARBALIGN_BENCH_THRESHOLD")
        .unwrap_or_else(|_| "0.2".to_string())
        .parse::<f64>()
        .expect("ARBALIGN_BENCH_THRESHOLD must be a number");
    let threads: usize = std::env::var("ARBALIGN_BENCH_THREADS")
        .unwrap_or_else(|_| "1".to_string())
        .parse::<usize>()
        .expect("ARBALIGN_BENCH_THREADS must be a number");
    let runs: usize = std::env::var("ARBALIGN_BENCH_RUNS")
        .unwrap_or_else(|_| "5".to_string())
        .parse::<usize>()
        .expect("ARBALIGN_BENCH_RUNS must be a number")
        .max(1);

    let database = load_database(Path::new(&path)).expect("load benchmark database");
    let structures: Vec<_> = database.records.iter().map(prepare_arbalign).collect();

    let mut times = Vec::with_capacity(runs);
    for _ in 0..runs {
        let start = Instant::now();
        let (_mask, report) = arbalign_mask(&structures, threshold, threads);
        times.push(start.elapsed());
        assert_eq!(report.skipped, 0, "benchmark corpus must be preparable");
    }
    times.sort();
    let median = times[runs / 2];
    println!(
        "ARBALIGN_SWEEP_NS={} structures={} threads={} threshold={}",
        median.as_nanos(),
        structures.len(),
        threads,
        threshold
    );
}
