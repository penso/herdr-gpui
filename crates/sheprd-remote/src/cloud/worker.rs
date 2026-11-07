//! The background workers a cloud error can name as stopped.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Worker {
    // keep-sorted start
    Destroy,
    ImageBuild,
    ImageReset,
    Job,
    Settings,
    Setup,
    SignIn,
    // keep-sorted end
}

impl fmt::Display for Worker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            // keep-sorted start
            Self::Destroy => "destroy",
            Self::ImageBuild => "image build",
            Self::ImageReset => "image reset",
            Self::Job => "job",
            Self::Settings => "settings",
            Self::Setup => "setup",
            Self::SignIn => "sign-in",
            // keep-sorted end
        })
    }
}
