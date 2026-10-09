//! A local checkout's files and the definitions in them, for Go to File, Go
//! to Symbol, and the code viewer's outline. Building one lists the files
//! Git knows (tracked, and untracked ones it does not ignore) and scans each
//! source file's lines, so it is blocking work for a background executor.
//! Everything is bounded: the listing, the files scanned and their size, and
//! the definitions kept. Only regular files are read, never through a
//! symlink, so a FIFO or a device in the checkout cannot stall the build,
//! and a symlinked folder cannot pass outside files off as the checkout's.

mod changes;
mod symbols;

pub(crate) use changes::Change;
pub(crate) use symbols::{Kind, Language, scan};

use std::{
    collections::{HashMap, HashSet},
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

/// The most files listed, which bounds the index of the largest monorepos.
const MAX_FILES: usize = 100_000;
/// The longest file listing read from Git.
const MAX_LISTING_BYTES: usize = 32 << 20;
/// Larger files are generated or data, and are listed but not scanned.
const MAX_SCANNED_BYTES: u64 = 1 << 20;
/// The most definitions kept from one file, and from the whole checkout.
const MAX_PER_FILE: usize = 5_000;
const MAX_SYMBOLS: usize = 300_000;
/// How long Git may take to list or locate a checkout.
const GIT_DEADLINE: Duration = Duration::from_secs(30);

/// One definition, and the file it is in as an index into [`Index::files`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Symbol {
    pub(crate) name: String,
    pub(crate) kind: Kind,
    pub(crate) file: usize,
    pub(crate) line: u32,
}

/// A checkout's files, relative to its root, and their definitions.
#[derive(Debug, Default)]
pub(crate) struct Index {
    root: PathBuf,
    files: Vec<String>,
    symbols: Vec<Symbol>,
    /// Uncommitted changes, newest first, and each one's place by file.
    changes: Vec<Change>,
    changed: HashMap<usize, usize>,
    /// Whether a limit left files or definitions out.
    truncated: bool,
}

impl Index {
    /// Lists and scans the checkout at `root`. Blocking.
    pub(crate) fn build(root: &Path, cancelled: &AtomicBool) -> crate::Result<Self> {
        let checkout = root.to_str().ok_or(crate::Error::CodeIndexRoot)?;
        let stop = || cancelled.load(Ordering::Acquire);
        let git = |args: &[&str], operation| {
            crate::git::git_bytes(
                checkout,
                args,
                operation,
                Instant::now() + GIT_DEADLINE,
                &stop,
                MAX_LISTING_BYTES,
            )
        };
        let tracked = git(&["ls-files", "-z", "--cached"], "list files")?;
        let untracked = git(
            &["ls-files", "-z", "--others", "--exclude-standard"],
            "list untracked files",
        )?;
        let mut index = Self::from_listing(root, &[tracked.as_slice(), &untracked].concat());
        // A checkout with no commit yet has nothing to compare with. Git's
        // warnings share the output's stream, and a `core.autocrlf` checkout
        // (the default on Windows) warns about line endings on every diff,
        // which would read as the first file's counts. This read-only diff
        // turns that warning off; the user's own commits keep their setting.
        let counts = git(
            &[
                "-c",
                "core.safecrlf=false",
                "diff",
                "--numstat",
                "-z",
                "HEAD",
            ],
            "count changes",
        )
        .map(|output| changes::numstat(&output))
        .unwrap_or_default();
        let mut untracked: Vec<String> = untracked
            .split(|byte| *byte == 0)
            .filter_map(|name| std::str::from_utf8(name).ok())
            .map(str::to_owned)
            .collect();
        untracked.sort_unstable();
        index.set_changes(changes::collect(root, &index.files, &counts, &untracked));
        let mut real = HashSet::new();
        for file in 0..index.files.len() {
            if stop() {
                return Err(crate::Error::CodeIndexCancelled);
            }
            if index.symbols.len() >= MAX_SYMBOLS {
                index.truncated = true;
                break;
            }
            let Some(language) = Language::of(&index.files[file]) else {
                continue;
            };
            if !real_folders(root, &index.files[file], &mut real) {
                continue;
            }
            let Some(text) = read_source(&root.join(&index.files[file])) else {
                continue;
            };
            let room = MAX_PER_FILE.min(MAX_SYMBOLS - index.symbols.len());
            index
                .symbols
                .extend(scan(language, &text, room).into_iter().map(|found| Symbol {
                    name: found.name,
                    kind: found.kind,
                    file,
                    line: found.line,
                }));
        }
        Ok(index)
    }

    /// The files of a NUL-separated `git ls-files -z` listing, sorted, with
    /// any name that is not UTF-8 or could leave the checkout left out.
    fn from_listing(root: &Path, listing: &[u8]) -> Self {
        let mut files: Vec<String> = listing
            .split(|byte| *byte == 0)
            .filter_map(|name| std::str::from_utf8(name).ok())
            .filter(|name| relative(name))
            .map(str::to_owned)
            .collect();
        files.sort_unstable();
        files.dedup();
        let truncated = files.len() > MAX_FILES;
        files.truncate(MAX_FILES);
        Self {
            root: root.to_owned(),
            files,
            symbols: Vec::new(),
            changes: Vec::new(),
            changed: HashMap::new(),
            truncated,
        }
    }

    fn set_changes(&mut self, changes: Vec<Change>) {
        self.changed = changes
            .iter()
            .enumerate()
            .map(|(place, change)| (change.file, place))
            .collect();
        self.changes = changes;
    }

    /// Uncommitted changes, newest first.
    pub(crate) fn changes(&self) -> &[Change] {
        &self.changes
    }

    /// File `file`'s uncommitted change, if it has one.
    pub(crate) fn change(&self, file: usize) -> Option<&Change> {
        self.changed.get(&file).map(|&place| &self.changes[place])
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn files(&self) -> &[String] {
        &self.files
    }

    pub(crate) fn symbols(&self) -> &[Symbol] {
        &self.symbols
    }

    pub(crate) fn truncated(&self) -> bool {
        self.truncated
    }

    /// An index of `files` and `symbols`, as a build would make it.
    #[cfg(test)]
    pub(crate) fn of(root: &Path, files: &[&str], symbols: Vec<Symbol>) -> Self {
        Self {
            root: root.to_owned(),
            files: files.iter().map(|file| (*file).to_owned()).collect(),
            symbols,
            changes: Vec::new(),
            changed: HashMap::new(),
            truncated: false,
        }
    }

    /// `of`, with `changes` newest first.
    #[cfg(test)]
    pub(crate) fn with_changes(mut self, changes: Vec<Change>) -> Self {
        self.set_changes(changes);
        self
    }

    /// The absolute path of file `file`.
    pub(crate) fn path(&self, file: usize) -> Option<PathBuf> {
        self.files.get(file).map(|name| self.root.join(name))
    }
}

/// Whether a listed name stays inside the checkout and shows as it reads,
/// without control or direction-override characters.
fn relative(name: &str) -> bool {
    !name.is_empty()
        && !name.chars().any(crate::notifications::unsafe_char)
        && Path::new(name)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
}

/// Whether every folder on the way from `root` to listed file `name` is a
/// folder, not a symlink: Git lists `src/main.rs` even after `src` became a
/// link to somewhere else. `real` remembers the folders already checked, so
/// each is looked at once per build. Blocking.
fn real_folders(root: &Path, name: &str, real: &mut HashSet<PathBuf>) -> bool {
    let Some(parent) = Path::new(name).parent() else {
        return true;
    };
    let mut folder = PathBuf::new();
    for part in parent.components() {
        folder.push(part);
        if real.contains(&folder) {
            continue;
        }
        let is_folder =
            std::fs::symlink_metadata(root.join(&folder)).is_ok_and(|metadata| metadata.is_dir());
        if !is_folder {
            return false;
        }
        real.insert(folder.clone());
    }
    true
}

/// A regular file's text, when it is small enough and reads as UTF-8 text.
pub(crate) fn read_source(path: &Path) -> Option<String> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_SCANNED_BYTES {
        return None;
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).ok()?);
    std::fs::File::open(path)
        .ok()?
        .take(MAX_SCANNED_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    // A NUL byte early on marks a binary file, as Git judges one.
    if bytes.iter().take(8000).any(|byte| *byte == 0) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

/// The root of the Git checkout holding `dir`. Blocking.
pub(crate) fn checkout_root(dir: &str, cancelled: &AtomicBool) -> crate::Result<PathBuf> {
    let root = crate::git::git(
        dir,
        &["rev-parse", "--show-toplevel"],
        "find the checkout",
        Instant::now() + GIT_DEADLINE,
        &|| cancelled.load(Ordering::Acquire),
    )?;
    let root = PathBuf::from(root);
    if !root.is_absolute() {
        return Err(crate::Error::CodeIndexRoot);
    }
    Ok(root)
}

#[cfg(test)]
mod tests;
