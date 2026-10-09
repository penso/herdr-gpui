//! Links under the pointer. While the link modifier is held over a pane, the
//! daemon is asked which cells the link there covers, so a URL that wraps
//! onto the next row is underlined and opened whole. A click, on a web
//! address or a file path alike, goes to the daemon first when it offers
//! activation, which lets a plugin link handler claim it, `file://` links
//! included; otherwise, or when nothing claims it, this client opens the
//! address or file itself. A handler that fails spends the click. Daemons
//! without these methods keep the row-local detector.

use super::{HerdrWindow, file_links::FileLink};
use crate::{
    browser::WebUrl,
    links::{Activation, LinkCell, LinkInbox, LinkRequest, ResolvedLink},
    terminal::RowLink,
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
    /// modifier is held, underlined when the daemon resolved nothing there.
    local: Option<RowLink>,
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
    in_tab: bool,
}

/// What this client opens itself for a click no plugin handler claimed.
enum Fallback {
    Web(WebUrl),
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
    /// A file path pressed with the link modifier, where no web link is.
    pub(crate) file: Option<FileLink>,
    pub(crate) position: Point<Pixels>,
}

impl PressedLink {
    /// Whether a release reads the same link as this press did.
    fn same_link(&self, release: &Self) -> bool {
        self.url == release.url
            && self.file == release.file
            && match (&self.cell, &release.cell) {
                (Some(pressed), Some(released)) => pressed.same_content(released),
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
        LinkCell::at(
            self.live.surface.as_deref()?,
            f32::from(position.x - self.bounds.origin.x),
            f32::from(position.y - self.bounds.origin.y),
            self.cell_width,
            self.config.terminal.line_height(),
        )
    }

    fn terminal_hyperlink_at(&self, position: Point<Pixels>) -> bool {
        if self.menu.page.is_some()
            || !self.live.surface_ready()
            || !self.bounds.contains(&position)
        {
            return false;
        }
        self.live.surface.as_deref().is_some_and(|surface| {
            crate::terminal::pane_hyperlink_at(
                surface,
                f32::from(position.x - self.bounds.origin.x),
                f32::from(position.y - self.bounds.origin.y),
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

    /// The row-local link under the pointer while the link modifier is held.
    pub(crate) fn hovered_local_link(&self) -> Option<&RowLink> {
        self.links.local.as_ref()
    }

    /// The cell at `position` when it lies on the hovered resolved link.
    pub(crate) fn daemon_link_at(&self, position: Point<Pixels>) -> Option<LinkCell> {
        let cell = self.link_cell_at(position)?;
        self.hovered_daemon_link()?.covers(&cell).then_some(cell)
    }

    /// Follows the pointer and the link modifier, asking the daemon about a
    /// cell no earlier answer covers.
    pub(crate) fn hover_link(
        &mut self,
        position: Point<Pixels>,
        modifiers: Modifiers,
        cx: &mut Context<Self>,
    ) {
        let local = modifiers
            .secondary()
            .then(|| self.local_link_at(position))
            .flatten()
            .map(|link| link.link);
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

    /// The link a press at `position` is on, by either reading. A file path
    /// counts only with the link modifier held, as prose is full of words
    /// that look like paths.
    pub(crate) fn terminal_link_press(
        &self,
        position: Point<Pixels>,
        modifiers: Modifiers,
    ) -> Option<PressedLink> {
        let url = self.terminal_link_at(position);
        let daemon = self.daemon_link_at(position);
        // Read whether or not a hover already resolved the cell, so a press
        // and its release agree, and a `file://` hyperlink the daemon reads
        // still has its file to open when no handler claims it.
        let file = (url.is_none() && modifiers.secondary())
            .then(|| self.file_link_at(position))
            .flatten();
        // A hyperlink this client cannot open, such as an SSH host's own
        // `file://` link, may still be claimed by a handler on its host.
        let claimable = url.is_none()
            && file.is_none()
            && modifiers.secondary()
            && self.live.supports_link_activate
            && self.terminal_hyperlink_at(position);
        if url.is_none() && daemon.is_none() && file.is_none() && !claimable {
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
        let fallback = match release.file {
            Some(link) => Some(Fallback::File {
                link,
                in_editor: (self.config.open_files_in == crate::config::FileTarget::Editor)
                    != modifiers.alt,
            }),
            None => release
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
            let connection = &self.endpoints[self.selected_endpoint].connection;
            match connection.request_link(LinkRequest::Activate, cell) {
                Ok(id) => {
                    self.links.activating = Some(InFlight {
                        id,
                        inbox: connection.links.clone(),
                        context: PendingActivation { fallback, in_tab },
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
        PendingActivation { fallback, in_tab }: PendingActivation,
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
                self.open_link(url.map(Fallback::Web).or(fallback), in_tab, window, cx);
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
            Some(Fallback::File { link, in_editor }) => self.open_file_link(link, in_editor, cx),
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
