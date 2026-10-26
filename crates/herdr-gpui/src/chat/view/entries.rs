//! What one transcript entry shows: a message, a thought, a tool call, a
//! picture, a notice, or how a turn ended. The chat's layout decides the
//! row it sits in.

use super::{
    super::{
        chats::ChatPane,
        prepared::{Body, Part, ToolSummary, folded_thought, notice_label, tool_header, tool_text},
        rich::{Block, Inline},
        selection::{Cell, Region},
        visual::Visuals,
    },
    Hues,
    copy::CopyPlacement,
    disclosure::{HeaderText, chevron},
    layout::{Row, layout_for, side},
    visuals::Frame,
};
use crate::{HerdrWindow, config::corners, fonts::StyledFont, notifications::safe_text};
use gpui::{prelude::*, *};
use herdr_chat_protocol::{Entry, EntryKind, NoticeLevel, Tool, ToolKind, ToolStatus};

mod questions;

/// Locations listed under a folded tool call.
const LOCATIONS: usize = 3;
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

/// An overlay copy control may cover the last block or an inline control.
/// Plain prose keeps the layout's compact, floating copy treatment.
fn copy_placement(body: Option<&Body>) -> CopyPlacement {
    let Some(Body::Message(parts)) = body else {
        return CopyPlacement::Floating;
    };
    let covers = parts.iter().any(|part| match part {
        Part::Text(_) => false,
        Part::Rich(blocks) => blocks.iter().any(|block| match block {
            Block::Paragraph { content, .. } => content.iter().any(|inline| match inline {
                Inline::Math { .. } => true,
                Inline::Text(_, style) => style.code || style.link.is_some(),
                Inline::Break => false,
            }),
            _ => true,
        }),
        _ => true,
    });
    if covers {
        CopyPlacement::Footer
    } else {
        CopyPlacement::Floating
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
            EntryKind::User { text, .. } | EntryKind::Agent { text } => {
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
                            .child(self.render_chat_copy(
                                entry.id.0,
                                text,
                                CopyPlacement::Footer,
                                cx,
                            ));
                        return self.render_chat_entry_row(chat, index, card);
                    }
                    _ => self.render_chat_text(
                        pane_id,
                        entry.id,
                        Region::Body(0),
                        Cell::default(),
                        text.clone(),
                        cx,
                    ),
                };
                div()
                    .group("chat-agent-message")
                    .child(message)
                    .child(self.render_chat_copy(
                        entry.id.0,
                        text,
                        copy_placement(body.as_deref()),
                        cx,
                    ))
            }
            EntryKind::Turn(summary) => {
                let text = self.render_chat_text(
                    pane_id,
                    entry.id,
                    Region::Body(0),
                    Cell::default(),
                    summary.describe(),
                    cx,
                );
                layout_for(self.config.chat.layout).turn(text.into_any_element(), &self.chat_look())
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
            EntryKind::Notice { .. } => self.render_chat_notice(pane_id, entry, cx),
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
            folded_thought(text)
        };
        let id = entry.id;
        let header = div()
            .id(("chat-thought", id.0))
            .debug_selector(move || format!("chat-thought-{}", id.0))
            .flex()
            .gap(px(6.))
            .text_color(rgb(theme.dimmed(theme.foreground)))
            .italic()
            .child(chevron(expanded, theme).mt(px(3.)))
            .child(
                div()
                    .id(("chat-thought-text", id.0))
                    .min_w_0()
                    .when(!expanded, |row| row.truncate())
                    .when(expanded, |row| row.overflow_x_scroll())
                    .child(self.render_chat_text(
                        pane_id,
                        id,
                        Region::Body(0),
                        Cell::default(),
                        shown,
                        cx,
                    )),
            );
        div().child(self.chat_disclosure(header, pane_id, id, expanded, HeaderText::Body, cx))
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
        let small = self.chat_small_text();
        let summary = match body {
            Some(Body::Tool(summary)) => Some(summary),
            _ => None,
        };
        let expanded = self
            .chats
            .pane(pane_id)
            .is_some_and(|chat| chat.expanded.contains(&entry));
        // The gutter's dot carries the state's color; the row stays plain.
        let header = div()
            .id(("chat-tool", entry.0))
            .debug_selector(move || format!("chat-tool-{}", entry.0))
            .flex()
            .items_center()
            .gap(px(6.))
            .child(chevron(expanded, theme))
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
                    .child(self.render_chat_text(
                        pane_id,
                        entry,
                        Region::Label,
                        Cell::default(),
                        tool_header(&tool.title),
                        cx,
                    )),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(small)
                    .text_color(rgb(color))
                    .child(status),
            );
        div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(self.chat_disclosure(header, pane_id, entry, expanded, HeaderText::Label, cx))
            .children(
                page_written(tool).map(|path| self.render_chat_file_page(pane_id, entry, path, cx)),
            )
            .children(self.render_chat_tool_glance(entry, tool, summary, expanded, visuals))
            .when(expanded, |card| {
                card.child(
                    div()
                        .id(("chat-tool-details", entry.0))
                        .debug_selector(move || format!("chat-tool-details-{}", entry.0))
                        .min_w_0()
                        .w_full()
                        .overflow_x_scroll()
                        .text_size(small)
                        .text_font(&self.config.terminal)
                        .text_color(rgb(theme.subtext()))
                        .child(self.render_chat_text(
                            pane_id,
                            entry,
                            Region::ToolTitle,
                            Cell::default(),
                            tool.title.clone(),
                            cx,
                        ))
                        .children(tool.locations.iter().enumerate().map(|(index, location)| {
                            self.render_chat_text(
                                pane_id,
                                entry,
                                Region::ToolLocations,
                                Cell::row(index),
                                location.clone(),
                                cx,
                            )
                        }))
                        .children(tool.content.iter().enumerate().filter_map(|(index, item)| {
                            tool_text(item).map(|text| {
                                // Keep the full retained output; long unbroken lines
                                // scroll horizontally instead of disappearing.
                                div()
                                    .id(SharedString::from(format!(
                                        "chat-tool-content-{}-{index}",
                                        entry.0
                                    )))
                                    .debug_selector(move || {
                                        format!("chat-tool-content-{}-{index}", entry.0)
                                    })
                                    .w_full()
                                    .overflow_x_scroll()
                                    .py(px(3.))
                                    .child(self.render_chat_text(
                                        pane_id,
                                        entry,
                                        Region::ToolContent,
                                        Cell::block(index),
                                        text,
                                        cx,
                                    ))
                            })
                        })),
                )
            })
    }

    /// What a tool call shows without being opened: each file's changed line
    /// counts and its pictures always, and while folded, its first locations
    /// and output lines. Opening it shows the full text in their place.
    fn render_chat_tool_glance(
        &self,
        entry: herdr_chat_protocol::EntryId,
        tool: &Tool,
        summary: Option<&ToolSummary>,
        expanded: bool,
        visuals: &Visuals,
    ) -> Vec<AnyElement> {
        let theme = &self.theme;
        let hues = Hues::of(theme);
        let small = self.chat_small_text();
        let note = |text: String| {
            div()
                .text_size(small)
                .text_color(rgb(theme.muted))
                .child(text)
                .into_any_element()
        };
        let mut glance = Vec::new();
        let Some(summary) = summary else {
            return glance;
        };
        glance.extend(summary.diffs.iter().enumerate().map(|(index, diff)| {
            div()
                .debug_selector(move || format!("chat-tool-diff-{}-{index}", entry.0))
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
                .into_any_element()
        }));
        if !summary.images.is_empty() {
            glance.push(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(6.))
                    .py(px(2.))
                    .children(
                        summary
                            .images
                            .iter()
                            .map(|id| self.render_chat_visual(*id, visuals, Frame::ToolImage)),
                    )
                    .into_any_element(),
            );
        }
        if expanded {
            return glance;
        }
        glance.extend(tool.locations.iter().take(LOCATIONS).map(|location| {
            div()
                .text_size(small)
                .text_color(rgb(theme.subtext()))
                .truncate()
                .child(safe_text(location, 300))
                .into_any_element()
        }));
        let more_locations = tool.locations.len().saturating_sub(LOCATIONS);
        if more_locations > 0 {
            glance.push(note(format!("and {more_locations} more")));
        }
        glance.extend(summary.lines.iter().map(|line| {
            div()
                .debug_selector(move || format!("chat-tool-preview-{}", entry.0))
                .text_size(small)
                .text_font(&self.config.terminal)
                .text_color(rgb(theme.subtext()))
                .whitespace_nowrap()
                .overflow_hidden()
                .child(line.clone())
                .into_any_element()
        }));
        if summary.more > 0 {
            glance.push(note(format!("{} more lines", summary.more)));
        }
        if summary.terminals > 0 {
            glance.push(note(
                "Terminal output shows in the program's own view".to_owned(),
            ));
        }
        glance
    }

    fn render_chat_notice(&self, pane_id: &str, entry: &Entry, cx: &mut Context<Self>) -> Div {
        let EntryKind::Notice {
            answers,
            details,
            level,
            text,
        } = &entry.kind
        else {
            return div();
        };
        if !answers.is_empty() {
            return self.render_chat_answers(pane_id, entry, cx);
        }
        self.render_chat_notice_record(pane_id, entry.id, *level, text, details.as_deref(), cx)
    }

    fn render_chat_notice_record(
        &self,
        pane_id: &str,
        id: herdr_chat_protocol::EntryId,
        level: NoticeLevel,
        text: &str,
        details: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let hues = Hues::of(theme);
        let color = match level {
            NoticeLevel::Info => theme.subtext(),
            NoticeLevel::Warning => hues.yellow,
            NoticeLevel::Error => hues.red,
        };
        let Some(details) = details else {
            return div().text_color(rgb(color)).child(self.render_chat_text(
                pane_id,
                id,
                Region::Body(0),
                Cell::default(),
                text.to_owned(),
                cx,
            ));
        };
        let expanded = self
            .chats
            .pane(pane_id)
            .is_some_and(|chat| chat.expanded.contains(&id));
        let header = div()
            .id(("chat-notice", id.0))
            .debug_selector(move || format!("chat-notice-{}", id.0))
            .flex()
            .items_center()
            .min_w_0()
            .gap(px(6.))
            .child(chevron(expanded, theme))
            .child(div().min_w_0().truncate().child(self.render_chat_text(
                pane_id,
                id,
                Region::Body(0),
                Cell::default(),
                notice_label(text),
                cx,
            )));
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .gap(px(4.))
            .text_color(rgb(color))
            .when_some(
                layout_for(self.config.chat.layout).backdrop(&self.chat_look()),
                |card, fill| card.bg(rgb(fill)).rounded(px(corners::SMALL)).p(px(4.)),
            )
            .child(self.chat_disclosure(header, pane_id, id, expanded, HeaderText::Label, cx))
            .when(expanded, |card| {
                card.child(
                    div()
                        .id(("chat-notice-details", id.0))
                        .debug_selector(move || format!("chat-notice-details-{}", id.0))
                        .group("chat-agent-message")
                        .relative()
                        .min_w_0()
                        .w_full()
                        .child(
                            div()
                                .id(("chat-notice-source", id.0))
                                .debug_selector(move || format!("chat-notice-source-{}", id.0))
                                .w_full()
                                .overflow_x_scroll()
                                .text_font(&self.config.terminal)
                                .text_size(self.chat_small_text())
                                // Imported content is inert source text, including
                                // XML, commands, URLs, and output-file paths.
                                .child(self.render_chat_text(
                                    pane_id,
                                    id,
                                    Region::Details,
                                    Cell::default(),
                                    details.to_owned(),
                                    cx,
                                )),
                        )
                        .child(self.render_chat_copy(id.0, details, CopyPlacement::Footer, cx)),
                )
            })
    }
}

#[cfg(test)]
mod tests;

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
