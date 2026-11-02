//! A mod's files and the hash approval is pinned to. Only `mod.toml`,
//! `*.luau`, and `*.svg` at the top of the folder count: they are what can
//! change what a mod does or shows. Other files (a README) are ignored, and
//! subfolders and symbolic links are never followed.

use crate::{Error, Manifest, Problem, Result, manifest};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::SystemTime,
};

/// Counted files a mod may have.
pub const MAX_FILES: usize = 64;
/// The largest single file.
pub const MAX_FILE_BYTES: u64 = 256 * 1024;
/// All counted files together.
pub const MAX_TOTAL_BYTES: u64 = 1024 * 1024;
const MANIFEST: &str = "mod.toml";
pub(crate) const MAIN: &str = "main.luau";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sources {
    pub manifest: Manifest,
    /// Module name (file stem) to source text.
    luau: BTreeMap<String, String>,
    /// File name to SVG text.
    icons: BTreeMap<String, String>,
    /// SHA-256 over every counted file, in lowercase hex.
    pub hash: String,
}

impl Sources {
    /// Reads a mod's folder. The folder's name must match the manifest's.
    pub fn read(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref();
        let read = |path: &Path| {
            let path = path.to_owned();
            move |source| Error::Read { path, source }
        };
        let mut files = Vec::new();
        let mut total = 0u64;
        for entry in fs::read_dir(dir).map_err(read(dir))? {
            let entry = entry.map_err(read(dir))?;
            let path = entry.path();
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if !counted(&name) {
                continue;
            }
            // `file_type` does not follow links, so a link is never read.
            if !entry.file_type().map_err(read(&path))?.is_file() {
                continue;
            }
            if files.len() == MAX_FILES {
                return Err(Error::Source {
                    file: name,
                    problem: Problem::Entries { limit: MAX_FILES },
                });
            }
            let bytes = read_bounded(&path)?;
            total += bytes.len() as u64;
            if total > MAX_TOTAL_BYTES {
                return Err(Error::Source {
                    file: name,
                    problem: Problem::TooLarge {
                        limit: MAX_TOTAL_BYTES,
                    },
                });
            }
            files.push((name, bytes));
        }
        let sources = Self::new(files)?;
        let folder = dir.file_name().and_then(|name| name.to_str());
        if folder != Some(sources.manifest.name.as_str()) {
            return Err(Error::Manifest {
                at: "name".to_owned(),
                problem: Problem::Expected {
                    expected: "the mod's folder name",
                },
            });
        }
        Ok(sources)
    }

    /// Builds sources from file names and contents, as [`Sources::read`]
    /// does after reading them; files that do not count are ignored.
    pub fn new(files: impl IntoIterator<Item = (String, Vec<u8>)>) -> Result<Self> {
        let mut counted_files = BTreeMap::new();
        for (name, bytes) in files {
            if !counted(&name) {
                continue;
            }
            if bytes.len() as u64 > MAX_FILE_BYTES {
                return Err(Error::Source {
                    file: name,
                    problem: Problem::TooLarge {
                        limit: MAX_FILE_BYTES,
                    },
                });
            }
            let text = String::from_utf8(bytes).map_err(|_| Error::Source {
                file: name.clone(),
                problem: Problem::Expected { expected: "UTF-8" },
            })?;
            counted_files.insert(name, text);
        }
        let hash = hash(&counted_files);
        let manifest = Manifest::parse(counted_files.get(MANIFEST).ok_or(Error::NoManifest)?)?;
        if !counted_files.contains_key(MAIN) {
            return Err(Error::NoMain);
        }
        let mut luau = BTreeMap::new();
        let mut icons = BTreeMap::new();
        for (name, text) in counted_files {
            if let Some(stem) = name.strip_suffix(".luau") {
                manifest::identifier(stem).map_err(|problem| Error::Source {
                    file: name.clone(),
                    problem,
                })?;
                luau.insert(stem.to_owned(), text);
            } else if name.ends_with(".svg") {
                icons.insert(name, text);
            }
        }
        if let Some(activity) = &manifest.activity
            && !icons.contains_key(&activity.icon)
        {
            return Err(Error::Manifest {
                at: "activity.icon".to_owned(),
                problem: Problem::Missing,
            });
        }
        Ok(Self {
            manifest,
            luau,
            icons,
            hash,
        })
    }

    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    /// A Luau module's text, by its file stem (`main`, `ui_parts`).
    pub fn module(&self, stem: &str) -> Option<&str> {
        self.luau.get(stem).map(String::as_str)
    }

    /// Every Luau module, by file stem.
    pub fn modules(&self) -> impl Iterator<Item = (&str, &str)> {
        self.luau
            .iter()
            .map(|(stem, text)| (stem.as_str(), text.as_str()))
    }

    pub fn icon(&self, file: &str) -> Option<&str> {
        self.icons.get(file).map(String::as_str)
    }
}

/// What changes when a counted file in a mod's folder changes: each one's
/// name, length, and modification time. Cheap enough to take every second;
/// a new stamp means [`Sources::read`] again.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stamp(Vec<(String, u64, Option<SystemTime>)>);

impl Stamp {
    pub fn take(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref();
        let read = |source| Error::Read {
            path: dir.to_owned(),
            source,
        };
        let mut files = Vec::new();
        for entry in fs::read_dir(dir).map_err(read)? {
            let entry = entry.map_err(read)?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if !counted(&name) || files.len() > MAX_FILES {
                continue;
            }
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if entry.file_type().is_ok_and(|kind| kind.is_file()) {
                files.push((name, meta.len(), meta.modified().ok()));
            }
        }
        files.sort();
        Ok(Self(files))
    }
}

fn counted(name: &str) -> bool {
    !name.starts_with('.')
        && (name == MANIFEST || name.ends_with(".luau") || name.ends_with(".svg"))
}

fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let read = |source| Error::Read {
        path: PathBuf::from(path),
        source,
    };
    let file = fs::File::open(path).map_err(read)?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(read)?;
    Ok(bytes)
}

/// Each file's name, a zero byte, its length, and its bytes, in name order,
/// so moving text between files changes the hash.
fn hash(files: &BTreeMap<String, String>) -> String {
    let mut digest = Sha256::new();
    for (name, text) in files {
        digest.update(name.as_bytes());
        digest.update([0]);
        digest.update((text.len() as u64).to_le_bytes());
        digest.update(text.as_bytes());
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
