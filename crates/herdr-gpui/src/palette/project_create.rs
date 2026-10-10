//! Creating a new project inside a configured palette root, and opening it.
//!
//! The palette already opens an *existing* directory as a workspace
//! (`palette::project_open`). This supplies the missing verb: making the
//! directory. Nothing here invents a location — the new project goes inside a
//! `palette.project_roots` entry the user configured, which is the same
//! convention project discovery already uses.

use super::projects;
use crate::{Error, pull_request::run};
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

/// Generous: a large repository over a slow link is not an error.
const CLONE_TIMEOUT: Duration = Duration::from_secs(900);

/// What fills the new directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Source {
    /// An empty folder.
    Folder,
    /// `git clone <url>`.
    Clone { url: String },
}

/// A project the palette can create: where it goes, what it is called, and how
/// it is filled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NewProject {
    pub(super) path: PathBuf,
    pub(super) label: String,
    pub(super) source: Source,
}

/// One path segment that is safe to use as a folder name.
///
/// Deliberately narrow: a leading dot (hidden), a separator, or a colon (a
/// Windows drive or an scp-style remote) is refused rather than guessed at.
pub(super) fn parse_name(text: &str) -> Option<String> {
    let name = text.trim();
    if name.is_empty() || name.len() > 128 || name == "." || name == ".." {
        return None;
    }
    if name.starts_with('.') {
        return None;
    }
    if name
        .chars()
        .any(|ch| ch.is_control() || matches!(ch, '/' | '\\' | ':'))
    {
        return None;
    }
    Some(name.to_owned())
}

/// Recognise a repository reference, returning `(url, name)`.
///
/// Only explicit URLs: a bare `owner/repo` is refused because it would have to
/// pick a forge on the user's behalf.
pub(super) fn parse_repo(text: &str) -> Option<(String, String)> {
    let raw = text.trim();
    let explicit = ["https://", "http://", "ssh://", "git://"]
        .iter()
        .any(|prefix| raw.starts_with(prefix))
        || raw.starts_with("git@");
    if !explicit {
        return None;
    }
    let tail = raw.trim_end_matches('/').rsplit(['/', ':']).next()?;
    let name = tail.strip_suffix(".git").unwrap_or(tail);
    parse_name(name)?;
    Some((raw.to_owned(), name.to_owned()))
}

/// Plan a new project from the palette's input.
///
/// Returns `Ok(None)` when the input is not something we would create, so the
/// palette simply offers no row rather than showing an error while typing.
pub(super) fn plan(roots: &[String], text: &str) -> Result<Option<NewProject>, Error> {
    let Some(root) = roots.first() else {
        return Ok(None);
    };
    let root = projects::expand_root(root)?;
    let (label, source) = match parse_repo(text) {
        Some((url, name)) => (name, Source::Clone { url }),
        None => match parse_name(text) {
            Some(name) => (name, Source::Folder),
            None => return Ok(None),
        },
    };
    Ok(Some(NewProject {
        path: root.join(&label),
        label,
        source,
    }))
}

/// Make the directory, or clone into it. Blocking: callers run it off the UI thread.
pub(super) fn materialise(
    project: &NewProject,
    cancelled: &impl Fn() -> bool,
) -> Result<(), Error> {
    let Some(root) = project.path.parent() else {
        return Err(Error::PaletteProjectRootMissing);
    };
    if !root.is_dir() {
        return Err(Error::PaletteProjectRootMissing);
    }
    // Claim the destination atomically. This is the whole concurrency story:
    // whichever attempt creates the directory owns it, a second one fails here
    // instead of sharing it, and only the owner may remove it on failure. An
    // `exists()` check cannot do this — two windows can both pass it.
    std::fs::create_dir(&project.path).map_err(claim_error)?;
    let Source::Clone { url } = &project.source else {
        return Ok(());
    };
    let deadline = Instant::now() + CLONE_TIMEOUT;
    // `git clone` accepts a destination that already exists and is empty, which
    // is exactly the directory just claimed. Cloning straight into it, rather
    // than beside it and moving it, means there is no step that could replace
    // another window's folder.
    let mut command = clone_command(url, &project.path);
    let result = match run(&mut command, deadline, cancelled) {
        Ok((true, _)) => Ok(()),
        Ok((false, output)) => Err(Error::PaletteProjectClone(output.trim().to_owned())),
        // A timeout kills the child and lands here, so this arm cleans up too.
        Err(error) => Err(error),
    };
    if result.is_err() {
        // Only the directory this attempt claimed is removed.
        let _ = std::fs::remove_dir_all(&project.path);
    }
    result
}

/// An existing name is the ordinary case; anything else is a real failure.
fn claim_error(source: std::io::Error) -> Error {
    if source.kind() == std::io::ErrorKind::AlreadyExists {
        Error::PaletteProjectExists
    } else {
        Error::PaletteProjectCreate { source }
    }
}

/// The `git clone` invocation, kept apart so its arguments can be asserted
/// without a network or a repository.
pub(super) fn clone_command(url: &str, destination: &Path) -> Command {
    let mut command = Command::new("git");
    // The GUI has nowhere to show a credential prompt, so never block on one.
    command.env("GIT_TERMINAL_PROMPT", "0");
    command
        .arg("-c")
        .arg("core.fsmonitor=false")
        .arg("clone")
        // `--` so a URL that begins with a dash is treated as data.
        .arg("--")
        .arg(url)
        .arg(destination);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_folder_names() {
        assert_eq!(parse_name("my-project").as_deref(), Some("my-project"));
        assert_eq!(
            parse_name("  spaced name  ").as_deref(),
            Some("spaced name")
        );
        assert_eq!(parse_name("v2.0").as_deref(), Some("v2.0"));
    }

    #[test]
    fn refuses_names_that_are_not_one_segment() {
        for bad in ["", "   ", ".", "..", ".hidden", "a/b", "a\\b", "a:b"] {
            assert_eq!(parse_name(bad), None, "should refuse {bad:?}");
        }
    }

    #[test]
    fn recognises_explicit_repository_urls() {
        assert_eq!(
            parse_repo("https://github.com/owner/repo.git"),
            Some(("https://github.com/owner/repo.git".into(), "repo".into()))
        );
        assert_eq!(
            parse_repo("git@github.com:owner/repo.git"),
            Some(("git@github.com:owner/repo.git".into(), "repo".into()))
        );
        assert_eq!(
            parse_repo("ssh://git@host/owner/repo"),
            Some(("ssh://git@host/owner/repo".into(), "repo".into()))
        );
    }

    #[test]
    fn refuses_a_bare_owner_repo() {
        // Choosing a forge for the user is not ours to do.
        assert_eq!(parse_repo("owner/repo"), None);
        assert_eq!(parse_repo("just-a-name"), None);
    }

    #[test]
    fn plans_a_folder_in_the_first_root() -> anyhow::Result<()> {
        let root = std::env::var("HOME")?;
        let Some(plan) = plan(std::slice::from_ref(&root), "brand-new")? else {
            anyhow::bail!("expected a plan for a plain name");
        };
        assert_eq!(plan.path, PathBuf::from(&root).join("brand-new"));
        assert_eq!(plan.label, "brand-new");
        assert_eq!(plan.source, Source::Folder);
        Ok(())
    }

    #[test]
    fn plans_a_clone_and_names_it_after_the_repository() -> anyhow::Result<()> {
        let root = std::env::var("HOME")?;
        let Some(plan) = plan(
            std::slice::from_ref(&root),
            "https://github.com/owner/thing.git",
        )?
        else {
            anyhow::bail!("expected a plan for a repository URL");
        };
        assert_eq!(plan.path, PathBuf::from(root).join("thing"));
        assert_eq!(
            plan.source,
            Source::Clone {
                url: "https://github.com/owner/thing.git".into()
            }
        );
        Ok(())
    }

    #[test]
    fn plans_nothing_without_a_root() -> anyhow::Result<()> {
        assert_eq!(plan(&[], "anything")?, None);
        Ok(())
    }

    #[test]
    fn the_clone_command_passes_url_and_destination_as_data() {
        let command = clone_command("https://example.test/a.git", Path::new("/tmp/a"));
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            vec![
                "-c",
                "core.fsmonitor=false",
                "clone",
                "--",
                "https://example.test/a.git",
                "/tmp/a"
            ]
        );
        let prompt = command
            .get_envs()
            .find(|(key, _)| *key == "GIT_TERMINAL_PROMPT")
            .and_then(|(_, value)| value)
            .map(|value| value.to_string_lossy().into_owned());
        assert_eq!(prompt.as_deref(), Some("0"));
    }

    #[test]
    fn a_second_project_of_the_same_name_is_refused() -> anyhow::Result<()> {
        let directory = std::env::temp_dir().join(format!("herdr-plan-{}", std::process::id()));
        std::fs::create_dir_all(&directory)?;
        let project = NewProject {
            path: directory.clone(),
            label: "taken".into(),
            source: Source::Folder,
        };
        assert!(matches!(
            materialise(&project, &|| false),
            Err(Error::PaletteProjectExists)
        ));
        std::fs::remove_dir_all(&directory)?;
        Ok(())
    }
}
