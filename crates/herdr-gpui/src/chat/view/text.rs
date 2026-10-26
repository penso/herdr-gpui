//! Selection for prepared fallback lines and the plain text of activity cards.

use super::selection::Scope;
use crate::chat::selection::{Cell, Region};
use crate::{HerdrWindow, release_notes};
use gpui::{prelude::*, *};
use herdr_chat_protocol::EntryId;

impl HerdrWindow {
    pub(in crate::chat) fn chat_text_scope<'a>(
        &'a self,
        pane_id: &'a str,
        entry: EntryId,
        region: Region,
    ) -> Option<Scope<'a>> {
        let chat = self.chats.pane(pane_id)?;
        Some(Scope::new(
            pane_id,
            entry,
            region,
            chat.prepared.text(),
            &chat.selection,
            &chat.text_geometry,
        ))
    }

    /// One selectable run of an entry's plain text, at `cell` of `region`.
    /// It is drawn plain when the selection document holds other text there.
    pub(super) fn render_chat_text(
        &self,
        pane_id: &str,
        entry: EntryId,
        region: Region,
        cell: Cell,
        text: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> Div {
        let text = text.into();
        let scope = self.chat_text_scope(pane_id, entry, region);
        let Some(fragment) = scope.and_then(|scope| {
            (scope.text(cell) == Some(text.as_ref()))
                .then(|| scope.fragment(cell))
                .flatten()
        }) else {
            return div().child(text);
        };
        let styled = fragment.styled(text, [], &self.theme);
        let layout = styled.layout().clone();
        fragment.wrap(div().min_w_0().child(styled), layout, cx)
    }

    pub(super) fn render_chat_lines(
        &self,
        id: &'static str,
        scope: Option<Scope<'_>>,
        lines: &[release_notes::Line],
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .min_w_0()
            .children(lines.iter().enumerate().map(|(index, line)| {
                let Some(fragment) = scope
                    .as_ref()
                    .and_then(|scope| scope.fragment(Cell::row(index)))
                else {
                    return release_notes::line(
                        id,
                        index,
                        line,
                        &self.theme,
                        &self.config.terminal,
                    );
                };
                let styled = fragment.styled(
                    line.text.clone(),
                    release_notes::line_highlights(
                        line,
                        &self.theme,
                        self.theme.ink(self.theme.palette[4]),
                    ),
                    &self.theme,
                );
                let layout = styled.layout().clone();
                let content = if line.links.is_empty() {
                    styled.into_any_element()
                } else {
                    let links = line.links.clone();
                    let guard = fragment.clone();
                    InteractiveText::new(("chat-line-link", index), styled)
                        .on_click(
                            links.iter().map(|link| link.range.clone()).collect(),
                            move |clicked, _, cx| {
                                if guard.allows_link()
                                    && let Some(link) = links.get(clicked)
                                {
                                    cx.open_url(&link.url);
                                }
                            },
                        )
                        .into_any_element()
                };
                let selected = fragment
                    .wrap(div().min_w_0().child(content), layout, cx)
                    .into_any_element();
                release_notes::line_with(
                    id,
                    index,
                    line,
                    &self.theme,
                    &self.config.terminal,
                    selected,
                )
            }))
    }
}
