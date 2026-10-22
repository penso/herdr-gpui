//! Opens a URL in the user's default web browser by starting the browser
//! itself with the URL as one argument. The system opener drops the query
//! and fragment of a `file://` URL; a browser started this way keeps them.
//!
//! On Windows the browser's command comes from the `https` (then `http`)
//! URL association in the registry, and [`command_line`] fills the URL into
//! it. On macOS it is the executable of the application Launch Services
//! picks for an `https` URL; Safari ignores a URL passed this way, so it is
//! refused as [`Error::Unsupported`], as is every other platform. Callers
//! fall back to the system opener on any error: this module never does.

#[cfg(target_os = "macos")]
mod launch_services;
#[cfg(target_os = "macos")]
use launch_services as platform;
#[cfg(windows)]
mod registry;
#[cfg(windows)]
use registry as platform;

#[cfg(test)]
mod tests;

#[cfg(any(windows, target_os = "macos"))]
use std::path::Path;
#[cfg(any(windows, target_os = "macos", test))]
use std::path::PathBuf;

pub(crate) type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    /// The default browser's application bundle has no executable to start.
    #[cfg(target_os = "macos")]
    #[error("The default browser at {} has no executable to start.", app.display())]
    BrowserBundle { app: PathBuf },
    #[cfg(any(windows, target_os = "macos"))]
    #[error("No default web browser is set.")]
    NoDefaultBrowser,
    #[cfg(windows)]
    #[error("Could not read the registry key {key}: {source}")]
    Registry {
        key: String,
        #[source]
        source: windows_result::Error,
    },
    #[cfg(any(windows, target_os = "macos"))]
    #[error("Could not start the default browser {}: {source}", program.display())]
    Spawn {
        program: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[cfg(any(windows, test))]
    #[error("The default browser's command line could not be read: {command}")]
    UnparsableCommand { command: String },
    #[cfg(not(windows))]
    #[error("The default browser cannot be started with a URL on this platform.")]
    Unsupported,
    // keep-sorted end
}

/// Start the default browser on `url`, without waiting for it to exit.
/// Blocking: it reads the browser's registration and starts a process, so
/// run it on the background executor.
// A test window never starts a real browser (see `window::start_browser`),
// so nothing calls this under test.
#[cfg_attr(test, allow(dead_code))]
pub(crate) fn open(url: &url::Url) -> Result<()> {
    platform::open(url.as_str())
}

/// No other platform can start its default browser with a URL intact.
#[cfg(not(any(windows, target_os = "macos")))]
mod platform {
    pub(super) fn open(_: &str) -> super::Result<()> {
        Err(super::Error::Unsupported)
    }
}

/// Split a Windows `shell\open\command` `template` into the program and its
/// arguments with `CommandLineToArgvW`'s rules, then put `url` in place of
/// each `%1`, `%l` or `%L`, dropping a `%*` argument. With no placeholder
/// the URL becomes the last argument. The URL is always inside one argument,
/// whatever quotes or spaces it holds.
#[cfg(any(windows, test))]
pub(super) fn command_line(template: &str, url: &str) -> Result<(PathBuf, Vec<String>)> {
    let unparsable = || Error::UnparsableCommand {
        command: template.to_owned(),
    };
    let (program, rest) = split_program(template).ok_or_else(unparsable)?;
    let tokens = split_arguments(rest).ok_or_else(unparsable)?;
    let mut placed = false;
    let mut args: Vec<String> = tokens
        .into_iter()
        .filter(|token| token != "%*")
        .map(|token| {
            let (arg, replaced) = fill_placeholders(&token, url);
            placed |= replaced;
            arg
        })
        .collect();
    if !placed {
        args.push(url.to_owned());
    }
    Ok((PathBuf::from(program), args))
}

/// The program is the first token: a quoted one ends at the next quote with
/// no escapes, an unquoted one at the first whitespace.
#[cfg(any(windows, test))]
fn split_program(template: &str) -> Option<(&str, &str)> {
    let template = template.trim_start_matches(is_blank);
    let (program, rest) = match template.strip_prefix('"') {
        Some(quoted) => quoted.split_once('"')?,
        None => template.split_once(is_blank).unwrap_or((template, "")),
    };
    (!program.is_empty()).then_some((program, rest))
}

/// Split the arguments after the program: whitespace outside quotes ends an
/// argument, `2n` backslashes before a quote give `n` backslashes and toggle
/// quoting, `2n+1` give `n` and a literal quote, and other backslashes are
/// literal. An unterminated quote is refused.
#[cfg(any(windows, test))]
fn split_arguments(rest: &str) -> Option<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quoted = false;
    let mut backslashes = 0;
    for character in rest.chars() {
        if character == '\\' {
            backslashes += 1;
            started = true;
            continue;
        }
        if character == '"' {
            current.extend(std::iter::repeat_n('\\', backslashes / 2));
            if backslashes % 2 == 1 {
                current.push('"');
            } else {
                quoted = !quoted;
            }
            backslashes = 0;
            started = true;
            continue;
        }
        current.extend(std::iter::repeat_n('\\', backslashes));
        backslashes = 0;
        if is_blank(character) && !quoted {
            if started {
                args.push(std::mem::take(&mut current));
                started = false;
            }
            continue;
        }
        current.push(character);
        started = true;
    }
    if quoted {
        return None;
    }
    current.extend(std::iter::repeat_n('\\', backslashes));
    if started {
        args.push(current);
    }
    Some(args)
}

/// Replace every `%1`, `%l` and `%L` in `token` with `url` in one pass, so a
/// placeholder-like sequence inside the URL is never replaced in turn.
#[cfg(any(windows, test))]
fn fill_placeholders(token: &str, url: &str) -> (String, bool) {
    let mut filled = String::with_capacity(token.len());
    let mut replaced = false;
    let mut characters = token.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '%' && matches!(characters.peek(), Some('1' | 'l' | 'L')) {
            characters.next();
            filled.push_str(url);
            replaced = true;
        } else {
            filled.push(character);
        }
    }
    (filled, replaced)
}

#[cfg(any(windows, test))]
fn is_blank(character: char) -> bool {
    character == ' ' || character == '\t'
}

/// Start `program` with `args` and reap it on a thread of its own, so a
/// browser that hands the URL to a running instance and exits is not left
/// as a zombie.
#[cfg(any(windows, target_os = "macos"))]
fn spawn(program: &Path, args: &[String]) -> Result<()> {
    let mut child = herdr_client::new_command(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|source| Error::Spawn {
            program: program.to_owned(),
            source,
        })?;
    let reaper = std::thread::Builder::new()
        .name("herdr-browser-reap".into())
        .spawn(move || {
            // The browser's exit status says nothing about whether the URL
            // opened, and the caller has already moved on; only a failed
            // wait is worth a log line.
            if let Err(error) = child.wait() {
                tracing::warn!(%error, "could not wait for the default browser process");
            }
        });
    if let Err(error) = reaper {
        tracing::warn!(%error, "could not start the thread that reaps the default browser process");
    }
    Ok(())
}
