//! Following a parsed link in a window: switching to a device, or showing a
//! local checkout. A checkout already open in a workspace is focused; any
//! other is only opened after the user agrees in the window, since a link can
//! come from any web page and opening one starts a shell in that directory.
// i18n: migrated

use super::Link;
use crate::{
    HerdrWindow, NavigationTarget,
    control::{ErrorCode, Response},
    endpoint::LOCAL,
    i18n::{FluentValue, Localize, fl},
};
use gpui::{App, Context, Global, Window, WindowHandle};
use herdr_client::protocol::ClientShellSnapshot;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("{} could not be read", .path.display())]
    Unreadable {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{} is not a folder", .0.display())]
    NotFolder(PathBuf),
    #[error("{} is not a Git checkout or worktree", .0.display())]
    NotCheckout(PathBuf),
    /// Only Windows paths can name another computer's share.
    #[cfg(windows)]
    #[error("{} is on another computer", .0.display())]
    Remote(PathBuf),
}

/// A checkout a link names, and the workspace already showing it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Checkout {
    pub path: PathBuf,
    pub workspace: Option<String>,
}

/// A link the app was started with, followed once a window can.
#[derive(Default)]
pub(crate) struct Pending(Option<Link>);

impl Global for Pending {}

/// Why a link was not followed. The command line reports it in English,
/// as all its output is; the window shows it in its own language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    Closed,
    DialogOpen,
    NoDevice(String),
    NotConnected,
    NotificationGone,
}

impl Refusal {
    fn code(&self) -> ErrorCode {
        match self {
            Self::Closed => ErrorCode::NoWindow,
            Self::DialogOpen => ErrorCode::WindowBusy,
            Self::NoDevice(_) | Self::NotificationGone => ErrorCode::NotFound,
            Self::NotConnected => ErrorCode::Busy,
        }
    }

    /// The message saying why, with its arguments.
    fn message(&self) -> (&'static str, Vec<(&'static str, FluentValue<'static>)>) {
        match self {
            Self::Closed => ("link-refused-closed", Vec::new()),
            Self::DialogOpen => ("link-refused-dialog-open", Vec::new()),
            Self::NoDevice(id) => (
                "link-refused-no-device",
                vec![("device", FluentValue::from(id.clone()))],
            ),
            Self::NotConnected => ("link-refused-not-connected", Vec::new()),
            Self::NotificationGone => ("link-refused-notification-gone", Vec::new()),
        }
    }

    pub(crate) fn response(&self) -> Response {
        let (id, args) = self.message();
        Response::error(self.code(), crate::i18n::english(id, &args))
    }

    /// The reason in the window's language.
    pub(crate) fn shown(&self) -> String {
        let (id, args) = self.message();
        crate::i18n::message(id, &args)
    }
}

/// Puts a link's complaint on the status line, remembered by the window so
/// the next link it follows clears it.
fn complain(view: &mut HerdrWindow, text: String, cx: &mut Context<HerdrWindow>) {
    view.link_complaint = Some(text.clone());
    view.local_error = Some(text);
    cx.notify();
}

pub(crate) fn defer(link: Link, cx: &mut App) {
    cx.set_global(Pending(Some(link)));
}

/// Follows the link the app was started with once this window has what it
/// needs: a device once the saved devices are loaded (a window starts with
/// Local alone), a checkout once this computer's Herdr is connected. A notification from before the app started is not known, so
/// starting the app was all its click could do.
pub(crate) fn poll(view: &HerdrWindow, window: &Window, cx: &mut Context<HerdrWindow>) {
    let ready = match cx
        .try_global::<Pending>()
        .and_then(|pending| pending.0.as_ref())
    {
        None => return,
        Some(Link::Notification { .. }) => {
            cx.global_mut::<Pending>().0 = None;
            return;
        }
        Some(Link::Device { id }) => {
            view.catalog.loaded() || view.endpoints.iter().any(|endpoint| &endpoint.id == id)
        }
        Some(Link::Open { .. }) => local_snapshot(view).is_some(),
    };
    let Some(handle) = window.window_handle().downcast::<HerdrWindow>() else {
        return;
    };
    if !ready {
        return;
    }
    let Some(link) = cx.global_mut::<Pending>().0.take() else {
        return;
    };
    cx.defer(move |cx| {
        if let Err(refusal) = follow_link(link, handle, cx) {
            let _ = handle.update(cx, |view, _, cx| {
                complain(view, fl!("link-open-failed", reason = refusal.shown()), cx);
            });
        }
    });
}

/// Follows `link` for the command line, which reports in English.
pub(crate) fn follow(link: Link, handle: WindowHandle<HerdrWindow>, cx: &mut App) -> Response {
    follow_link(link, handle, cx).unwrap_or_else(|refusal| refusal.response())
}

/// Follows `link`, answering what was done in the command line's English,
/// or why it was not.
fn follow_link(
    link: Link,
    handle: WindowHandle<HerdrWindow>,
    cx: &mut App,
) -> Result<Response, Refusal> {
    match link {
        Link::Notification { tag } => {
            if crate::window::system_notifications::open(&tag, cx) {
                // i18n: data
                Ok(Response::done("Opened the notification's pane"))
            } else {
                Err(Refusal::NotificationGone)
            }
        }
        Link::Device { id } => handle
            .update(cx, |view, window, cx| {
                if view.menu.page.is_some() {
                    return Err(Refusal::DialogOpen);
                }
                if !view.select_endpoint(&id, cx) {
                    return Err(Refusal::NoDevice(id));
                }
                bring_forward(window, cx);
                Ok(Response::done(format!(
                    // i18n: data
                    "Showing {}",
                    view.endpoints[view.selected_endpoint].label
                )))
            })
            .unwrap_or(Err(Refusal::Closed)),
        Link::Open { path } => {
            let Ok(snapshot) = handle.read_with(cx, |view, _| local_snapshot(view)) else {
                return Err(Refusal::Closed);
            };
            let Some(snapshot) = snapshot else {
                return Err(Refusal::NotConnected);
            };
            // i18n: data
            let message = format!("Opening {}", path.display());
            let check = cx
                .background_executor()
                .spawn(async move { checkout_at(&path, &snapshot) });
            cx.spawn(async move |cx| {
                let found = check.await;
                let _ = handle.update(cx, |view, window, cx| {
                    view.show_checkout(found, window, cx);
                });
            })
            .detach();
            Ok(Response::done(message))
        }
    }
}

/// Whether the window is connected to this computer's Herdr, so it can open
/// a checkout.
pub(crate) fn shows_local(view: &HerdrWindow) -> bool {
    local_snapshot(view).is_some()
}

/// The snapshot of this computer's Herdr, when the window is connected to it.
fn local_snapshot(view: &HerdrWindow) -> Option<Arc<ClientShellSnapshot>> {
    let index = view.endpoints.iter().position(|e| e.id == LOCAL)?;
    let live = if index == view.selected_endpoint {
        &view.live
    } else {
        &view.endpoints[index].live
    };
    live.status
        .is_connected()
        .then(|| live.snapshot.clone())
        .flatten()
}

fn bring_forward(window: &mut Window, cx: &mut App) {
    cx.activate(true);
    window.activate_window();
}

/// Checks a link's folder on disk, off the UI thread, and finds the
/// workspace already showing it.
fn checkout_at(path: &Path, snapshot: &ClientShellSnapshot) -> Result<Checkout, Error> {
    let path = canonical(path)?;
    if !path.is_dir() {
        return Err(Error::NotFolder(path));
    }
    // A main checkout has a `.git` folder, a linked worktree a `.git` file.
    if std::fs::symlink_metadata(path.join(".git")).is_err() {
        return Err(Error::NotCheckout(path));
    }
    let workspace = snapshot.workspaces.iter().find_map(|workspace| {
        let root = Path::new(crate::palette::launch_root(
            snapshot,
            &workspace.workspace_id,
        )?);
        (root.is_absolute() && canonical(root).is_ok_and(|root| root == path))
            .then(|| workspace.workspace_id.clone())
    });
    Ok(Checkout { path, workspace })
}

/// The path with links resolved, in the form Herdr reports workspace
/// directories in: on Windows a plain drive path, never the `\\?\` form
/// `canonicalize` returns, and never another computer's share.
fn canonical(path: &Path) -> Result<PathBuf, Error> {
    let resolved = std::fs::canonicalize(path).map_err(|source| Error::Unreadable {
        path: path.to_owned(),
        source,
    })?;
    #[cfg(windows)]
    {
        let text = resolved.to_string_lossy();
        if let Some(drive) = text.strip_prefix(r"\\?\")
            && drive.as_bytes().get(1) == Some(&b':')
        {
            return Ok(PathBuf::from(drive));
        }
        if text.starts_with(r"\\") {
            return Err(Error::Remote(resolved));
        }
    }
    Ok(resolved)
}

impl HerdrWindow {
    fn show_checkout(
        &mut self,
        found: Result<Checkout, Error>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        bring_forward(window, cx);
        // An earlier link's complaint no longer applies.
        if let Some(complaint) = self.link_complaint.take()
            && self.local_error.as_ref() == Some(&complaint)
        {
            self.local_error = None;
        }
        match found {
            Ok(Checkout {
                workspace: Some(workspace),
                ..
            }) => {
                if !self.navigate_endpoint(LOCAL, NavigationTarget::Workspace(&workspace), cx) {
                    complain(self, fl!("link-open-dialog-first"), cx);
                }
            }
            Ok(Checkout {
                path,
                workspace: None,
            }) => self.ask_to_open_checkout(path, window, cx),
            Err(error) => {
                complain(self, fl!("link-open-failed", reason = error.localize()), cx);
            }
        }
        cx.notify();
    }
}

#[cfg(test)]
#[path = "follow_tests.rs"]
mod tests;
