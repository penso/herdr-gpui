//! Picking the branch a branch review is compared with. The header's base
//! opens a list of the checkout's branches, the closest first, read off the
//! UI thread; the first row goes back to the automatic choice: the pull
//! request's base, else the remote's default branch. The pick holds while
//! the review is open.
use super::Review;
use crate::{
    HerdrWindow,
    browser::TabId,
    review::diff::{self, BaseChoice},
};
use gpui::{prelude::*, *};

/// The first row, which drops the user's pick.
const AUTOMATIC: &str = "Automatic";
/// The widest the header's base button grows before its label truncates.
const BASE_BUTTON_MAX_WIDTH: f32 = 240.;

pub(super) enum BaseList {
    Loading,
    Listed(Vec<BaseChoice>),
    Failed(String),
}

/// The open list of bases and the row picked with the keyboard: 0 is
/// [`AUTOMATIC`], then each listed branch.
pub(super) struct BaseMenu {
    pub(super) list: BaseList,
    pub(super) selected: usize,
    focus: FocusHandle,
    scroll: ScrollHandle,
}

impl BaseMenu {
    fn rows(&self) -> usize {
        match &self.list {
            BaseList::Listed(choices) => choices.len() + 1,
            BaseList::Loading | BaseList::Failed(_) => 1,
        }
    }

    /// The ref row `row` picks; None for [`AUTOMATIC`].
    fn reference(&self, row: usize) -> Option<String> {
        let BaseList::Listed(choices) = &self.list else {
            return None;
        };
        let index = row.checked_sub(1)?;
        choices.get(index).map(|choice| choice.reference.clone())
    }
}

impl Review {
    /// The base the header names: the one compared with once read, the
    /// user's pick while reading, or a prompt when none was found.
    pub(super) fn base_label(&self) -> &str {
        if let Some(base) = self.loaded().and_then(|loaded| loaded.base.as_deref()) {
            return base;
        }
        match (&self.state, self.base.as_deref()) {
            (_, Some(chosen)) => diff::short_ref(chosen),
            (super::State::Failed(_), None) => "choose a base",
            _ => "\u{2026}",
        }
    }
}

impl HerdrWindow {
    /// Opens the list of bases and reads the checkout's branches for it.
    pub(crate) fn open_review_bases(
        &mut self,
        id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(review) = self.reviews.get_mut(&id) else {
            return;
        };
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        review.base_menu = Some(BaseMenu {
            list: BaseList::Loading,
            selected: 0,
            focus,
            scroll: ScrollHandle::new(),
        });
        let checkout = review.checkout.clone();
        let listing = cx
            .background_executor()
            .spawn(async move { diff::bases(&checkout) });
        cx.spawn(async move |this, cx| {
            let result = listing.await;
            this.update(cx, |this, cx| {
                this.review_bases_listed(id, result);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Shows the branches read for an open list, with the current pick
    /// selected.
    pub(super) fn review_bases_listed(
        &mut self,
        id: TabId,
        result: crate::Result<Vec<BaseChoice>>,
    ) {
        let Some(review) = self.reviews.get_mut(&id) else {
            return;
        };
        let chosen = review.base.clone();
        let Some(menu) = review
            .base_menu
            .as_mut()
            .filter(|menu| matches!(menu.list, BaseList::Loading))
        else {
            return;
        };
        match result {
            Ok(choices) => {
                menu.selected = chosen
                    .and_then(|chosen| choices.iter().position(|choice| choice.reference == chosen))
                    .map_or(0, |index| index + 1);
                menu.list = BaseList::Listed(choices);
                menu.scroll.scroll_to_item(menu.selected);
            }
            Err(error) => {
                tracing::warn!(%error, "Could not list the branches to compare with");
                menu.list = BaseList::Failed(error.to_string());
            }
        }
    }

    /// Closes the list of bases and gives the keyboard back to the diff.
    pub(crate) fn close_review_bases(
        &mut self,
        id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(review) = self.reviews.get_mut(&id)
            && review.base_menu.take().is_some()
        {
            window.focus(&review.focus, cx);
        }
        cx.notify();
    }

    /// Compares the branch with `base`, a branch's full ref, or with the
    /// automatic choice when None, and reads the review again.
    pub(crate) fn set_review_base(
        &mut self,
        id: TabId,
        base: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_review_bases(id, window, cx);
        let Some(review) = self.reviews.get_mut(&id) else {
            return;
        };
        if review.base == base {
            return;
        }
        review.base = base;
        self.reload_review(id, cx);
    }

    fn review_base_key(
        &mut self,
        id: TabId,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = self
            .reviews
            .get_mut(&id)
            .and_then(|review| review.base_menu.as_mut())
        else {
            return;
        };
        if event.keystroke.modifiers.modified() {
            return;
        }
        let rows = menu.rows();
        match event.keystroke.key.as_str() {
            "escape" => self.close_review_bases(id, window, cx),
            key @ ("up" | "down") => {
                let step = if key == "up" { rows - 1 } else { 1 };
                menu.selected = (menu.selected + step) % rows;
                menu.scroll.scroll_to_item(menu.selected);
            }
            "enter" => {
                let base = menu.reference(menu.selected);
                self.set_review_base(id, base, window, cx);
            }
            _ => return,
        }
        cx.stop_propagation();
        window.prevent_default();
        cx.notify();
    }

    /// The base in the header, in a branch review: a button that opens the
    /// list of branches to compare with.
    pub(super) fn render_review_base(
        &self,
        id: TabId,
        review: &Review,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        div()
            .id("review-base")
            .debug_selector(|| "review-base".into())
            // A long branch name truncates rather than pushing the header's
            // controls aside; the dropdown marker always stays visible.
            .flex()
            .items_center()
            .gap_1()
            .min_w_0()
            .max_w(px(BASE_BUTTON_MAX_WIDTH))
            .px_1()
            .rounded(px(crate::config::corners::CONTROL))
            .cursor_pointer()
            .text_color(rgb(theme.foreground))
            .hover(|button| button.bg(rgb(theme.active)))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .child(review.base_label().to_owned()),
            )
            .child(div().flex_none().child("\u{25be}"))
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.open_review_bases(id, window, cx);
            }))
            .when_some(review.base_menu.as_ref(), |button, menu| {
                button.child(
                    deferred(anchored().child(crate::motion::pop_in(
                        self.render_base_menu(id, review, menu, cx),
                        "review-base-menu",
                    )))
                    .with_priority(1),
                )
            })
    }

    fn render_base_menu(
        &self,
        id: TabId,
        review: &Review,
        menu: &BaseMenu,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let row = |index: usize, label: String, chosen: bool| {
            div()
                .id(("review-base-option", index))
                .debug_selector(move || format!("review-base-option-{index}"))
                .flex()
                .gap_2()
                .px_2()
                .py_1()
                .rounded(px(crate::config::corners::CONTROL))
                .cursor_pointer()
                .when(index == menu.selected, |row| row.bg(rgb(theme.active)))
                .hover(|row| row.bg(rgb(theme.active)))
                .child(div().flex_1().min_w_0().truncate().child(label))
                .when(chosen, |row| {
                    row.child(
                        div()
                            .flex_none()
                            .text_color(rgb(theme.muted))
                            .child("Current"),
                    )
                })
        };
        let pick = |row: Stateful<Div>, base: Option<String>| {
            row.on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.set_review_base(id, base.clone(), window, cx);
            }))
        };
        let mut rows = vec![pick(row(0, AUTOMATIC.into(), review.base.is_none()), None)];
        match &menu.list {
            BaseList::Loading => rows.push(
                div()
                    .id("review-base-reading")
                    .px_2()
                    .py_1()
                    .text_color(rgb(theme.muted))
                    .child("Reading branches\u{2026}"),
            ),
            BaseList::Failed(error) => rows.push(
                div()
                    .id("review-base-error")
                    .px_2()
                    .py_1()
                    .text_color(crate::menu::danger(theme))
                    .child(error.clone()),
            ),
            BaseList::Listed(choices) => {
                rows.extend(choices.iter().enumerate().map(|(index, choice)| {
                    let chosen = review.base.as_deref() == Some(choice.reference.as_str());
                    pick(
                        row(index + 1, choice.label().to_owned(), chosen),
                        Some(choice.reference.clone()),
                    )
                }));
            }
        }
        div()
            .id("review-base-menu")
            .debug_selector(|| "review-base-menu".into())
            .absolute()
            .top_full()
            .left_0()
            .w(px(280.))
            .max_h(px(320.))
            .overflow_y_scroll()
            .track_scroll(&menu.scroll)
            .p_1()
            .bg(rgb(theme.surface))
            .border_1()
            .border_color(rgb(theme.active))
            .rounded(px(crate::config::corners::CONTROL))
            .occlude()
            .track_focus(&menu.focus)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down_out(cx.listener(move |this, _, window, cx| {
                this.close_review_bases(id, window, cx);
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                this.review_base_key(id, event, window, cx);
            }))
            .children(rows)
    }
}
