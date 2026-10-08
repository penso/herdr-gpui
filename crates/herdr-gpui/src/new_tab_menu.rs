//! The menu behind a group's "+" button: a Herdr tab, a blank browser tab,
//! or the focused checkout's review, each opened in the group that asked,
//! then the focused workspace's listening ports, each opening its page
//! there. The shortcuts skip the menu: New Tab and New Browser Tab open in
//! the group in use.
use crate::{HerdrWindow, browser::GroupId, controls::Command, listening_ports::Link, menu::Page};
use gpui::{prelude::*, *};

/// The most a port's process name takes in its row. Names run to 32
/// characters, and the address beside it is what tells ports apart.
const PROCESS_WIDTH: f32 = 64.;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Row {
    Terminal,
    Browser,
    /// The review tab of the checkout the Git chip tracks.
    Review,
    /// One of the focused workspace's listening ports and where it opens.
    Port {
        number: u16,
        process: String,
        link: Link,
    },
}

impl Row {
    fn icon(&self) -> &'static str {
        match self {
            Self::Terminal => "icons/terminal.svg",
            Self::Browser => "icons/globe.svg",
            Self::Review => "icons/diff-unified.svg",
            Self::Port { .. } => "icons/arrow-right.svg",
        }
    }

    /// The row's text, which for a port is where its page opens.
    fn label(&self) -> SharedString {
        match self {
            Self::Terminal => "New Terminal Tab".into(),
            Self::Browser => "New Browser Tab".into(),
            Self::Review => "Review Changes".into(),
            Self::Port { link, .. } => link.label().into(),
        }
    }

    /// Stable for tests: `Port5173` rather than the port's whole value.
    fn selector(&self) -> String {
        match self {
            Self::Port { number, .. } => format!("new-tab-menu-Port{number}"),
            row => format!("new-tab-menu-{row:?}"),
        }
    }
}

pub(crate) struct NewTabMenu {
    group: GroupId,
    selected: Option<usize>,
}

impl HerdrWindow {
    /// What "+" offers now. Review needs a checkout; ports come from the
    /// last scan, so they follow the status bar's.
    fn new_tab_rows(&self) -> Vec<Row> {
        let mut rows = vec![Row::Terminal, Row::Browser];
        if self.git.tracked().is_some() {
            rows.push(Row::Review);
        }
        if let Some((_, _, listed)) = self.focused_listening_ports() {
            rows.extend(listed.ports.iter().filter_map(|port| {
                Some(Row::Port {
                    number: port.number,
                    process: port.process.clone(),
                    link: port.link(listed.origin)?,
                })
            }));
        }
        rows
    }

    pub(crate) fn open_new_tab_menu(
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
        self.menu.page = Some(Page::NewTab);
        self.menu.new_tab = Some(NewTabMenu {
            group,
            selected: None,
        });
    }

    fn activate_new_tab_row(&mut self, row: Row, window: &mut Window, cx: &mut Context<Self>) {
        let Some(group) = self.menu.new_tab.as_ref().map(|menu| menu.group) else {
            return;
        };
        self.dismiss_menu(window, cx);
        match row {
            // The tab arrives from the daemon later; it lands in `group`.
            Row::Terminal => {
                self.expect_new_tab_in(group);
                self.command(Command::Tab, window, cx);
            }
            Row::Browser => self.open_browser_tab_in(group, window, cx),
            Row::Review => {
                self.activate_group(group, window, cx);
                self.open_review(window, cx);
            }
            Row::Port { link, .. } => {
                let Some((endpoint, workspace)) = self
                    .focused_listening_ports()
                    .map(|(endpoint, workspace, _)| (endpoint.to_owned(), workspace.to_owned()))
                else {
                    return;
                };
                self.activate_group(group, window, cx);
                self.open_port_link(&endpoint, &workspace, &link, window, cx);
            }
        }
    }

    pub(crate) fn new_tab_menu_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rows = self.new_tab_rows();
        let Some(menu) = &mut self.menu.new_tab else {
            return;
        };
        let key = event.keystroke.key.as_str();
        cx.stop_propagation();
        window.prevent_default();
        let count = rows.len();
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
                if let Some(row) = menu.selected.and_then(|index| rows.get(index)).cloned() {
                    self.activate_new_tab_row(row, window, cx);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn render_new_tab_menu(&self, cx: &mut Context<Self>) -> Div {
        let Some(menu) = &self.menu.new_tab else {
            return div();
        };
        let theme = &self.theme;
        let mut body = div().flex().flex_col();
        let mut ports_shown = false;
        for (index, row) in self.new_tab_rows().into_iter().enumerate() {
            if matches!(row, Row::Port { .. }) && !ports_shown {
                ports_shown = true;
                body = body.child(
                    div()
                        .mt(px(4.))
                        .pt(px(6.))
                        .pb(px(2.))
                        .px(px(8.))
                        .border_t_1()
                        .border_color(rgb(theme.active))
                        .text_xs()
                        .text_color(rgb(theme.muted))
                        .child("Listening"),
                );
            }
            let detail: SharedString = match &row {
                Row::Terminal => self.keymap().primary(Command::Tab).to_owned().into(),
                Row::Browser => self
                    .keymap()
                    .primary(Command::NewBrowserTab)
                    .to_owned()
                    .into(),
                Row::Review => "".into(),
                Row::Port { process, .. } => process.clone().into(),
            };
            let selector = row.selector();
            let (label, process) = (format!("{selector}-label"), format!("{selector}-detail"));
            let capped = matches!(row, Row::Port { .. });
            body = body.child(
                div()
                    .id(("new-tab-menu-row", index))
                    .debug_selector(move || selector.clone())
                    .min_h(px(self.config.ui.line_height() + 12.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .cursor_pointer()
                    .when(menu.selected == Some(index), |row| {
                        row.bg(rgb(theme.active))
                    })
                    .hover(|row| row.bg(rgb(theme.active)))
                    .child(
                        svg()
                            .path(row.icon())
                            .size(px(14.))
                            .flex_none()
                            .text_color(rgb(theme.muted)),
                    )
                    .child(
                        div()
                            .debug_selector(move || label.clone())
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(row.label()),
                    )
                    .when(!detail.is_empty(), |line| {
                        line.child(
                            div()
                                .debug_selector(move || process.clone())
                                .flex_none()
                                .when(capped, |detail| detail.max_w(px(PROCESS_WIDTH)).truncate())
                                .text_color(rgb(theme.muted))
                                .child(detail),
                        )
                    })
                    .on_hover(cx.listener(move |this, hovered, _, cx| {
                        if *hovered && let Some(menu) = &mut this.menu.new_tab {
                            menu.selected = Some(index);
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.activate_new_tab_row(row.clone(), window, cx)
                    })),
            );
        }
        body
    }
}

#[cfg(test)]
mod tests;
