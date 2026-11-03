//! Windows paths as a WSL distribution sees them, for files dropped or pasted
//! into one of its panes. The distribution's own `wslpath` maps each one: a
//! drive under its automount root (`/mnt/c` unless `wsl.conf` moves it), and a
//! file inside that distribution (`\\wsl$\NAME\…` or `\\wsl.localhost\NAME\…`)
//! to its own path. Nothing is copied, so a file of any size is ready at once
//! and nothing is left behind to clean up.
use super::{Error, valid_distro};
use crate::{Result, ScriptHost, ScriptLimits, run_script};
use std::{path::Path, sync::atomic::AtomicBool, time::Duration};

/// Paths in one drop, as for an SSH file transfer.
const PATH_LIMIT: usize = 256;
/// Longest Windows path accepted, in bytes: the extended-length limit.
const PATH_BYTES: usize = 32 * 1024;
/// Starting a stopped distribution can take a while; `wslpath` itself is
/// instant once it runs.
const IDLE: Duration = Duration::from_secs(30);

/// Reads one Windows path per line and prints where each one is, or `-` for
/// one the distribution cannot reach (another distribution's files, a network
/// share, an unmounted drive), so every input line has one answer line.
const SCRIPT: &str = r#"while IFS= read -r herdr_path; do
    if herdr_unix=$(wslpath -u "$herdr_path" 2>/dev/null) && [ -n "$herdr_unix" ]; then
        printf '%s\n' "$herdr_unix"
    else
        printf -- '-\n'
    fi
done"#;

/// Where each of `paths`, absolute Windows paths, is inside `distro`, in
/// order. Fails as a whole when any one cannot be reached there, naming it.
///
/// Blocking: one `wsl.exe` child, bounded by output size and an idle limit,
/// so call it from a background worker. Accepts at most 256 paths, which
/// travel on the child's standard input, never in its arguments.
pub fn wsl_paths(
    distro: &str,
    paths: &[impl AsRef<Path>],
    cancelled: &AtomicBool,
) -> Result<Vec<String>> {
    if !valid_distro(distro) {
        return Err(Error::InvalidDistro.into());
    }
    let input = input(paths)?;
    let mut output = Vec::new();
    let limits = ScriptLimits {
        // `/mnt/c/` is a few bytes longer than `C:\`.
        output: ((PATH_BYTES + 64) * paths.len()) as u64,
        idle: IDLE,
    };
    run_script(
        ScriptHost::Wsl(distro),
        SCRIPT,
        input.as_bytes(),
        &mut output,
        limits,
        cancelled,
    )?;
    parse(&output, paths)
}

/// One path per line, each absolute, UTF-8, and free of control characters,
/// so no line can split or join another.
fn input(paths: &[impl AsRef<Path>]) -> Result<String> {
    if paths.len() > PATH_LIMIT {
        return Err(Error::PathLimit.into());
    }
    let mut input = String::new();
    for path in paths {
        let path = path.as_ref();
        let text = path
            .to_str()
            .filter(|text| {
                text.len() <= PATH_BYTES
                    && !text.chars().any(char::is_control)
                    && windows_absolute(text)
            })
            .ok_or_else(|| Error::PathInvalid(path.to_path_buf()))?;
        input.push_str(text);
        input.push('\n');
    }
    Ok(input)
}

/// `C:\…` or `\\server\…`: what a drop names, judged the same on every
/// platform so the check is testable anywhere.
fn windows_absolute(text: &str) -> bool {
    let bytes = text.as_bytes();
    match bytes {
        [drive, b':', b'\\' | b'/', ..] => drive.is_ascii_alphabetic(),
        [b'\\', b'\\', rest @ ..] => rest.first().is_some_and(|b| *b != b'\\'),
        _ => false,
    }
}

/// One answer per path: an absolute path inside the distribution, or `-` for
/// one it cannot reach.
fn parse(output: &[u8], paths: &[impl AsRef<Path>]) -> Result<Vec<String>> {
    let text = std::str::from_utf8(output).map_err(|_| Error::PathResponse)?;
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() != paths.len() {
        return Err(Error::PathResponse.into());
    }
    lines
        .into_iter()
        .zip(paths)
        .map(|(line, path)| match line {
            "-" => Err(Error::PathUnmapped(path.as_ref().to_path_buf()).into()),
            _ if line.starts_with('/') && !line.chars().any(char::is_control) => {
                Ok(line.to_owned())
            }
            _ => Err(Error::PathResponse.into()),
        })
        .collect()
}

#[cfg(test)]
#[path = "tests/paths.rs"]
mod tests;
