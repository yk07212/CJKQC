//! The performance contract of `rqm/parser-framework.md`: the
//! allocation-freedom of a structural walk. This file holds exactly one
//! test and one global allocator — the counting allocator is process
//! wide, so no other test of this binary may run beside it.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};

use compiled_jkqc::{FileWindows, LinePattern};

thread_local! {
    static MEASURING: Cell<bool> = const { Cell::new(false) };
}

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

/// A counting wrapper over the system allocator: every allocation on
/// the measuring thread is counted, so the walk under test can be
/// observed. Only the one test of this binary toggles `MEASURING`, and
/// it is the only thread that runs, so the count it reads is the walk's
/// own.
// rq-8311db22
struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if MEASURING.with(Cell::get) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if MEASURING.with(Cell::get) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        System.realloc(ptr, layout, new_size)
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if MEASURING.with(Cell::get) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        System.alloc_zeroed(layout)
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn locate_allocations(lines: usize, tested: &Cell<usize>) -> (usize, usize) {
    // A window of `lines` ordinary lines, none a candidate: the first
    // field of each line is a word the pattern refuses.
    let content = "ordinary output line of no interest to any pattern\n".repeat(lines);
    let mut cursor = Cursor::new(content.as_bytes());
    let windows = FileWindows::from_seekable(&mut cursor, 1 << 20, 1 << 20).expect("windows");
    // A Where pattern whose test iterates the line's leading fields
    // without collecting them — the shape the performance contract
    // demands of a per-line test.
    let pattern = LinePattern::Where(&|line: &str| {
        tested.set(tested.get() + 1);
        let mut fields = line.split_whitespace();
        fields.next() == Some("ordinary") && fields.next() == Some("never")
    });
    ALLOCATIONS.store(0, Ordering::Relaxed);
    MEASURING.with(|measuring| measuring.set(true));
    let matches = windows.locate(&[pattern]);
    MEASURING.with(|measuring| measuring.set(false));
    let allocated = ALLOCATIONS.load(Ordering::Relaxed);
    assert_eq!(matches.first(0), None, "no line is a candidate");
    (allocated, tested.get())
}

// rq-8311db22
#[test]
fn a_structural_walk_performs_no_allocation() {
    let small = locate_allocations(10, &Cell::new(0));
    let large = locate_allocations(500, &Cell::new(0));
    // Every line is tested, whatever the window's size.
    assert_eq!(small.1, 10);
    assert_eq!(large.1, 500);
    // The walk itself allocates nothing per line: the allocation count
    // of a 500-line walk is the 10-line walk's own — the constant cost
    // of the walk's result buffers, not a per-line cost.
    assert_eq!(small.0, large.0);
}
