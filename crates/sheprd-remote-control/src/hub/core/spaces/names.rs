//! Automatic space names follow the first terminal's live folder and repository.

use std::{
    collections::HashMap,
    path::Path,
    time::{Duration, Instant},
};

#[derive(Default)]
pub(in crate::hub::core) struct Names {
    cached: HashMap<String, (Instant, String)>,
}

impl Names {
    pub(super) fn name(&mut self, folder: &str) -> String {
        if let Some((at, name)) = self.cached.get(folder)
            && at.elapsed() < Duration::from_secs(60)
        {
            return name.clone();
        }
        let name = name(folder);
        if self.cached.len() >= 256 {
            self.cached.clear();
        }
        self.cached
            .insert(folder.to_owned(), (Instant::now(), name.clone()));
        name
    }
}

fn name(folder: &str) -> String {
    let path = Path::new(folder);
    if !folder.is_empty()
        && let Ok(repo) = gix::discover(path)
    {
        let common = repo.common_dir();
        let root = if common.file_name().is_some_and(|name| name == ".git") {
            common.parent().unwrap_or(common)
        } else {
            common
        };
        if let Some(name) = root.file_name().and_then(|name| name.to_str()) {
            return name.strip_suffix(".git").unwrap_or(name).to_owned();
        }
    }
    let home = sheprd_client::login_env::var(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
    if folder.is_empty() || home.as_ref().is_some_and(|home| path == Path::new(home)) {
        return "~".to_owned();
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(folder)
        .to_owned()
}
