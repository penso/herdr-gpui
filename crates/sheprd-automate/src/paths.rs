//! Where the daemon keeps its state and finds personal scripts. Everything
//! lives under the user's home; `HERDR_AUTOMATE_HOME` moves all of it, so
//! tests never touch a real installation. The user service is given the
//! directories the command that installed it used, as
//! `HERDR_AUTOMATE_STATE_DIR` and `HERDR_AUTOMATE_CONFIG_DIR`, so the daemon
//! reads what the user's shell does without that shell's environment.

use crate::{Error, Result};
use std::{
    env,
    path::{Path, PathBuf},
};

/// The state directory itself, as the user service is given it.
pub const STATE_DIR: &str = "HERDR_AUTOMATE_STATE_DIR";
/// The config directory itself, as the user service is given it.
pub const CONFIG_DIR: &str = "HERDR_AUTOMATE_CONFIG_DIR";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    /// Runtime state: the database, the lock, logs, run output.
    pub state: PathBuf,
    /// The user's settings and personal scripts.
    pub config: PathBuf,
}

impl Paths {
    pub fn from_env() -> Result<Self> {
        Self::from_vars(|name| {
            env::var_os(name)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        })
    }

    /// As `from_env`, reading set, non-empty variables through `var`.
    fn from_vars(var: impl Fn(&str) -> Option<PathBuf>) -> Result<Self> {
        if let (Some(state), Some(config)) = (var(STATE_DIR), var(CONFIG_DIR)) {
            return Ok(Self { state, config });
        }
        if let Some(root) = var("HERDR_AUTOMATE_HOME") {
            return Ok(Self::under(&root));
        }
        let home = var("HOME")
            .or_else(|| var("USERPROFILE"))
            .ok_or(Error::NoHome)?;
        if cfg!(windows) {
            let local = var("LOCALAPPDATA").unwrap_or_else(|| home.join("AppData").join("Local"));
            let roaming = var("APPDATA").unwrap_or_else(|| home.join("AppData").join("Roaming"));
            return Ok(Self {
                state: local.join("herdr-automate"),
                config: roaming.join("herdr-automate"),
            });
        }
        let state = var("XDG_STATE_HOME").unwrap_or_else(|| home.join(".local").join("state"));
        let config = var("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"));
        Ok(Self {
            state: state.join("herdr-automate"),
            config: config.join("herdr-automate"),
        })
    }

    /// Both directories under one root, for tests and `HERDR_AUTOMATE_HOME`.
    pub fn under(root: &Path) -> Self {
        Self {
            state: root.join("state"),
            config: root.join("config"),
        }
    }

    pub fn database(&self) -> PathBuf {
        self.state.join("state.sqlite")
    }

    pub fn lock(&self) -> PathBuf {
        self.state.join("daemon.lock")
    }

    pub fn log(&self) -> PathBuf {
        self.state.join("daemon.log")
    }

    /// Step output, one directory per run.
    pub fn runs(&self) -> PathBuf {
        self.state.join("runs")
    }

    pub fn run_dir(&self, run: i64) -> PathBuf {
        self.runs().join(run.to_string())
    }

    pub fn step_log(&self, run: i64, position: u32) -> PathBuf {
        self.run_dir(run).join(format!("step-{position}.log"))
    }

    /// Personal scripts, which run against no repository.
    pub fn personal(&self) -> PathBuf {
        self.config.join("automations")
    }

    pub fn settings(&self) -> PathBuf {
        self.config.join("config.toml")
    }
}

/// Creates `dir` and its parents, readable only by the user: step output,
/// the database and logs can hold secrets. Tightens a directory that
/// already exists.
pub fn private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Options for a file only the user may read, when it is created.
pub fn private_file() -> std::fs::OpenOptions {
    #[cfg(unix)]
    {
        let mut options = std::fs::OpenOptions::new();
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        options
    }
    #[cfg(not(unix))]
    std::fs::OpenOptions::new()
}

/// Where a repository keeps its scripts.
pub fn repository_scripts(repo: &Path) -> PathBuf {
    repo.join(".herdr").join("automations")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
