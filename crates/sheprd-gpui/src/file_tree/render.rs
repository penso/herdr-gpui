//! A file tree tab paints the prepared rows; reading lives elsewhere.

use super::{FileTree, State};
use crate::{
    SheprdWindow, browser::TabId, config::Theme, fonts::StyledFont, i18n::fl, review::tree::Node,
};
use gpui::{prelude::*, *};
use sheprd_vcs::file_tree::Change;

/// The letter a file's change shows as, and its colour.
fn mark(theme: &Theme, change: Change) -> (&'static str, u32) {
    match change {
        Change::Added => ("A", theme.ink(theme.palette[2])),
        Change::Conflicted => ("C", theme.ink(theme.palette[5])),
        Change::Deleted => ("D", theme.ink(theme.palette[1])),
        Change::Modified => ("M", theme.ink(theme.palette[3])),
        Change::Untracked => ("U", theme.ink(theme.palette[2])),
    }
}

impl SheprdWindow {
    fn file_tree_row_height(&self) -> f32 {
        (self.config.ui.size * 1.7).max(18.)
    }

    fn file_tree_rows(
        &mut self,
        id: TabId,
        range: std::ops::Range<usize>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let theme = &self.theme;
        let height = self.file_tree_row_height();
        let Some(view) = self.file_trees.get(&id) else {
            return Vec::new();
        };
        let Some(prepared) = view.prepared.clone() else {
            return Vec::new();
        };
        range
            .filter_map(|line| {
                let index = *view.shown.get(line)?;
                let node = prepared.tree.nodes.get(index)?;
                let row = div()
                    .id(("file-tree-row", line))
                    .debug_selector(move || format!("file-tree-row-{line}"))
                    .w_full()
                    .h(px(height))
                    .pr_2()
                    .pl(px(8. + 12. * node.depth() as f32))
                    .flex()
                    .items_center()
                    .gap_1()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .cursor_pointer()
                    .rounded(px(crate::config::corners::CONTROL))
                    .when(view.picked == Some(line), |row| row.bg(rgb(theme.active)))
                    .hover(|row| row.bg(rgb(theme.active)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.activate_file_tree_row(id, line, window, cx);
                    }));
                Some(match node {
                    Node::Folder { path, label, .. } => {
                        // A filter shows every folder open.
                        let closed = view.matched.is_none() && view.closed.contains(path);
                        let changed = prepared.changed.get(index).copied().unwrap_or(false);
                        row.text_color(rgb(theme.muted))
                            .child(
                                svg()
                                    .path(if closed {
                                        "icons/chevron-right.svg"
                                    } else {
                                        "icons/chevron-down.svg"
                                    })
                                    .flex_none()
                                    .size(px(10.))
                                    .text_color(rgb(theme.muted)),
                            )
                            .child(div().flex_1().min_w_0().truncate().child(label.clone()))
                            .when(changed, |row| {
                                row.child(
                                    div()
                                        .debug_selector(move || {
                                            format!("file-tree-folder-changed-{line}")
                                        })
                                        .flex_none()
                                        .size(px(6.))
                                        .rounded_full()
                                        .bg(rgb(theme.ink(theme.palette[3]))),
                                )
                            })
                            .into_any_element()
                    }
                    Node::File { file, .. } => {
                        let entry = prepared.entry(*file)?;
                        let name = entry.path.rsplit('/').next().unwrap_or(&entry.path);
                        let deleted = entry.change == Some(Change::Deleted);
                        row.when(deleted, |row| row.cursor_default().line_through())
                            .text_color(rgb(if deleted {
                                theme.muted
                            } else {
                                theme.foreground
                            }))
                            .child(div().flex_none().w(px(10.)))
                            .child(div().flex_1().min_w_0().truncate().child(name.to_owned()))
                            .when_some(entry.change, |row, change| {
                                let (letter, colour) = mark(theme, change);
                                row.child(
                                    div()
                                        .debug_selector(move || {
                                            format!("file-tree-change-{line}-{letter}")
                                        })
                                        .flex_none()
                                        .w(px(12.))
                                        .text_color(rgb(colour))
                                        .child(letter),
                                )
                            })
                            .into_any_element()
                    }
                })
            })
            .collect()
    }

    fn file_tree_header(&self, id: TabId, view: &FileTree, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let count = view.file_count();
        let summary = match view.prepared.as_ref() {
            Some(prepared) if prepared.listing.truncated() => {
                fl!("file-tree-count-truncated", count = count)
            }
            _ => fl!("file-tree-count", count = count),
        };
        div()
            .flex_none()
            .px(px(10.))
            .py(px(6.))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .debug_selector(|| "file-tree-filter".into())
                    .flex_1()
                    .min_w_0()
                    .px_1()
                    .flex()
                    .items_center()
                    .gap_1()
                    .rounded(px(crate::config::corners::CONTROL))
                    .border_1()
                    .border_color(rgb(theme.active))
                    .child(
                        svg()
                            .path("icons/search.svg")
                            .flex_none()
                            .size(px(12.))
                            .text_color(rgb(theme.muted)),
                    )
                    .child(div().flex_1().min_w_0().child(view.filter.clone())),
            )
            .child(
                div()
                    .debug_selector(|| "file-tree-count".into())
                    .flex_none()
                    .text_color(rgb(theme.muted))
                    .child(summary),
            )
            .child(
                div()
                    .id("file-tree-refresh")
                    .debug_selector(|| "file-tree-refresh".into())
                    .flex_none()
                    .px_2()
                    .py_1()
                    .rounded(px(crate::config::corners::CONTROL))
                    .cursor_pointer()
                    .hover(|button| button.bg(rgb(theme.active)))
                    .child(fl!("file-tree-refresh"))
                    .on_click(cx.listener(move |this, _, _, cx| this.load_file_tree(id, cx))),
            )
    }

    pub(super) fn render_file_tree(&self, id: TabId, cx: &mut Context<Self>) -> AnyElement {
        let theme = &self.theme;
        let Some(view) = self.file_trees.get(&id) else {
            return div().into_any_element();
        };
        let message = |text: String| div().p_3().text_color(rgb(theme.muted)).child(text);
        let body = match &view.state {
            State::Loading => message(fl!("file-tree-reading")).into_any_element(),
            State::Failed(error) => div()
                .debug_selector(|| "file-tree-error".into())
                .p_3()
                .text_color(crate::menu::danger(theme))
                .child(error.clone())
                .into_any_element(),
            State::Ready if view.shown.is_empty() => {
                let text = if view.matched.is_some() {
                    fl!("file-tree-no-matches")
                } else {
                    fl!("file-tree-empty")
                };
                message(text).into_any_element()
            }
            State::Ready => uniform_list(
                "file-tree-list",
                view.shown.len(),
                cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                    this.file_tree_rows(id, range, cx)
                }),
            )
            .track_scroll(&view.scroll)
            .flex_1()
            .min_h_0()
            .into_any_element(),
        };
        div()
            .debug_selector(|| "file-tree".into())
            .size_full()
            .flex()
            .flex_col()
            .text_font(&self.config.ui)
            .text_size(px(self.config.ui.size))
            .child(self.file_tree_header(id, view, cx))
            .child(body)
            .into_any_element()
    }
}
