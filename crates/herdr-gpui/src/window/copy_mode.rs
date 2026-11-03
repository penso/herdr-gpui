//! The window's side of keyboard copy mode: entering and leaving it, routing
//! keys and input-method text to it instead of the terminal, the `/` and `?`
//! search prompt, sending the daemon's motions and searches, keeping the
//! cursor on screen, and copying the selection through `pane.selection.read`.
//! The mode's own rules live in [`crate::copy_mode`].
//!
//! The prompt is the find bar's native field, so composition works in it.
//! While it has focus the terminal's key handler steps aside, as it does for
//! the find bar; Enter and Escape bubble out of the field to the prompt.
// i18n: migrated

use super::HerdrWindow;
use crate::i18n::{Localize, fl};
use crate::{
    copy_mode::{Command, CopyMode, Outcome, Request},
    scrollback::Inbox,
    search_input::SearchInput,
    terminal_painter::Highlight,
};
use gpui::{prelude::*, *};
use herdr_client::{
    Method,
    protocol::{PaneSurfaceFrame, PaneSurfacePane},
    scrollback::{SearchDirection, SelectionReadParams, TextRange},
};
use std::sync::{Arc, Mutex};

pub(crate) struct CopyModeState {
    mode: CopyMode,
    boot_id: String,
    /// The mailbox of the connection copy mode began on; a reconnect
    /// replaces it, which ends copy mode with the connection.
    inbox: Arc<Mutex<Inbox>>,
    /// The search prompt `/` or `?` opened, until Enter submits it or
    /// Escape closes it.
    prompt: Option<Prompt>,
}

struct Prompt {
    direction: SearchDirection,
    input: Entity<SearchInput>,
}

impl HerdrWindow {
    /// Enters copy mode on the focused pane, at the terminal's cursor.
    pub(crate) fn enter_copy_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.live.supports_copy_motion {
            self.show_flash(
                super::Flash::warning(fl!("window-find-copy-mode-needs-newer")),
                cx,
            );
            return;
        }
        let Some(pane) = self.focused_surface_pane() else {
            return;
        };
        let cursor = self
            .live
            .surface
            .as_deref()
            .and_then(|surface| surface.frame.cursor.as_ref())
            .filter(|cursor| cursor.visible)
            .map(|cursor| (cursor.x, cursor.y));
        let mode = CopyMode::new(pane, cursor);
        let Some(boot_id) = self.live.snapshot.as_ref().map(|s| s.boot_id.clone()) else {
            return;
        };
        self.leave_copy_mode(cx);
        self.selection = None;
        self.discard_composition(cx);
        self.copy_mode = Some(CopyModeState {
            mode,
            boot_id,
            inbox: self.endpoints[self.selected_endpoint]
                .connection
                .scrollback
                .clone(),
            prompt: None,
        });
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Leaves copy mode, scrolling the pane back to where it was when copy
    /// mode began.
    pub(crate) fn leave_copy_mode(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.copy_mode.take() else {
            return;
        };
        if let Some(request) = state.mode.in_flight()
            && let Ok(mut inbox) = state.inbox.try_lock()
        {
            inbox.discard(request);
        }
        if let Some(offset) = state.mode.entry_offset() {
            self.scroll_pane(&state.boot_id, state.mode.pane_id(), offset, cx);
        }
        cx.notify();
    }

    /// The cells copy mode has marked in `pane`, while it walks that pane
    /// for the connection and session it began on.
    pub(super) fn copy_mode_marked(&self, boot: &str, pane: &PaneSurfacePane) -> Option<TextRange> {
        let state = self.copy_mode.as_ref()?;
        let current = &self.endpoints[self.selected_endpoint].connection.scrollback;
        (state.boot_id == boot && Arc::ptr_eq(&state.inbox, current))
            .then(|| state.mode.marked(pane))
            .flatten()
    }

    /// Whether copy mode holds the keyboard, so nothing typed reaches the
    /// terminal.
    pub(crate) fn copy_mode_active(&self) -> bool {
        self.copy_mode.is_some()
    }

    /// A keystroke while copy mode is on: a copy-mode key runs, and every
    /// other key is swallowed rather than typed into the pane. A prompt left
    /// open while the terminal took the keyboard back is closed first.
    pub(crate) fn copy_mode_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keystroke = &event.keystroke;
        let modifiers = keystroke.modifiers;
        if modifiers.platform || modifiers.alt || modifiers.function {
            return;
        }
        if let Some(state) = &mut self.copy_mode
            && state.prompt.take().is_some()
        {
            cx.notify();
        }
        if let Some(command) = Command::from_key(&keystroke.key, modifiers.shift, modifiers.control)
        {
            self.copy_mode_command(command, None, window, cx);
        }
    }

    /// Text an input method committed while copy mode is on, read as keys.
    /// What follows a `/` or `?` is the start of the search query.
    pub(crate) fn copy_mode_text(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for (index, c) in text.char_indices() {
            let Some(command) = Command::from_char(c) else {
                continue;
            };
            let rest = matches!(command, Command::Prompt(_))
                .then(|| &text[index.saturating_add(c.len_utf8())..]);
            self.copy_mode_command(command, rest, window, cx);
            if rest.is_some() {
                return;
            }
        }
    }

    /// Runs `command`; `query` starts the prompt it may open.
    fn copy_mode_command(
        &mut self,
        command: Command,
        query: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = &mut self.copy_mode else {
            return;
        };
        let Some(pane) = pane_of(self.live.surface.as_deref(), state.mode.pane_id()) else {
            return;
        };
        match state.mode.command(command, pane) {
            Outcome::Nothing => {}
            Outcome::Abandoned(request) => {
                if let Some(request) = request
                    && let Ok(mut inbox) = state.inbox.try_lock()
                {
                    inbox.discard(&request);
                }
            }
            Outcome::Moved => self.reveal_copy_cursor(cx),
            Outcome::Prompt(direction) => {
                self.open_copy_search(direction, query.unwrap_or_default(), window, cx);
            }
            Outcome::Send(request) => self.send_copy_request(request, cx),
            Outcome::Copy {
                range,
                content_revision,
            } => {
                self.copy_range(range, content_revision, cx);
                self.close_copy_search(window, cx);
                self.leave_copy_mode(cx);
            }
            Outcome::Exit => {
                self.close_copy_search(window, cx);
                self.leave_copy_mode(cx);
            }
        }
        cx.notify();
    }

    /// Opens the search prompt over the copy-mode pane, focused, holding
    /// `query`.
    fn open_copy_search(
        &mut self,
        direction: SearchDirection,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.copy_mode.is_none() {
            return;
        }
        if !self.live.supports_copy_search {
            self.show_flash(
                super::Flash::warning(fl!("window-find-copy-search-needs-newer")),
                cx,
            );
            return;
        }
        let input = cx.new(SearchInput::new);
        input.update(cx, |input, cx| {
            input.set_placeholder_with(|| fl!("window-find-copy-search-placeholder"), cx);
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
            if !query.is_empty() {
                input.replace_text_in_range(None, query, window, cx);
            }
        });
        let focus = input.read(cx).focus.clone();
        if let Some(state) = &mut self.copy_mode {
            state.prompt = Some(Prompt { direction, input });
        }
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Closes the prompt, handing the keyboard back to copy mode, and
    /// returns the query and direction it held.
    fn close_copy_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<(String, SearchDirection)> {
        let prompt = self.copy_mode.as_mut()?.prompt.take()?;
        let input = prompt.input.read(cx);
        let query = input.text().to_owned();
        if input.focus.is_focused(window) {
            window.focus(&self.focus, cx);
        }
        cx.notify();
        Some((query, prompt.direction))
    }

    /// Whether the search prompt holds the keyboard, so the terminal must
    /// not act on a keystroke bubbling out of it.
    pub(crate) fn copy_search_focused(&self, window: &Window, cx: &App) -> bool {
        self.copy_mode
            .as_ref()
            .and_then(|state| state.prompt.as_ref())
            .is_some_and(|prompt| prompt.input.read(cx).focus.is_focused(window))
    }

    /// Enter searches for the prompt's query and Escape closes the prompt
    /// without searching; both stay in copy mode. Every other key is the
    /// field's, or the input method's while it composes.
    fn copy_search_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(prompt) = self
            .copy_mode
            .as_ref()
            .and_then(|state| state.prompt.as_ref())
        else {
            return;
        };
        if prompt.input.read(cx).is_composing() || event.keystroke.modifiers.modified() {
            return;
        }
        match event.keystroke.key.as_str() {
            "escape" => {
                self.close_copy_search(window, cx);
            }
            "enter" => {
                if let Some((query, direction)) = self.close_copy_search(window, cx) {
                    self.copy_mode_command(Command::Search { query, direction }, None, window, cx);
                }
            }
            _ => return,
        }
        cx.stop_propagation();
        window.prevent_default();
    }

    fn send_copy_request(&mut self, request: Request, cx: &mut Context<Self>) {
        let Some(state) = &mut self.copy_mode else {
            return;
        };
        let Some(handle) = &self.endpoints[self.selected_endpoint].connection.handle else {
            return;
        };
        let sent = match state.inbox.try_lock() {
            Ok(mut inbox) => inbox.send(|| match &request {
                Request::Motion(params) => handle.copy_motion(&state.boot_id, params),
                Request::Search { params, .. } => handle.copy_search(&state.boot_id, params),
            }),
            Err(_) => Err(herdr_client::Error::Full),
        };
        match sent {
            Ok(id) => state.mode.sent(id, &request),
            Err(error) => {
                state.mode.send_failed();
                let request = match request {
                    Request::Motion(_) => "motion",
                    Request::Search { .. } => "search",
                };
                self.local_error = Some(fl!(
                    "window-find-copy-request-not-sent",
                    request = request,
                    error = error.localize()
                ));
                cx.notify();
            }
        }
    }

    /// Reads `range` from the daemon, at `content_revision` when given, and
    /// copies it when it comes back, the same way a selection dragged past
    /// the screen is copied.
    fn copy_range(
        &mut self,
        range: TextRange,
        content_revision: Option<u64>,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = &self.copy_mode else {
            return;
        };
        let Some(handle) = &self.endpoints[self.selected_endpoint].connection.handle else {
            return;
        };
        let params = SelectionReadParams {
            pane_id: state.mode.pane_id().to_owned(),
            anchor: range.start,
            cursor: range.end,
            content_revision,
        };
        let sent = match state.inbox.try_lock() {
            Ok(mut inbox) => inbox.send(|| handle.read_selection(&state.boot_id, &params)),
            Err(_) => Err(herdr_client::Error::Full),
        };
        match sent {
            Ok(request) => self.await_selection_read(state.inbox.clone(), request),
            Err(error) => {
                self.local_error = Some(fl!(
                    "window-find-selection-not-copied",
                    error = error.localize()
                ));
                cx.notify();
            }
        }
    }

    fn reveal_copy_cursor(&mut self, cx: &mut Context<Self>) {
        let Some(state) = &mut self.copy_mode else {
            return;
        };
        let Some(pane) = pane_of(self.live.surface.as_deref(), state.mode.pane_id()) else {
            return;
        };
        if let Some(offset) = state.mode.reveal(pane) {
            let (boot, pane) = (state.boot_id.clone(), state.mode.pane_id().to_owned());
            self.scroll_pane(&boot, &pane, offset, cx);
        }
    }

    fn scroll_pane(&mut self, boot_id: &str, pane_id: &str, offset: u64, cx: &mut Context<Self>) {
        let Some(handle) = &self.endpoints[self.selected_endpoint].connection.handle else {
            return;
        };
        if let Err(error) = handle.request(
            boot_id,
            Method::PaneScroll,
            serde_json::json!({"pane_id": pane_id, "offset_from_bottom": offset}),
        ) {
            self.local_error = Some(fl!(
                "window-find-copy-scroll-failed",
                error = error.localize()
            ));
            cx.notify();
        }
    }

    /// Runs every tick: ends copy mode whose pane or connection is gone,
    /// applies a motion's or search's answer, retries a stale one, and runs
    /// keys that waited behind it.
    pub(crate) fn poll_copy_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = &mut self.copy_mode else {
            return;
        };
        let connection = &self.endpoints[self.selected_endpoint].connection;
        let current = Arc::ptr_eq(&state.inbox, &connection.scrollback)
            && self.live.snapshot.as_ref().is_some_and(|snapshot| {
                snapshot.boot_id == state.boot_id
                    && snapshot
                        .panes
                        .iter()
                        .any(|pane| pane.pane_id == state.mode.pane_id())
            });
        if !current {
            // Nothing to scroll back on a connection that is gone.
            self.close_copy_search(window, cx);
            self.copy_mode = None;
            cx.notify();
            return;
        }
        // Its pane left the screen with its tab; the keyboard must not stay
        // with a mode nobody can see.
        if self.live.surface_ready()
            && pane_of(self.live.surface.as_deref(), state.mode.pane_id()).is_none()
        {
            self.close_copy_search(window, cx);
            self.leave_copy_mode(cx);
            return;
        }
        let answer = state.mode.in_flight().and_then(|request| {
            let answer = state.inbox.try_lock().ok()?.take(request)?;
            Some((request.to_owned(), answer))
        });
        if let Some((request, answer)) = answer {
            match state.mode.answer(&request, answer) {
                Ok(true) => self.reveal_copy_cursor(cx),
                Ok(false) => {}
                Err(error) => {
                    self.local_error = Some(fl!(
                        "window-find-copy-request-failed",
                        error = error.localize()
                    ))
                }
            }
            cx.notify();
        }
        let Some(state) = &mut self.copy_mode else {
            return;
        };
        if let Some(request) = pane_of(self.live.surface.as_deref(), state.mode.pane_id())
            .and_then(|pane| state.mode.due_retry(pane))
        {
            self.send_copy_request(request, cx);
        }
        while let Some(command) = self
            .copy_mode
            .as_mut()
            .and_then(|state| state.mode.next_queued())
        {
            self.copy_mode_command(command, None, window, cx);
        }
    }

    /// The copy-mode cursor and selection to paint over `surface`.
    pub(crate) fn copy_mode_highlights(&self, surface: &PaneSurfaceFrame) -> Vec<Highlight> {
        let Some(state) = &self.copy_mode else {
            return Vec::new();
        };
        if surface.popup.is_some() {
            return Vec::new();
        }
        pane_of(Some(surface), state.mode.pane_id())
            .map(|pane| state.mode.highlights(pane))
            .unwrap_or_default()
    }

    /// A small marker in the bottom right corner of the pane in copy mode,
    /// with the last search and its count, or the search prompt while one is
    /// open.
    pub(crate) fn render_copy_mode_badge(
        &self,
        surface: Option<&PaneSurfaceFrame>,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let state = self.copy_mode.as_ref()?;
        let surface = surface.filter(|surface| surface.popup.is_none())?;
        let pane = pane_of(Some(surface), state.mode.pane_id())?;
        let cell_height = self.config.terminal.line_height();
        let rect = pane.rect;
        let theme = &self.theme;
        let bottom = f32::from(rect.y.saturating_add(rect.height)) * cell_height;
        let corner = div()
            .absolute()
            .left(px(f32::from(rect.x) * self.cell_width))
            .w(px(f32::from(rect.width) * self.cell_width))
            .flex()
            .justify_end()
            .px(px(6.));
        if let Some(prompt) = &state.prompt {
            let marker = match prompt.direction {
                SearchDirection::Forward => "/",
                SearchDirection::Backward => "?",
            };
            return Some(
                corner.top(px(bottom - 44.)).child(
                    div()
                        .id("copy-search")
                        .debug_selector(|| "copy-search".into())
                        .occlude()
                        .min_w_0()
                        .w(px(280.))
                        .flex_shrink(1.)
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .p(px(4.))
                        .rounded(px(crate::config::corners::CONTROL))
                        .border_1()
                        .border_color(rgb(theme.active))
                        .bg(rgb(theme.surface))
                        .text_color(rgb(theme.foreground))
                        .text_size(px(self.config.ui.size))
                        .shadow_md()
                        .on_key_down(cx.listener(Self::copy_search_key_down))
                        .child(div().flex_none().px(px(4.)).child(marker))
                        .child(div().flex_1().min_w_0().child(prompt.input.clone())),
                ),
            );
        }
        let label = state.mode.search_label().map_or_else(
            || fl!("window-find-copy-mode"),
            |search| fl!("window-find-copy-mode-search", search = search),
        );
        Some(
            corner.top(px(bottom - 32.)).child(
                div()
                    .debug_selector(|| "copy-mode".into())
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .px(px(8.))
                    .py(px(2.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .bg(rgb(theme.palette[3]))
                    .text_color(rgb(theme.text_on(theme.palette[3])))
                    .text_size(px(self.config.ui.size))
                    .child(label),
            ),
        )
    }
}

fn pane_of<'a>(
    surface: Option<&'a PaneSurfaceFrame>,
    pane_id: &str,
) -> Option<&'a PaneSurfacePane> {
    surface?.panes.iter().find(|pane| pane.pane_id == pane_id)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
