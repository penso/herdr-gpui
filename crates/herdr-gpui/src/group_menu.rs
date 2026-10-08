//! The menu behind a group's "…" button: closing tabs in the group and
//! splitting it. New tabs come from the group's "+". Closing here only ever
//! takes tabs out of this group's strip, as an editor's group menu does: the
//! tabs stay open in Herdr, in the browser, and in every other group. Only a
//! tab's own close, unsplit, reaches Herdr, through its confirmation.
use crate::{
    HerdrWindow,
    browser::{GroupId, Pick},
    menu::Page,
};
use gpui::{prelude::*, *};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Close,
    CloseOthers,
    /// Closes the group, and with it every tab in it.
    CloseAll,
    Split,
}

impl Action {
    fn label(self) -> &'static str {
        match self {
            Self::Close => "Close",
            Self::CloseOthers => "Close Others",
            Self::CloseAll => "Close All",
            Self::Split => "Split Right",
        }
    }

    /// Whether a rule separates this row from the one above it.
    fn starts_section(self) -> bool {
        matches!(self, Self::Split)
    }
}

pub(crate) struct GroupMenu {
    group: GroupId,
    selected: Option<usize>,
}

impl HerdrWindow {
    /// The rows worth offering for `group`. Closing belongs to a split: a
    /// lone group has nowhere else to keep its tabs.
    fn group_actions(&self, group: GroupId, cx: &App) -> Vec<Action> {
        let mut actions = Vec::new();
        if self.is_split() {
            let pick = self.group_pick(group);
            if pick.is_some() {
                actions.push(Action::Close);
            }
            if self
                .group_tabs(group, cx)
                .iter()
                .any(|tab| Some(tab) != pick.as_ref())
            {
                actions.push(Action::CloseOthers);
            }
            actions.push(Action::CloseAll);
        }
        actions.push(Action::Split);
        actions
    }

    pub(crate) fn open_group_menu(
        &mut self,
        group: GroupId,
        anchor: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.open_menu(window, cx) {
            return;
        }
        self.menu.anchor = anchor;
        self.menu.page = Some(Page::Group);
        self.menu.group = Some(GroupMenu {
            group,
            selected: None,
        });
    }

    fn activate_group_menu(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        let Some(group) = self.menu.group.as_ref().map(|menu| menu.group) else {
            return;
        };
        self.dismiss_menu(window, cx);
        self.activate_group(group, window, cx);
        match action {
            Action::Close => {
                if let Some(pick) = self.group_pick(group) {
                    self.close_in_group(group, vec![pick], window, cx);
                }
            }
            Action::CloseOthers => {
                let keep = self.group_pick(group);
                let others: Vec<Pick> = self
                    .group_tabs(group, cx)
                    .into_iter()
                    .filter(|tab| Some(tab) != keep.as_ref())
                    .collect();
                self.close_in_group(group, others, window, cx);
            }
            Action::CloseAll => self.close_group(group, window, cx),
            Action::Split => self.split_group(group, window, cx),
        }
    }

    /// Closes the tab `group` shows, for Close Tab: a page at once, a Herdr
    /// tab through its confirmation.
    pub(crate) fn close_group_tab(
        &mut self,
        group: GroupId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.group_pick(group) {
            Some(Pick::Herdr(tab)) => self.open_tab_close(&tab, window, cx),
            Some(Pick::Page(id)) => self.close_browser_tab(id, window, cx),
            None => {}
        }
    }

    pub(crate) fn group_menu_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(group) = self.menu.group.as_ref().map(|menu| menu.group) else {
            return;
        };
        let actions = self.group_actions(group, cx);
        let Some(menu) = &mut self.menu.group else {
            return;
        };
        let key = event.keystroke.key.as_str();
        cx.stop_propagation();
        window.prevent_default();
        let count = actions.len();
        match key {
            "escape" => self.dismiss_menu(window, cx),
            "up" | "down" => {
                menu.selected = Some(match (menu.selected, key) {
                    (None, "up") => count - 1,
                    (None, _) => 0,
                    (Some(i), "up") => (i + count - 1) % count,
                    (Some(i), _) => (i + 1) % count,
                });
                cx.notify();
            }
            "enter" => {
                if let Some(action) = menu.selected.and_then(|index| actions.get(index)) {
                    self.activate_group_menu(*action, window, cx);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn render_group_menu(&self, cx: &mut Context<Self>) -> Div {
        let Some(menu) = &self.menu.group else {
            return div();
        };
        let theme = &self.theme;
        let mut body = div().flex().flex_col();
        for (index, action) in self.group_actions(menu.group, cx).into_iter().enumerate() {
            body = body.child(
                div()
                    .id(("group-menu-action", index))
                    .debug_selector(move || format!("group-menu-{action:?}"))
                    .when(index > 0 && action.starts_section(), |row| {
                        row.mt(px(4.)).border_t_1().border_color(rgb(theme.active))
                    })
                    .min_h(px(self.config.ui.line_height() + 12.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .when(menu.selected == Some(index), |row| {
                        row.bg(rgb(theme.active))
                    })
                    .hover(|row| row.bg(rgb(theme.active)))
                    .child(action.label())
                    .on_hover(cx.listener(move |this, hovered, _, cx| {
                        if *hovered && let Some(menu) = &mut this.menu.group {
                            menu.selected = Some(index);
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.activate_group_menu(action, window, cx)
                    })),
            );
        }
        body
    }
}

#[cfg(test)]
mod tests;
