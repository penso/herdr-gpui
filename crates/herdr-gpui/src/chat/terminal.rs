//! Existing agents keep their own input editor. Only the upper terminal cells
//! are replaced by rich prose; the bottom cells, focus, IME, image paste and
//! approval dialogs remain the original running program's UI.

use super::{
    chats::{ChatInput, ChatPane},
    view::pane_bounds,
};
use crate::{HerdrWindow, fonts::StyledFont};
use gpui::{prelude::*, *};
use herdr_client::protocol::{
    AgentStatus, ClientSurfaceSize, CursorState, PaneSurfaceFrame, PaneSurfacePane, SurfaceRect,
};

const COMPACT_ROWS: u16 = 8;

mod resize;

/// Automatic expansion exposes a blocked agent's original question/approval.
/// Explicit compact mode still grows enough to keep its editing cursor visible.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputSize {
    #[default]
    Auto,
    Compact,
    Expanded,
    /// User-chosen height in original terminal rows; bounded by the pane.
    Rows(u16),
}

impl InputSize {
    fn expanded(self, blocked: bool) -> bool {
        match self {
            Self::Auto => blocked,
            Self::Compact => false,
            Self::Expanded => true,
            Self::Rows(rows) => rows > COMPACT_ROWS,
        }
    }

    fn rows(self, height: u16, blocked: bool) -> u16 {
        match self {
            Self::Rows(rows) => rows.clamp(3, height.saturating_sub(3).max(3)),
            _ if self.expanded(blocked) => height.saturating_sub(3).max(COMPACT_ROWS),
            _ => COMPACT_ROWS,
        }
        .min(height)
    }
}

/// What the bar above an observed agent's editor says of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputState {
    /// The daemon's frame is larger than this window shows, so the editor
    /// lies outside it until the window is activated and fits the frame.
    AwaitingFit,
    /// The agent asks a question or an approval in its editor.
    Blocked,
    /// The editor takes the user's input as usual.
    Ready,
}

/// All rectangles remain in the original terminal grid. No terminal resizing
/// or pointer-coordinate translation is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Layout {
    pub(crate) state: InputState,
    pub(crate) conversation: SurfaceRect,
    pub(crate) divider: SurfaceRect,
    pub(crate) input: SurfaceRect,
}

fn layout(
    rect: SurfaceRect,
    size: InputSize,
    blocked: bool,
    cursor: Option<&CursorState>,
    viewport: ClientSurfaceSize,
) -> Layout {
    let visible = SurfaceRect {
        width: rect.width.min(viewport.cols.saturating_sub(rect.x)),
        height: rect.height.min(viewport.rows.saturating_sub(rect.y)),
        ..rect
    };
    if visible != rect {
        // An inactive window can retain another client's larger terminal.
        // Its editor lies outside our viewport: do not label a clipped tail
        // as input, or move terminal cells away from their input coordinates.
        let bottom = visible.y.saturating_add(visible.height);
        return Layout::split(InputState::AwaitingFit, visible, bottom);
    }
    let bottom = rect.y.saturating_add(rect.height);
    let mut top = bottom.saturating_sub(size.rows(rect.height, blocked));
    // Multiline input and menus can move the cursor above the compact strip.
    // Keep the actual caret visible instead of hiding part of the editor.
    if let Some(cursor) = cursor.filter(|cursor| {
        cursor.visible
            && (rect.x..rect.x.saturating_add(rect.width)).contains(&cursor.x)
            && (rect.y..bottom).contains(&cursor.y)
    }) {
        top = top.min(cursor.y.saturating_sub(1).max(rect.y));
    }
    let state = if blocked {
        InputState::Blocked
    } else {
        InputState::Ready
    };
    Layout::split(state, rect, top)
}

impl Layout {
    /// `rect` split at row `top`: the conversation above, then a one-row
    /// divider when there is room for one, then the editor to the bottom.
    fn split(state: InputState, rect: SurfaceRect, top: u16) -> Self {
        let bottom = rect.y.saturating_add(rect.height);
        let line = u16::from(top > rect.y);
        Self {
            state,
            conversation: SurfaceRect {
                height: top - line - rect.y,
                ..rect
            },
            divider: SurfaceRect {
                y: top - line,
                height: line,
                ..rect
            },
            input: SurfaceRect {
                y: top,
                height: bottom - top,
                ..rect
            },
        }
    }
}

impl HerdrWindow {
    pub(crate) fn chat_terminal_layout(
        &self,
        pane: &PaneSurfacePane,
        surface: &PaneSurfaceFrame,
    ) -> Option<Layout> {
        let chat = self.chats.pane(&pane.pane_id)?;
        if !self.chats.shown(&pane.pane_id) || chat.input() != ChatInput::Terminal {
            return None;
        }
        let status = self.live.agent_status(&pane.pane_id);
        let blocked = status == Some(AgentStatus::Blocked);
        let editing = matches!(
            status,
            Some(AgentStatus::Idle | AgentStatus::Done | AgentStatus::Blocked)
        );
        Some(layout(
            pane.inner_rect,
            chat.terminal_size,
            blocked,
            // A working agent may leave its output cursor near the top. Only
            // its interactive editor/dialog cursor should grow this region.
            surface
                .frame
                .cursor
                .as_ref()
                .filter(|_| pane.focused && editing),
            self.options.surface_size,
        ))
    }

    /// A small separator above the uncovered terminal editor. Clicking its
    /// label focuses that editor; the only action is changing visible height.
    pub(super) fn render_chat_terminal_bar(
        &self,
        chat: &ChatPane,
        layout: Layout,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let bounds = pane_bounds(
            layout.divider,
            self.cell_width,
            self.config.terminal.line_height(),
        );
        let pane = chat.target.pane_id.clone();
        let focus_pane = pane.clone();
        let expanded = chat
            .terminal_size
            .expanded(layout.state == InputState::Blocked);
        let awaiting_fit = layout.state == InputState::AwaitingFit;
        let theme = &self.theme;
        let bar = div()
            .id(SharedString::from(format!("chat-terminal-input-{pane}")))
            .debug_selector(|| "chat-terminal-input".into())
            .absolute()
            .left(bounds.origin.x)
            .top(bounds.origin.y)
            .w(bounds.size.width)
            .h(bounds.size.height)
            .occlude()
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_between()
            .px(px(8.))
            .border_t_1()
            .border_color(rgb(theme.active))
            .bg(rgb(theme.surface))
            .text_color(rgb(theme.subtext()))
            .text_font(&self.config.ui)
            .text_size(px(self.config.terminal.size.min(12.)))
            .when(awaiting_fit, |bar| bar.cursor_pointer())
            .child(div().min_w_0().truncate().child(match layout.state {
                InputState::AwaitingFit => "Activate to fit agent input",
                InputState::Blocked => "Question or approval pending · original agent input",
                InputState::Ready => "Agent input · same session",
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.focus_existing_chat_input(&focus_pane, window, cx);
                }),
            )
            .child(
                div()
                    .id("chat-expand-input")
                    .debug_selector(move || {
                        if awaiting_fit {
                            "chat-fit-input".into()
                        } else {
                            "chat-expand-input".into()
                        }
                    })
                    .flex_none()
                    .cursor_pointer()
                    .px(px(6.))
                    .hover(|button| button.bg(rgb(theme.active)))
                    .child(if awaiting_fit {
                        "Fit input"
                    } else if expanded {
                        "Compact input"
                    } else {
                        "Expand input"
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        if !awaiting_fit && let Some(chat) = this.chats.pane_mut(&pane) {
                            chat.terminal_size = if expanded {
                                InputSize::Compact
                            } else {
                                InputSize::Expanded
                            };
                        }
                        this.focus_existing_chat_input(&pane, window, cx);
                        this.redraw_terminal(cx);
                    })),
            );
        self.resizable_chat_input(bar, chat, layout, cx)
            .into_any_element()
    }

    pub(super) fn focus_existing_chat_input(
        &mut self,
        pane: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_input();
        window.focus(&self.focus, cx);
        if self.input_ready() {
            self.focus_clicked_pane(pane, cx);
        }
    }
}

#[cfg(test)]
mod tests;
