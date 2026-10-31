//! What an upgrade in progress is doing. The updater's own steps are
//! variants, so the app can word them in the user's language; a line a
//! package manager printed is data and is shown as it came.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// Linux: the package transaction waits for the system's
    /// authentication prompt.
    Approve,
    /// Homebrew is asked to upgrade the cask, the first time.
    Asking,
    /// Homebrew's installed version is read back.
    Checking,
    /// A line Homebrew printed, bounded and stripped of control bytes.
    Homebrew(String),
    /// A line the system package transaction printed, bounded and stripped
    /// of control bytes.
    Package(String),
    /// The release is being installed, before the package manager starts.
    Preparing,
    /// Homebrew's metadata is refreshed with `brew update`.
    Refreshing,
    /// Homebrew is asked again after the refresh.
    Retrying,
    /// Homebrew has been asked to start, before its first line.
    Starting,
}

impl Progress {
    /// A line the Linux install helper printed: the helper reports its
    /// steps through [`fmt::Display`], so the one it names is parsed back.
    #[cfg(any(target_os = "linux", test))]
    pub(crate) fn from_helper(line: String) -> Self {
        if line == Self::Approve.to_string() {
            Self::Approve
        } else {
            Self::Package(line)
        }
    }
}

/// English, for the command line and the log.
impl fmt::Display for Progress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Approve => {
                f.write_str("Approve the system authentication prompt to install the package...")
            }
            Self::Asking => f.write_str("Asking Homebrew to upgrade the cask..."),
            Self::Checking => f.write_str("Checking the installed Homebrew cask version..."),
            Self::Homebrew(line) => write!(f, "Homebrew: {line}"),
            Self::Package(line) => f.write_str(line),
            Self::Preparing => f.write_str("Preparing installation..."),
            Self::Refreshing => f.write_str("Refreshing Homebrew metadata with brew update..."),
            Self::Retrying => f.write_str("Retrying Homebrew cask upgrade after refresh..."),
            Self::Starting => f.write_str("Starting Homebrew..."),
        }
    }
}
