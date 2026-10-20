//! Local file paths a pane prints or names in an explicit hyperlink, opened
//! by a link-modifier click. A path is read from its row alone; the click
//! resolves it against the pane's working directory and checks that it
//! exists off the UI thread, so a word that only looks like a path opens
//! nothing. A hyperlink's path is a link without that check, so a click on
//! one that is not there says so. A pane on an SSH host prints that host's
//! paths, which a click fetches from there (see `remote_files`), so they
//! are links by their shape alone. A WSL or cloud pane's paths are not read.
//!
//! Terminal output is untrusted, and a hyperlink's text need not match its
//! target, so a click never launches anything. Only a folder or a
//! regular, non-executable file of a known document type opens itself, judged
//! by where any symlinks lead; anything else, from an application to a script
//! the system might run, opens the folder that holds it. A network path,
//! which would reach out to another machine, opens nothing.

use super::HerdrWindow;
use crate::editor::EditorTarget;
use crate::terminal::{PaneLink, RowTarget};
use gpui::{Context, Pixels, Point};
use herdr_client::ConnectTarget;
use std::{
    fs::Metadata,
    path::{Component, Path, PathBuf, Prefix},
};

/// Extensions of documents the system opens in a viewer or editor rather
/// than running. Scripts are left out on purpose: `.py`, `.sh`, or `.js` may
/// be run by the default application for them.
const DOCUMENTS: [&str; 52] = [
    "bmp", "c", "cc", "cfg", "conf", "cpp", "cs", "css", "csv", "diff", "env", "gif", "go", "h",
    "hpp", "htm", "html", "ico", "ini", "java", "jpeg", "jpg", "json", "jsonc", "jsx", "kt",
    "lock", "log", "markdown", "md", "patch", "pdf", "png", "proto", "rs", "rst", "scss", "sql",
    "svg", "swift", "tex", "toml", "ts", "tsv", "tsx", "txt", "vue", "webp", "xml", "yaml", "yml",
    "zig",
];

/// Extensions of the pages a link's `?query` or `#fragment` is kept for, by
/// opening them in the default browser: web pages and PDFs. A file with no
/// extension is one too.
const PAGES: [&str; 5] = [
    // keep-sorted start
    "htm", "html", "pdf", "svg", "xhtml",
    // keep-sorted end
];

/// A path a pane printed, the line printed after it, and the pane and the
/// directory it was in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileLink {
    pub(crate) path: String,
    pub(crate) line: Option<u32>,
    /// The `?query` or `#fragment` printed after the path, with its `?` or
    /// `#`. A page that opens itself opens in the default browser with it.
    pub(crate) query: Option<String>,
    pub(crate) pane_id: String,
    pub(crate) cwd: Option<String>,
    /// Whether an explicit hyperlink named the path. Such a path is a link
    /// whether or not its file is there, so a click on a missing one says
    /// so; a path read from printed text is a link only when it exists.
    pub(crate) explicit: bool,
    /// The SSH target of the host the pane runs on, whose file the path
    /// names; `None` for a pane on this machine.
    pub(crate) remote: Option<String>,
}

/// Why a path has no real one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Unresolved {
    /// A step of the path is not there.
    Missing,
    /// A step could not be read, or is not a file name at all.
    Unreadable,
    /// The path leads to another machine.
    Remote,
    /// More symlinks than a path may follow.
    Loop,
}

/// Where a clicked path opens, decided off the UI thread.
enum Opened {
    Editor(EditorTarget),
    System(url::Url),
    /// An explicit hyperlink's file is not there.
    Missing(PathBuf),
}

impl FileLink {
    /// The absolute path this link names: `~/` under `home`, and a relative
    /// path under the pane's working directory. `None` when either is
    /// unknown.
    fn resolve(&self, home: Option<&Path>) -> Option<PathBuf> {
        let path = Path::new(&self.path);
        if let Ok(rest) = path.strip_prefix("~") {
            return Some(home?.join(rest));
        }
        if path.is_absolute() {
            return Some(path.to_owned());
        }
        let cwd = Path::new(self.cwd.as_deref()?);
        cwd.is_absolute().then(|| cwd.join(path))
    }
}

/// Symlinks followed while resolving one path, Linux's `MAXSYMLINKS`.
const MAX_SYMLINKS: usize = 40;

/// Whether `path` may name a share on another machine, which the system
/// would reach out to, credentials and all, merely to look at. On Windows
/// only a drive letter is known to be local: a UNC path, a device path, or
/// a verbatim one such as `\\?\GLOBALROOT\Device\Mup\…` can reach the
/// network. Elsewhere it is the automounter's `/net` host map.
fn remote(path: &Path) -> bool {
    match path.components().next() {
        Some(Component::Prefix(prefix)) => {
            !matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
        }
        _ => cfg!(unix) && path.starts_with("/net"),
    }
}

/// `path` with every symlink resolved, one step at a time, so a link that
/// leads to another machine is refused before anything follows it, unlike
/// `canonicalize`.
fn real_path(path: &Path) -> Result<PathBuf, Unresolved> {
    let unread = |error: std::io::Error| match error.kind() {
        std::io::ErrorKind::NotFound => Unresolved::Missing,
        _ => Unresolved::Unreadable,
    };
    let mut resolved = PathBuf::new();
    let mut pending: Vec<_> = path
        .components()
        .map(|c| c.as_os_str().to_owned())
        .collect();
    pending.reverse();
    let mut followed = 0;
    while let Some(part) = pending.pop() {
        match Path::new(&part)
            .components()
            .next()
            .ok_or(Unresolved::Unreadable)?
        {
            Component::CurDir => continue,
            Component::ParentDir => {
                resolved.pop();
                continue;
            }
            // A root or prefix, from the path or an absolute link target,
            // starts over from there.
            Component::RootDir | Component::Prefix(_) => resolved.push(&part),
            Component::Normal(name) => resolved.push(name),
        }
        if remote(&resolved) {
            return Err(Unresolved::Remote);
        }
        if !std::fs::symlink_metadata(&resolved)
            .map_err(unread)?
            .is_symlink()
        {
            continue;
        }
        followed += 1;
        if followed > MAX_SYMLINKS {
            return Err(Unresolved::Loop);
        }
        let target = std::fs::read_link(&resolved).map_err(unread)?;
        resolved.pop();
        pending.extend(target.components().rev().map(|c| c.as_os_str().to_owned()));
    }
    Ok(resolved)
}

/// What a click on `path` opens, once symlinks are resolved: a folder,
/// whatever its name, or a document itself, and otherwise the nearest
/// folder holding it that is not itself a bundle the system would launch.
/// `None` when the path does not exist or leads to another machine. Reads
/// the disk.
pub(crate) fn opened(path: &Path) -> Option<PathBuf> {
    let real = real_path(path).ok()?;
    let metadata = std::fs::metadata(&real).ok()?;
    let extension = real
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase());
    let opens = if metadata.is_dir() {
        // A folder opens itself, so `Tool.app` is the bundle clicked, not
        // the folder holding it.
        true
    } else {
        // An extensionless file, such as `Makefile`, opens as text.
        metadata.is_file()
            && !executable(&metadata)
            && extension.is_none_or(|extension| DOCUMENTS.contains(&extension.as_str()))
    };
    if opens {
        return Some(real);
    }
    // The folder holding `Tool.app/run.sh` is a bundle too.
    real.ancestors()
        .skip(1)
        .find(|folder| folder.extension().is_none())
        .map(Path::to_owned)
}

/// The address the system opens for a click on `path`: the page itself with
/// `query` when [`page_url`] keeps it, and otherwise what [`opened`]
/// decides, without it. Reads the disk.
pub(crate) fn opened_url(path: &Path, query: Option<&str>) -> Option<url::Url> {
    page_url(path, query).or_else(|| url::Url::from_file_path(opened(path)?).ok())
}

/// The address of the file at `path` with `query`, for the default browser,
/// when there is a query and the file is a page: a regular file that is a
/// web page, a PDF, or has no extension. A folder never takes a query.
/// Reads the disk.
fn page_url(path: &Path, query: Option<&str>) -> Option<url::Url> {
    let query = query?;
    let real = real_path(path).ok()?;
    if !std::fs::metadata(&real).is_ok_and(|metadata| metadata.is_file()) {
        return None;
    }
    let page = real.extension().is_none_or(|extension| {
        PAGES.contains(&extension.to_string_lossy().to_ascii_lowercase().as_str())
    });
    if !page {
        return None;
    }
    let url = url::Url::from_file_path(&real).ok()?;
    Some(with_query(url, query))
}

/// `url` with the `?query` and `#fragment` of `suffix`, which holds them as
/// [`FileLink::query`] does.
fn with_query(mut url: url::Url, suffix: &str) -> url::Url {
    let (query, fragment) = match suffix.strip_prefix('#') {
        Some(fragment) => (None, Some(fragment)),
        None => {
            let rest = suffix.strip_prefix('?').unwrap_or(suffix);
            match rest.split_once('#') {
                Some((query, fragment)) => (Some(query), Some(fragment)),
                None => (Some(rest), None),
            }
        }
    };
    url.set_query(query);
    url.set_fragment(fragment);
    url
}

/// Starts the default browser on an address: [`crate::default_browser::open`]
/// in the app, and a fake in tests. Blocking.
pub(crate) type StartBrowser =
    std::sync::Arc<dyn Fn(&url::Url) -> crate::default_browser::Result<()> + Send + Sync>;

/// Whether `error` says this platform cannot start its default browser with
/// a URL at all, which every page click there meets: not worth a warning.
#[cfg(not(windows))]
fn unsupported(error: &crate::default_browser::Error) -> bool {
    matches!(error, crate::default_browser::Error::Unsupported)
}

/// Windows always can; any failure there is worth a warning.
#[cfg(windows)]
fn unsupported(_: &crate::default_browser::Error) -> bool {
    false
}

/// The default browser a new window starts pages in.
#[cfg(not(test))]
pub(crate) fn start_browser() -> StartBrowser {
    std::sync::Arc::new(crate::default_browser::open)
}

/// A test window never starts a real browser: this one always fails, so the
/// system opener answers unless a test sets its own.
#[cfg(test)]
pub(crate) fn start_browser() -> StartBrowser {
    std::sync::Arc::new(|_: &url::Url| {
        Err(crate::default_browser::Error::UnparsableCommand {
            command: "test window".into(),
        })
    })
}

impl FileLink {
    /// Whether the path is there for a click to open. This reads the disk,
    /// so it runs off the UI thread.
    pub(crate) fn found(&self) -> bool {
        let home = crate::config::home().ok();
        self.resolve(home.as_deref())
            .is_some_and(|path| real_path(&path).is_ok())
    }
}

/// Whether a regular file carries an execute bit, which the system may honor
/// by running it.
#[cfg(unix)]
fn executable(metadata: &Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
}

/// Windows has no execute bit; what runs is chosen by extension.
#[cfg(not(unix))]
fn executable(_: &Metadata) -> bool {
    false
}

impl HerdrWindow {
    /// The row-local link of either kind under `position` in a pane. Paths
    /// count only on a pane this machine runs, or on an SSH host when this
    /// client can reach its files.
    pub(crate) fn local_link_at(&self, position: Point<Pixels>) -> Option<PaneLink> {
        if self.menu.page.is_some()
            || !self.live.surface_ready()
            || !self.bounds.contains(&position)
        {
            return None;
        }
        let (x, y) = self.drawn_position(position)?;
        let link = self.link_reading((x, y)).pane_link_at(
            self.live.surface.as_deref()?,
            x,
            y,
            self.cell_width,
            self.config.terminal.line_height(),
        )?;
        let paths = !self.selected_is_remote() || self.ssh_target().is_some();
        (!matches!(link.link.target, RowTarget::Path { .. }) || paths).then_some(link)
    }

    /// The SSH target the selected endpoint runs on, if it runs on one.
    fn ssh_target(&self) -> Option<String> {
        match &self.endpoints[self.selected_endpoint].connection.target {
            ConnectTarget::Ssh { target, .. } => Some(target.clone()),
            _ => None,
        }
    }

    /// The file path under `position`, with the directory it is relative to.
    #[cfg(test)]
    pub(crate) fn file_link_at(&self, position: Point<Pixels>) -> Option<FileLink> {
        self.file_link(self.local_link_at(position)?)
    }

    /// The file path `link` reads, with the directory it is relative to.
    pub(crate) fn file_link(&self, link: PaneLink) -> Option<FileLink> {
        let PaneLink { pane_id, link } = link;
        let RowTarget::Path {
            path,
            line,
            query,
            explicit,
        } = link.target
        else {
            return None;
        };
        let cwd = self.live.snapshot.as_ref().and_then(|snapshot| {
            let pane = snapshot.panes.iter().find(|pane| pane.pane_id == pane_id)?;
            pane.foreground_cwd.clone().or_else(|| pane.cwd.clone())
        });
        Some(FileLink {
            path,
            line,
            query,
            pane_id,
            cwd,
            explicit,
            remote: self.ssh_target(),
        })
    }

    /// Opens `path` with the system's default application under the same
    /// rule as a clicked path: a document itself, and otherwise the folder
    /// holding it, so an executable or a bundle is never launched. A page
    /// opens with `query`, the `?query` or `#fragment` a link printed after
    /// it; see [`page_url`] and [`Self::open_file_url`]. Checked on the
    /// background executor.
    pub(crate) fn open_in_system_app(
        &self,
        path: PathBuf,
        query: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let found = cx
            .background_executor()
            .spawn(async move { opened_url(&path, query.as_deref()) });
        cx.spawn(async move |this, cx| {
            if let Some(url) = found.await {
                // The window may have closed meanwhile; nothing opens then.
                let _ = this.update(cx, |this, cx| this.open_file_url(url, cx));
            }
        })
        .detach();
    }

    /// Opens the `file://` address `url` with the system's opener, which
    /// drops a query and a fragment. One that has either opens in the
    /// default browser, started on the background executor, which keeps
    /// them and never runs the file. When it cannot be started, the system
    /// opener opens what a click on the file opens without them, by
    /// [`opened`]: the page itself when it is a document, and otherwise the
    /// folder holding it, so an executable page is never launched.
    pub(crate) fn open_file_url(&self, url: url::Url, cx: &mut Context<Self>) {
        if url.query().is_none() && url.fragment().is_none() {
            cx.open_url(url.as_str());
            return;
        }
        let browser = self.default_browser.clone();
        let fallback = cx.background_executor().spawn(async move {
            let error = browser(&url).err()?;
            if unsupported(&error) {
                tracing::debug!(%error, "default browser not started; opening without the query");
            } else {
                tracing::warn!(%error, "default browser not started; opening without the query");
            }
            let path = url.to_file_path().ok()?;
            url::Url::from_file_path(opened(&path)?).ok()
        });
        cx.spawn(async move |this, cx| {
            if let Some(url) = fallback.await {
                // The window may have closed meanwhile; nothing opens then.
                let _ = this.update(cx, |_, cx| cx.open_url(url.as_str()));
            }
        })
        .detach();
    }

    /// Opens the file `link` names, once a background check finds it there:
    /// in the terminal editor beside its pane when `in_editor` and the file
    /// is one an editor shows, and otherwise with the system's default
    /// application, or the folder holding it when it is not a known document.
    pub(crate) fn open_file_link(
        &mut self,
        link: FileLink,
        in_editor: bool,
        cx: &mut Context<Self>,
    ) {
        let pane = link.pane_id.clone();
        let found = cx.background_executor().spawn(async move {
            let home = crate::config::home().ok();
            let path = link.resolve(home.as_deref())?;
            let real = match real_path(&path) {
                Ok(real) => real,
                // The system opens a missing file silently, so the click
                // on a link that promised one is answered here.
                Err(Unresolved::Missing) if link.explicit => return Some(Opened::Missing(path)),
                Err(_) => return None,
            };
            // A page with a query opens in the default browser with it,
            // wherever its kind would otherwise open.
            if let Some(url) = page_url(&path, link.query.as_deref()) {
                return Some(Opened::System(url));
            }
            if in_editor
                && std::fs::metadata(&real)
                    .is_ok_and(|metadata| EditorTarget::editable(&real, &metadata))
            {
                let target = EditorTarget {
                    path: path.clone(),
                    line: link.line,
                };
                if crate::editor::command_line(&target, None, None).is_ok() {
                    return Some(Opened::Editor(target));
                }
            }
            url::Url::from_file_path(opened(&path)?)
                .ok()
                .map(Opened::System)
        });
        cx.spawn(async move |this, cx| {
            let Some(opened) = found.await else {
                tracing::debug!("Clicked file path not found");
                return;
            };
            // The window may have closed meanwhile; the file is no longer
            // asked for then.
            let _ = this.update(cx, |this, cx| match opened {
                Opened::Editor(target) => this.open_in_editor(&target, Some(&pane), cx),
                Opened::System(url) => this.open_file_url(url, cx),
                Opened::Missing(path) => this.show_flash(
                    super::flash::Flash::warning(
                        crate::Error::LinkedFileMissing { path }.to_string(),
                    ),
                    cx,
                ),
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests;
