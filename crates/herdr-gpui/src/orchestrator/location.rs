//! Where a repository's shared database lives: agent-launcher's
//! `repositories/<uuid v5>/state.sqlite3` under the platform data directory.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};
use uuid::Uuid;

/// A repository the database is keyed by, through its git common directory.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Repository {
    /// The canonicalized `git rev-parse --git-common-dir` on this machine.
    Local(PathBuf),
    /// A repository on an SSH host. agent-launcher never derives these keys,
    /// so they are this machine's own.
    Ssh {
        destination: String,
        git_dir: String,
    },
}

impl Repository {
    /// The text hashed into the directory name.
    fn identity(&self) -> String {
        match self {
            Self::Local(git_dir) => git_dir.to_string_lossy().into_owned(),
            Self::Ssh {
                destination,
                git_dir,
            } => format!("ssh://{destination}{git_dir}"),
        }
    }

    pub(crate) fn id(&self) -> Uuid {
        Uuid::new_v5(&Uuid::NAMESPACE_URL, self.identity().as_bytes())
    }
}

/// The database file for `repository`, or `None` without a data directory.
pub(crate) fn database_path(repository: &Repository) -> Option<PathBuf> {
    Some(database_path_in(&data_local_dir()?, repository))
}

pub(super) fn database_path_in(data: &Path, repository: &Repository) -> PathBuf {
    data.join("agent-launcher")
        .join("repositories")
        .join(repository.id().hyphenated().to_string())
        .join("state.sqlite3")
}

fn data_local_dir() -> Option<PathBuf> {
    data_local_dir_with(|name| std::env::var_os(name))
}

/// The `dirs::data_local_dir()` agent-launcher uses, from injected variables.
pub(super) fn data_local_dir_with(var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let var = |name| {
        var(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    if cfg!(target_os = "macos") {
        return var("HOME").map(|home| home.join("Library").join("Application Support"));
    }
    if cfg!(windows) {
        return var("LOCALAPPDATA");
    }
    var("XDG_DATA_HOME")
        .filter(|path| path.is_absolute())
        .or_else(|| var("HOME").map(|home| home.join(".local").join("share")))
}
