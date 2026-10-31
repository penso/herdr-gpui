use super::*;
use crate::chat::{
    prepared::tool::COMPACT_LOCATIONS,
    view::{copy, visuals::Frame},
};

fn tool_state(
    activity: Option<ActivityState>,
    status: ToolStatus,
    kind: ToolKind,
) -> (&'static str, usize) {
    let word = super::tool_state(activity, status);
    match (activity, status, kind) {
        (None, ToolStatus::Pending, _) => ("waiting", 3),
        (None, ToolStatus::InProgress, ToolKind::Think) => ("thinking", 5),
        (Some(ActivityState::Running), _, _) | (None, ToolStatus::InProgress, _) => (word, 4),
        (Some(ActivityState::Finished), _, _) | (None, ToolStatus::Completed, _) => (word, 2),
        (Some(ActivityState::Failed), _, _) | (None, ToolStatus::Failed, _) => (word, 1),
        (Some(ActivityState::Killed), _, _) => (word, 3),
    }
}

impl HerdrWindow {
    pub(super) fn render_chat_tool(
        &self,
        pane_id: &str,
        entry: herdr_chat_protocol::EntryId,
        tool: &Tool,
        body: Option<&Body>,
        visuals: &Visuals,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let chat = self.chats.pane(pane_id);
        // A background task can outlive the tool call that started it.
        let activity = chat.and_then(|chat| chat.session.transcript().activity(&tool.call_id));
        let (status, palette) = tool_state(
            activity.map(|activity| activity.state),
            tool.status,
            tool.kind,
        );
        let steps = chat.map_or(&[][..], |chat| chat.prepared.steps(&tool.call_id));
        let color = super::super::text_ink(theme.palette[palette], theme.background);
        let small = px((self.chat_text_size() - 1.).max(10.));
        let summary = match body {
            Some(Body::Tool(summary)) => Some(summary),
            _ => None,
        };
        let expanded = chat.is_some_and(|chat| chat.expanded.contains(&entry));
        let label = summary
            .map(|summary| summary.label.clone())
            .unwrap_or_else(|| crate::chat::prepared::tool::label(&tool.title, None));
        // The gutter's dot carries the state's color; the row stays plain.
        div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .map(|header| {
                        #[cfg(feature = "integration-test")]
                        {
                            crate::chat::preview::disclosure::track_header(pane_id, entry, header)
                        }
                        #[cfg(not(feature = "integration-test"))]
                        {
                            header
                        }
                    })
                    .id(("chat-tool", entry.0))
                    .debug_selector(move || format!("chat-tool-{}", entry.0))
                    .role(Role::Button)
                    .aria_label(format!("Tool details: {label}"))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .cursor_pointer()
                    .child(
                        chevron(expanded, theme)
                            .debug_selector(move || format!("chat-tool-toggle-{}", entry.0)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(color))
                            .child(tool_kind(tool.kind)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_font(&self.config.terminal)
                            .text_size(self.chat_tool_text())
                            .text_color(rgb(theme.subtext()))
                            .child(self.render_chat_text(
                                pane_id,
                                entry,
                                Region::Label,
                                Cell::default(),
                                label,
                                cx,
                            )),
                    )
                    .when(!steps.is_empty(), |header| {
                        header.child(
                            div()
                                .flex_none()
                                .text_size(small)
                                .text_color(rgb(theme.subtext()))
                                .child(steps_label(steps.len())),
                        )
                    })
                    .child(
                        div()
                            .flex_none()
                            .text_size(small)
                            .text_color(rgb(color))
                            .child(status),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(small)
                            .text_color(rgb(theme.subtext()))
                            .child(if expanded { "Collapse" } else { "Expand" }),
                    )
                    .map(|header| {
                        self.chat_disclosure(
                            header,
                            pane_id,
                            entry,
                            expanded,
                            HeaderText::Label,
                            cx,
                        )
                    }),
            )
            .when(expanded && !steps.is_empty(), |card| {
                card.children(chat.map(|chat| self.render_chat_steps(chat, entry, steps)))
            })
            .children(
                page_written(tool)
                    .filter(|_| expanded)
                    .map(|path| self.render_chat_file_page(pane_id, entry, path, cx)),
            )
            .children(self.render_chat_tool_glance(
                (pane_id, entry),
                tool,
                summary,
                expanded,
                visuals,
                cx,
            ))
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
                        })),
                )
            })
            .children(summary.into_iter().flat_map(|summary| {
                summary
                    .previews
                    .iter()
                    .take(if expanded { usize::MAX } else { 0 })
                    .enumerate()
                    .map(|(index, preview)| {
                        let scope =
                            self.chat_text_scope(pane_id, entry, Region::ToolPreview(index));
                        div()
                            .id(SharedString::from(format!(
                                "chat-tool-content-{}-{index}",
                                entry.0
                            )))
                            .debug_selector(move || {
                                format!("chat-tool-content-{}-{index}", entry.0)
                            })
                            .min_w_0()
                            .w_full()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .py(px(3.))
                            .child(
                                div()
                                    .flex()
                                    .justify_between()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(
                                        div()
                                            .min_w_0()
                                            .truncate()
                                            .text_size(small)
                                            .text_color(rgb(theme.subtext()))
                                            .child(self.render_chat_text(
                                                pane_id,
                                                entry,
                                                Region::ToolPreviewLabel(index),
                                                Cell::default(),
                                                preview.label.clone(),
                                                cx,
                                            )),
                                    )
                                    .child(self.rich_copy(
                                        copy::ORIGINAL,
                                        preview.original.clone(),
                                        cx,
                                    )),
                            )
                            .when(expanded, |card| match &preview.before {
                                Some(before) => {
                                    card.child(div().text_size(small).child("Before")).child(
                                        div().id("before").child(
                                            self.render_chat_tool_code(
                                                before,
                                                self.chat_text_scope(
                                                    pane_id,
                                                    entry,
                                                    Region::ToolBefore(index),
                                                )
                                                .as_ref(),
                                                true,
                                                visuals,
                                                cx,
                                            ),
                                        ),
                                    )
                                }
                                None => card,
                            })
                            .when(preview.before.is_some(), |card| {
                                card.child(div().text_size(small).child("After"))
                            })
                            .child(div().id("after").child(self.render_chat_tool_code(
                                &preview.code,
                                scope.as_ref(),
                                expanded,
                                visuals,
                                cx,
                            )))
                    })
                    .collect::<Vec<_>>()
            }))
            .children(
                summary
                    .filter(|_| expanded)
                    .and_then(|summary| summary.remainder.as_ref())
                    .map(|source| {
                        div()
                            .text_size(small)
                            .text_color(rgb(theme.muted))
                            .child("Additional retained output: ")
                            .child(self.rich_copy(copy::REMAINING, source.clone(), cx))
                    }),
            )
    }

    /// Folded tools keep a bounded file/location summary. Output panels and
    /// images belong to the open card, so even short tools visibly collapse.
    fn render_chat_tool_glance(
        &self,
        (pane_id, entry): (&str, herdr_chat_protocol::EntryId),
        tool: &Tool,
        summary: Option<&ToolSummary>,
        expanded: bool,
        visuals: &Visuals,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(summary) = summary else {
            return Vec::new();
        };
        let theme = &self.theme;
        let hues = Hues::of(theme);
        let small = px((self.chat_text_size() - 1.).max(10.));
        let mut glance = Vec::new();
        for (index, diff) in summary
            .diffs
            .iter()
            .take(if expanded {
                usize::MAX
            } else {
                COMPACT_LOCATIONS
            })
            .enumerate()
        {
            glance.push(
                div()
                    .debug_selector(move || format!("chat-tool-diff-{}-{index}", entry.0))
                    .flex()
                    .gap(px(6.))
                    .text_size(small)
                    .child(div().min_w_0().truncate().child(safe_text(&diff.path, 300)))
                    .child(
                        div()
                            .flex_none()
                            .text_color(rgb(super::super::text_ink(hues.green, theme.background)))
                            .child(format!("+{}", diff.added)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_color(rgb(super::super::text_ink(hues.red, theme.background)))
                            .child(format!("−{}", diff.removed)),
                    )
                    .into_any_element(),
            );
        }
        if expanded && !summary.images.is_empty() {
            glance.push(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(6.))
                    .py(px(2.))
                    .children(summary.images.iter().map(|id| {
                        self.render_chat_visual(pane_id, *id, visuals, Frame::ToolImage, cx)
                    }))
                    .into_any_element(),
            );
        }
        if expanded || !layout_for(self.config.chat.layout).tool_preview() {
            return glance;
        }
        let more_files = summary.diffs.len().saturating_sub(COMPACT_LOCATIONS);
        if more_files > 0 {
            glance.push(
                div()
                    .text_size(small)
                    .text_color(rgb(theme.muted))
                    .child(format!("and {more_files} more changed files"))
                    .into_any_element(),
            );
        }
        if !summary.images.is_empty() {
            glance.push(
                div()
                    .text_size(small)
                    .text_color(rgb(theme.muted))
                    .child(format!("{} attached images", summary.images.len()))
                    .into_any_element(),
            );
        }
        for (index, location) in tool.locations.iter().take(COMPACT_LOCATIONS).enumerate() {
            glance.push(
                div()
                    .text_size(small)
                    .text_color(rgb(theme.subtext()))
                    .truncate()
                    .child(self.render_chat_text(
                        pane_id,
                        entry,
                        Region::ToolLocations,
                        Cell::row(index),
                        safe_text(location, 300),
                        cx,
                    ))
                    .into_any_element(),
            );
        }
        let more = tool.locations.len().saturating_sub(COMPACT_LOCATIONS);
        if more > 0 {
            glance.push(
                div()
                    .text_size(small)
                    .text_color(rgb(theme.muted))
                    .child(format!("and {more} more locations"))
                    .into_any_element(),
            );
        }
        if summary.terminals > 0 {
            glance.push(
                div()
                    .text_size(small)
                    .text_color(rgb(theme.muted))
                    .child("Terminal output shows in the program's own view")
                    .into_any_element(),
            );
        }
        glance
    }
}

#[cfg(test)]
mod tests;
