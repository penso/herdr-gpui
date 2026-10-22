//! What a copy's saves lead to: the rules for asking, uploading, and
//! resolving a conflict, as one pure step from a copy's state, its upload
//! policy, and what just happened to what it does next. Nothing is pushed
//! unless the user chose Upload or Always for the copy, and nothing replaces
//! a host's changed file unless they chose Overwrite anyway.

/// A SHA-256 digest.
pub(crate) type Sha256 = [u8; 32];

/// Whether a save asks before it is uploaded.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Policy {
    #[default]
    Ask,
    /// The user chose "Upload future saves without asking".
    Always,
}

/// Where a copy stands against the host's file. Each state after a save
/// holds the hash of that save.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum SyncState {
    /// The copy holds the bytes the host had when last fetched or pushed.
    #[default]
    Synced,
    /// A save waits for the next one: its upload failed.
    Edited { sha256: Sha256 },
    /// The upload question for a save is open or waiting to open.
    Asking { sha256: Sha256 },
    /// A push of the copy is running.
    Uploading { sha256: Sha256 },
    /// The host's file is being fetched over the copy.
    Refetching { sha256: Sha256 },
    /// The host's file changed since it was synced, so the save was not
    /// uploaded.
    Conflict { sha256: Sha256 },
    /// The user said "Not now" to this save.
    Declined { sha256: Sha256 },
}

/// The upload question's answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Answer {
    Upload,
    Always,
    NotNow,
}

/// The conflict question's answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Resolution {
    Overwrite,
    Refetch,
    KeepBoth,
}

/// Something that happened to a copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Event {
    /// A settled save whose bytes differ from the synced ones.
    Saved(Sha256),
    /// A settled save whose bytes are the synced ones again.
    Reverted,
    Answered(Answer),
    Resolved(Resolution),
    /// A push to the host's own path succeeded.
    Uploaded,
    /// The push beside the host's file succeeded.
    KeptBoth,
    /// The host's file was not what the push expected.
    Conflicted,
    /// The push failed for another reason.
    Failed,
    RefetchFailed,
}

/// Where a push goes and what it requires of the host's file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Push {
    /// The host's path, still holding the synced bytes.
    Expected,
    /// The host's path, whatever it holds.
    Any,
    /// The `n`th free conflict name beside the host's path.
    Beside(u8),
}

/// What to do after a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    Nothing,
    /// Queue the upload question.
    Ask,
    /// Queue the conflict question.
    ShowConflict,
    Push(Push),
    Refetch,
}

/// A copy's state, policy, and next action after `event`.
pub(crate) fn step(state: SyncState, policy: Policy, event: Event) -> (SyncState, Policy, Action) {
    use SyncState::*;
    let policy = match (state, event) {
        (Asking { .. }, Event::Answered(Answer::Always)) => Policy::Always,
        _ => policy,
    };
    let (state, action) = match (state, event) {
        (Uploading { .. } | Refetching { .. }, Event::Saved(_) | Event::Reverted) => {
            (state, Action::Nothing)
        }
        (Asking { .. }, Event::Saved(sha256)) => (Asking { sha256 }, Action::Nothing),
        (Declined { sha256: declined }, Event::Saved(sha256)) if declined == sha256 => {
            (state, Action::Nothing)
        }
        // The host still differs from what the copy was synced against, so
        // the conflict question comes back rather than another push.
        (Conflict { .. }, Event::Saved(sha256)) => (Conflict { sha256 }, Action::ShowConflict),
        (_, Event::Saved(sha256)) => saved(policy, sha256),
        // The copy holds no edits again: a question still waiting or open
        // about them, including Overwrite anyway, no longer applies.
        (Edited { .. } | Asking { .. } | Declined { .. } | Conflict { .. }, Event::Reverted) => {
            (Synced, Action::Nothing)
        }
        (Asking { sha256 }, Event::Answered(Answer::Upload | Answer::Always)) => {
            (Uploading { sha256 }, Action::Push(Push::Expected))
        }
        (Asking { sha256 }, Event::Answered(Answer::NotNow)) => {
            (Declined { sha256 }, Action::Nothing)
        }
        (Conflict { sha256 }, Event::Resolved(Resolution::Overwrite)) => {
            (Uploading { sha256 }, Action::Push(Push::Any))
        }
        (Conflict { sha256 }, Event::Resolved(Resolution::Refetch)) => {
            (Refetching { sha256 }, Action::Refetch)
        }
        (Conflict { sha256 }, Event::Resolved(Resolution::KeepBoth)) => {
            (Uploading { sha256 }, Action::Push(Push::Beside(1)))
        }
        (Uploading { .. }, Event::Uploaded) => (Synced, Action::Nothing),
        (Uploading { sha256 }, Event::KeptBoth) => (Declined { sha256 }, Action::Nothing),
        (Uploading { sha256 }, Event::Conflicted) => (Conflict { sha256 }, Action::ShowConflict),
        (Uploading { sha256 }, Event::Failed) => (Edited { sha256 }, Action::Nothing),
        // The conflict stands, so its question comes back for another try.
        (Refetching { sha256 }, Event::RefetchFailed) => {
            (Conflict { sha256 }, Action::ShowConflict)
        }
        _ => (state, Action::Nothing),
    };
    (state, policy, action)
}

fn saved(policy: Policy, sha256: Sha256) -> (SyncState, Action) {
    match policy {
        Policy::Ask => (SyncState::Asking { sha256 }, Action::Ask),
        Policy::Always => (
            SyncState::Uploading { sha256 },
            Action::Push(Push::Expected),
        ),
    }
}

/// The host's path for the copy kept beside `path` when both are kept: the
/// name's stem, `.conflict` (`.conflict-2` and on for `n` past one), then
/// its extension, split at the last dot. A name whose only dot leads it,
/// such as `.bashrc`, has no extension. Folders split at the host's own
/// separators: a backslash is a plain name character on a POSIX host.
pub(crate) fn conflict_path(path: &str, shell: herdr_client::RemoteShell, n: u8) -> String {
    let separators: &[char] = match shell {
        herdr_client::RemoteShell::Posix => &['/'],
        herdr_client::RemoteShell::PowerShell => &['/', '\\'],
    };
    let name_at = path.rfind(separators).map_or(0, |at| at + 1);
    let (folder, name) = path.split_at(name_at);
    let (stem, extension) = match name.rfind('.') {
        Some(dot) if dot > 0 && dot + 1 < name.len() => name.split_at(dot),
        _ => (name, ""),
    };
    let suffix = if n <= 1 {
        ".conflict".to_owned()
    } else {
        format!(".conflict-{n}")
    };
    format!("{folder}{stem}{suffix}{extension}")
}
