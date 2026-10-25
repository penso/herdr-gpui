//! The chat view over a pane: a header, the entries on a timeline, what
//! waits on the user, the composer, and a rail with the plan and the files
//! the turn changed. Everything is drawn from the session and the bodies
//! prepared when entries changed; nothing here reads a socket. Program text
//! is untrusted and only ever drawn as text.

mod composer;
mod embeds;
mod entries;
mod layout;
mod panels;
mod rail;
mod settings;
mod summary;

pub(super) use settings::matching;
mod visuals;

use super::chats::ChatPane;
use crate::{
    HerdrWindow,
    config::{Theme, corners},
    fonts::StyledFont,
};
use gpui::{prelude::*, *};
use herdr_chat_protocol::{Command, Status};
use herdr_client::protocol::{PaneSurfaceFrame, SurfaceRect};

/// Where a pane's cells are, relative to the terminal canvas: the box the
/// chat view paints in and takes the pointer over, and that its composer
/// places the IME in.
pub(crate) fn pane_bounds(rect: SurfaceRect, cell_width: f32, cell_height: f32) -> Bounds<Pixels> {
    Bounds::new(
        point(
            px(f32::from(rect.x) * cell_width),
            px(f32::from(rect.y) * cell_height),
        ),
        size(
            px(f32::from(rect.width) * cell_width),
            px(f32::from(rect.height) * cell_height),
        ),
    )
}

/// Widest the entries and the composer are drawn; a wider pane centers
/// them, so lines stay short enough to read.
const COLUMN: f32 = 860.;

/// `content` in the centered reading column.
pub(super) fn column(content: impl IntoElement) -> Div {
    div()
        .w_full()
        .flex()
        .justify_center()
        .child(div().w_full().min_w_0().max_w(px(COLUMN)).child(content))
}

impl HerdrWindow {
    /// The chat's text size: the terminal's, since the chat stands in for
    /// the pane's terminal and should read the same size.
    pub(in crate::chat) fn chat_text_size(&self) -> f32 {
        self.config.terminal.size
    }
}

/// The logo of the agent a chat runs, by its display name.
pub(super) fn agent_icon(agent: &str) -> &'static str {
    crate::icons::AgentIcon::from_identity(Some(agent.to_ascii_lowercase().as_str())).path()
}

/// Theme colors the view uses, by meaning.
pub(super) struct Hues {
    pub(super) blue: u32,
    pub(super) green: u32,
    pub(super) red: u32,
    pub(super) yellow: u32,
}

impl Hues {
    pub(super) fn of(theme: &Theme) -> Self {
        Self {
            blue: theme.ink(theme.palette[4]),
            green: theme.ink(theme.palette[2]),
            red: theme.ink(theme.palette[1]),
            yellow: theme.ink(theme.palette[3]),
        }
    }
}

fn status_label(status: &Status) -> &'static str {
    match status {
        Status::Starting => "Starting",
        Status::Idle => "Idle",
        Status::Working => "Working",
        Status::Failed { .. } => "Failed",
        Status::Exited { .. } => "Exited",
    }
}

/// The status, with how long the running turn has taken once it passes a
/// second.
pub(super) fn status_text(transcript: &herdr_chat_protocol::Transcript) -> String {
    let label = status_label(&transcript.status);
    match transcript.turn {
        Some(turn) if transcript.status == Status::Working => {
            let elapsed = super::now_ms().saturating_sub(turn.started_ms);
            if elapsed < 1000 {
                return label.to_owned();
            }
            format!(
                "{label} {}",
                herdr_chat_protocol::display::duration(elapsed)
            )
        }
        _ => label.to_owned(),
    }
}

impl HerdrWindow {
    /// The chat views over the panes of the frame on screen. A popup covers
    /// the panes, so none are drawn under it.
    pub(crate) fn render_chats(
        &self,
        surface: Option<&PaneSurfaceFrame>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(surface) = surface.filter(|surface| surface.popup.is_none()) else {
            return Vec::new();
        };
        let cell_height = self.config.terminal.line_height();
        surface
            .panes
            .iter()
            .filter_map(|pane| {
                let chat = self.chats.pane(&pane.pane_id)?;
                if !chat.available() {
                    return None;
                }
                let bounds = pane_bounds(pane.inner_rect, self.cell_width, cell_height);
                Some(if self.chats.raw(&pane.pane_id) {
                    self.render_chat_switch(&pane.pane_id, bounds, cx)
                } else {
                    self.render_chat(chat, bounds, cx)
                })
            })
            .collect()
    }

    /// The small button over a pane turned back to its terminal, which
    /// brings the chat view back.
    fn render_chat_switch(
        &self,
        pane_id: &str,
        bounds: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = &self.theme;
        let pane = pane_id.to_owned();
        div()
            .absolute()
            .left(bounds.origin.x)
            .top(bounds.origin.y)
            .w(bounds.size.width)
            .flex()
            .justify_end()
            .p(px(6.))
            .child(
                div()
                    .id(SharedString::from(format!("chat-switch-{pane_id}")))
                    .debug_selector(|| "chat-switch".into())
                    .occlude()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .px(px(8.))
                    .py(px(3.))
                    .rounded(px(corners::CONTROL))
                    .border_1()
                    .border_color(rgb(theme.active))
                    .bg(rgb(theme.surface))
                    .text_color(rgb(theme.foreground))
                    .text_size(px(self.chat_text_size()))
                    .text_font(&self.config.ui)
                    .cursor_pointer()
                    .hover(|button| button.bg(rgb(theme.active)))
                    .child(
                        svg()
                            .path("icons/note.svg")
                            .size(px(12.))
                            .text_color(rgb(theme.foreground)),
                    )
                    .child("Chat")
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.set_chat_raw(&pane, false, window, cx);
                    })),
            )
            .into_any_element()
    }

    fn render_chat(
        &self,
        chat: &ChatPane,
        bounds: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = &self.theme;
        let pane_id = chat.target.pane_id.clone();
        let entries = {
            let pane_id = pane_id.clone();
            list(
                chat.list.clone(),
                cx.processor(move |this, index: usize, _, cx| {
                    this.render_chat_entry(&pane_id, index, cx)
                }),
            )
            .flex_1()
            .min_h_0()
            // Margins rather than padding: the list leaves its padding out
            // of the end it reports, which would stop the glide short.
            .my(px(8.))
        };
        let focus_pane = pane_id.clone();
        let key_pane = pane_id.clone();
        let capture_pane = pane_id.clone();
        div()
            .id(SharedString::from(format!("chat-{pane_id}")))
            .debug_selector(|| "chat-view".into())
            .absolute()
            .left(bounds.origin.x)
            .top(bounds.origin.y)
            .w(bounds.size.width)
            .h(bounds.size.height)
            .occlude()
            .overflow_hidden()
            .flex()
            .flex_col()
            // A page in the panel shows from beneath the window's drawing
            // on macOS, so the chat's background paints around it.
            .when(!cfg!(target_os = "macos"), |view| {
                view.bg(rgb(theme.background))
            })
            // Files dragged over the chat become attachments when dropped.
            .drag_over::<ExternalPaths>({
                let wash = theme.primary_wash();
                move |style, _, _, _| style.bg(rgb(wash))
            })
            .text_color(rgb(theme.foreground))
            .text_size(px(self.chat_text_size()))
            .text_font(&self.config.ui)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.focus_chat(&focus_pane, window, cx);
                }),
            )
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                this.chat_key_down(&key_pane, event, window, cx);
            }))
            .capture_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                this.chat_capture_key(&capture_pane, event, window, cx);
            }))
            .capture_action(cx.listener({
                let pane = pane_id.clone();
                move |this, _: &crate::actions::Paste, window, cx| {
                    if this.chat_paste(&pane, window, cx) {
                        cx.stop_propagation();
                    }
                }
            }))
            .children(self.chat_ground())
            .children(self.chat_pages_check(chat))
            .child(self.render_chat_header(chat, rail::fits(bounds), cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(entries)
                            .child(chat.glide.watch(&chat.list))
                            .children(self.render_chat_working(chat).map(column))
                            .children(
                                self.render_chat_permissions(chat, cx)
                                    .into_iter()
                                    .map(column),
                            )
                            .children(self.render_chat_queue(chat, cx).into_iter().map(column))
                            .children(self.render_chat_completions(chat, cx).map(column))
                            .child(column(self.render_chat_composer(chat, cx))),
                    )
                    .children(self.render_chat_rail(chat, bounds)),
            )
            .into_any_element()
    }

    /// The layout's sign that the agent is working on a turn.
    fn render_chat_working(&self, chat: &ChatPane) -> Option<Div> {
        let working = chat.session.transcript().status == Status::Working;
        layout::layout_for(self.config.chat.layout).working(working, &self.chat_look())
    }

    /// The chat's background where no page shows: on macOS, where a page
    /// lies beneath the window's drawing. Elsewhere the view paints its
    /// own background.
    fn chat_ground(&self) -> Option<AnyElement> {
        #[cfg(target_os = "macos")]
        return Some(
            crate::browser::ground_around_pages(
                rgb(self.theme.background).into(),
                self.browser.drawn_pages(),
            )
            .into_any_element(),
        );
        #[cfg(not(target_os = "macos"))]
        None
    }

    /// The check that asks for another frame when this chat's pages moved
    /// into or out of view; see [`super::page`].
    fn chat_pages_check(&self, chat: &ChatPane) -> Option<AnyElement> {
        #[cfg(any(target_os = "macos", windows))]
        {
            if chat.pages.is_empty() {
                return None;
            }
            let ids = chat.pages.iter().map(|page| page.id).collect();
            Some(
                self.chat_pages_settle(ids, self.chats.presented.clone(), super::page::PAGE_HEIGHT)
                    .into_any_element(),
            )
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            let _ = chat;
            None
        }
    }

    /// The terminal's background where no page shows, for the window's
    /// terminal area on macOS; see [`Self::chat_ground`].
    pub(crate) fn terminal_ground(&self, color: Hsla) -> Option<AnyElement> {
        #[cfg(target_os = "macos")]
        return Some(
            crate::browser::ground_around_pages(color, self.browser.drawn_pages())
                .into_any_element(),
        );
        #[cfg(not(target_os = "macos"))]
        {
            let _ = color;
            None
        }
    }

    /// Gives the chat's composer the keyboard and makes its pane Herdr's
    /// focused one.
    fn focus_chat(&mut self, pane_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.note_input();
        if let Some(input) = self
            .chats
            .pane(pane_id)
            .and_then(|pane| pane.composer.clone())
        {
            let focus = input.read(cx).focus.clone();
            window.focus(&focus, cx);
        }
        if self.input_ready() {
            self.focus_clicked_pane(pane_id, cx);
        }
        if let Some(pane) = self.chats.pane_mut(pane_id)
            && pane.menu.is_some()
        {
            pane.menu = None;
            pane.menu_search = None;
            cx.notify();
        }
    }

    /// The agent and its status, Stop while a turn runs, and the buttons
    /// for the layout, the rail, and the terminal. `rail_fits` says whether the pane is
    /// wide enough to show the rail at all.
    fn render_chat_header(&self, chat: &ChatPane, rail_fits: bool, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let hues = Hues::of(theme);
        let transcript = chat.session.transcript();
        let status = &transcript.status;
        let dot = match status {
            Status::Starting => theme.muted,
            Status::Idle => hues.green,
            Status::Working => hues.yellow,
            Status::Failed { .. } | Status::Exited { .. } => hues.red,
        };
        let pane_id = chat.target.pane_id.clone();
        let button = |id: &'static str| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .flex_none()
                .flex()
                .items_center()
                .gap(px(4.))
                .px(px(6.))
                .h(px(22.))
                .rounded(px(corners::CONTROL))
                .cursor_pointer()
                .hover(|button| button.bg(rgb(theme.active)))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        };
        let cancel = (*status == Status::Working).then(|| {
            let pane_id = pane_id.clone();
            button("chat-cancel")
                .text_color(rgb(hues.red))
                .child("Stop")
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.chat_send(&pane_id, Command::Cancel, cx);
                }))
        });
        let watching = (transcript.clients > 1).then(|| {
            div()
                .flex_none()
                .text_color(rgb(theme.subtext()))
                .child(format!("{} watching", transcript.clients))
        });
        let reconnecting = (!chat.attached()).then(|| {
            div()
                .flex_none()
                .text_color(rgb(hues.yellow))
                .child("Reconnecting\u{2026}")
        });
        let layout = {
            let current = self.config.chat.layout;
            button("chat-layout")
                .text_size(px((self.chat_text_size() - 1.).max(10.)))
                .text_color(rgb(theme.subtext()))
                .child(current.label())
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.set_chat_layout(current.next(), cx);
                }))
        };
        let rail = self.render_chat_rail_controls(chat, rail_fits, cx);
        let terminal = {
            let pane_id = pane_id.clone();
            button("chat-terminal")
                .child(
                    svg()
                        .path("icons/terminal.svg")
                        .size(px(13.))
                        .text_color(rgb(theme.foreground)),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    this.set_chat_raw(&pane_id, true, window, cx);
                }))
        };
        let message = match status {
            Status::Failed { message } | Status::Exited { message } if !message.is_empty() => {
                Some(crate::notifications::safe_text(message, 400))
            }
            _ => None,
        };
        div()
            .flex_none()
            .flex()
            .flex_col()
            .border_b_1()
            .border_color(rgb(theme.active))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(12.))
                    .h(px(38.))
                    .child(
                        svg()
                            .flex_none()
                            .path(agent_icon(chat.session.agent()))
                            .size(px(15.))
                            .text_color(rgb(theme.foreground)),
                    )
                    .child(
                        div()
                            .debug_selector(|| "chat-agent".into())
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(crate::notifications::safe_text(chat.session.agent(), 80)),
                    )
                    .child(div().flex_none().size(px(7.)).rounded_full().bg(rgb(dot)))
                    .child(
                        div()
                            .debug_selector(|| "chat-status".into())
                            .flex_none()
                            .text_color(rgb(theme.subtext()))
                            .child(status_text(transcript)),
                    )
                    .child(div().flex_1())
                    .children(reconnecting)
                    .children(watching)
                    .children(cancel)
                    .child(layout)
                    .children(rail)
                    .child(terminal),
            )
            .children(message.map(|message| {
                div()
                    .px(px(10.))
                    .pb(px(6.))
                    .text_color(rgb(hues.red))
                    .child(message)
            }))
    }
}
