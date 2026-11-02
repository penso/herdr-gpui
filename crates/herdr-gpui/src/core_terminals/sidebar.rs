//! The sidebar's Herdr 2 rows: every pane of the focused workspace's
//! herdr-core tabs, under the agents, with the status dot its agent reports.
//! Clicking a row shows its tab and gives that pane the keyboard. Mirrors are
//! left out: the terminal they watch is listed where it types.

use crate::{
    HerdrWindow,
    browser::{Location, Store, TabId},
    fonts::StyledFont,
    i18n::fl,
    sidebar::{label_text, status_indicator},
};
use gpui::{prelude::*, *};
use herdr_backend::TerminalId;

impl HerdrWindow {
    /// The Herdr 2 rows, or nothing when the workspace has no herdr-core tab.
    pub(crate) fn core_sidebar_rows(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let store = cx.try_global::<Store>()?;
        let (scope, workspace) = self.browser_key()?;
        let panes: Vec<(TabId, TerminalId)> = store
            .in_workspace(&scope, &workspace)
            .filter_map(|tab| match &tab.location {
                Some(Location::CoreTerminal {
                    panes,
                    observe: false,
                }) => Some((tab.id, panes.terminals())),
                _ => None,
            })
            .flat_map(|(tab, terminals)| {
                terminals
                    .into_iter()
                    .map(move |terminal| (tab, terminal.clone()))
            })
            .collect();
        if panes.is_empty() {
            return None;
        }
        let theme = &self.theme;
        let font = &self.config.sidebar;
        let indicators = self.core_indicators(cx);
        let hover = theme.active;
        let rows = panes.into_iter().map(|(tab, terminal)| {
            let (title, status) = self.core_pane_label(tab, &terminal, cx);
            let id = SharedString::from(format!("core-sidebar-{}-{terminal}", u64::from(tab)));
            let clicked = terminal.clone();
            // The ID beside a title, unless the title already is the ID.
            let address = (title != terminal.as_str()).then(|| terminal.to_string());
            div()
                .id(id.clone())
                .debug_selector(move || id.to_string())
                .px_3()
                .py_1()
                .mx_1()
                .flex()
                .items_center()
                .gap_2()
                .rounded(px(crate::config::corners::CONTROL))
                .cursor_pointer()
                .hover(move |row| row.bg(rgb(hover)))
                .child(status_indicator(status, font, indicators).flex_none())
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(theme.foreground))
                        .child(label_text(&title)),
                )
                .children(address.map(|address| {
                    div()
                        .flex_none()
                        .text_color(rgb(theme.muted))
                        .child(label_text(&address))
                }))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.show_core_pane(tab, &clicked, window, cx);
                }))
        });
        Some(
            div()
                .flex()
                .flex_col()
                .pt_2()
                .text_font(font)
                .text_size(px(font.size))
                .child(
                    div()
                        .px_3()
                        .pb_1()
                        .text_color(rgb(theme.muted))
                        .child(label_text(&fl!("core-terminals-sidebar"))),
                )
                .children(rows)
                .into_any_element(),
        )
    }

    /// Shows `tab` with `terminal` as its pane in use, unzoomed if another
    /// pane filled it, and gives that pane the keyboard.
    fn show_core_pane(
        &mut self,
        tab: TabId,
        terminal: &TerminalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(shown) = self.core_terminals.tabs.get_mut(&tab) {
            if shown.zoom.is_open() && shown.zoomed.as_ref() != Some(terminal) {
                shown.set_zoom(None);
            }
            shown.active = terminal.clone();
        }
        self.show_browser_tab(tab, window, cx);
        cx.notify();
    }
}
