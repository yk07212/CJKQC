//! The shared head and tail file reader: every file is presented as two
//! bounded windows — a head from the start and a tail from the end — and
//! parsers treat the windows as the entire file; no API exposes the elided
//! middle. A file small enough to fit in the windows is presented whole.
//! Window cut points fall on line and UTF-8 character boundaries, and
//! invalid UTF-8 anywhere in a window becomes U+FFFD replacement
//! characters: parsing never aborts over stray bytes from crashed
//! programs, and binary junk simply fails detection.
//!
//! (rq-6f803572)

use std::io::{self, Read, Seek, SeekFrom};

/// The default head window size: 256 KiB, overridable per call.
///
/// (rq-6f803572)
pub const DEFAULT_HEAD_BYTES: usize = 256 * 1024;

/// The default tail window size: 4 MiB, overridable per call.
///
/// (rq-6f803572)
pub const DEFAULT_TAIL_BYTES: usize = 4 * 1024 * 1024;

/// A pair of window sizes for one reader call; `Default` is the
/// framework's `DEFAULT_HEAD_BYTES` and `DEFAULT_TAIL_BYTES`. The CLI
/// builds one from `--head-bytes` and `--tail-bytes` so a user can widen
/// the windows past a seam that cut a record's data.
///
/// (rq-4311126f)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowBudget {
    /// Bytes read from the file's start.
    pub head_bytes: usize,
    /// Bytes read from the file's end.
    pub tail_bytes: usize,
}

impl Default for WindowBudget {
    fn default() -> Self {
        WindowBudget {
            head_bytes: DEFAULT_HEAD_BYTES,
            tail_bytes: DEFAULT_TAIL_BYTES,
        }
    }
}

/// The head and tail view of one file's content. When the file is not
/// covered, `head` holds complete lines from the start, `tail` complete
/// lines from the end (including a final line without a trailing
/// newline), and at most one partial line is dropped at each seam.
///
/// (rq-16630511)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileWindows {
    /// The head window text, always valid UTF-8 (invalid bytes become
    /// U+FFFD replacement characters).
    pub head: String,
    /// The tail window text, always valid UTF-8.
    pub tail: String,
    /// The file's size in bytes, before any lossy conversion.
    pub total_len: u64,
    /// The head window size this view was constructed with.
    head_limit: usize,
    /// The tail window size this view was constructed with.
    tail_limit: usize,
}

impl FileWindows {
    /// Reads the head from the start and the tail from the end by seeking;
    /// the bytes read are bounded by the two window sizes regardless of
    /// file size. A file no larger than `head_bytes + tail_bytes` is read
    /// whole once, covered, with `head` and `tail` both the entire
    /// content.
    ///
    /// (rq-16630511)
    pub fn from_seekable<S: Read + Seek>(
        seekable: &mut S,
        head_bytes: usize,
        tail_bytes: usize,
    ) -> io::Result<Self> {
        let total_len = seekable.seek(SeekFrom::End(0))?;
        if total_len <= window_budget(head_bytes, tail_bytes) {
            let length = usize::try_from(total_len).map_err(|_| file_too_large())?;
            let mut raw = vec![0u8; length];
            seekable.seek(SeekFrom::Start(0))?;
            seekable.read_exact(&mut raw)?;
            let text = String::from_utf8_lossy(&raw).into_owned();
            return Ok(Self::whole(text, total_len, head_bytes, tail_bytes));
        }

        let mut raw_head = vec![0u8; head_bytes];
        seekable.seek(SeekFrom::Start(0))?;
        seekable.read_exact(&mut raw_head)?;
        let head = String::from_utf8_lossy(head_window(&raw_head)).into_owned();

        // The byte just before the tail window tells whether the window
        // begins at a line start; without it a final line that happens to
        // start exactly at the seam would be dropped.
        let tail_start = total_len - tail_bytes as u64;
        let mut boundary = [0u8; 1];
        seekable.seek(SeekFrom::Start(tail_start - 1))?;
        seekable.read_exact(&mut boundary)?;
        let mut raw_tail = vec![0u8; tail_bytes];
        seekable.seek(SeekFrom::Start(tail_start))?;
        seekable.read_exact(&mut raw_tail)?;
        let tail =
            String::from_utf8_lossy(tail_window(&raw_tail, boundary[0] == b'\n')).into_owned();

        Ok(Self {
            head,
            tail,
            total_len,
            head_limit: head_bytes,
            tail_limit: tail_bytes,
        })
    }

    /// The fallback for non-seekable sources (pipes, stdin): reads the
    /// stream once from start to end, retaining at most the two windows in
    /// memory. For identical content and window sizes it yields windows
    /// identical to `from_seekable`'s.
    ///
    /// (rq-16630511)
    pub fn from_stream<R: Read>(
        reader: &mut R,
        head_bytes: usize,
        tail_bytes: usize,
    ) -> io::Result<Self> {
        let mut head_raw: Vec<u8> = Vec::new();
        let mut tail_raw: Vec<u8> = Vec::new();
        // Whether the first byte of `tail_raw` starts a line (it begins
        // at stream position 0, so the flag starts true and tracks the
        // byte dropped at the front of the sliding window).
        let mut tail_starts_at_line_start = true;
        let mut total_len: u64 = 0;
        let mut chunk = [0u8; 8192];
        loop {
            let read = reader.read(&mut chunk)?;
            if read == 0 {
                break;
            }
            total_len += read as u64;
            let bytes = &chunk[..read];

            if head_raw.len() < head_bytes {
                let take = (head_bytes - head_raw.len()).min(bytes.len());
                head_raw.extend_from_slice(&bytes[..take]);
            }

            tail_raw.extend_from_slice(bytes);
            if tail_raw.len() > tail_bytes {
                let dropped = tail_raw.len() - tail_bytes;
                tail_starts_at_line_start = tail_raw[dropped - 1] == b'\n';
                tail_raw.drain(..dropped);
            }
        }

        if total_len <= window_budget(head_bytes, tail_bytes) {
            // Covered: the two retained pieces already span the whole
            // stream (they overlap or abut), so the entire content is
            // their union — bounded by the window sizes.
            let mut whole = head_raw;
            if total_len > whole.len() as u64 {
                let tail_start = total_len - tail_raw.len() as u64;
                whole.extend_from_slice(&tail_raw[head_bytes - tail_start as usize..]);
            }
            let text = String::from_utf8_lossy(&whole).into_owned();
            return Ok(Self::whole(text, total_len, head_bytes, tail_bytes));
        }

        let head = String::from_utf8_lossy(head_window(&head_raw)).into_owned();
        let tail =
            String::from_utf8_lossy(tail_window(&tail_raw, tail_starts_at_line_start)).into_owned();
        Ok(Self {
            head,
            tail,
            total_len,
            head_limit: head_bytes,
            tail_limit: tail_bytes,
        })
    }

    /// Whether the whole file is visible: the file is no larger than the
    /// head window plus the tail window. When covered, `head` and `tail`
    /// are both the entire content.
    ///
    /// (rq-16630511)
    pub fn covered(&self) -> bool {
        self.total_len <= window_budget(self.head_limit, self.tail_limit)
    }

    /// The first visible line, in file order, that satisfies the
    /// predicate.
    ///
    /// (rq-16630511)
    pub fn first_line_where(&self, predicate: impl Fn(&str) -> bool) -> Option<&str> {
        self.head
            .lines()
            .find(|&line| predicate(line))
            .or_else(|| self.tail.lines().find(|&line| predicate(line)))
    }

    /// The last visible line, in file order, that satisfies the
    /// predicate.
    ///
    /// (rq-16630511)
    pub fn last_line_where(&self, predicate: impl Fn(&str) -> bool) -> Option<&str> {
        self.tail
            .lines()
            .filter(|&line| predicate(line))
            .last()
            .or_else(|| self.head.lines().filter(|&line| predicate(line)).last())
    }

    /// An iterator over the visible lines that follow the first line
    /// satisfying the predicate, in file order. Iteration never crosses a
    /// window boundary: when the matched line lies in the head window of
    /// an uncovered file, iteration ends where the head window ends.
    ///
    /// (rq-16630511)
    pub fn after_first_line_where(
        &self,
        predicate: impl Fn(&str) -> bool,
    ) -> impl Iterator<Item = &str> + '_ {
        if let Some(index) = self.head.lines().position(&predicate) {
            self.head.lines().skip(index + 1)
        } else if let Some(index) = self.tail.lines().position(&predicate) {
            self.tail.lines().skip(index + 1)
        } else {
            // no visible line matches, so nothing follows a match
            self.head.lines().skip(usize::MAX)
        }
    }

    /// An iterator over the visible lines that follow the last line
    /// satisfying the predicate, in file order. Iteration never crosses a
    /// window boundary: when the matched line lies in the head window of
    /// an uncovered file, iteration ends where the head window ends.
    ///
    /// (rq-16630511)
    pub fn after_last_line_where(
        &self,
        predicate: impl Fn(&str) -> bool,
    ) -> impl Iterator<Item = &str> + '_ {
        let last_match = |window: &str| {
            window
                .lines()
                .enumerate()
                .filter(|&(_, line)| predicate(line))
                .map(|(index, _)| index)
                .last()
        };
        if let Some(index) = last_match(&self.tail) {
            self.tail.lines().skip(index + 1)
        } else if let Some(index) = last_match(&self.head) {
            self.head.lines().skip(index + 1)
        } else {
            // no visible line matches, so nothing follows a match
            self.head.lines().skip(usize::MAX)
        }
    }

    /// The multi-pattern search: each visible window is read a constant
    /// number of times — one line walk shared by the structural
    /// (`Where`) patterns, whole-window byte scans for the literal
    /// patterns — and never once per marker. A covered window is read once
    /// in total. `LineMatches` answers the same questions as the
    /// single-pattern methods, plus the tail-scoped `last_in_tail` the
    /// per-format status rules use.
    ///
    /// (rq-16630511 rq-15b1763d)
    pub fn locate<'p>(&self, patterns: &[LinePattern<'p>]) -> LineMatches<'_> {
        let mut hits: Vec<Hit<'_>> = patterns.iter().map(|_| Hit::default()).collect();
        let covered = self.covered();
        self.scan_window(&self.head, Side::Head, covered, patterns, &mut hits);
        if !covered {
            self.scan_window(&self.tail, Side::Tail, true, patterns, &mut hits);
        }
        LineMatches {
            windows: self,
            hits,
        }
    }

    /// One identification pass over a window: the structural (and
    /// degenerate) patterns share a single line walk, while each scannable
    /// literal pattern is located by whole-window byte scans resolved
    /// against a newline table. Each pattern is evaluated against each
    /// line at most once.
    fn scan_window<'w, 'p>(
        &self,
        window: &'w str,
        side: Side,
        counts_as_tail: bool,
        patterns: &[LinePattern<'p>],
        hits: &mut [Hit<'w>],
    ) {
        let bytes = window.as_bytes();

        // Patterns are classified once per window, never per line.
        let mut walk = Vec::new();
        let mut scan = Vec::new();
        for (index, pattern) in patterns.iter().enumerate() {
            if pattern.is_scannable() {
                scan.push(index);
            } else {
                walk.push(index);
            }
        }

        if !walk.is_empty() {
            let mut start = 0usize;
            for (index, line) in window.lines().enumerate() {
                // The physical line's extent, so the next line's byte
                // offset survives `\r\n` line endings.
                let line_end = start + line.len();
                let next_start = match bytes.get(line_end) {
                    Some(b'\r') if bytes.get(line_end + 1) == Some(&b'\n') => line_end + 2,
                    Some(_) => line_end + 1,
                    None => bytes.len(),
                };
                for &slot in &walk {
                    if patterns[slot].matches(line) {
                        record(
                            &mut hits[slot],
                            line,
                            side,
                            index,
                            next_start,
                            counts_as_tail,
                        );
                    }
                }
                start = next_start;
            }
        }

        if scan.is_empty() {
            return;
        }
        // One newline table maps byte offsets to line numbers for every
        // literal scan of this window.
        let newlines: Vec<u32> = memchr::memchr_iter(b'\n', bytes)
            .map(|offset| offset as u32)
            .collect();
        for &slot in &scan {
            let (needle, trimmed_prefix, trimmed_equals) = match patterns[slot] {
                LinePattern::Contains(text) => (text.as_bytes(), false, false),
                LinePattern::TrimmedStartsWith(text) => (text.as_bytes(), true, false),
                LinePattern::TrimmedEquals(text) => (text.as_bytes(), false, true),
                LinePattern::Where(_) => continue,
            };
            for hit_start in memchr::memmem::find_iter(bytes, needle) {
                let line_index = newlines.partition_point(|&end| (end as usize) < hit_start);
                let line_start = match line_index {
                    0 => 0,
                    _ => newlines[line_index - 1] as usize + 1,
                };
                let mut line_end = match newlines.get(line_index) {
                    Some(&end) => end as usize,
                    None => bytes.len(),
                };
                let next_start = match newlines.get(line_index) {
                    Some(&end) => end as usize + 1,
                    None => bytes.len(),
                };
                // `str::lines` discards a trailing carriage return, and
                // located line text matches its output exactly.
                if line_end > line_start && bytes[line_end - 1] == b'\r' {
                    line_end -= 1;
                }
                if trimmed_prefix && !is_all_whitespace(&bytes[line_start..hit_start]) {
                    continue;
                }
                if trimmed_equals
                    && !(is_all_whitespace(&bytes[line_start..hit_start])
                        && is_all_whitespace(&bytes[hit_start + needle.len()..line_end]))
                {
                    continue;
                }
                let line = &window[line_start..line_end];
                record(
                    &mut hits[slot],
                    line,
                    side,
                    line_index,
                    next_start,
                    counts_as_tail,
                );
            }
        }
    }

    /// Builds a covered view in which both windows hold the entire
    /// content.
    fn whole(text: String, total_len: u64, head_bytes: usize, tail_bytes: usize) -> Self {
        Self {
            head: text.clone(),
            tail: text,
            total_len,
            head_limit: head_bytes,
            tail_limit: tail_bytes,
        }
    }
}

/// A pattern a `locate` walk tests against each line.
///
/// (rq-fefddb16)
#[derive(Clone, Copy)]
pub enum LinePattern<'a> {
    /// The line contains the substring.
    Contains(&'a str),
    /// The whitespace-trimmed line equals the text.
    TrimmedEquals(&'a str),
    /// The leading-trimmed line starts with the text.
    TrimmedStartsWith(&'a str),
    /// A structural predicate for signatures no literal text expresses;
    /// the test rejects a non-candidate line without allocation.
    Where(&'a dyn Fn(&str) -> bool),
}

impl LinePattern<'_> {
    /// Whether the pattern can be located by whole-window byte scans:
    /// literal text that is neither empty nor spans a line break (a
    /// `str::lines` line never carries `\n` or a line-ending `\r`, and
    /// degenerate texts are carried exactly by the shared line walk).
    fn is_scannable(&self) -> bool {
        match self {
            LinePattern::Contains(text)
            | LinePattern::TrimmedEquals(text)
            | LinePattern::TrimmedStartsWith(text) => {
                !text.is_empty()
                    && !text.as_bytes().contains(&b'\n')
                    && !text.as_bytes().contains(&b'\r')
            }
            LinePattern::Where(_) => false,
        }
    }

    fn matches(&self, line: &str) -> bool {
        match self {
            LinePattern::Contains(needle) => line.contains(needle),
            LinePattern::TrimmedEquals(text) => line.trim() == *text,
            LinePattern::TrimmedStartsWith(prefix) => line.trim_start().starts_with(prefix),
            LinePattern::Where(test) => test(line),
        }
    }
}

/// The pair `last_in_tail` returns: the matched line and its zero-based
/// line number within the tail window, usable to order two located
/// markers.
///
/// (rq-6e71ffd1)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TailLine<'w> {
    /// The matched text.
    pub line: &'w str,
    /// The line's zero-based index within the tail window.
    pub index: usize,
}

/// Which window a located line lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Head,
    Tail,
}

/// A located line and where the window's following line begins.
#[derive(Debug, Clone, Copy)]
struct Place<'w> {
    line: &'w str,
    side: Side,
    next_start: usize,
}

/// The located lines of one `locate` call, per pattern.
#[derive(Debug, Default)]
struct Hit<'w> {
    first: Option<Place<'w>>,
    last: Option<Place<'w>>,
    last_tail: Option<TailLine<'w>>,
}

/// The located lines of one `locate` call, borrowed from the windows.
/// Each accessor takes the pattern's index.
///
/// (rq-c6dcf9fc)
#[derive(Debug)]
pub struct LineMatches<'w> {
    windows: &'w FileWindows,
    hits: Vec<Hit<'w>>,
}

impl<'w> LineMatches<'w> {
    /// The first visible line, in file order, matching the pattern (head
    /// window first, like `first_line_where`).
    ///
    /// # Panics
    ///
    /// Panics if `pattern` is not an index of the slice passed to
    /// `locate`.
    ///
    /// (rq-c6dcf9fc)
    pub fn first(&self, pattern: usize) -> Option<&'w str> {
        self.assert_pattern(pattern);
        self.hits[pattern].first.map(|place| place.line)
    }

    /// The first matching line of the head window with its tail line
    /// number ignored; `None` when no head line matches — a match
    /// confined to the tail is not a match in the head. (The first hit
    /// is recorded in file order, head before tail, so a head-sourced
    /// first is exactly a head match.)
    ///
    /// # Panics
    ///
    /// Panics if `pattern` is not an index of the slice passed to
    /// `locate`.
    ///
    /// (rq-c6dcf9fc)
    pub fn first_in_head(&self, pattern: usize) -> Option<&'w str> {
        self.assert_pattern(pattern);
        self.hits[pattern]
            .first
            .filter(|place| place.side == Side::Head)
            .map(|place| place.line)
    }

    /// The last visible line, in file order, matching the pattern (tail
    /// window first, like `last_line_where`).
    ///
    /// # Panics
    ///
    /// Panics if `pattern` is not an index of the slice passed to
    /// `locate`.
    ///
    /// (rq-c6dcf9fc)
    pub fn last(&self, pattern: usize) -> Option<&'w str> {
        self.assert_pattern(pattern);
        self.hits[pattern].last.map(|place| place.line)
    }

    /// The last matching line of the tail window with its tail line
    /// number; `None` when no tail line matches — a match confined to the
    /// head is not a match in the tail.
    ///
    /// # Panics
    ///
    /// Panics if `pattern` is not an index of the slice passed to
    /// `locate`.
    ///
    /// (rq-c6dcf9fc rq-6e71ffd1)
    pub fn last_in_tail(&self, pattern: usize) -> Option<TailLine<'w>> {
        self.assert_pattern(pattern);
        self.hits[pattern].last_tail
    }

    /// The visible lines following the first match, in file order, never
    /// crossing a window boundary (`after_first_line_where` semantics).
    ///
    /// # Panics
    ///
    /// Panics if `pattern` is not an index of the slice passed to
    /// `locate`.
    ///
    /// (rq-c6dcf9fc)
    pub fn after_first(&self, pattern: usize) -> std::str::Lines<'w> {
        self.assert_pattern(pattern);
        following(self.windows, self.hits[pattern].first)
    }

    /// The visible lines following the last match, in file order, never
    /// crossing a window boundary (`after_last_line_where` semantics).
    ///
    /// # Panics
    ///
    /// Panics if `pattern` is not an index of the slice passed to
    /// `locate`.
    ///
    /// (rq-c6dcf9fc)
    pub fn after_last(&self, pattern: usize) -> std::str::Lines<'w> {
        self.assert_pattern(pattern);
        following(self.windows, self.hits[pattern].last)
    }

    fn assert_pattern(&self, pattern: usize) {
        debug_assert!(
            pattern < self.hits.len(),
            "pattern index {pattern} is out of range for the {} patterns of this locate call",
            self.hits.len(),
        );
    }
}

/// The window lines after a located place; a head place of an uncovered
/// file stops where the head window ends, and no match follows nothing.
fn following<'w>(windows: &'w FileWindows, place: Option<Place<'w>>) -> std::str::Lines<'w> {
    match place {
        Some(place) => {
            let window = match place.side {
                Side::Head => &windows.head,
                Side::Tail => &windows.tail,
            };
            window[place.next_start..].lines()
        }
        None => "".lines(),
    }
}

/// Record one located line: the first match is only ever set by the
/// earliest hit (head scanned before tail), the last match is overwritten
/// by every later hit, and hits in lines that belong to the tail window
/// also settle `last_in_tail`.
fn record<'w>(
    hit: &mut Hit<'w>,
    line: &'w str,
    side: Side,
    index: usize,
    next_start: usize,
    counts_as_tail: bool,
) {
    if hit.first.is_none() {
        hit.first = Some(Place {
            line,
            side,
            next_start,
        });
    }
    hit.last = Some(Place {
        line,
        side,
        next_start,
    });
    if counts_as_tail {
        hit.last_tail = Some(TailLine { line, index });
    }
}

/// Whether a located hit's line boundary holds only whitespace, matching
/// what `str::trim` would remove. A window is valid UTF-8 with
/// line-aligned boundaries; a malformed slice (which cannot arise) simply
/// never qualifies a hit.
fn is_all_whitespace(bytes: &[u8]) -> bool {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.chars().all(char::is_whitespace),
        Err(_) => false,
    }
}

/// The size at or below which a file is covered: both windows together.
///
/// (rq-6f803572)
fn window_budget(head_bytes: usize, tail_bytes: usize) -> u64 {
    (head_bytes as u64).saturating_add(tail_bytes as u64)
}

/// The raw head bytes trimmed back to their last complete line: the
/// bytes through the final newline are kept and at most one partial
/// trailing line is dropped; a window without a newline holds no
/// complete line.
///
/// (rq-6f803572)
fn head_window(raw: &[u8]) -> &[u8] {
    match raw.iter().rposition(|&byte| byte == b'\n') {
        Some(end) => &raw[..=end],
        None => &[],
    }
}

/// The raw tail bytes advanced to their first line start: a window that
/// already starts at a line start is kept whole — including a final line
/// that lacks its trailing newline. Otherwise the bytes through the
/// first newline are dropped; a window without a newline is one mid-line
/// fragment and keeps nothing.
///
/// (rq-6f803572)
fn tail_window(raw: &[u8], starts_at_line_start: bool) -> &[u8] {
    if starts_at_line_start {
        return raw;
    }
    match raw.iter().position(|&byte| byte == b'\n') {
        Some(end) => &raw[end + 1..],
        None => &[],
    }
}

/// Error for a file too large to hold in memory, on platforms where the
/// window budget exceeds the address space.
fn file_too_large() -> io::Error {
    io::Error::other("file is larger than the addressable window budget")
}
