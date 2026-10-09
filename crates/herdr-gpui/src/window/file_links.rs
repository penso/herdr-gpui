//! Local file paths a pane prints, opened by a link-modifier click. A path is
//! read from its row alone; the click resolves it against the pane's working
//! directory and checks that it exists off the UI thread, so a word that only
//! looks like a path opens nothing. A pane on an SSH host prints that host's
//! paths, which this machine cannot open, so none are read there.
//!
//! Terminal output is untrusted, and a hyperlink's text need not match its
//! target, so a click never launches anything. Only a plain folder or a
//! regular, non-executable file of a known document type opens itself, judged
//! by where any symlinks lead; anything else, from an application to a script
//! the system might run, opens the folder that holds it. A network path,
//! which would reach out to another machine, opens nothing.

use super::HerdrWindow;
use crate::editor::EditorTarget;
use crate::terminal::{PaneLink, RowTarget, pane_link_at};
use gpui::{Context, Pixels, Point};
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

/// A path a pane printed, the line printed after it, and the pane and the
/// directory it was in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileLink {
    pub(crate) path: String,
    pub(crate) line: Option<u32>,
    pub(crate) pane_id: String,
    pub(crate) cwd: Option<String>,
}

/// Where a clicked path opens, decided off the UI thread.
enum Opened {
    Editor(EditorTarget),
    System(url::Url),
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
/// `canonicalize`. `None` when a step is missing, remote, or loops.
fn real_path(path: &Path) -> Option<PathBuf> {
    let mut resolved = PathBuf::new();
    let mut pending: Vec<_> = path
        .components()
        .map(|c| c.as_os_str().to_owned())
        .collect();
    pending.reverse();
    let mut followed = 0;
    while let Some(part) = pending.pop() {
        match Path::new(&part).components().next()? {
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
            return None;
        }
        if !std::fs::symlink_metadata(&resolved).ok()?.is_symlink() {
            continue;
        }
        followed += 1;
        if followed > MAX_SYMLINKS {
            return None;
        }
        let target = std::fs::read_link(&resolved).ok()?;
        resolved.pop();
        pending.extend(target.components().rev().map(|c| c.as_os_str().to_owned()));
    }
    Some(resolved)
}

/// What a click on `path` opens, once symlinks are resolved: a plain folder
/// or a document itself, and otherwise the nearest folder holding it that
/// is not itself a bundle the system would launch. `None` when the path
/// does not exist or leads to another machine.
fn opened(path: &Path) -> Option<PathBuf> {
    let real = real_path(path)?;
    let metadata = std::fs::metadata(&real).ok()?;
    let extension = real
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase());
    let opens = if metadata.is_dir() {
        // A folder with an extension may be a bundle the system launches.
        extension.is_none()
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
    /// count only on a pane this machine runs.
    pub(crate) fn local_link_at(&self, position: Point<Pixels>) -> Option<PaneLink> {
        if self.menu.page.is_some()
            || !self.live.surface_ready()
            || !self.bounds.contains(&position)
        {
            return None;
        }
        let link = pane_link_at(
            self.live.surface.as_deref()?,
            f32::from(position.x - self.bounds.origin.x),
            f32::from(position.y - self.bounds.origin.y),
            self.cell_width,
            self.config.terminal.line_height(),
        )?;
        (matches!(link.link.target, RowTarget::Web(_)) || !self.selected_is_remote())
            .then_some(link)
    }

    /// The file path under `position`, with the directory it is relative to.
    pub(crate) fn file_link_at(&self, position: Point<Pixels>) -> Option<FileLink> {
        let PaneLink { pane_id, link } = self.local_link_at(position)?;
        let RowTarget::Path { path, line } = link.target else {
            return None;
        };
        let cwd = self.live.snapshot.as_ref().and_then(|snapshot| {
            let pane = snapshot.panes.iter().find(|pane| pane.pane_id == pane_id)?;
            pane.foreground_cwd.clone().or_else(|| pane.cwd.clone())
        });
        Some(FileLink {
            path,
            line,
            pane_id,
            cwd,
        })
    }

    /// Opens `path` with the system's default application under the same
    /// rule as a clicked path: a document itself, and otherwise the folder
    /// holding it, so an executable or a bundle is never launched. Checked
    /// on the background executor.
    pub(crate) fn open_in_system_app(&self, path: PathBuf, cx: &mut Context<Self>) {
        let found = cx
            .background_executor()
            .spawn(async move { url::Url::from_file_path(opened(&path)?).ok() });
        cx.spawn(async move |_, cx| {
            if let Some(url) = found.await {
                cx.update(|cx| cx.open_url(url.as_str()));
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
            let real = real_path(&path)?;
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
                Opened::System(url) => cx.open_url(url.as_str()),
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests;
