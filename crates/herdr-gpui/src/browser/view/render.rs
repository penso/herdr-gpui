//! Drawing browser tabs: their entries in a group's strip, and the page with
//! its toolbar where the terminal would be.

use super::{store, tab_label};
use crate::{
    HerdrWindow,
    browser::{
        Location, Store, Tab, TabId, WebUrl,
        groups::{GroupId, Pick, Slot},
    },
    listening_ports::Link,
    tab_layout::{Mark, Probe, TabContext, TabFace},
    window::Flash,
};
use gpui::{prelude::*, *};

/// What a page's tab shows until the page reports an icon of its own.
const GLOBE: &str = "icons/globe.svg";

impl HerdrWindow {
    /// A page, filling the space it is given. The page records where it
    /// draws as it lays out.
    pub(in crate::browser) fn page_area(&self, page: impl IntoElement) -> Div {
        div().flex_1().min_h_0().child(page)
    }

    /// The workspace's browser tabs, after its Herdr tabs in a group's strip.
    pub(crate) fn browser_tab_entries(
        &self,
        slot: Slot,
        shown: Option<TabId>,
        faces: &TabContext<'_>,
        cx: &mut Context<Self>,
    ) -> Vec<(TabId, Stateful<Div>)> {
        let (Some((scope, workspace)), Some(store)) = (self.browser_key(), store(cx)) else {
            return Vec::new();
        };
        let tabs: Vec<Tab> = store
            .in_workspace(&scope, &workspace)
            .filter(|tab| self.group_lists(slot.id, &Pick::Page(tab.id)))
            .cloned()
            .collect();
        let layout = self.tab_layout();
        tabs.into_iter()
            .map(|tab| {
                let id = tab.id;
                // The VS Code tab bears its mark; a review tab shows a diff,
                // a code tab a file, and the overview devices, not a page.
                let icon = match (tab.place.editor(), &tab.location) {
                    (Some(editor), _) => editor.icon(),
                    (_, Some(Location::Review { .. })) => "icons/diff-unified.svg",
                    (_, Some(Location::Code { .. })) => "icons/code.svg",
                    (_, Some(Location::Orchestrator { .. })) => "icons/pulse.svg",
                    (_, Some(Location::Devices)) => "icons/devices.svg",
                    _ => GLOBE,
                };
                // A page's own icon stands in for the globe.
                let mark = match self.tab_favicon(id).filter(|_| icon == GLOBE) {
                    Some(favicon) => Mark::Favicon(favicon),
                    None => Mark::Icon(icon),
                };
                let base = div()
                    .id(SharedString::from(format!("browser-tab-{id}")))
                    .debug_selector(move || slot.selector(&format!("browser-tab-{id}")));
                let face = TabFace {
                    label: tab_label(&tab.title),
                    mark,
                    zoomed: false,
                    probe: Probe::Page(id),
                    // Split, the page closes in its group alone.
                    close: Box::new(cx.listener(move |this, _, window, cx| {
                        this.close_strip_tab(Pick::Page(id), window, cx);
                    })),
                };
                let tab = layout
                    .tab(
                        base,
                        face,
                        self.tab_state(shown == Some(id), slot.id),
                        faces,
                    )
                    .map(|tab| self.grow_tab(tab, slot.id, &Pick::Page(id)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.show_browser_tab_in(Some(slot.id), id, window, cx);
                    }));
                (id, tab)
            })
            .collect()
    }

    fn toolbar_button(
        &self,
        slot: Slot,
        id: &'static str,
        icon: &'static str,
        enabled: bool,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let color = if enabled {
            self.theme.foreground
        } else {
            self.theme.muted
        };
        div()
            .id(id)
            .debug_selector(move || slot.selector(id))
            .size(px(24.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(crate::config::corners::CONTROL))
            .when(enabled, |button| {
                button
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(self.theme.active)))
                    .on_click(cx.listener(move |this, _, window, cx| action(this, window, cx)))
            })
            .child(svg().path(icon).size(px(14.)).text_color(rgb(color)))
    }

    /// The page with its toolbar, drawn in a group where the terminal would
    /// be. `keyboard` marks the one element holding the window's focus
    /// handle when no terminal is drawn to hold it.
    pub(crate) fn render_browser(
        &mut self,
        slot: Slot,
        tab: &Tab,
        keyboard: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = tab.id;
        let code = tab.place.is_editor();
        let loaded = tab.location.is_some();
        let external = match &tab.location {
            Some(Location::Web { url }) => Some(url.clone()),
            _ => None,
        };
        #[cfg(any(target_os = "macos", windows))]
        let (annotate_button, panel) = {
            let annotating = self.browser.annotations.armed(id);
            // The panel slides open and closed beside the page: its content
            // keeps its width and the edge moves.
            let shown = self
                .browser
                .annotations
                .panel_shown(id, crate::motion::now());
            let panel = (shown > 0.).then(|| {
                div()
                    .flex_none()
                    .h_full()
                    .w(px(self.notes_panel_width() * shown))
                    .overflow_hidden()
                    .child(self.render_annotations(tab, cx))
                    .into_any_element()
            });
            let button = {
                // Annotating needs the page itself, so a blank tab or a
                // build without pages has nothing to annotate.
                let enabled = loaded && crate::browser::EMBEDDED;
                let color = if annotating {
                    self.theme.text_on(self.theme.primary_wash())
                } else if enabled {
                    self.theme.foreground
                } else {
                    self.theme.muted
                };
                div()
                    .id("browser-annotate")
                    .debug_selector(move || slot.selector("browser-annotate"))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .h(px(24.))
                    .px(px(6.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .when(annotating, |button| {
                        button.bg(rgb(self.theme.primary_wash()))
                    })
                    .text_color(rgb(color))
                    .child(
                        svg()
                            .path("icons/pencil.svg")
                            .size(px(13.))
                            .text_color(rgb(color)),
                    )
                    .child("Annotate")
                    .when(enabled, |button| {
                        button
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(self.theme.active)))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.toggle_annotating(id, window, cx);
                            }))
                    })
            };
            (Some(button), panel)
        };
        // Linux builds show no pages, so there is nothing to annotate.
        #[cfg(not(any(target_os = "macos", windows)))]
        let (annotate_button, panel): (Option<Stateful<Div>>, Option<AnyElement>) = (None, None);
        #[cfg(any(target_os = "macos", windows))]
        let page = self.browser.pages.element(id);
        #[cfg(not(any(target_os = "macos", windows)))]
        let page: Option<AnyView> = None;
        let failure = self.browser.failed.get(&id).cloned();
        let placeholder: SharedString = match (&failure, loaded) {
            (Some(message), _) => format!("Could not show this page: {message}").into(),
            (None, false) => "Type an address above and press Return.".into(),
            (None, true) if !crate::browser::EMBEDDED => {
                "This build cannot show pages in the window; open it in the system browser.".into()
            }
            (None, true) => "Loading\u{2026}".into(),
        };
        let toolbar = div()
            .debug_selector(move || slot.selector("browser-toolbar"))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(4.))
            .px(px(6.))
            .py(px(4.))
            .bg(rgb(self.theme.surface))
            .border_b_1()
            .border_color(rgb(self.theme.active))
            .child(self.toolbar_button(
                slot,
                "browser-back",
                "icons/arrow-left.svg",
                loaded,
                cx,
                move |this, _, _| {
                    #[cfg(any(target_os = "macos", windows))]
                    this.browser.pages.back(id);
                    #[cfg(not(any(target_os = "macos", windows)))]
                    let _ = this;
                },
            ))
            .child(self.toolbar_button(
                slot,
                "browser-forward",
                "icons/arrow-right.svg",
                loaded,
                cx,
                move |this, _, _| {
                    #[cfg(any(target_os = "macos", windows))]
                    this.browser.pages.forward(id);
                    #[cfg(not(any(target_os = "macos", windows)))]
                    let _ = this;
                },
            ))
            .child(self.toolbar_button(
                slot,
                "browser-reload",
                "icons/refresh.svg",
                loaded,
                cx,
                move |this, _, _| {
                    #[cfg(any(target_os = "macos", windows))]
                    this.browser.pages.reload(id);
                    #[cfg(not(any(target_os = "macos", windows)))]
                    let _ = this;
                },
            ))
            .child(
                div()
                    .id("browser-address")
                    .debug_selector(move || slot.selector("browser-address"))
                    .flex_1()
                    .min_w_0()
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                        match event.keystroke.key.as_str() {
                            "enter" => this.submit_address(slot.id, id, window, cx),
                            "escape" => {
                                let tab = store(cx).and_then(|store| store.get(id)).cloned();
                                this.sync_address(slot.id, tab.as_ref(), true, window, cx);
                                window.focus(&this.focus, cx);
                            }
                            _ => return,
                        }
                        cx.stop_propagation();
                    }))
                    .child(self.group_address(slot.id, cx)),
            )
            // A split shows the window's flash once, in the group in use.
            .children(
                self.flash_shown()
                    .filter(|_| Some(slot.id) == self.active_group())
                    .map(|(flash, shown)| {
                        div()
                            .flex_none()
                            .opacity(shown)
                            .max_w(px(240.))
                            .truncate()
                            .text_color(rgb(flash.accent(&self.theme)))
                            .child(flash.text.clone())
                    }),
            )
            .children(self.zoom_chip(slot, tab, cx))
            .children(annotate_button)
            .child(self.toolbar_button(
                slot,
                "browser-external",
                "icons/external.svg",
                external.is_some(),
                cx,
                move |_, _, cx| {
                    if let Some(url) = &external {
                        cx.open_url(url.as_str());
                    }
                },
            ));
        let content = match (page, &failure, tab.place.editor()) {
            (Some(page), None, _) => self.page_area(page).into_any_element(),
            (_, _, Some(editor)) => self.render_editor_status(editor, failure.clone(), cx),
            _ => div()
                .flex_1()
                .min_h_0()
                .flex()
                .items_center()
                .justify_center()
                .flex_col()
                .gap(px(10.))
                .px_4()
                .text_color(rgb(self.theme.muted))
                .child(
                    div()
                        .debug_selector(move || slot.selector("browser-placeholder"))
                        .child(placeholder),
                )
                .children(
                    (failure.is_none() && !loaded)
                        .then(|| self.render_blank_ports(slot, id, cx))
                        .flatten(),
                )
                .into_any_element(),
        };
        let browser = div()
            .id(SharedString::from(slot.selector("browser")))
            .debug_selector(move || slot.selector("browser"))
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            // Keeps window shortcuts reachable while the page does not hold
            // the keyboard; nothing here types into a terminal. A focus
            // handle belongs to one element, so a drawn terminal keeps it.
            .when(keyboard, |browser| browser.track_focus(&self.focus))
            // VS Code fills its tab: it is an editor, not a page to browse,
            // so it has no address, navigation, or notes of a browser's.
            .when(!code, |browser| browser.child(toolbar))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(div().flex().flex_col().flex_1().min_w_0().child(content))
                    .children(panel.filter(|_| !code)),
            );
        self.with_ground(browser, rgb(self.theme.background))
    }

    /// The workspace's listening ports under a blank tab's prompt, each
    /// opening its page in this tab; None while it listens on none.
    fn render_blank_ports(&self, slot: Slot, id: TabId, cx: &mut Context<Self>) -> Option<Div> {
        let (_, _, listed) = self.focused_listening_ports()?;
        let theme = &self.theme;
        let chips = listed.ports.iter().filter_map(|port| {
            let link = port.link(listed.origin)?;
            let selector = slot.selector(&format!("blank-port-{}", port.number));
            Some(
                div()
                    .id(SharedString::from(selector.clone()))
                    .debug_selector(move || selector.clone())
                    .flex()
                    .gap(px(6.))
                    .px(px(10.))
                    .py(px(2.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .border_1()
                    .border_color(rgb(theme.active))
                    .cursor_pointer()
                    .hover(|chip| chip.bg(rgb(theme.active)))
                    .child(
                        div()
                            .flex_none()
                            .text_color(rgb(theme.foreground))
                            .child(link.label()),
                    )
                    // A long process name gives way; the address stays whole.
                    .when(!port.process.is_empty(), |chip| {
                        chip.child(div().max_w(px(160.)).truncate().child(port.process.clone()))
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_blank_port(slot.id, id, &link, window, cx);
                    })),
            )
        });
        Some(
            div()
                .flex()
                .flex_wrap()
                .justify_center()
                .gap(px(6.))
                .children(chips),
        )
    }

    /// A port picked on a blank tab loads in that tab. A port behind an SSH
    /// tunnel opens as its status bar number does, once the tunnel is up.
    fn open_blank_port(
        &mut self,
        group: GroupId,
        id: TabId,
        link: &Link,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match link {
            Link::Page(url) => {
                self.visit(group, id, Location::Web { url: url.clone() }, window, cx);
            }
            Link::Tunnel(_) => {
                let Some((endpoint, workspace)) = self
                    .focused_listening_ports()
                    .map(|(endpoint, workspace, _)| (endpoint.to_owned(), workspace.to_owned()))
                else {
                    return;
                };
                self.open_port_link(&endpoint, &workspace, link, window, cx);
            }
        }
    }

    fn submit_address(
        &mut self,
        group: GroupId,
        id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self.group_address(group, cx).read(cx).text().to_owned();
        let Ok(url) = WebUrl::from_typed(&text) else {
            self.show_flash(Flash::warning("Not an http or https address"), cx);
            return;
        };
        self.visit(group, id, Location::Web { url }, window, cx);
    }

    /// Loads `location` in tab `id`, shown in `group`.
    fn visit(
        &mut self,
        group: GroupId,
        id: TabId,
        location: Location,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        Store::update(cx, |store| store.visited(id, Some(location.clone()), None));
        #[cfg(any(target_os = "macos", windows))]
        {
            // A page still being built goes there once built.
            self.browser.pages.load(id, &location);
            self.browser.pages.focus(id);
        }
        self.show_browser_tab_in(Some(group), id, window, cx);
    }
}
