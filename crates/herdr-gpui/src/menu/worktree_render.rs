//! Painting the new worktree dialog: the shared search, the tab strip, and the
//! listings behind it. Rows are drawn from the prepared filter, never from a
//! query made while rendering.

use super::{
    listener,
    worktree_source::{Row, Tab},
};
use crate::{
    HerdrWindow,
    repo_items::{Kind, Origin},
};
use gpui_kit::{
    component::{
        ActiveTheme as _, h_flex,
        input::Input,
        list::ListItem,
        tab::{Tab as KitTab, TabBar},
        v_flex,
    },
    prelude::*,
    *,
};

/// The key context listings bind Up and Down in, so the highlight moves while
/// the search field keeps focus. A single-line field leaves both keys alone.
pub(crate) const LIST_CONTEXT: &str = "HerdrMenuList";

actions!(herdr_menu, [SelectPrevious, SelectNext]);

pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", SelectPrevious, Some(LIST_CONTEXT)),
        KeyBinding::new("down", SelectNext, Some(LIST_CONTEXT)),
    ]);
}

/// Gives `body` the listing keys: Up and Down move whichever listing the
/// dialog shows, while focus stays in its search field.
pub(super) fn list_keys(body: Div, weak: &WeakEntity<HerdrWindow>) -> Div {
    let step = |weak: &WeakEntity<HerdrWindow>, by: isize| {
        let weak = weak.clone();
        move |cx: &mut App| {
            let _ = weak.update(cx, |this, cx| {
                if let Some(source) = &mut this.menu.worktree {
                    source.step(by);
                }
                if let Some(picker) = &mut this.menu.worktree_open {
                    picker.step(by);
                }
                cx.notify();
            });
        }
    };
    let (previous, next) = (step(weak, -1), step(weak, 1));
    body.key_context(LIST_CONTEXT)
        .on_action(move |_: &SelectPrevious, _, cx| previous(cx))
        .on_action(move |_: &SelectNext, _, cx| next(cx))
}

impl HerdrWindow {
    /// The search every tab shares, then the tab strip. Each listed tab counts
    /// what the search kept in it, so a match elsewhere is visible from any
    /// tab. The GitHub tabs are disabled until an account is connected,
    /// because neither listing can be fetched without one.
    pub(super) fn render_worktree_tabs(&self, weak: &WeakEntity<HerdrWindow>, _: &App) -> Div {
        let connected = self.menu.github.connected();
        let Some(source) = &self.menu.worktree else {
            return div();
        };
        let busy = source.busy();
        let selected = Tab::ALL
            .iter()
            .position(|tab| *tab == source.tab)
            .unwrap_or_default();
        let tabs = Tab::ALL.into_iter().map(|tab| {
            let enabled = (connected || tab.kind().is_none()) && !busy;
            let label = match source.hits(tab).filter(|_| enabled) {
                Some(hits) => format!("{} {hits}", tab.label()),
                None => tab.label().to_owned(),
            };
            KitTab::new().label(label).disabled(!enabled)
        });
        let pick = weak.clone();
        v_flex()
            .gap_2()
            .child(
                div()
                    .debug_selector(|| "worktree-search".into())
                    .child(Input::new(&source.search)),
            )
            .child(
                TabBar::new("worktree-tabs")
                    .segmented()
                    .selected_index(selected)
                    .children(tabs)
                    .on_click(move |index, window, cx| {
                        let Some(tab) = Tab::ALL.get(*index).copied() else {
                            return;
                        };
                        let _ =
                            pick.update(cx, |this, cx| this.select_worktree_tab(tab, window, cx));
                    }),
            )
    }

    /// The open listing: how much of it the search kept, the rows themselves,
    /// and whatever its source last had to say.
    pub(super) fn render_worktree_items(&self, weak: &WeakEntity<HerdrWindow>, cx: &App) -> Div {
        let muted = cx.theme().muted_foreground;
        let Some(source) = &self.menu.worktree else {
            return div();
        };
        let (shown, total) = source.counts();
        let (noun, loading, message, empty) = match source.tab {
            Tab::New => return div(),
            Tab::Existing => (
                "checkouts not open in Herdr",
                source.checkouts.request.is_some(),
                source.checkouts.message.as_ref(),
                if total > 0 {
                    "No matching checkouts."
                } else {
                    "Every checkout of this repository is already open."
                },
            ),
            Tab::Branches => (
                "local branches without a checkout",
                source.branches.loading,
                source.branches.message.as_ref(),
                if total > 0 {
                    "No matching branches."
                } else {
                    "Every branch already has a checkout."
                },
            ),
            Tab::Items(kind) => (
                match kind {
                    Kind::PullRequest => "open pull requests",
                    Kind::Issue => "open issues",
                },
                source.lookup.loading,
                source.lookup.message.as_ref(),
                kind.empty_label(),
            ),
        };
        let status = if let Some(pending) = &source.pending {
            // Dismissing only closes the panel; the daemon keeps queued work,
            // as the branch tab's own waiting note says.
            if pending.opens() {
                format!(
                    "Opening {}. Dismissing does not cancel it.",
                    pending.label()
                )
            } else {
                format!(
                    "Creating a checkout for {}. Dismissing does not cancel it.",
                    pending.label()
                )
            }
        } else if let Some(error) = message {
            error.clone()
        } else if loading {
            "Loading...".to_owned()
        } else {
            match source.tab {
                Tab::Existing => "Opening adds the checkout to Herdr; nothing is created.".into(),
                Tab::Branches => "The checkout uses the local branch as it is.".into(),
                _ => source
                    .lookup
                    .origin
                    .as_ref()
                    .map(Origin::slug)
                    .unwrap_or_default(),
            }
        };
        let rows = (0..source.filtered.len()).filter_map(|row| self.worktree_row(row, weak, cx));
        v_flex()
            .gap_2()
            .child(
                div()
                    .debug_selector(|| "worktree-count".into())
                    .text_sm()
                    .text_color(muted)
                    .child(format!("{shown} of {total} {noun}")),
            )
            .child(
                v_flex()
                    .id("worktree-items")
                    .h(px(280.))
                    .overflow_y_scroll()
                    .track_scroll(&source.scroll)
                    .when(source.filtered.is_empty(), |list| {
                        list.child(
                            div()
                                .debug_selector(|| "worktree-empty".into())
                                .py_3()
                                .text_color(muted)
                                .child(if loading { "Loading..." } else { empty }),
                        )
                    })
                    .children(rows),
            )
            .child(
                div()
                    .debug_selector(|| "worktree-status".into())
                    .text_sm()
                    .text_color(muted)
                    .child(status),
            )
    }

    fn worktree_row(
        &self,
        row: usize,
        weak: &WeakEntity<HerdrWindow>,
        cx: &App,
    ) -> Option<ListItem> {
        let source = self.menu.worktree.as_ref()?;
        let listed = source.row(row)?;
        // A row is a title line with an optional muted tag, and a detail line
        // when there is more to say than the title. A branch is only its name.
        let (number, title, tag, detail) = match listed {
            Row::Checkout(entry) => (
                None,
                entry.branch.clone().unwrap_or_else(|| entry.label.clone()),
                entry.is_detached.then_some("detached"),
                Some(entry.path.clone()),
            ),
            Row::Branch(branch) => (None, branch.name.clone(), None, None),
            Row::Item(item) => (
                Some(format!("#{}", item.number)),
                item.title.clone(),
                item.draft.then_some("draft"),
                // A fork's head branch has no ref on `origin`, so the row says
                // why it cannot be picked rather than failing once it is.
                Some(if item.fork_owner.is_some() {
                    "from a fork - check out manually".to_owned()
                } else {
                    let branch = item.branch();
                    match item.author.is_empty() {
                        true => branch,
                        false => format!("{} - {branch}", item.author),
                    }
                }),
            ),
        };
        let muted = cx.theme().muted_foreground;
        Some(
            ListItem::new(("worktree-row", row))
                .selected(row == source.selected)
                .child(
                    v_flex()
                        .debug_selector(move || format!("worktree-row-{row}"))
                        .w_full()
                        .min_w_0()
                        .child(
                            h_flex()
                                .gap_2()
                                .min_w_0()
                                .children(
                                    number.map(|number| div().text_color(muted).child(number)),
                                )
                                .child(div().flex_1().min_w_0().truncate().child(title))
                                .children(tag.map(|tag| div().text_color(muted).child(tag))),
                        )
                        .children(detail.map(|detail| {
                            div().truncate().text_sm().text_color(muted).child(detail)
                        })),
                )
                .on_click(listener(weak, move |this, _, cx| {
                    this.pick_worktree_row(row, cx)
                })),
        )
    }
}
