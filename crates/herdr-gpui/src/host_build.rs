//! Copying one of this app's helper programs onto a host: the host's
//! platform from `uname -sm`, the build this app ships for it, and the host
//! script that takes the bytes on stdin, checks their SHA-256, and puts them
//! in the user's home. Nothing needs root and nothing is written outside
//! `$HOME`:
//!
//! - `~/.local/share/PROGRAM/bin/VERSION/PROGRAM`, the build;
//! - `~/.local/bin/PROGRAM`, a link to it.
//!
//! Automations copy `herdr-automate` this way, and remote control copies
//! `herdr-remote-control`.
// i18n: migrated

use herdr_client::shell_quote;
use sha2::{Digest, Sha256};
use std::{
    io::{self, Read},
    path::{Path, PathBuf},
};

/// The copy script's exit status when the copy did not match.
pub(crate) const CHECKSUM_EXIT: i32 = 3;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Could not read the {program} build at {}", path.display())]
    Binary {
        program: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("The copy on the host did not match this app's build; nothing was installed")]
    Checksum,
    #[error("Cannot tell this host's platform from `uname -sm`: {0}")]
    Platform(String),
    // keep-sorted end
}

/// A platform as `uname -sm` names it, in Rust's words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Platform {
    /// `linux` or `macos`.
    pub(crate) os: &'static str,
    /// `x86_64` or `aarch64`.
    pub(crate) arch: &'static str,
}

impl Platform {
    /// Reads `uname -sm` output such as `Linux x86_64` or `Darwin arm64`.
    pub(crate) fn parse(uname: &str) -> Result<Self, Error> {
        let unknown = || Error::Platform(crate::notifications::safe_text(uname, 80));
        let mut words = uname.split_whitespace();
        let (Some(system), Some(machine)) = (words.next(), words.next()) else {
            return Err(unknown());
        };
        let os = match system {
            "Linux" => "linux",  // i18n: data
            "Darwin" => "macos", // i18n: data
            _ => return Err(unknown()),
        };
        let arch = match machine {
            "x86_64" | "amd64" => "x86_64",
            "aarch64" | "arm64" => "aarch64",
            _ => return Err(unknown()),
        };
        Ok(Self { os, arch })
    }

    /// This computer's platform.
    pub(crate) fn local() -> Self {
        Self {
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
        }
    }

    /// The target a build for this platform is made for.
    pub(crate) fn triple(&self) -> String {
        match self.os {
            "macos" => format!("{}-apple-darwin", self.arch),
            _ => format!("{}-unknown-linux-musl", self.arch),
        }
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let os = match self.os {
            "macos" => "macOS",
            "linux" => "Linux", // i18n: data
            other => other,
        };
        write!(f, "{os} {}", self.arch)
    }
}

/// The build of `program` for `platform` this app ships: `bundled`, its own,
/// when the host is this kind of computer, else `BUILDS/TRIPLE/PROGRAM`
/// beside the executable.
pub(crate) fn build_for(
    platform: &Platform,
    beside: &Path,
    builds: &str,
    program: &str,
    bundled: Option<PathBuf>,
) -> Option<PathBuf> {
    let same = *platform == Platform::local();
    if let Some(bundled) = bundled.filter(|_| same) {
        return Some(bundled);
    }
    let path = beside.join(builds).join(platform.triple()).join(program);
    path.is_file().then_some(path)
}

/// The directory holding this app's executable.
pub(crate) fn beside_app() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_default()
}

/// The file's size and SHA-256, read in pieces.
pub(crate) fn digest(program: &'static str, path: &Path) -> Result<(u64, String), Error> {
    let binary = |source| Error::Binary {
        program,
        path: path.to_owned(),
        source,
    };
    let mut file = std::fs::File::open(path).map_err(binary)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    let mut size = 0u64;
    loop {
        let read = file.read(&mut buffer).map_err(binary)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        size += read as u64;
    }
    let sha256 = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok((size, sha256))
}

/// The host script that receives `program`'s build on stdin and puts it in
/// place, exiting [`CHECKSUM_EXIT`] when the copy does not match `sha256`.
pub(crate) fn copy_script(program: &str, version: &str, sha256: &str) -> String {
    format!(
        // i18n: data
        r#"program={program}
version={version}
sum={sum}
dir="$HOME/.local/share/$program/bin/$version"
mkdir -p "$dir" "$HOME/.local/bin"
part="$dir/.$program.part.$$"
trap 'rm -f "$part"' EXIT
cat > "$part"
if command -v sha256sum >/dev/null 2>&1; then got=$(sha256sum "$part"); else got=$(shasum -a 256 "$part"); fi
got=${{got%% *}}
if [ "$got" != "$sum" ]; then echo "the copy does not match its checksum" >&2; exit {CHECKSUM_EXIT}; fi
chmod 755 "$part"
mv -f "$part" "$dir/$program"
ln -sfn "$dir/$program" "$HOME/.local/bin/$program"
"#,
        program = shell_quote(program),
        version = shell_quote(version),
        sum = shell_quote(sha256),
    )
}
