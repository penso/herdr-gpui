//! Bounded, path-checked extraction of a remote folder's archive into a
//! private staging folder. Every entry must live under the folder's own
//! name, nothing is written outside the staging folder, and no entry is
//! ever created as a link.

use super::cache;
use std::{
    collections::HashSet,
    fs::{DirBuilder, File, OpenOptions},
    io::{self, Read},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[cfg(test)]
mod tests;

/// The most entries one archive may hold.
const MAX_ENTRIES: usize = 100_000;

/// The most bytes one archive may expand to, so a small archive of many
/// repeated files cannot fill the disk.
const MAX_EXPANDED: u64 = 2 * 1024 * 1024 * 1024;

/// Bytes in the megabyte the caps report.
const MB: u64 = 1024 * 1024;

/// The archive a host sends a folder in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArchiveFormat {
    TarGz,
    Zip,
}

/// What one extraction skipped.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Extracted {
    pub(crate) links_skipped: u32,
}

/// Why an archive was not extracted. Public because the remote files error
/// holds it; `archive` is a private module, so it stays inside the crate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The copy was cancelled, so the staging folder is left for removal.
    #[error("the folder's copy was cancelled")]
    Cancelled,
    #[error("two archive entries would both be written as {path}")]
    Collision { path: String },
    #[error(
        "the archive expands to more than {} MB, so it was not unpacked",
        .limit / MB
    )]
    ExpandedTooLarge { limit: u64 },
    #[error("the archive's entry {entry} is not a plain path")]
    Path { entry: String },
    #[error("could not read the archive: {0}")]
    Read(#[source] io::Error),
    #[error("the archive's entries are not under {expected}: {entry}")]
    Top { expected: String, entry: String },
    #[error("the archive holds more than {limit} entries")]
    TooManyEntries { limit: usize },
    #[error("the archive holds data after its last entry")]
    Trailing,
    #[error("could not unpack the archive: {0}")]
    Unpack(#[source] zip::result::ZipError),
    #[error("could not write {}: {source}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

type Result<T, E = Error> = std::result::Result<T, E>;

/// Extract `archive` into the empty folder `staging`, whose entries all live
/// under `top`, and write nothing outside it.
///
/// Args:
///     archive: The archive to unpack. Left as it is.
///     staging: The empty folder every entry is written under.
///     top: The folder's own name, which starts every entry's path and is
///         stripped.
///     format: How the archive is packed.
///     cancelled: Checked between entries; set means the caller removes
///         `staging`.
///
/// Returns:
///     What the extraction skipped.
///
/// Errors:
///     [`Error`] for an entry outside `top`, an unsafe path, two entries
///     that sanitize to one path, a cap, cancellation, or a read or write
///     failure. Nothing outside `staging` is touched.
pub(crate) fn extract(
    archive: &Path,
    staging: &Path,
    top: &str,
    format: ArchiveFormat,
    cancelled: &AtomicBool,
) -> Result<Extracted> {
    let file = File::open(archive).map_err(Error::Read)?;
    let mut sink = Sink {
        staging,
        cancelled,
        seen: HashSet::new(),
        entries: 0,
        expanded: 0,
        links_skipped: 0,
    };
    match format {
        ArchiveFormat::TarGz => sink.unpack_tar(file, top)?,
        ArchiveFormat::Zip => sink.unpack_zip(file, top)?,
    }
    Ok(Extracted {
        links_skipped: sink.links_skipped,
    })
}

/// Creates `path` and any missing parents, private to the user.
fn create_dirs(path: &Path) -> io::Result<()> {
    let mut builder = DirBuilder::new();
    builder.recursive(true);
    // Windows has no mode bits; the folder inherits the staging folder's ACL.
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

/// Creates the file at `path`, readable and writable only by the user and
/// never executable. Fails when a file is already there.
pub(super) fn create_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    // Windows has no mode bits; the file inherits its folder's ACL.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// The checks and the bookkeeping every entry of either format goes through.
struct Sink<'a> {
    staging: &'a Path,
    cancelled: &'a AtomicBool,
    /// The paths written so far, so a second entry cannot replace one.
    seen: HashSet<PathBuf>,
    entries: usize,
    expanded: u64,
    links_skipped: u32,
}

impl Sink<'_> {
    /// Checks cancellation and the entry cap before an entry is looked at.
    fn next(&mut self) -> Result<()> {
        if self.cancelled.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        self.entries += 1;
        if self.entries > MAX_ENTRIES {
            return Err(Error::TooManyEntries { limit: MAX_ENTRIES });
        }
        Ok(())
    }

    /// The staging-relative path of `entry`, which must start with `top`.
    /// An entry that is `top` itself is the staging folder.
    fn relative(&mut self, entry: &str, top: &str) -> Result<PathBuf> {
        let entry = entry.strip_suffix('/').unwrap_or(entry);
        let mut parts = entry.split('/');
        if parts.next() != Some(top) {
            return Err(Error::Top {
                expected: top.to_owned(),
                entry: entry.to_owned(),
            });
        }
        let mut relative = PathBuf::new();
        for part in parts {
            if part.is_empty() || part == "." || part == ".." {
                return Err(Error::Path {
                    entry: entry.to_owned(),
                });
            }
            relative.push(cache::local_name(part, cfg!(windows)).as_ref());
        }
        // A name that sanitizing left with a separator would still be inside
        // `staging` only if every component is a plain one.
        if relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(Error::Path {
                entry: entry.to_owned(),
            });
        }
        if !self.seen.insert(relative.clone()) {
            return Err(Error::Collision {
                path: relative.display().to_string(),
            });
        }
        Ok(relative)
    }

    fn directory(&mut self, relative: &Path) -> Result<()> {
        let path = self.staging.join(relative);
        create_dirs(&path).map_err(|source| Error::Write { path, source })
    }

    /// Creates the regular file `relative` holds, after checking the
    /// expanded cap.
    fn file(&mut self, entry: &str, relative: &Path, size: u64) -> Result<(PathBuf, File)> {
        if relative.as_os_str().is_empty() {
            return Err(Error::Path {
                entry: entry.to_owned(),
            });
        }
        self.expanded = self
            .expanded
            .checked_add(size)
            .filter(|total| *total <= MAX_EXPANDED)
            .ok_or(Error::ExpandedTooLarge {
                limit: MAX_EXPANDED,
            })?;
        let path = self.staging.join(relative);
        if let Some(parent) = path.parent() {
            create_dirs(parent).map_err(|source| Error::Write {
                path: path.clone(),
                source,
            })?;
        }
        let file = create_file(&path).map_err(|source| Error::Write {
            path: path.clone(),
            source,
        })?;
        Ok((path, file))
    }

    fn unpack_tar(&mut self, file: File, top: &str) -> Result<()> {
        let decoder = flate2::read::MultiGzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        for entry in archive.entries().map_err(Error::Read)? {
            self.next()?;
            let mut entry = entry.map_err(Error::Read)?;
            let path = entry.path().map_err(Error::Read)?.into_owned();
            let entry_path = path.to_str().ok_or_else(|| Error::Path {
                entry: path.display().to_string(),
            })?;
            let relative = self.relative(entry_path, top)?;
            let kind = entry.header().entry_type();
            if kind.is_dir() {
                if !relative.as_os_str().is_empty() {
                    self.directory(&relative)?;
                }
            } else if kind.is_file() {
                let (path, mut out) = self.file(entry_path, &relative, entry.size())?;
                io::copy(&mut entry, &mut out).map_err(|source| Error::Write { path, source })?;
            } else {
                self.links_skipped = self.links_skipped.saturating_add(1);
            }
        }
        // Reading to the end surfaces a truncated archive's failed trailer
        // and any data after its last entry.
        self.drain(&mut archive.into_inner())?;
        Ok(())
    }

    /// Reads `reader` to the end, refusing anything but padding after the
    /// last entry, so a partial compression stream is never taken whole.
    fn drain(&self, reader: &mut impl Read) -> Result<()> {
        let mut buffer = [0; 65536];
        loop {
            if self.cancelled.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            let count = reader.read(&mut buffer).map_err(Error::Read)?;
            if count == 0 {
                return Ok(());
            }
            if buffer[..count].iter().any(|byte| *byte != 0) {
                return Err(Error::Trailing);
            }
        }
    }

    fn unpack_zip(&mut self, file: File, top: &str) -> Result<()> {
        let mut archive = zip::ZipArchive::new(file).map_err(Error::Unpack)?;
        for index in 0..archive.len() {
            self.next()?;
            let mut entry = archive.by_index(index).map_err(Error::Unpack)?;
            let entry_path = entry.name().map_err(Error::Unpack)?.into_owned();
            let relative = self.relative(&entry_path, top)?;
            if entry.is_dir() {
                if !relative.as_os_str().is_empty() {
                    self.directory(&relative)?;
                }
            } else if entry.is_file() {
                let size = entry.size();
                let (path, mut out) = self.file(&entry_path, &relative, size)?;
                io::copy(&mut entry, &mut out).map_err(|source| Error::Write { path, source })?;
            } else {
                self.links_skipped = self.links_skipped.saturating_add(1);
            }
        }
        Ok(())
    }
}
