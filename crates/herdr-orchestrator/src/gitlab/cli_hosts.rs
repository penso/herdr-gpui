//! The GitLab CLI's `hosts` table, read for where an instance's API is when
//! its remotes use another SSH host: glab's `ssh_host`, `api_host`, and
//! `api_protocol` for each instance it is signed in to.
//!
//! glab keeps them in YAML. Only the block form glab writes is read, one
//! `key: value` per line under each host; anything else is skipped, and a
//! remote it does not map keeps its own host as before.

use std::{
    ffi::OsString,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

/// A few instances take a few kilobytes; the rest is room for comments.
const LIMIT: u64 = 256 * 1024;

/// One instance in glab's `hosts` table, with the fields that place its API.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CliHost {
    /// The instance's name, which glab saves its token under.
    pub host: String,
    pub api_host: Option<String>,
    pub api_protocol: Option<String>,
    pub ssh_host: Option<String>,
}

/// glab's config file, where glab looks for it: `GLAB_CONFIG_DIR`, else the
/// legacy `~/.config/glab-cli` when its file exists, else `glab-cli` in the
/// platform's config folder, which `XDG_CONFIG_HOME` overrides.
pub fn config_path(
    variable: impl Fn(&str) -> Option<OsString>,
    home: Option<&Path>,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let set = |name| variable(name).filter(|value| !value.is_empty());
    if let Some(dir) = set("GLAB_CONFIG_DIR") {
        return Some(PathBuf::from(dir).join("config.yml"));
    }
    let legacy = home.map(|home| home.join(".config/glab-cli/config.yml"));
    if let Some(legacy) = legacy.filter(|path| exists(path)) {
        return Some(legacy);
    }
    let root = match set("XDG_CONFIG_HOME") {
        Some(root) => PathBuf::from(root),
        None if cfg!(target_os = "macos") => home?.join("Library/Application Support"),
        None if cfg!(windows) => PathBuf::from(set("LOCALAPPDATA")?),
        None => home?.join(".config"),
    };
    Some(root.join("glab-cli/config.yml"))
}

/// The instances glab's config on this machine lists, or none when it has
/// no readable one. The variables come from the login shell, as they do for
/// the `glab` that `cli_token` runs, so a launch from Finder or a
/// desktop entry finds the same file. Reads a file and may probe the shell:
/// call it from a worker.
pub fn read() -> Vec<CliHost> {
    let home = herdr_client::login_env::var("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .or_else(herdr_client::local_path::home_dir);
    let Some(path) = config_path(herdr_client::login_env::var, home.as_deref(), Path::is_file)
    else {
        return Vec::new();
    };
    let mut text = String::new();
    let read = File::open(&path).and_then(|file| file.take(LIMIT + 1).read_to_string(&mut text));
    match read {
        Ok(size) if size as u64 <= LIMIT => parse(&text),
        Ok(_) => Vec::new(),
        Err(error) => {
            tracing::debug!(
                category = "orchestrator_gitlab",
                %error,
                "The GitLab CLI's config could not be read"
            );
            Vec::new()
        }
    }
}

/// The `hosts` table of glab's config. Each host is a key one level in, and
/// its fields the `key: value` lines one level further; nested lists such as
/// `custom_headers` are deeper and skipped.
pub fn parse(text: &str) -> Vec<CliHost> {
    let mut hosts = Vec::new();
    let mut in_hosts = false;
    // The indentation of host names and of their fields, from their first line.
    let mut host_indent = None;
    let mut field_indent = None;
    // Whether the last host name began a block whose fields belong to it.
    let mut open = false;
    for line in text.lines() {
        let content = line.trim_start_matches(' ');
        let indent = line.len() - content.len();
        let content = content.trim_end();
        if content.is_empty() || content.starts_with('#') {
            continue;
        }
        let entry = entry(content);
        if indent == 0 {
            in_hosts = matches!(&entry, Some((key, None)) if key == "hosts");
            continue;
        }
        let (true, Some((key, value))) = (in_hosts, entry) else {
            continue;
        };
        let host_level = *host_indent.get_or_insert(indent);
        if indent <= host_level {
            open = indent == host_level && value.is_none();
            if open {
                hosts.push(CliHost {
                    host: key,
                    ..CliHost::default()
                });
            }
            continue;
        }
        let (true, Some(host)) = (open, hosts.last_mut()) else {
            continue;
        };
        if indent != *field_indent.get_or_insert(indent) {
            continue;
        }
        match key.as_str() {
            "api_host" => host.api_host = value,
            "api_protocol" => host.api_protocol = value,
            "ssh_host" => host.ssh_host = value,
            _ => {}
        }
    }
    hosts
}

/// A mapping line's key and its scalar value, if it has one. A list item or
/// a line without a `key:` is none.
fn entry(content: &str) -> Option<(String, Option<String>)> {
    if content.starts_with('-') {
        return None;
    }
    let (key, rest) = match content.chars().next()? {
        quote @ ('"' | '\'') => {
            let (key, rest) = content[1..].split_once(quote)?;
            (key, rest.strip_prefix(':')?)
        }
        // A host may carry a port, so only a colon that ends the line or is
        // followed by a space ends the key.
        _ => match content.split_once(": ") {
            Some((key, rest)) => (key, rest),
            None => (content.strip_suffix(':')?, ""),
        },
    };
    let key = key.trim();
    (!key.is_empty()).then(|| (key.to_owned(), scalar(rest)))
}

/// A plain or quoted scalar, without a trailing comment; none when empty.
fn scalar(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let value = match raw.chars().next()? {
        quote @ ('"' | '\'') => raw[1..].split_once(quote)?.0,
        '#' => return None,
        _ => raw
            .split_once(" #")
            .map_or(raw, |(value, _)| value)
            .trim_end(),
    };
    (!value.is_empty()).then(|| value.to_owned())
}

/// The instance glab reaches over SSH at `remote_host` when that is not an
/// instance of its own: the one whose `ssh_host` it is.
pub fn by_ssh_host<'a>(hosts: &'a [CliHost], remote_host: &str) -> Option<&'a CliHost> {
    if hosts
        .iter()
        .any(|host| host.host.eq_ignore_ascii_case(remote_host))
    {
        return None;
    }
    let remote = without_port(remote_host);
    hosts.iter().find(|host| {
        host.ssh_host
            .as_deref()
            .is_some_and(|ssh| without_port(ssh).eq_ignore_ascii_case(remote))
    })
}

/// `host` without a `:port`; a bracketed IPv6 address keeps its colons.
fn without_port(host: &str) -> &str {
    match host.strip_prefix('[') {
        Some(rest) => rest.find(']').map_or(host, |end| &host[..end + 2]),
        None => match host.split_once(':') {
            Some((name, port)) if !port.contains(':') => name,
            _ => host,
        },
    }
}
