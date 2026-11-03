//! VS Code's machine settings for the servers the app starts. VS Code's web
//! build is light until told otherwise, so a server with no machine settings
//! yet gets `window.autoDetectColorScheme`, which makes it follow the page's
//! light or dark; the app sets that from its theme (see
//! `browser::page::Page::set_light`). A file with anything in it is the
//! user's and is never changed. `host/serve.sh` does the same on a host.
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

/// What a server with no machine settings gets.
pub(super) const SETTINGS: &str = "{\n\t\"window.autoDetectColorScheme\": true\n}\n";

/// Where `code serve-web` keeps machine settings under `home`, by default.
pub(super) fn path(home: &Path) -> PathBuf {
    home.join(".vscode-server/data/Machine/settings.json") // i18n: data
}

/// Writes [`SETTINGS`] to `path` when there is no file or it is empty.
/// Returns whether it wrote them. Blocking: for the server's worker.
pub(super) fn seed(path: &Path) -> io::Result<bool> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.len() > 0 => return Ok(false),
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(SETTINGS.as_bytes())?;
    Ok(true)
}

#[cfg(test)]
mod tests;
