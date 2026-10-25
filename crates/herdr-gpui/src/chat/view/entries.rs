//! What one transcript entry shows: a message, a thought, a tool call, a
//! picture, a notice, or how a turn ended. The chat's layout decides the
//! row it sits in.

use super::{
    super::{chats::ChatPane, prepared::Body, visual::Visuals},
    Hues,
    layout::{Row, layout_for, side},
    visuals::Frame,
};
use crate::{HerdrWindow, config::corners, fonts::StyledFont, notifications::safe_text};
use gpui::{prelude::*, *};
use herdr_chat_protocol::{Entry, EntryKind, NoticeLevel, Tool, ToolKind, ToolStatus};

/// Locations listed under a tool call.
const LOCATIONS: usize = 3;
/// Characters of a folded thought shown.
const FOLDED: usize = 120;
fn tool_kind(kind: ToolKind) -> &'static str {
    match kind {
        ToolKind::Read => "Read",
        ToolKind::Edit => "Edit",
        ToolKind::Delete => "Delete",
        ToolKind::Move => "Move",
        ToolKind::Search => "Search",
        ToolKind::Execute => "Run",
        ToolKind::Think => "Think",
        ToolKind::Fetch => "Fetch",
        ToolKind::SwitchMode => "Mode",
        ToolKind::Other => "Tool",
    }
}

impl HerdrWindow {
    pub(super) fn render_chat_entry(
        &self,
        pane_id: &str,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(chat) = self.chats.pane(pane_id) else {
            return div().into_any_element();
        };
        let entries = &chat.session.transcript().entries;
        let Some(entry) = entries.get(index) else {
            return div().into_any_element();
        };
        let body = chat.prepared.body(entry.id);
        let content = match &entry.kind {
            EntryKind::User { text } => div().child(text.clone()),
            EntryKind::Agent { text } => {
                let message = match body.as_deref() {
                    Some(Body::Message(parts)) => self.render_chat_message(
                        pane_id,
                        entry.id,
                        parts,
                        &chat.prepared.visuals,
                        cx,
                    ),
                    Some(Body::Summary(summary)) => {
                        // Copy takes the whole summary, as its cut note says.
                        let card = div()
                            .group("chat-agent-message")
                            .child(self.render_chat_summary(
                                pane_id,
                                entry,
                                summary,
                                chat.expanded.contains(&entry.id),
                                cx,
                            ))
                            .child(self.render_chat_copy(entry.id.0, text, cx));
                        return self.render_chat_entry_row(chat, index, card);
                    }
                    _ => div().child(text.clone()),
                };
                div()
                    .group("chat-agent-message")
                    .child(message)
                    .child(self.render_chat_copy(entry.id.0, text, cx))
            }
            EntryKind::Turn(summary) => {
                layout_for(self.config.chat.layout).turn(&summary.describe(), &self.chat_look())
            }
            EntryKind::Thought { text } => self.render_chat_thought(
                pane_id,
                entry,
                text,
                chat.expanded.contains(&entry.id),
                cx,
            ),
            EntryKind::Image { author, .. } => match body.as_deref() {
                Some(Body::Image(id)) => {
                    self.render_chat_image(*author, *id, &chat.prepared.visuals)
                }
                _ => div(),
            },
            EntryKind::Tool(tool) => self.render_chat_tool(
                pane_id,
                entry.id,
                tool,
                body.as_deref(),
                &chat.prepared.visuals,
                cx,
            ),
            EntryKind::Notice { level, text } => self.render_chat_notice(*level, text),
        };
        self.render_chat_entry_row(chat, index, content)
    }

    /// An entry's row in the list, as the chat's layout draws it.
    fn render_chat_entry_row(&self, chat: &ChatPane, index: usize, content: Div) -> AnyElement {
        let entries = &chat.session.transcript().entries;
        let Some(entry) = entries.get(index) else {
            return div().into_any_element();
        };
        let near = |index: Option<usize>| {
            index
                .and_then(|index| entries.get(index))
                .and_then(|entry| side(&entry.kind))
        };
        let transcript = chat.session.transcript();
        let row = Row {
            entry,
            previous: near(index.checked_sub(1)),
            next: near(index.checked_add(1)),
            agent: chat.session.agent(),
            live: index + 1 == entries.len()
                && transcript.status == herdr_chat_protocol::Status::Working,
            content,
        };
        layout_for(self.config.chat.layout).row(row, &self.chat_look())
    }

    /// A copy button by an agent message, shown while the pointer is on it.
    fn render_chat_copy(&self, id: u64, text: &str, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let text = text.to_owned();
        layout_for(self.config.chat.layout).copy(
            div()
                .id(("chat-copy", id))
                .debug_selector(move || format!("chat-copy-{id}"))
                .invisible()
                .group_hover("chat-agent-message", |button| button.visible())
                .flex()
                .items_center()
                .gap(px(4.))
                .px(px(4.))
                .rounded(px(corners::SMALL))
                .cursor_pointer()
                .text_size(px((self.chat_text_size() - 2.).max(10.)))
                .text_color(rgb(theme.muted))
                .hover(|button| button.bg(rgb(theme.active)))
                .child(
                    svg()
                        .path("icons/copy.svg")
                        .size(px(11.))
                        .text_color(rgb(theme.muted)),
                )
                .child("Copy")
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                    this.show_flash(crate::window::Flash::success("Copied the message"), cx);
                })),
            &self.chat_look(),
        )
    }

    fn render_chat_thought(
        &self,
        pane_id: &str,
        entry: &Entry,
        text: &str,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let shown = if expanded {
            text.to_owned()
        } else {
            let line = text
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("");
            safe_text(line, FOLDED)
        };
        let pane_id = pane_id.to_owned();
        let id = entry.id;
        div().child(
            div()
                .id(("chat-thought", id.0))
                .flex()
                .gap(px(6.))
                .cursor_pointer()
                .text_color(rgb(theme.dimmed(theme.foreground)))
                .italic()
                .child(
                    svg()
                        .path(if expanded {
                            "icons/chevron-down.svg"
                        } else {
                            "icons/chevron-right.svg"
                        })
                        .flex_none()
                        .mt(px(3.))
                        .size(px(10.))
                        .text_color(rgb(theme.muted)),
                )
                .child(
                    div()
                        .min_w_0()
                        .when(!expanded, |row| row.truncate())
                        .child(shown),
                )
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    if let Some(pane) = this.chats.pane_mut(&pane_id) {
                        if !pane.expanded.remove(&id) {
                            pane.expanded.insert(id);
                        }
                        pane.remeasure(id);
                    }
                    cx.notify();
                })),
        )
    }

    fn render_chat_tool(
        &self,
        pane_id: &str,
        entry: herdr_chat_protocol::EntryId,
        tool: &Tool,
        body: Option<&Body>,
        visuals: &Visuals,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let hues = Hues::of(theme);
        let (status, color) = match tool.status {
            ToolStatus::Pending => ("pending", theme.muted),
            ToolStatus::InProgress => ("running", hues.yellow),
            ToolStatus::Completed => ("done", hues.green),
            ToolStatus::Failed => ("failed", hues.red),
        };
        let small = px((self.chat_text_size() - 1.).max(10.));
        let summary = match body {
            Some(Body::Tool(summary)) => Some(summary),
            _ => None,
        };
        let more_locations = tool.locations.len().saturating_sub(LOCATIONS);
        // The gutter's dot carries the state's color; the row stays plain.
        div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex_none()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(hues.blue))
                            .child(tool_kind(tool.kind)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_font(&self.config.terminal)
                            .text_size(px(self.config.terminal.size - 1.))
                            .text_color(rgb(theme.subtext()))
                            .child(safe_text(&tool.title, 300)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(small)
                            .text_color(rgb(color))
                            .child(status),
                    ),
            )
            .children(
                page_written(tool).map(|path| self.render_chat_file_page(pane_id, entry, path, cx)),
            )
            .children(tool.locations.iter().take(LOCATIONS).map(|location| {
                div()
                    .text_size(small)
                    .text_color(rgb(theme.subtext()))
                    .truncate()
                    .child(safe_text(location, 300))
            }))
            .children((more_locations > 0).then(|| {
                div()
                    .text_size(small)
                    .text_color(rgb(theme.muted))
                    .child(format!("and {more_locations} more"))
            }))
            .children(summary.into_iter().flat_map(|summary| {
                let diffs = summary.diffs.iter().map(|diff| {
                    div()
                        .flex()
                        .gap(px(6.))
                        .text_size(small)
                        .child(div().min_w_0().truncate().child(safe_text(&diff.path, 300)))
                        .child(
                            div()
                                .flex_none()
                                .text_color(rgb(hues.green))
                                .child(format!("+{}", diff.added)),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_color(rgb(hues.red))
                                .child(format!("\u{2212}{}", diff.removed)),
                        )
                });
                let lines = summary.lines.iter().map(|line| {
                    div()
                        .text_size(small)
                        .text_font(&self.config.terminal)
                        .text_color(rgb(theme.subtext()))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .child(line.clone())
                });
                let more = (summary.more > 0).then(|| {
                    div()
                        .text_size(small)
                        .text_color(rgb(theme.muted))
                        .child(format!("{} more lines", summary.more))
                });
                let terminals = (summary.terminals > 0).then(|| {
                    div()
                        .text_size(small)
                        .text_color(rgb(theme.muted))
                        .child("Terminal output shows in the program's own view")
                });
                let images = (!summary.images.is_empty()).then(|| {
                    div().flex().flex_wrap().gap(px(6.)).py(px(2.)).children(
                        summary
                            .images
                            .iter()
                            .map(|id| self.render_chat_visual(*id, visuals, Frame::ToolImage)),
                    )
                });
                diffs
                    .chain(images)
                    .chain(lines)
                    .chain(more)
                    .chain(terminals)
                    .collect::<Vec<_>>()
            }))
    }

    fn render_chat_notice(&self, level: NoticeLevel, text: &str) -> Div {
        let theme = &self.theme;
        let hues = Hues::of(theme);
        let color = match level {
            NoticeLevel::Info => theme.subtext(),
            NoticeLevel::Warning => hues.yellow,
            NoticeLevel::Error => hues.red,
        };
        div().text_color(rgb(color)).child(text.to_owned())
    }
}

/// The HTML page a finished tool call wrote, by an absolute path, which
/// the user may preview.
fn page_written(tool: &Tool) -> Option<std::path::PathBuf> {
    if tool.status != ToolStatus::Completed || tool.kind != ToolKind::Edit {
        return None;
    }
    tool.locations.iter().find_map(|location| {
        // A location may end with the line the call touched.
        let path = match location.rsplit_once(':') {
            Some((path, line)) if line.bytes().all(|byte| byte.is_ascii_digit()) => path,
            _ => location.as_str(),
        };
        let path = std::path::Path::new(path);
        let html = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("html") || extension.eq_ignore_ascii_case("htm")
            });
        (html && path.is_absolute()).then(|| path.to_owned())
    })
}
