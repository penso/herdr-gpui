use std::{io, path::PathBuf};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the daemon is already running (pid {pid})")]
    AlreadyRunning { pid: u32 },
    #[error(transparent)]
    Approve(#[from] crate::approve::Error),
    #[error(transparent)]
    Automation(#[from] herdr_automation::Error),
    #[error(transparent)]
    Herdr(#[from] crate::herdr::Error),
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("no home directory; set HOME")]
    NoHome,
    #[error("{} is not in a personal or repository automations folder", .0.display())]
    NotAutomation(PathBuf),
    #[error("{0} is not approved on this host in its current form; run `herdr-automate approve`")]
    NotApproved(String),
    #[error("{} is not a git repository", .0.display())]
    NotRepository(PathBuf),
    #[error("run {0} is not running")]
    NotRunning(i64),
    #[error("no run {0}")]
    NoRun(i64),
    #[error("no script {} is known; is its repository added with `repo add`?", .0.display())]
    NoScript(PathBuf),
    #[error("the state database has schema {found}; this herdr-automate reads schema 1")]
    Schema { found: i64 },
    #[error(transparent)]
    Service(#[from] crate::service::Error),
    #[error(transparent)]
    Settings(#[from] crate::settings::Error),
    #[error("state database: {0}")]
    Store(#[from] rusqlite::Error),
    #[error("the daemon did not start within {seconds} seconds; see `herdr-automate log --daemon`")]
    StartTimeout { seconds: u64 },
}

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>) -> impl FnOnce(io::Error) -> Self {
        let path = path.into();
        move |source| Self::Io { path, source }
    }
}
