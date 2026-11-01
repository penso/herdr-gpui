//! The SSH key machine sync gives every machine it joins, offered by this
//! client's own `ssh` children beside the user's keys. The user's SSH config
//! is never edited, so a plain `ssh` in a terminal does not offer it.
use std::path::{Path, PathBuf};

/// The key's file name in `~/.ssh`, on every machine.
pub const KEY_NAME: &str = "herdr_ed25519";

/// OpenSSH's own default identities. Naming any `IdentityFile` on the command
/// line stops OpenSSH from trying these, so the ones present are named too.
const DEFAULTS: [&str; 6] = [
    "id_rsa",
    "id_ecdsa",
    "id_ecdsa_sk",
    "id_ed25519",
    "id_ed25519_sk",
    "id_xmss",
];

/// `~/.ssh/herdr_ed25519` on this machine, or `None` without a home.
pub fn key_path() -> Option<PathBuf> {
    crate::local_path::home_dir().map(|home| home.join(".ssh").join(KEY_NAME))
}

/// The `-o IdentityFile=` options for the herdr key, after the default keys
/// present, or none while there is no herdr key: then OpenSSH picks its
/// identities exactly as it always has.
pub(crate) fn options() -> Vec<String> {
    let Some(key) = key_path() else {
        return Vec::new();
    };
    options_for(&key, Path::exists)
}

fn options_for(key: &Path, exists: impl Fn(&Path) -> bool) -> Vec<String> {
    if !exists(key) {
        return Vec::new();
    }
    let Some(directory) = key.parent() else {
        return Vec::new();
    };
    DEFAULTS
        .iter()
        .map(|name| directory.join(name))
        .filter(|path| exists(path))
        .chain(std::iter::once(key.to_owned()))
        .filter_map(|path| identity_option(&path))
        .flat_map(|option| ["-o".to_owned(), option])
        .collect()
}

/// One `IdentityFile` option, quoted as an SSH config value, with `%` escaped
/// from token expansion. A path that cannot be quoted is left out.
fn identity_option(path: &Path) -> Option<String> {
    let path = path.to_str()?;
    if path.contains('"') || path.chars().any(char::is_control) {
        return None;
    }
    Some(format!("IdentityFile=\"{}\"", path.replace('%', "%%")))
}

#[cfg(test)]
mod tests;
