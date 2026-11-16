//! ArbAlign structure-alignment filtering (`rqm/arbalign.md`).
//!
//! Records are prepared once by moving their geometry to the principal
//! axes of the inertia tensor, then comparable pairs are compared with the
//! ArbAlign search: 48 axis operations, per-element linear assignment, and
//! Kabsch RMSD. The module keeps its own 3x3 symmetric eigensolver, 3x3
//! SVD, and assignment solver so the crate gains no linear-algebra or
//! assignment dependency.

// The small fixed-size matrix loops below read more clearly with explicit
// row/column indices than with iterator gymnastics.
#![allow(clippy::needless_range_loop)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Barrier;
use std::thread;

use crate::metrics::atomic_mass;
use crate::record::{Components, Record};

/// One record's principal-axis alignment form, built once per run
/// (`rqm/arbalign.md`).
///
/// (rq-7330b65b)
#[derive(Debug, Clone, PartialEq)]
pub struct ArbAlignStructure {
    /// Element symbols in the geometry's original atom order.
    pub symbols: Vec<String>,
    /// Principal-axis coordinates in the geometry's original atom order.
    pub coordinates: Vec<[f64; 3]>,
    /// Unweighted geometric centroid of the principal-axis coordinates.
    pub center: [f64; 3],
    sorted_symbols: Vec<String>,
    sorted_coordinates: Vec<[f64; 3]>,
    groups: Vec<Group>,
    radius_of_gyration: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct Group {
    start: usize,
    len: usize,
}

/// The arbalign stage's diagnostics.
///
/// (rq-76495264)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ArbAlignReport {
    /// Records left untouched because they lack usable geometry.
    pub skipped: usize,
}

/// Parses the `--arbalign` threshold: finite and non-negative, in Å.
///
/// (rq-4ddb030f)
pub fn parse_arbalign_threshold(value: &str) -> Result<f64, String> {
    let threshold = value
        .parse::<f64>()
        .map_err(|_| format!("`{value}` is not a number"))?;
    if !threshold.is_finite() {
        return Err(format!("`{value}` is not a finite RMSD threshold"));
    }
    if threshold < 0.0 {
        return Err(format!(
            "`{value}` is negative; the RMSD threshold must be at least 0"
        ));
    }
    Ok(threshold)
}

/// Principal-axis preprocessing and grouping; `None` without usable
/// geometry (`rqm/arbalign.md` Principal-Axis Preprocessing).
///
/// (rq-a637fc31)
pub fn prepare_arbalign(record: &Record) -> Option<ArbAlignStructure> {
    let geometry = record.geometry.as_ref()?;
    let atoms = geometry.symbols.len();
    if atoms == 0 || geometry.coordinates.len() != 3 * atoms {
        return None;
    }
    if geometry.coordinates.iter().any(|value| !value.is_finite()) {
        return None;
    }
    let mut masses = Vec::with_capacity(atoms);
    for symbol in &geometry.symbols {
        masses.push(atomic_mass(symbol)?);
    }

    let coordinates = to_principal_axes(&geometry.coordinates, &masses);
    let center = centroid(&coordinates);
    let mut sum_squared = 0.0;
    for point in &coordinates {
        for axis in 0..3 {
            let delta = point[axis] - center[axis];
            sum_squared += delta * delta;
        }
    }
    let radius_of_gyration = (sum_squared / atoms as f64).sqrt();

    let mut order: Vec<usize> = (0..atoms).collect();
    order.sort_by(|&left, &right| {
        geometry.symbols[left]
            .cmp(&geometry.symbols[right])
            .then_with(|| coordinates[left][0].total_cmp(&coordinates[right][0]))
            .then_with(|| coordinates[left][1].total_cmp(&coordinates[right][1]))
            .then_with(|| coordinates[left][2].total_cmp(&coordinates[right][2]))
    });

    let mut sorted_symbols = Vec::with_capacity(atoms);
    let mut sorted_coordinates = Vec::with_capacity(atoms);
    for &index in &order {
        sorted_symbols.push(geometry.symbols[index].clone());
        sorted_coordinates.push(coordinates[index]);
    }
    let mut groups = Vec::new();
    let mut start = 0;
    for index in 1..=atoms {
        if index == atoms || sorted_symbols[index] != sorted_symbols[start] {
            groups.push(Group {
                start,
                len: index - start,
            });
            start = index;
        }
    }

    Some(ArbAlignStructure {
        symbols: geometry.symbols.clone(),
        coordinates,
        sorted_symbols,
        sorted_coordinates,
        groups,
        center,
        radius_of_gyration,
    })
}

/// The ArbAlign RMSD in Å for a comparable pair; `None` when atom counts
/// or element multisets differ (`rqm/arbalign.md` Pairwise RMSD).
///
/// (rq-bb73fa3f)
pub fn arbalign_rmsd(a: &ArbAlignStructure, b: &ArbAlignStructure) -> Option<f64> {
    let mut scratch = Scratch::default();
    rmsd_with_scratch(a, b, &mut scratch)
}

/// Pairwise elimination over one composition group in the given order.
///
/// (rq-8b4b0278)
pub fn arbalign_mask(
    structures: &[Option<ArbAlignStructure>],
    threshold: f64,
    parallel: usize,
) -> (Vec<bool>, ArbAlignReport) {
    let len = structures.len();
    let skipped = structures
        .iter()
        .filter(|structure| structure.is_none())
        .count();
    let report = ArbAlignReport { skipped };

    // Strict `< 0` never holds, so a zero threshold removes nothing.
    if threshold <= 0.0 || len < 2 {
        return (vec![true; len], report);
    }

    let workers = parallel.max(1).min(len);
    if workers <= 1 {
        return (sequential_sweep(structures, threshold), report);
    }
    (pooled_sweep(structures, threshold, workers), report)
}

fn sequential_sweep(structures: &[Option<ArbAlignStructure>], threshold: f64) -> Vec<bool> {
    let mut alive = vec![true; structures.len()];
    let mut scratch = Scratch::default();
    for i in 0..structures.len() {
        if !alive[i] {
            continue;
        }
        let Some(a) = structures[i].as_ref() else {
            continue;
        };
        for j in i + 1..structures.len() {
            if !alive[j] {
                continue;
            }
            let Some(b) = structures[j].as_ref() else {
                continue;
            };
            if should_eliminate(a, b, threshold, &mut scratch) {
                alive[j] = false;
            }
        }
    }
    alive
}

/// A persistent pool: the workers are spawned once per composition group and
/// reused across every survivor sweep (`rqm/arbalign.md` Parallelism).
fn pooled_sweep(
    structures: &[Option<ArbAlignStructure>],
    threshold: f64,
    workers: usize,
) -> Vec<bool> {
    let len = structures.len();
    let alive: Vec<AtomicBool> = (0..len).map(|_| AtomicBool::new(true)).collect();
    let eliminated: Vec<AtomicBool> = (0..len).map(|_| AtomicBool::new(false)).collect();
    let barrier = Barrier::new(workers + 1);
    let done = AtomicBool::new(false);
    let current = AtomicUsize::new(0);
    let next = AtomicUsize::new(0);
    let mut scratches: Vec<Scratch> = (0..workers).map(|_| Scratch::default()).collect();

    thread::scope(|scope| {
        for scratch in &mut scratches {
            let alive = &alive;
            let eliminated = &eliminated;
            let barrier = &barrier;
            let done = &done;
            let current = &current;
            let next = &next;
            scope.spawn(move || loop {
                barrier.wait();
                if done.load(Ordering::SeqCst) {
                    break;
                }
                let i = current.load(Ordering::SeqCst);
                let a = structures[i]
                    .as_ref()
                    .expect("the coordinator starts only preparable sweeps");
                loop {
                    let j = next.fetch_add(1, Ordering::SeqCst);
                    if j >= len {
                        break;
                    }
                    if !alive[j].load(Ordering::SeqCst) {
                        continue;
                    }
                    let Some(b) = structures[j].as_ref() else {
                        continue;
                    };
                    if should_eliminate(a, b, threshold, scratch) {
                        eliminated[j].store(true, Ordering::SeqCst);
                    }
                }
                barrier.wait();
            });
        }

        for i in 0..len {
            if !alive[i].load(Ordering::SeqCst) || structures[i].is_none() {
                continue;
            }
            if i + 1 >= len {
                break;
            }
            for flag in &eliminated[i + 1..] {
                flag.store(false, Ordering::SeqCst);
            }
            current.store(i, Ordering::SeqCst);
            next.store(i + 1, Ordering::SeqCst);
            barrier.wait();
            barrier.wait();
            for j in i + 1..len {
                if eliminated[j].load(Ordering::SeqCst) {
                    alive[j].store(false, Ordering::SeqCst);
                }
            }
        }

        done.store(true, Ordering::SeqCst);
        barrier.wait();
    });

    alive.into_iter().map(|flag| flag.into_inner()).collect()
}

fn should_eliminate(
    a: &ArbAlignStructure,
    b: &ArbAlignStructure,
    threshold: f64,
    scratch: &mut Scratch,
) -> bool {
    if (a.radius_of_gyration - b.radius_of_gyration).abs() >= threshold {
        return false;
    }
    rmsd_with_scratch(a, b, scratch).is_some_and(|rmsd| rmsd < threshold)
}

/// Groups records by exact `components`, runs the elimination per group,
/// and retains survivors in their original relative order
/// (`rqm/arbalign.md` Grouping and Comparability).
///
/// (rq-1967b16e)
pub fn apply_arbalign(
    records: &mut Vec<Record>,
    threshold: f64,
    parallel: usize,
) -> ArbAlignReport {
    let mut ids: HashMap<Option<Components>, usize> = HashMap::new();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (index, record) in records.iter().enumerate() {
        let key = record.components.clone();
        let id = match ids.get(&key) {
            Some(&id) => id,
            None => {
                let id = groups.len();
                ids.insert(key, id);
                groups.push(Vec::new());
                id
            }
        };
        groups[id].push(index);
    }

    let mut keep = vec![false; records.len()];
    let mut report = ArbAlignReport::default();
    for members in groups {
        let structures: Vec<Option<ArbAlignStructure>> = members
            .iter()
            .map(|&index| prepare_arbalign(&records[index]))
            .collect();
        let (mask, group_report) = arbalign_mask(&structures, threshold, parallel);
        report.skipped += group_report.skipped;
        for (position, &index) in members.iter().enumerate() {
            keep[index] = mask[position];
        }
    }
    let mut flags = keep.into_iter();
    records.retain(|_| flags.next().unwrap_or(false));
    report
}

const PERMUTATIONS: [[usize; 3]; 6] = [
    [0, 1, 2],
    [0, 2, 1],
    [1, 0, 2],
    [1, 2, 0],
    [2, 0, 1],
    [2, 1, 0],
];

const SIGNS: [[f64; 3]; 8] = [
    [1.0, 1.0, 1.0],
    [1.0, 1.0, -1.0],
    [1.0, -1.0, 1.0],
    [1.0, -1.0, -1.0],
    [-1.0, 1.0, 1.0],
    [-1.0, 1.0, -1.0],
    [-1.0, -1.0, 1.0],
    [-1.0, -1.0, -1.0],
];

/// Reusable per-worker buffers; the inner loops never allocate.
#[derive(Default)]
struct Scratch {
    a_first: Vec<[f64; 3]>,
    b_first: Vec<[f64; 3]>,
    b_first_t: Vec<[f64; 3]>,
    b_perm: Vec<[f64; 3]>,
    b_trans: Vec<[f64; 3]>,
    group_old: Vec<[f64; 3]>,
    dist: Vec<f64>,
    assignment: Vec<usize>,
    lap: LapScratch,
}

impl Scratch {
    fn ensure(&mut self, atoms: usize, max_group: usize) {
        if self.b_perm.len() < atoms {
            self.b_perm.resize(atoms, [0.0; 3]);
            self.b_trans.resize(atoms, [0.0; 3]);
        }
        if self.a_first.len() < max_group {
            self.a_first.resize(max_group, [0.0; 3]);
            self.b_first.resize(max_group, [0.0; 3]);
            self.b_first_t.resize(max_group, [0.0; 3]);
            self.group_old.resize(max_group, [0.0; 3]);
            self.assignment.resize(max_group, 0);
        }
        let matrix = max_group.saturating_mul(max_group);
        if self.dist.len() < matrix {
            self.dist.resize(matrix, 0.0);
        }
        self.lap.ensure(max_group);
    }
}

#[derive(Default)]
struct LapScratch {
    u: Vec<f64>,
    v: Vec<f64>,
    p: Vec<usize>,
    way: Vec<usize>,
    minv: Vec<f64>,
    used: Vec<bool>,
}

impl LapScratch {
    fn ensure(&mut self, size: usize) {
        let needed = size + 1;
        if self.u.len() < needed {
            self.u.resize(needed, 0.0);
            self.v.resize(needed, 0.0);
            self.p.resize(needed, 0);
            self.way.resize(needed, 0);
            self.minv.resize(needed, 0.0);
            self.used.resize(needed, false);
        }
    }
}

fn rmsd_with_scratch(
    a: &ArbAlignStructure,
    b: &ArbAlignStructure,
    scratch: &mut Scratch,
) -> Option<f64> {
    let atoms = a.coordinates.len();
    if atoms == 0 || atoms != b.coordinates.len() || a.sorted_symbols != b.sorted_symbols {
        return None;
    }

    let initial = kabsch_rmsd(&a.coordinates, &b.coordinates);
    if initial < 0.001 {
        return Some(initial);
    }

    let max_group = a.groups.iter().map(|group| group.len).max().unwrap_or(1);
    scratch.ensure(atoms, max_group);

    let first = &a.groups[0];
    let first_len = first.len;
    let mut mean_a = [0.0; 3];
    let mut mean_b = [0.0; 3];
    for i in 0..first_len {
        for axis in 0..3 {
            mean_a[axis] += a.sorted_coordinates[first.start + i][axis];
            mean_b[axis] += b.sorted_coordinates[first.start + i][axis];
        }
    }
    for axis in 0..3 {
        mean_a[axis] /= first_len as f64;
        mean_b[axis] /= first_len as f64;
    }
    for i in 0..first_len {
        for axis in 0..3 {
            scratch.a_first[i][axis] = a.sorted_coordinates[first.start + i][axis] - mean_a[axis];
            scratch.b_first[i][axis] = b.sorted_coordinates[first.start + i][axis] - mean_b[axis];
        }
    }

    let mut best = f64::INFINITY;
    for permutation in PERMUTATIONS {
        for signs in SIGNS {
            for i in 0..first_len {
                scratch.b_first_t[i] = apply_operation(scratch.b_first[i], permutation, signs);
            }
            let first_len_squared = first_len * first_len;
            for i in 0..first_len {
                for j in 0..first_len {
                    scratch.dist[i * first_len + j] =
                        distance(scratch.a_first[i], scratch.b_first_t[j]);
                }
            }
            solve_lap(
                &scratch.dist[..first_len_squared],
                first_len,
                &mut scratch.assignment[..first_len],
                &mut scratch.lap,
            );

            scratch.b_perm[..atoms].copy_from_slice(&b.sorted_coordinates);
            for i in 0..first_len {
                scratch.b_perm[first.start + i] =
                    b.sorted_coordinates[first.start + scratch.assignment[i]];
            }
            for i in 0..atoms {
                scratch.b_trans[i] = apply_operation(scratch.b_perm[i], permutation, signs);
            }

            for group in &a.groups {
                let len = group.len;
                let len_squared = len * len;
                for i in 0..len {
                    for j in 0..len {
                        scratch.dist[i * len + j] = distance(
                            a.sorted_coordinates[group.start + i],
                            scratch.b_trans[group.start + j],
                        );
                    }
                }
                solve_lap(
                    &scratch.dist[..len_squared],
                    len,
                    &mut scratch.assignment[..len],
                    &mut scratch.lap,
                );
                scratch.group_old[..len]
                    .copy_from_slice(&scratch.b_trans[group.start..group.start + len]);
                for i in 0..len {
                    scratch.b_trans[group.start + i] = scratch.group_old[scratch.assignment[i]];
                }
                let rmsd = kabsch_rmsd(&a.sorted_coordinates, &scratch.b_trans[..atoms]);
                if rmsd < best {
                    best = rmsd;
                }
            }
        }
    }

    if initial < best {
        Some(initial)
    } else {
        Some(best)
    }
}

fn apply_operation(point: [f64; 3], permutation: [usize; 3], signs: [f64; 3]) -> [f64; 3] {
    [
        signs[0] * point[permutation[0]],
        signs[1] * point[permutation[1]],
        signs[2] * point[permutation[2]],
    ]
}

fn distance(left: [f64; 3], right: [f64; 3]) -> f64 {
    let dx = left[0] - right[0];
    let dy = left[1] - right[1];
    let dz = left[2] - right[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// Minimum-total-distance perfect matching for a square matrix; exact and
/// deterministic. `assignment[row] = column` (`rqm/arbalign.md`).
fn solve_lap(cost: &[f64], size: usize, assignment: &mut [usize], scratch: &mut LapScratch) {
    if size == 0 {
        return;
    }
    if size == 1 {
        assignment[0] = 0;
        return;
    }
    scratch.ensure(size);
    let u = &mut scratch.u[..=size];
    let v = &mut scratch.v[..=size];
    let p = &mut scratch.p[..=size];
    let way = &mut scratch.way[..=size];
    let minv = &mut scratch.minv[..=size];
    let used = &mut scratch.used[..=size];
    u.fill(0.0);
    v.fill(0.0);
    p.fill(0);
    way.fill(0);
    minv.fill(0.0);
    used.fill(false);

    for row in 1..=size {
        p[0] = row;
        let mut column = 0usize;
        minv.fill(f64::INFINITY);
        used.fill(false);
        loop {
            used[column] = true;
            let current = p[column];
            let mut delta = f64::INFINITY;
            let mut next = 0usize;
            for j in 1..=size {
                if used[j] {
                    continue;
                }
                let cur = cost[(current - 1) * size + (j - 1)] - u[current] - v[j];
                if cur < minv[j] {
                    minv[j] = cur;
                    way[j] = column;
                }
                if minv[j] < delta {
                    delta = minv[j];
                    next = j;
                }
            }
            for j in 0..=size {
                if used[j] {
                    u[p[j]] += delta;
                    v[j] -= delta;
                } else {
                    minv[j] -= delta;
                }
            }
            column = next;
            if p[column] == 0 {
                break;
            }
        }
        loop {
            let previous = way[column];
            p[column] = p[previous];
            column = previous;
            if column == 0 {
                break;
            }
        }
    }

    for column in 1..=size {
        if p[column] != 0 {
            assignment[p[column] - 1] = column - 1;
        }
    }
}

/// Kabsch RMSD with unweighted centroid centering and ArbAlignCPP's
/// determinant correction (`rqm/arbalign.md` Pairwise RMSD).
fn kabsch_rmsd(left: &[[f64; 3]], right: &[[f64; 3]]) -> f64 {
    let atoms = left.len();
    if atoms == 0 {
        return 0.0;
    }
    let left_center = centroid(left);
    let right_center = centroid(right);

    let mut covariance = [[0.0; 3]; 3];
    for index in 0..atoms {
        for row in 0..3 {
            let left_value = left[index][row] - left_center[row];
            for column in 0..3 {
                covariance[row][column] +=
                    left_value * (right[index][column] - right_center[column]);
            }
        }
    }

    let (mut u, v) = svd3(covariance);
    let determinant = determinant3(&v) * determinant3(&u);
    if determinant < 0.0 {
        for row in 0..3 {
            u[row][2] = -u[row][2];
        }
    }
    let mut rotation = [[0.0; 3]; 3];
    for row in 0..3 {
        for column in 0..3 {
            let mut sum = 0.0;
            for k in 0..3 {
                sum += u[row][k] * v[column][k];
            }
            rotation[row][column] = sum;
        }
    }

    let mut sum_squared = 0.0;
    for index in 0..atoms {
        let mut delta = [0.0; 3];
        for axis in 0..3 {
            delta[axis] = left[index][axis] - left_center[axis];
        }
        for column in 0..3 {
            let rotated = delta[0] * rotation[0][column]
                + delta[1] * rotation[1][column]
                + delta[2] * rotation[2][column];
            let difference = rotated - (right[index][column] - right_center[column]);
            sum_squared += difference * difference;
        }
    }
    (sum_squared / atoms as f64).sqrt()
}

fn centroid(points: &[[f64; 3]]) -> [f64; 3] {
    let mut center = [0.0; 3];
    for point in points {
        for axis in 0..3 {
            center[axis] += point[axis];
        }
    }
    if !points.is_empty() {
        let count = points.len() as f64;
        for value in &mut center {
            *value /= count;
        }
    }
    center
}

fn to_principal_axes(coordinates: &[f64], masses: &[f64]) -> Vec<[f64; 3]> {
    let atoms = masses.len();
    let total_mass: f64 = masses.iter().sum();
    let mut center_of_mass = [0.0; 3];
    for atom in 0..atoms {
        for axis in 0..3 {
            center_of_mass[axis] += masses[atom] * coordinates[3 * atom + axis];
        }
    }
    for value in &mut center_of_mass {
        *value /= total_mass;
    }

    let mut inertia = [[0.0; 3]; 3];
    for atom in 0..atoms {
        let mass = masses[atom];
        let x = coordinates[3 * atom] - center_of_mass[0];
        let y = coordinates[3 * atom + 1] - center_of_mass[1];
        let z = coordinates[3 * atom + 2] - center_of_mass[2];
        inertia[0][0] += mass * (y * y + z * z);
        inertia[1][1] += mass * (x * x + z * z);
        inertia[2][2] += mass * (x * x + y * y);
        inertia[0][1] -= mass * x * y;
        inertia[0][2] -= mass * x * z;
        inertia[1][2] -= mass * y * z;
    }
    inertia[1][0] = inertia[0][1];
    inertia[2][0] = inertia[0][2];
    inertia[2][1] = inertia[1][2];

    let (_, mut eigenvectors) = jacobi_eigen(inertia);
    fix_eigenvector_signs(&mut eigenvectors);

    let mut rotated = Vec::with_capacity(atoms);
    for atom in 0..atoms {
        let centered = [
            coordinates[3 * atom] - center_of_mass[0],
            coordinates[3 * atom + 1] - center_of_mass[1],
            coordinates[3 * atom + 2] - center_of_mass[2],
        ];
        let mut point = [0.0; 3];
        for axis in 0..3 {
            point[axis] = centered[0] * eigenvectors[0][axis]
                + centered[1] * eigenvectors[1][axis]
                + centered[2] * eigenvectors[2][axis];
        }
        rotated.push(point);
    }
    rotated
}

fn fix_eigenvector_signs<const N: usize>(eigenvectors: &mut [[f64; N]; N]) {
    for column in 0..N {
        let mut pivot = 0;
        let mut largest = 0.0;
        for row in 0..N {
            let magnitude = eigenvectors[row][column].abs();
            if magnitude > largest {
                largest = magnitude;
                pivot = row;
            }
        }
        if eigenvectors[pivot][column] < 0.0 {
            for row in 0..N {
                eigenvectors[row][column] = -eigenvectors[row][column];
            }
        }
    }
}

/// SVD of a 3x3 matrix through the symmetric eigenproblem of `HᵀH`, with
/// an orthonormal completion for rank-deficient matrices.
fn svd3(h: [[f64; 3]; 3]) -> ([[f64; 3]; 3], [[f64; 3]; 3]) {
    let mut gram = [[0.0; 3]; 3];
    for row in 0..3 {
        for column in 0..3 {
            let mut sum = 0.0;
            for k in 0..3 {
                sum += h[k][row] * h[k][column];
            }
            gram[row][column] = sum;
        }
    }
    let (eigenvalues, eigenvectors) = jacobi_eigen(gram);

    let order = [2usize, 1, 0];
    let mut v = [[0.0; 3]; 3];
    let mut sigma = [0.0; 3];
    for (column, &source) in order.iter().enumerate() {
        for row in 0..3 {
            v[row][column] = eigenvectors[row][source];
        }
        sigma[column] = eigenvalues[source].max(0.0).sqrt();
    }

    let largest = sigma[0].max(sigma[1]).max(sigma[2]);
    let tolerance = largest.max(1.0) * 1e-12;
    let mut u = [[0.0; 3]; 3];
    let mut filled = [false; 3];
    for column in 0..3 {
        if sigma[column] <= tolerance {
            continue;
        }
        for row in 0..3 {
            let mut sum = 0.0;
            for k in 0..3 {
                sum += h[row][k] * v[k][column];
            }
            u[row][column] = sum / sigma[column];
        }
        let length = vector_length([u[0][column], u[1][column], u[2][column]]);
        if length > 0.0 {
            for row in 0..3 {
                u[row][column] /= length;
            }
            filled[column] = true;
        }
    }

    for column in 0..3 {
        if filled[column] {
            continue;
        }
        let mut best_axis = 0;
        let mut best_length = -1.0;
        let mut best_vector = [0.0; 3];
        for axis in 0..3 {
            let mut candidate = [0.0; 3];
            candidate[axis] = 1.0;
            for other in 0..3 {
                if !filled[other] {
                    continue;
                }
                let dot = candidate[0] * u[0][other]
                    + candidate[1] * u[1][other]
                    + candidate[2] * u[2][other];
                for row in 0..3 {
                    candidate[row] -= dot * u[row][other];
                }
            }
            let length = vector_length(candidate);
            if length > best_length {
                best_length = length;
                best_axis = axis;
                best_vector = candidate;
            }
        }
        if best_length > 1e-15 {
            for row in 0..3 {
                u[row][column] = best_vector[row] / best_length;
            }
        } else {
            u[best_axis][column] = 1.0;
        }
        filled[column] = true;
    }

    (u, v)
}

fn vector_length(vector: [f64; 3]) -> f64 {
    (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt()
}

fn determinant3(matrix: &[[f64; 3]; 3]) -> f64 {
    matrix[0][0] * (matrix[1][1] * matrix[2][2] - matrix[1][2] * matrix[2][1])
        - matrix[0][1] * (matrix[1][0] * matrix[2][2] - matrix[1][2] * matrix[2][0])
        + matrix[0][2] * (matrix[1][0] * matrix[2][1] - matrix[1][1] * matrix[2][0])
}

/// Jacobi eigen decomposition of a symmetric NxN matrix; eigenvalues are
/// returned in ascending order and eigenvectors as columns.
fn jacobi_eigen<const N: usize>(mut matrix: [[f64; N]; N]) -> ([f64; N], [[f64; N]; N]) {
    let mut eigenvectors = [[0.0; N]; N];
    for (index, row) in eigenvectors.iter_mut().enumerate() {
        row[index] = 1.0;
    }

    for _ in 0..64 {
        let mut p = 0;
        let mut q = 1;
        let mut largest = 0.0;
        for row in 0..N {
            for column in row + 1..N {
                let magnitude = matrix[row][column].abs();
                if magnitude > largest {
                    largest = magnitude;
                    p = row;
                    q = column;
                }
            }
        }
        if largest < 1e-15 {
            break;
        }

        let app = matrix[p][p];
        let aqq = matrix[q][q];
        let apq = matrix[p][q];
        let phi = 0.5 * (aqq - app) / apq;
        let t = if phi >= 0.0 {
            1.0 / (phi + (1.0 + phi * phi).sqrt())
        } else {
            -1.0 / (-phi + (1.0 + phi * phi).sqrt())
        };
        let c = 1.0 / (1.0 + t * t).sqrt();
        let s = t * c;

        for k in 0..N {
            if k == p || k == q {
                continue;
            }
            let akp = matrix[k][p];
            let akq = matrix[k][q];
            matrix[k][p] = c * akp - s * akq;
            matrix[k][q] = s * akp + c * akq;
            matrix[p][k] = matrix[k][p];
            matrix[q][k] = matrix[k][q];
        }
        matrix[p][p] = c * c * app - 2.0 * s * c * apq + s * s * aqq;
        matrix[q][q] = s * s * app + 2.0 * s * c * apq + c * c * aqq;
        matrix[p][q] = 0.0;
        matrix[q][p] = 0.0;

        for k in 0..N {
            let vkp = eigenvectors[k][p];
            let vkq = eigenvectors[k][q];
            eigenvectors[k][p] = c * vkp - s * vkq;
            eigenvectors[k][q] = s * vkp + c * vkq;
        }
    }

    let mut values = [0.0; N];
    for index in 0..N {
        values[index] = matrix[index][index];
    }
    let mut order: [usize; N] = std::array::from_fn(|index| index);
    order.sort_unstable_by(|&left, &right| {
        values[left]
            .total_cmp(&values[right])
            .then(left.cmp(&right))
    });
    let mut sorted_values = [0.0; N];
    let mut sorted_vectors = [[0.0; N]; N];
    for (column, &source) in order.iter().enumerate() {
        sorted_values[column] = values[source];
        for row in 0..N {
            sorted_vectors[row][column] = eigenvectors[row][source];
        }
    }
    (sorted_values, sorted_vectors)
}
