//! Where a browser tab's page comes from: the web, or a local file an agent
//! wrote; or, for a review tab, the checkout whose changes it shows. Pages cannot open `file:` addresses, so a local file is served
//! from its folder through a private scheme, and only from that folder.
use super::WebUrl;
use crate::i18n::fl;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

#[cfg(any(target_os = "macos", windows))]
/// The private scheme local pages load through. On Windows, WebView2 spells
/// it `http://herdr-preview.localhost/`.
pub(crate) const PREVIEW_SCHEME: &str = "herdr-preview";
#[cfg(any(target_os = "macos", windows, test))]
const PREVIEW_ORIGINS: [&str; 3] = [
    "herdr-preview://localhost/",
    "http://herdr-preview.localhost/",
    "https://herdr-preview.localhost/",
];

/// The frame a page's message came from, by its address. wry gives every
/// frame of every page `window.ipc`, so only this tells a local page's own
/// script from a web page it embeds or is navigating to.
#[cfg(any(target_os = "macos", windows, test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PostedFrom {
    /// A frame showing a local file through the preview scheme.
    Preview,
    /// Any other frame: a web page, `about:` frames, or an unknown address.
    Elsewhere,
}

#[cfg(any(target_os = "macos", windows, test))]
impl PostedFrom {
    pub(crate) fn of(frame_url: &str) -> Self {
        if PREVIEW_ORIGINS
            .iter()
            .any(|origin| frame_url.starts_with(origin))
        {
            Self::Preview
        } else {
            Self::Elsewhere
        }
    }
}

#[cfg(any(target_os = "macos", windows, test))]
/// Everything but unreserved characters is escaped in a path segment.
const SEGMENT: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Location {
    Web {
        url: WebUrl,
    },
    Local {
        file: LocalFile,
    },
    /// A review of a checkout's changes, drawn by the app, never a page.
    Review {
        checkout: ReviewCheckout,
    },
    /// A repository's issues, pull requests, and agent runs, drawn by the
    /// app, never a page.
    Orchestrator {
        repo: OrchestratorRepo,
    },
    /// A local source file, drawn read-only by the app, never a page.
    Code {
        file: crate::code_view::CodeFile,
    },
    /// Every device this window connects to, drawn by the app, never a page.
    Devices,
    /// herdr-core (Herdr 2) terminals, emulated and drawn by the app, split
    /// into panes. Tabs saved before splits named one `terminal`.
    CoreTerminal {
        #[serde(alias = "terminal")]
        panes: crate::core_terminals::panes::Panes,
        /// A watch-only mirror: it follows the terminal's size and never
        /// types into it.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        observe: bool,
    },
}

/// The checkout an orchestrator view lists the repository of, on the view's
/// endpoint, which may be an SSH host. Tabs saved by earlier builds carry
/// one, so it is checked again whenever it is read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "SavedOrchestratorRepo")]
pub(crate) struct OrchestratorRepo {
    pub(crate) checkout: String,
}

#[derive(Deserialize)]
struct SavedOrchestratorRepo {
    checkout: String,
}

impl TryFrom<SavedOrchestratorRepo> for OrchestratorRepo {
    type Error = crate::orchestrator::Error;

    fn try_from(saved: SavedOrchestratorRepo) -> crate::orchestrator::Result<Self> {
        Self::new(saved.checkout)
    }
}

impl OrchestratorRepo {
    /// An absolute, bounded path without control characters; a remote one
    /// is a POSIX path whatever this machine's own convention.
    pub(crate) fn new(checkout: String) -> crate::orchestrator::Result<Self> {
        let valid = !checkout.is_empty()
            && checkout.len() <= 4096
            && !checkout.chars().any(char::is_control)
            && (checkout.starts_with('/') || Path::new(&checkout).is_absolute());
        if !valid {
            return Err(crate::orchestrator::Error::InvalidCheckout);
        }
        Ok(Self { checkout })
    }

    /// The checkout's folder name, for the tab title.
    pub(crate) fn name(&self) -> &str {
        self.checkout
            .trim_end_matches(['/', '\\'])
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(&self.checkout)
    }
}

/// The local checkout a review tab shows. Saved with the tabs, so it is
/// checked again whenever it is read. `repo_key` is the daemon's Git common
/// directory; a space it has no worktree metadata for is saved by its
/// directory alone, which the worker resolves and verifies on every read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "SavedReviewCheckout")]
pub(crate) struct ReviewCheckout {
    pub(crate) repo_key: Option<String>,
    pub(crate) branch: String,
    pub(crate) checkout: Option<String>,
}

#[derive(Deserialize)]
struct SavedReviewCheckout {
    #[serde(default)]
    repo_key: Option<String>,
    branch: String,
    checkout: Option<String>,
}

impl TryFrom<SavedReviewCheckout> for ReviewCheckout {
    type Error = crate::Error;

    fn try_from(saved: SavedReviewCheckout) -> crate::Result<Self> {
        let path = |text: &str| {
            !text.is_empty()
                && text.len() <= 4096
                && !text.chars().any(char::is_control)
                && Path::new(text).is_absolute()
        };
        let branch = !saved.branch.is_empty()
            && saved.branch.len() <= 1024
            && !saved.branch.chars().any(char::is_control);
        let named = saved.repo_key.is_some() || saved.checkout.is_some();
        if !named
            || saved.repo_key.as_deref().is_some_and(|key| !path(key))
            || !branch
            || saved.checkout.as_deref().is_some_and(|c| !path(c))
        {
            return Err(crate::Error::InvalidReviewCheckout);
        }
        Ok(Self {
            repo_key: saved.repo_key,
            branch: saved.branch,
            checkout: saved.checkout,
        })
    }
}

/// A review is saved by the repository the daemon names, or else by the
/// workspace directory the Git chip follows; one naming neither has nothing
/// to read.
impl TryFrom<&crate::pull_request::Input> for ReviewCheckout {
    type Error = crate::Error;

    fn try_from(input: &crate::pull_request::Input) -> crate::Result<Self> {
        if input.repo_key.is_none() && input.checkout.is_none() {
            return Err(herdr_github::pull_request::Error::Metadata.into());
        }
        Ok(Self {
            repo_key: input.repo_key.clone(),
            branch: input.branch.clone(),
            checkout: input.checkout.clone(),
        })
    }
}

impl From<&ReviewCheckout> for crate::pull_request::Input {
    fn from(checkout: &ReviewCheckout) -> Self {
        Self {
            checkout: checkout.checkout.clone(),
            repo_key: checkout.repo_key.clone(),
            branch: checkout.branch.clone(),
        }
    }
}

impl Location {
    /// The address the page is loaded from.
    #[cfg(any(target_os = "macos", windows, test))]
    pub(crate) fn page_url(&self) -> String {
        match self {
            Self::Web { url } => url.as_str().to_owned(),
            Self::Local { file } => file.page_url(),
            // Never loaded: a review is drawn by the app.
            Self::Review { .. }
            | Self::Code { .. }
            | Self::Orchestrator { .. }
            | Self::Devices
            | Self::CoreTerminal { .. } => "about:blank".to_owned(),
        }
    }

    /// Whether a native page shows it, rather than the app drawing it.
    pub(crate) fn is_page(&self) -> bool {
        !matches!(
            self,
            Self::Review { .. }
                | Self::Code { .. }
                | Self::Orchestrator { .. }
                | Self::Devices
                | Self::CoreTerminal { .. }
        )
    }

    /// What an agent's prompt names it by, in English like the prompt.
    pub(crate) fn display(&self) -> String {
        match self {
            Self::Web { url } => url.as_str().to_owned(),
            Self::Local { file } => file.path().display().to_string(),
            // i18n: data
            Self::Review { checkout } => format!("Review of {}", checkout.branch),
            Self::Code { file } => file.path.clone(),
            // i18n: data
            Self::Orchestrator { repo } => format!("Orchestrator of {}", repo.name()),
            Self::Devices => "Devices overview".to_owned(), // i18n: data
            Self::CoreTerminal { panes, .. } => {
                let terminals: Vec<String> =
                    panes.terminals().iter().map(ToString::to_string).collect();
                format!("herdr-core terminal {}", terminals.join(", ")) // i18n: data
            }
        }
    }

    /// A tab's title before its page names itself, as it is saved and the
    /// command line lists it: in English. The window shows [`Self::drawn_title`]
    /// for a tab the app draws.
    pub(crate) fn default_title(&self) -> String {
        match self {
            Self::Web { url } => url.host().to_owned(),
            Self::Local { file } => file.entry.rsplit('/').next().unwrap_or_default().to_owned(),
            Self::Review { checkout } => format!("Review \u{00b7} {}", checkout.branch), // i18n: data
            Self::Code { file } => file.name().to_owned(),
            // i18n: data
            Self::Orchestrator { repo } => format!("Orchestrator \u{00b7} {}", repo.name()),
            Self::Devices => "Devices".to_owned(), // i18n: data
            Self::CoreTerminal { .. } => "Terminal".to_owned(), // i18n: data
        }
    }

    /// The title of a tab the app draws, in the window's language; `None`
    /// for a page or a file, which name themselves.
    pub(crate) fn drawn_title(&self) -> Option<String> {
        match self {
            Self::Review { checkout } => {
                Some(fl!("browser-tab-review", branch = checkout.branch.as_str()))
            }
            Self::Orchestrator { repo } => {
                Some(fl!("browser-tab-orchestrator", repo = repo.name()))
            }
            Self::Devices => Some(fl!("browser-tab-devices")),
            Self::CoreTerminal { .. } => Some(fl!("browser-tab-terminal")),
            Self::Web { .. } | Self::Local { .. } | Self::Code { .. } => None,
        }
    }

    #[cfg(any(target_os = "macos", windows, test))]
    /// Where a page reported it went, relative to this tab: a local page
    /// stays in its folder, so another file there keeps the same root.
    pub(crate) fn visited(&self, reported: &str) -> Option<Self> {
        if let Some(entry) = PREVIEW_ORIGINS
            .iter()
            .find_map(|origin| reported.strip_prefix(origin))
        {
            let Self::Local { file } = self else {
                return None;
            };
            let entry = entry.split(['?', '#']).next().unwrap_or_default();
            let entry = decode_entry(entry)?;
            return Some(Self::Local {
                file: LocalFile {
                    root: file.root.clone(),
                    entry,
                },
            });
        }
        WebUrl::try_from(reported).ok().map(|url| Self::Web { url })
    }
}

/// A file inside a served folder. Only the folder's own files are served,
/// never a hidden one, and never from a folder holding the home directory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "SavedLocalFile")]
pub(crate) struct LocalFile {
    root: PathBuf,
    /// Relative to `root`, `/`-separated.
    entry: String,
}

#[derive(Deserialize)]
struct SavedLocalFile {
    root: PathBuf,
    entry: String,
}

impl TryFrom<SavedLocalFile> for LocalFile {
    type Error = crate::Error;

    fn try_from(saved: SavedLocalFile) -> crate::Result<Self> {
        let file = Self {
            root: saved.root,
            entry: saved.entry,
        };
        let home = crate::config::home().ok();
        file.check(home.as_deref())?;
        Ok(file)
    }
}

/// Plain names only: no `..`, no hidden files or folders, nothing empty.
fn plain_component(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains(['/', '\\', '\0'])
        && !name.chars().any(char::is_control)
}

#[cfg(any(target_os = "macos", windows, test))]
fn decode_entry(encoded: &str) -> Option<String> {
    let mut parts = Vec::new();
    for part in encoded.split('/') {
        let part = percent_encoding::percent_decode_str(part)
            .decode_utf8()
            .ok()?;
        if !plain_component(&part) {
            return None;
        }
        parts.push(part.into_owned());
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

impl LocalFile {
    /// The file at `path`, served from its folder. `path` must already be
    /// absolute and resolved; the caller's own process does that, so the UI
    /// never touches the disk here.
    pub(crate) fn new(path: &Path, home: Option<&Path>) -> crate::Result<Self> {
        let (Some(root), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str()))
        else {
            return Err(crate::Error::InvalidLocalPage);
        };
        let file = Self {
            root: root.to_owned(),
            entry: name.to_owned(),
        };
        file.check(home)?;
        Ok(file)
    }

    fn check(&self, home: Option<&Path>) -> crate::Result<()> {
        let absolute = self.root.is_absolute()
            && self.root.components().all(|part| {
                matches!(
                    part,
                    Component::RootDir | Component::Prefix(_) | Component::Normal(_)
                )
            })
            && self
                .root
                .components()
                .any(|part| matches!(part, Component::Normal(_)));
        let normal = |part: Component<'_>| match part {
            Component::Normal(name) => name.to_str().is_some_and(|name| !name.starts_with('.')),
            _ => true,
        };
        // A folder holding the home directory would expose all of it.
        let exposes_home = home.is_some_and(|home| home.starts_with(&self.root));
        if !absolute
            || exposes_home
            || !self.root.components().all(normal)
            || !self.entry.split('/').all(plain_component)
        {
            return Err(crate::Error::InvalidLocalPage);
        }
        Ok(())
    }

    #[cfg(any(target_os = "macos", windows, test))]
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn path(&self) -> PathBuf {
        self.entry
            .split('/')
            .fold(self.root.clone(), |path, part| path.join(part))
    }

    #[cfg(any(target_os = "macos", windows, test))]
    fn page_url(&self) -> String {
        let path: Vec<String> = self
            .entry
            .split('/')
            .map(|part| percent_encoding::utf8_percent_encode(part, SEGMENT).to_string())
            .collect();
        format!("{}{}", PREVIEW_ORIGINS[0], path.join("/"))
    }
}

#[cfg(any(target_os = "macos", windows, test))]
/// The file a preview request names under `root`, or `None` when the name
/// is not a plain relative path. The caller still resolves links on disk.
pub(crate) fn requested_file(root: &Path, request_path: &str) -> Option<PathBuf> {
    let entry = request_path.trim_start_matches('/');
    let entry = if entry.is_empty() || entry.ends_with('/') {
        format!("{entry}index.html")
    } else {
        entry.to_owned()
    };
    let entry = decode_entry(&entry)?;
    Some(
        entry
            .split('/')
            .fold(root.to_owned(), |path, part| path.join(part)),
    )
}

#[cfg(test)]
mod tests;
