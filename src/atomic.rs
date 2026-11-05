//! The shared atomic replacement write used by the database writer and
//! the movie export: the content lands in a temporary file in the
//! target's parent directory, is flushed and synced, then renamed over
//! the target, so a failed write leaves any previous file intact.
//! Crate-internal; not public API.
//!
//! (rq-0c3f8c54)

use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Writes `contents` as the complete new content of `path`, atomically:
/// the content lands in a temporary file in the target's parent
/// directory, is flushed and synced to the storage device, then renamed
/// over `path`, replacing any existing file atomically. Any failure
/// before the rename leaves the previous content of `path` (if any)
/// intact and any temporary file removed. After a successful rename the
/// parent directory is synced as well where the platform allows it, so
/// the replacement itself is durable. The parent directory must already
/// exist. Zero-length content produces an empty file.
pub(crate) fn write_atomically(path: &Path, contents: &str) -> io::Result<()> {
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{}: not a file path", path.display()),
        )
    })?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let temp = parent.join(format!(
        ".{}.tmp-{}-{}",
        file_name.to_string_lossy(),
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let outcome = write_temp_and_rename(&temp, contents, path);
    if outcome.is_ok() {
        // Persist the rename itself (the directory entry) where the
        // platform allows syncing a directory; best effort.
        #[cfg(unix)]
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    } else {
        let _ = fs::remove_file(&temp);
        outcome
    }
}

fn write_temp_and_rename(temp: &Path, contents: &str, path: &Path) -> io::Result<()> {
    let file = File::create(temp)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(contents.as_bytes())?;
    writer.flush()?;
    let file = writer.into_inner().map_err(io::Error::from)?;
    file.sync_all()?;
    fs::rename(temp, path)
}
