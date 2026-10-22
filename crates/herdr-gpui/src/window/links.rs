//! Links under the pointer. A pointer that only passes over text asks
//! nothing and draws nothing. While the link modifier is held over a pane,
//! the text there is verified and, once it is, underlined: a well-formed
//! address at once, a file path when a check off the UI thread finds it on
//! disk, and a web address through the daemon, which is asked which cells
//! the link covers so that one wrapping onto the next row is underlined and
//! opened whole.
//!
//! Only a click with the link modifier held opens a link. It goes to the
//! daemon first when it offers activation, which lets a plugin link handler
//! claim it, `file://` links included; otherwise, or when nothing claims it,
//! this client opens the address or file itself. A handler that fails spends
//! the click. Daemons without these methods keep the local detector, which
//! also reads a link that wraps onto the next rows whole. Where the local
//! reading strictly contains the daemon's, as at an application's word wrap,
//! the local one is underlined and opened.

use super::{HerdrWindow, RemotePath, file_links::FileLink, remote_paths};
use crate::terminal::PaneLink;
use crate::{
    browser::WebUrl,
    links::{Activation, LinkCell, LinkInbox, LinkRequest, ResolvedLink},
    terminal::{ExternalUrl, InputTarget, LinkReading, RowLink, RowTarget, wheel_target},
};
use gpui::{Context, Modifiers, Pixels, Point, Task, Window};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

/// Pointer movement is coalesced: at most one resolve leaves per delay, for
/// wherever the pointer is when it fires.
const RESOLVE_DELAY: Duration = Duration::from_millis(40);

#[derive(Default)]
pub(crate) struct DaemonLinks {
    /// The pane cell under the pointer while the link modifier is held.
    hover: Option<LinkCell>,
    /// The latest answer. It stays shown while the pointer moves along the
    /// link and the pane content it was read from is unchanged.
    resolved: Option<ResolvedLink>,
    resolving: Option<InFlight<LinkCell>>,
    activating: Option<InFlight<PendingActivation>>,
    delay: Option<Task<()>>,
    /// What the row-local detector reads under the pointer while the link
    /// modifier is held, once it is verified: underlined when the daemon
    /// resolved nothing there. A file path counts once it is found on disk.
    local: Option<RowLink>,
    /// The file path last looked for on disk, and whether it is there;
    /// `None` while the check runs.
    checked: Option<(FileLink, Option<bool>)>,
    /// Where the pointer last was, for the check's answer to be applied to.
    pointer: Option<(Point<Pixels>, Modifiers)>,
}

/// A request in the mailbox it was queued on. A reconnect or another
/// endpoint replaces that mailbox, which retires the request unanswered.
struct InFlight<T> {
    id: String,
    inbox: Arc<Mutex<LinkInbox>>,
    context: T,
}

impl<T> InFlight<T> {
    /// The answer once it has arrived, or `Err` once it never can.
    fn poll(&self, current: &Arc<Mutex<LinkInbox>>, request: LinkRequest) -> PollResult {
        if !Arc::ptr_eq(&self.inbox, current) {
            return PollResult::Retired;
        }
        match self
            .inbox
            .try_lock()
            .ok()
            .and_then(|mut inbox| inbox.take(request, &self.id))
        {
            Some(result) => PollResult::Answered(result),
            None => PollResult::Waiting,
        }
    }
}

enum PollResult {
    Waiting,
    Retired,
    Answered(crate::Result<serde_json::Value>),
}

struct PendingActivation {
    /// What the row-local detector read at the click, opened when the
    /// daemon neither handles nor reads a web address there.
    fallback: Option<Fallback>,
    /// Whether the fallback wins over the address the daemon declines with:
    /// the local reading strictly contains the daemon's link, which it
    /// reads cut off at an application's word wrap.
    local_first: bool,
    in_tab: bool,
}

/// What this client opens itself for a click no plugin handler claimed.
enum Fallback {
    Web(WebUrl),
    /// An address with another scheme: opened at once when the scheme is
    /// trusted, and after the user confirms it otherwise.
    External(ExternalUrl),
    /// An explicit hyperlink this client never opens, which a click only
    /// explains.
    Unopened(crate::Error),
    /// A file, in the terminal editor or the system's default application,
    /// as the click's modifiers chose.
    File {
        link: FileLink,
        in_editor: bool,
    },
}

/// A press that may become a click on a link.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PressedLink {
    /// The address the row-local detector reads there, if any.
    pub(crate) url: Option<String>,
    /// The pane cell the daemon is asked to activate, when it can.
    pub(crate) cell: Option<LinkCell>,
    /// A file path, where no web link is.
    pub(crate) file: Option<FileLink>,
    /// What else the row reads there: an address with another scheme, or an
    /// explicit hyperlink that a click only explains.
    pub(crate) other: Option<RowTarget>,
    pub(crate) position: Point<Pixels>,
}

impl PressedLink {
    /// Whether a release reads the same link as this press did.
    ///
    /// A locally read web address stays that link when the pane viewport is
    /// unchanged, so a redraw of other cells can move the content revision
    /// between press and release. Activation still sends the release cell and
    /// the revision it was read at. A file path or a link only the daemon
    /// knows keeps the revision it was pressed on. A press and a release that
    /// disagree about whether a daemon cell is there are different clicks.
    fn same_link(&self, release: &Self) -> bool {
        self.url == release.url
            && self.file == release.file
            && self.other == release.other
            && match (&self.cell, &release.cell) {
                (Some(pressed), Some(released)) => {
                    if self.url.as_deref().is_some_and(|url| !url.is_empty()) {
                        pressed.same_viewport(released)
                    } else {
                        pressed.same_content(released)
                    }
                }
                (pressed, released) => pressed.is_none() && released.is_none(),
            }
    }
}

impl HerdrWindow {
    fn link_cell_at(&self, position: Point<Pixels>) -> Option<LinkCell> {
        if self.menu.page.is_some()
            || !self.live.surface_ready()
            || !self.bounds.contains(&position)
        {
            return None;
        }
        let (x, y) = self.drawn_position(position)?;
        LinkCell::at(
            self.live.surface.as_deref()?,
            x,
            y,
            self.cell_width,
            self.config.terminal.line_height(),
        )
    }

    pub(crate) fn terminal_hyperlink_at(&self, position: Point<Pixels>) -> bool {
        if self.menu.page.is_some()
            || !self.live.surface_ready()
            || !self.bounds.contains(&position)
        {
            return false;
        }
        let Some((x, y)) = self.drawn_position(position) else {
            return false;
        };
        self.live.surface.as_deref().is_some_and(|surface| {
            crate::terminal::pane_hyperlink_at(
                surface,
                x,
                y,
                self.cell_width,
                self.config.terminal.line_height(),
            )
        })
    }

    /// The resolved link under the pointer, while it still reads the live
    /// content it was resolved from.
    pub(crate) fn hovered_daemon_link(&self) -> Option<&ResolvedLink> {
        let hover = self.links.hover.as_ref()?;
        let link = self
            .links
            .resolved
            .as_ref()
            .filter(|link| link.covers(hover))?;
        link.cell
            .current(self.live.surface.as_deref()?)
            .then_some(link)
    }

    /// The verified local link under the pointer while the link modifier
    /// is held.
    pub(crate) fn hovered_local_link(&self) -> Option<&RowLink> {
        self.links.local.as_ref()
    }

    /// How this client reads plain text on the selected daemon, at the
    /// `drawn` surface point it is about to read.
    pub(crate) fn link_reading(&self, drawn: (f32, f32)) -> LinkReading {
        LinkReading {
            raw_url_wraps: !self.live.supports_link_resolve,
            windows_paths: self.windows_paths_at(drawn),
        }
    }

    /// Whether the pane at the `drawn` surface point runs on a host that
    /// writes Windows paths, judged by its working directory as a file link
    /// resolves against it; by this machine when no directory is known.
    fn windows_paths_at(&self, (x, y): (f32, f32)) -> bool {
        let pane_id = self.live.surface.as_deref().and_then(|surface| {
            let hit = wheel_target(
                surface,
                x,
                y,
                self.cell_width,
                self.config.terminal.line_height(),
            )?;
            match hit.target {
                InputTarget::Pane(id) => Some(id),
                InputTarget::Popup(_) => None,
            }
        });
        let cwd = pane_id.and_then(|pane_id| {
            let snapshot = self.live.snapshot.as_ref()?;
            let pane = snapshot.panes.iter().find(|pane| pane.pane_id == pane_id)?;
            pane.foreground_cwd.clone().or_else(|| pane.cwd.clone())
        });
        match cwd.as_deref() {
            Some(cwd) if remote_paths::windows(cwd) => true,
            Some(cwd) if cwd.starts_with('/') => false,
            _ => cfg!(windows),
        }
    }

    /// The cells to underline: the daemon's resolved link while it reads
    /// the content on screen, unless the local reading strictly contains
    /// it, as it does for an application's word wrap, which reads to the
    /// daemon as separate lines; otherwise the local reading.
    pub(crate) fn underlined_link_rows(&self) -> Vec<(u16, std::ops::Range<u16>)> {
        let daemon: Option<Vec<_>> = self
            .live
            .surface
            .as_deref()
            .zip(self.hovered_daemon_link())
            .filter(|(surface, link)| link.cell.current(surface))
            .map(|(_, link)| link.frame_rows().collect());
        let local = self.hovered_local_link();
        match (daemon, local) {
            (Some(daemon), Some(local)) if local.strictly_contains(&daemon) => local.rows.clone(),
            (Some(daemon), _) => daemon,
            (None, Some(local)) => local.rows.clone(),
            (None, None) => Vec::new(),
        }
    }

    /// Whether the local reading at `position` strictly contains the
    /// daemon's resolved link there, so a click opens the local target when
    /// no handler claims it. A click before the hover's answer arrives has
    /// no resolved link to compare: then a local reading spanning more than
    /// one row wins. It is an application's word wrap or a raw wrap, and a
    /// daemon offering `pane.link.resolve` is left every raw-wrapped
    /// address, so a multi-row address read here is one the daemon reads as
    /// separate lines. A handler still claims the click first.
    fn local_contains_daemon(&self, position: Point<Pixels>) -> bool {
        let Some(local) = self.local_link_at(position) else {
            return false;
        };
        match self.hovered_daemon_link() {
            Some(daemon) => {
                let rows: Vec<_> = daemon.frame_rows().collect();
                local.link.strictly_contains(&rows)
            }
            None => local.link.rows.len() > 1,
        }
    }

    /// The cell at `position` when it lies on the hovered resolved link.
    pub(crate) fn daemon_link_at(&self, position: Point<Pixels>) -> Option<LinkCell> {
        let cell = self.link_cell_at(position)?;
        self.hovered_daemon_link()?.covers(&cell).then_some(cell)
    }

    /// Whether the file `link` names is on disk, as far as is known yet. The
    /// first question about a path starts a check on the background
    /// executor, whose answer is applied to wherever the pointer is then.
    fn file_found(&mut self, link: FileLink, cx: &mut Context<Self>) -> bool {
        // A host's file counts by its shape: asking the host on every hover
        // would cost an SSH round trip each time.
        if link.remote.is_some() {
            return RemotePath::from_link(&link).is_some();
        }
        if let Some((checked, found)) = &self.links.checked
            && *checked == link
        {
            return *found == Some(true);
        }
        self.links.checked = Some((link.clone(), None));
        let asked = link.clone();
        let found = cx.background_executor().spawn(async move { asked.found() });
        cx.spawn(async move |this, cx| {
            let found = found.await;
            // The window may have closed while waiting; nothing is owed then.
            let _ = this.update(cx, |this, cx| {
                // The pointer may have moved on to another path meanwhile.
                if !matches!(&this.links.checked, Some((checked, None)) if *checked == link) {
                    return;
                }
                this.links.checked = Some((link, Some(found)));
                if let Some((position, modifiers)) = this.links.pointer {
                    this.hover_link(position, modifiers, cx);
                }
            });
        })
        .detach();
        false
    }

    /// The file `link` names, when a click may open it: always for an
    /// explicit hyperlink, and for printed text unless a check found it
    /// missing. A click before the check ends is not lost: opening reads the
    /// disk again, and opens nothing for a printed path that is not there.
    fn clickable_file(&self, link: PaneLink) -> Option<FileLink> {
        let file = self.file_link(link)?;
        if file.remote.is_some() {
            return RemotePath::from_link(&file).map(|_| file);
        }
        let missing =
            matches!(&self.links.checked, Some((checked, Some(false))) if *checked == file);
        (file.explicit || !missing).then_some(file)
    }

    /// Whether a press is on a link known to be one: anything but a printed
    /// path whose check has not found it yet, which may name nothing. A
    /// host's file is one by its shape, as for a hover.
    pub(crate) fn verified_press(&self, press: &PressedLink) -> bool {
        press.file.as_ref().is_none_or(|file| {
            file.explicit
                || file.remote.is_some()
                || matches!(&self.links.checked, Some((checked, Some(true))) if checked == file)
        })
    }

    /// Follows the pointer and the link modifier. Without the modifier
    /// nothing is read, asked, or shown; with it, the text under the pointer
    /// is verified, and the daemon is asked about a cell no earlier answer
    /// covers.
    pub(crate) fn hover_link(
        &mut self,
        position: Point<Pixels>,
        modifiers: Modifiers,
        cx: &mut Context<Self>,
    ) {
        self.links.pointer = Some((position, modifiers));
        let mut examined = false;
        let local = modifiers
            .secondary()
            .then(|| self.local_link_at(position))
            .flatten()
            .and_then(|link| {
                let row = link.link.clone();
                match row.target {
                    // A well-formed address is all that can be verified of it.
                    RowTarget::Web(_) | RowTarget::External(_) => Some(row),
                    // Nothing verifies these, so they keep what they have at rest.
                    RowTarget::OtherHost(_) | RowTarget::Malformed => None,
                    RowTarget::Path { .. } => {
                        let file = self.file_link(link)?;
                        examined = true;
                        self.file_found(file, cx).then_some(row)
                    }
                }
            });
        // The file may appear or go away meanwhile, so a path is checked
        // again each time the user looks at it anew: after letting go of the
        // modifier, or after leaving the path.
        if !examined {
            self.links.checked = None;
        }
        if local != self.links.local {
            self.links.local = local;
            cx.notify();
        }
        let cell = (modifiers.secondary() && self.live.supports_link_resolve)
            .then(|| self.link_cell_at(position))
            .flatten();
        if cell == self.links.hover {
            return;
        }
        let shown = self.hovered_daemon_link().cloned();
        self.links.hover = cell;
        if self.hovered_daemon_link() != shown.as_ref() {
            cx.notify();
        }
        self.schedule_link_resolve(cx);
    }

    /// The hovered cell when no answer, in flight or arrived, covers it.
    fn unresolved_hover(&self) -> Option<&LinkCell> {
        let hover = self.links.hover.as_ref()?;
        let answered = self
            .links
            .resolved
            .as_ref()
            .is_some_and(|link| link.cell == *hover || link.covers(hover));
        (!answered).then_some(hover)
    }

    fn schedule_link_resolve(&mut self, cx: &mut Context<Self>) {
        if self.links.delay.is_some()
            || self.links.resolving.is_some()
            || self.unresolved_hover().is_none()
        {
            return;
        }
        self.links.delay = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(RESOLVE_DELAY).await;
            // The window may have closed while waiting; nothing is owed then.
            let _ = this.update(cx, |this, cx| {
                this.links.delay = None;
                this.send_link_resolve(cx);
            });
        }));
    }

    fn send_link_resolve(&mut self, cx: &mut Context<Self>) {
        if self.links.resolving.is_some() {
            return;
        }
        let Some(cell) = self.unresolved_hover().cloned() else {
            return;
        };
        let connection = &self.endpoints[self.selected_endpoint].connection;
        match connection.request_link(LinkRequest::Resolve, &cell) {
            Ok(id) => {
                self.links.resolving = Some(InFlight {
                    id,
                    inbox: connection.links.clone(),
                    context: cell,
                });
            }
            // The event reader holds the mailbox for a moment; try again.
            Err(crate::Error::ConnectionBusy) => self.schedule_link_resolve(cx),
            // Hover is optional: a cell that cannot be asked about shows no
            // link and is not asked about again.
            Err(error) => {
                tracing::debug!(%error, "Link resolve not sent");
                self.links.resolved = Some(ResolvedLink {
                    cell,
                    regions: Vec::new(),
                });
            }
        }
    }

    /// Folds link answers into the window, and follows a pane that changed
    /// or scrolled under a still pointer.
    pub(crate) fn poll_links(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.endpoints[self.selected_endpoint]
            .connection
            .links
            .clone();
        if let Some(flight) = &self.links.resolving {
            match flight.poll(&current, LinkRequest::Resolve) {
                PollResult::Waiting => {}
                PollResult::Retired => self.links.resolving = None,
                PollResult::Answered(result) => {
                    let cell = flight.context.clone();
                    self.links.resolving = None;
                    let shown = self.hovered_daemon_link().cloned();
                    self.links.resolved = Some(match result {
                        Ok(response) => ResolvedLink::from_response(cell, &response),
                        Err(_) => ResolvedLink {
                            cell,
                            regions: Vec::new(),
                        },
                    });
                    if self.hovered_daemon_link() != shown.as_ref() {
                        cx.notify();
                    }
                }
            }
        }
        if let Some(flight) = &self.links.activating {
            match flight.poll(&current, LinkRequest::Activate) {
                PollResult::Waiting => {}
                // The daemon may already have run a handler; opening the
                // address as well could act on one click twice.
                PollResult::Retired => self.links.activating = None,
                PollResult::Answered(result) => {
                    if let Some(flight) = self.links.activating.take() {
                        self.finish_activation(
                            Activation::from(result),
                            flight.context,
                            window,
                            cx,
                        );
                    }
                }
            }
        }
        self.hover_link(window.mouse_position(), window.modifiers(), cx);
        self.schedule_link_resolve(cx);
    }

    /// The link a press at `position` is on, by either reading. Only a press
    /// with the link modifier held is one: a plain click selects, focuses, or
    /// goes to the application, and never opens anything.
    pub(crate) fn terminal_link_press(
        &self,
        position: Point<Pixels>,
        modifiers: Modifiers,
    ) -> Option<PressedLink> {
        if !modifiers.secondary() {
            return None;
        }
        let url = self.terminal_link_at(position);
        let daemon = self.daemon_link_at(position);
        // Read whether or not a hover already resolved the cell, so a press
        // and its release agree, and a `file://` hyperlink the daemon reads
        // still has its file to open when no handler claims it.
        let local = url
            .is_none()
            .then(|| self.local_link_at(position))
            .flatten();
        let other = local
            .as_ref()
            .map(|link| &link.link.target)
            .filter(|target| !matches!(target, RowTarget::Path { .. } | RowTarget::Web(_)))
            .cloned();
        let file = local.and_then(|link| self.clickable_file(link));
        // A hyperlink this client cannot read, such as a path on an SSH
        // host, may still be claimed by a handler on its host.
        let claimable = url.is_none()
            && file.is_none()
            && other.is_none()
            && self.live.supports_link_activate
            && self.terminal_hyperlink_at(position);
        if url.is_none() && daemon.is_none() && file.is_none() && other.is_none() && !claimable {
            return None;
        }
        // A plugin handler may claim any link the daemon can read, a file
        // path included, not only one the pointer hovered with the modifier
        // held.
        let cell = self
            .live
            .supports_link_activate
            .then(|| daemon.or_else(|| self.link_cell_at(position)))
            .flatten();
        Some(PressedLink {
            url,
            cell,
            file,
            other,
            position,
        })
    }

    /// Opens the link a click released on, if it is the one it pressed.
    pub(crate) fn activate_terminal_link(
        &mut self,
        pressed: &PressedLink,
        release: Point<Pixels>,
        modifiers: Modifiers,
        in_tab: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(release) = self.terminal_link_press(release, modifiers) else {
            return false;
        };
        if !pressed.same_link(&release) {
            return false;
        }
        let fallback = match (release.file, release.other) {
            (Some(link), _) => Some(Fallback::File {
                link,
                in_editor: (self.config.open_files_in == crate::config::FileTarget::Editor)
                    != modifiers.alt,
            }),
            (None, Some(RowTarget::External(url))) => Some(Fallback::External(url)),
            (None, Some(RowTarget::OtherHost(host))) => {
                Some(Fallback::Unopened(crate::Error::LinkOnAnotherHost { host }))
            }
            (None, Some(RowTarget::Malformed)) => {
                Some(Fallback::Unopened(crate::Error::MalformedLink))
            }
            (None, _) => release
                .url
                .as_deref()
                .and_then(|url| WebUrl::try_from(url).ok())
                .map(Fallback::Web),
        };
        if let Some(cell) = &release.cell {
            // One click at a time: a second while the first is unanswered
            // would race its handler.
            if self.links.activating.is_some() {
                return true;
            }
            let local_first = self.local_contains_daemon(release.position);
            let connection = &self.endpoints[self.selected_endpoint].connection;
            match connection.request_link(LinkRequest::Activate, cell) {
                Ok(id) => {
                    self.links.activating = Some(InFlight {
                        id,
                        inbox: connection.links.clone(),
                        context: PendingActivation {
                            fallback,
                            local_first,
                            in_tab,
                        },
                    });
                    return true;
                }
                Err(error) => tracing::debug!(%error, "Link activation not sent"),
            }
        }
        self.open_link(fallback, in_tab, window, cx);
        true
    }

    fn finish_activation(
        &mut self,
        activation: Activation,
        PendingActivation {
            fallback,
            local_first,
            in_tab,
        }: PendingActivation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match activation {
            Activation::Handled => {}
            Activation::HandlerFailed(error) => {
                self.local_error = Some(format!("Link handler failed: {error}"));
                cx.notify();
            }
            Activation::Declined(url) => {
                let declined = url.map(Fallback::Web);
                let link = if local_first {
                    fallback.or(declined)
                } else {
                    declined.or(fallback)
                };
                self.open_link(link, in_tab, window, cx);
            }
        }
    }

    fn open_link(
        &mut self,
        link: Option<Fallback>,
        in_tab: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match link {
            Some(Fallback::Web(url)) => self.open_web_link(url, in_tab, window, cx),
            // The terminal editor never opens a pane on another host, so a
            // host's file always opens in the system's application.
            Some(Fallback::File { link, .. }) if link.remote.is_some() => {
                if let Some(remote) = RemotePath::from_link(&link) {
                    let unjoined = RemotePath::unjoined(&link);
                    self.open_remote_file(remote, link.query, unjoined, link.cwd, cx);
                }
            }
            Some(Fallback::File { link, in_editor }) => self.open_file_link(link, in_editor, cx),
            Some(Fallback::External(url)) if url.trusted() => cx.open_url(url.as_str()),
            Some(Fallback::External(url)) => self.ask_to_open_link(url, window, cx),
            Some(Fallback::Unopened(error)) => {
                self.show_flash(super::flash::Flash::warning(error.to_string()), cx)
            }
            None => {}
        }
    }

    /// Opens a web address in a browser tab or the system browser, as the
    /// settings and the click's modifiers chose.
    fn open_web_link(
        &mut self,
        url: WebUrl,
        in_tab: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if in_tab && crate::browser::EMBEDDED {
            self.open_browser_tab(Some(url), window, cx);
        } else {
            cx.open_url(&String::from(url));
        }
    }
}

#[cfg(test)]
mod tests;
