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
