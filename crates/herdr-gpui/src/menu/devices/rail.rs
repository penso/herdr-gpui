//! The Add Device dialog's frame: one title, a rail of what can be added (this
//! platform's own hosts, then the cloud providers in use here and **Add
//! provider** for the rest), and the chosen source's form beside it. Each source keeps its own form state,
//! so switching back and forth does not lose what was typed.

use super::super::Page;
use crate::HerdrWindow;
use crate::i18n::fl;
use gpui::{prelude::*, *};

/// Below this panel width the rail shows only icons.
pub(in crate::menu) const COMPACT_BELOW: f32 = 560.;
/// The dialog's width when there is room: the rail and a form beside it.
pub(in crate::menu) const WIDTH: f32 = 640.;
/// The dialog's height, the same for every source so switching never
/// resizes it: room for the SSH form with its nearby list. A form taller
/// than that scrolls inside.
pub(in crate::menu) const HEIGHT: f32 = 620.;

/// One entry of the rail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::menu) enum Source {
    /// SSH hosts.
    Host,
    /// WSL distributions on this machine, listed only on Windows.
    Wsl,
    #[cfg(feature = "cloud")]
    Cloud(crate::cloud::CloudProvider),
    /// The providers the rail leaves out, as tiles opening their Settings.
    #[cfg(feature = "cloud")]
    Catalog,
}

impl Source {
    /// The rail entry a page belongs to, if it is part of Add Device.
    pub(in crate::menu) fn of(page: Page) -> Option<Self> {
        match page {
            Page::AddDevice => Some(Self::Host),
            Page::AddWsl => Some(Self::Wsl),
            #[cfg(feature = "coder")]
            Page::AddCoder => Some(Self::Cloud(crate::cloud::CloudProvider::Coder)),
            #[cfg(feature = "cloud")]
            Page::AddCloud(provider) => Some(Self::Cloud(provider)),
            #[cfg(feature = "cloud")]
            Page::AddProvider => Some(Self::Catalog),
            _ => None,
        }
    }

    fn name(self) -> String {
        match self {
            Self::Host => fl!("add-device-source-host"),
            // A product's name.
            Self::Wsl => "WSL".to_owned(), // i18n: data
            #[cfg(feature = "cloud")]
            Self::Cloud(provider) => crate::cloud::name(provider).to_owned(),
            #[cfg(feature = "cloud")]
            Self::Catalog => fl!("add-device-source-catalog"),
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Host => "icons/terminal.svg",
            Self::Wsl => "icons/computer.svg",
            #[cfg(feature = "cloud")]
            Self::Cloud(provider) => crate::cloud::icon(provider),
            #[cfg(feature = "cloud")]
            Self::Catalog => "icons/plus.svg",
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::Host => "add-device-source-host",
            Self::Wsl => "add-device-source-wsl",
            #[cfg(feature = "azure")]
            Self::Cloud(crate::cloud::CloudProvider::Azure) => "add-device-source-azure",
            #[cfg(feature = "boat")]
            Self::Cloud(crate::cloud::CloudProvider::Boat) => "add-device-source-boat",
            #[cfg(feature = "coder")]
            Self::Cloud(crate::cloud::CloudProvider::Coder) => "add-device-source-coder",
            #[cfg(feature = "container")]
            Self::Cloud(crate::cloud::CloudProvider::Container) => "add-device-source-container",
            #[cfg(feature = "daytona")]
            Self::Cloud(crate::cloud::CloudProvider::Daytona) => "add-device-source-daytona",
            #[cfg(feature = "railway")]
            Self::Cloud(crate::cloud::CloudProvider::Railway) => "add-device-source-railway",
            #[cfg(feature = "opensandbox")]
            Self::Cloud(crate::cloud::CloudProvider::OpenSandbox) => {
                "add-device-source-opensandbox"
            }
            #[cfg(feature = "sprites")]
            Self::Cloud(crate::cloud::CloudProvider::Sprites) => "add-device-source-sprites",
            #[cfg(feature = "cloud")]
            Self::Catalog => "add-device-source-catalog",
        }
    }

    /// Whether this source starts the rail's cloud section.
    fn is_cloud(self) -> bool {
        #[cfg(feature = "cloud")]
        return matches!(self, Self::Cloud(_) | Self::Catalog);
        #[cfg(not(feature = "cloud"))]
        false
    }
}

/// What a rail entry says about its source, at its trailing edge.
enum State {
    None,
    /// Hosts found nearby and not saved yet.
    Nearby(usize),
    /// In use here: an account is configured, or a device was made.
    #[cfg(feature = "cloud")]
    Ready,
}

/// The Devices picker's line under "Add Device…".
pub(super) fn add_device_offer(cloud: bool) -> String {
    match (cfg!(windows), cloud) {
        (true, true) => fl!("add-device-offer-ssh-wsl-cloud"),
        (true, false) => fl!("add-device-offer-ssh-wsl"),
        (false, true) => fl!("add-device-offer-ssh-cloud"),
        (false, false) => fl!("add-device-offer-ssh"),
    }
}

impl HerdrWindow {
    /// Whether Add Device lists cloud providers: built in and able to connect.
    pub(super) fn cloud_offered(&self) -> bool {
        #[cfg(feature = "cloud")]
        return crate::cloud::unavailable().is_none()
            && !crate::cloud::CloudProvider::ALL.is_empty();
        #[cfg(not(feature = "cloud"))]
        false
    }

    /// This platform's own hosts first, then the cloud providers in use here,
    /// then **Add provider** while any is left out.
    pub(in crate::menu) fn rail_sources(&self) -> Vec<Source> {
        let mut sources = vec![Source::Host];
        if cfg!(windows) {
            sources.push(Source::Wsl);
        }
        #[cfg(feature = "cloud")]
        if self.cloud_offered() {
            sources.extend(self.cloud_listed().into_iter().map(Source::Cloud));
            if !self.cloud_unlisted().is_empty() {
                sources.push(Source::Catalog);
            }
        }
        sources
    }

    fn source_state(&self, source: Source, cx: &App) -> State {
        match source {
            Source::Host => self
                .menu
                .device_setup
                .as_ref()
                .filter(|setup| setup.discovery.is_some())
                .map(|setup| {
                    let text = setup.fields[0].read(cx).text();
                    self.device_completion(setup, text).unsaved
                })
                .filter(|count| *count > 0)
                .map_or(State::None, State::Nearby),
            Source::Wsl => State::None,
            // The rail lists only providers in use.
            #[cfg(feature = "cloud")]
            Source::Cloud(_) => State::Ready,
            #[cfg(feature = "cloud")]
            Source::Catalog => State::None,
        }
    }

    /// Show `source`'s form, reopening the one it had if it is still there.
    pub(in crate::menu) fn open_source(
        &mut self,
        source: Source,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu.page.and_then(Source::of) == Some(source) {
            return;
        }
        self.menu.error = None;
        match source {
            Source::Wsl => {
                if self.menu.wsl_setup.is_some() {
                    self.menu.page = Some(Page::AddWsl);
                    window.focus(&self.menu.focus.clone(), cx);
                } else {
                    self.open_add_wsl(window, cx);
                }
            }
            Source::Host => match &self.menu.device_setup {
                Some(setup) => {
                    let focus = setup.fields[0].read(cx).focus.clone();
                    self.menu.page = Some(Page::AddDevice);
                    window.focus(&focus, cx);
                }
                None => self.open_add_device(window, cx),
            },
            #[cfg(feature = "cloud")]
            Source::Cloud(provider) => self.open_cloud_source(provider, window, cx),
            #[cfg(feature = "cloud")]
            Source::Catalog => self.open_add_provider(window, cx),
        }
        cx.notify();
    }

    /// Keyboard access to the rail: Ctrl-Tab and Ctrl-Shift-Tab step through
    /// it from any field, wrapping at the ends. Returns whether the key was
    /// used. Cmd or Ctrl with a digit stay with the app-wide shortcuts.
    pub(in crate::menu) fn add_device_rail_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let sources = self.rail_sources();
        let count = sources.len();
        if count < 2 {
            return false;
        }
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let current = self
            .menu
            .page
            .and_then(Source::of)
            .and_then(|shown| sources.iter().position(|source| *source == shown))
            .unwrap_or(0);
        if key != "tab" || !modifiers.control || modifiers.alt || modifiers.platform {
            return false;
        }
        let index = if modifiers.shift {
            (current + count - 1) % count
        } else {
            (current + 1) % count
        };
        let Some(&source) = sources.get(index) else {
            return false;
        };
        self.open_source(source, window, cx);
        true
    }

    #[cfg(feature = "cloud")]
    fn open_cloud_source(
        &mut self,
        provider: crate::cloud::CloudProvider,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(feature = "coder")]
        if provider == crate::cloud::CloudProvider::Coder
            && self.cloud_providers().contains(&provider)
        {
            if self.menu.coder.is_some() {
                self.menu.page = Some(Page::AddCoder);
                window.focus(&self.menu.focus.clone(), cx);
            } else {
                self.open_coder_setup(window, cx);
            }
            return;
        }
        self.menu.page = Some(Page::AddCloud(provider));
        window.focus(&self.menu.focus.clone(), cx);
    }

    /// The whole dialog: the shared header, the rail, and `pane`, which draws
    /// its own body and footer. `selector` and `close` keep each source's
    /// own debug names for its frame and Close button.
    pub(in crate::menu) fn add_device_frame(
        &self,
        selector: &'static str,
        close: &'static str,
        compact: bool,
        pane: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let header = div()
            .debug_selector(|| "device-setup-header".into())
            .flex_none()
            .p(px(16.))
            .border_b_1()
            .border_color(rgb(theme.active))
            .flex()
            .items_center()
            .gap(px(12.))
            .child(
                div()
                    .flex_1()
                    .text_size(px(self.config.ui.size * 1.35))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(fl!("add-device-title")),
            )
            .child(
                div()
                    .id(close)
                    .debug_selector(move || close.into())
                    .px_2()
                    .py_1()
                    .cursor_pointer()
                    .rounded(px(crate::config::corners::CONTROL))
                    .hover(|s| s.bg(rgb(theme.active)))
                    .child(fl!("add-device-close"))
                    .on_click(cx.listener(|this, _, window, cx| this.dismiss_menu(window, cx))),
            );
        let sources = self.rail_sources();
        let body = div().flex_1().min_h_0().flex();
        // A lone source needs no rail.
        let body = if sources.len() > 1 {
            body.child(self.render_add_device_rail(&sources, compact, cx))
        } else {
            body
        };
        div()
            .debug_selector(move || selector.into())
            .flex_1()
            .flex()
            .flex_col()
            .min_h_0()
            .child(header)
            .child(
                body.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .child(pane),
                ),
            )
    }

    fn render_add_device_rail(
        &self,
        sources: &[Source],
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let current = self.menu.page.and_then(Source::of);
        // A provider's pane opened from elsewhere, such as Coder's sign-in
        // falling back, belongs under Add provider while the rail omits it.
        #[cfg(feature = "cloud")]
        let current = current.map(|shown| {
            if sources.contains(&shown) {
                shown
            } else {
                Source::Catalog
            }
        });
        let mut rail = div()
            .debug_selector(|| "add-device-rail".into())
            .flex_none()
            // Wide enough for its names in any language, up to a limit.
            .when(compact, |rail| rail.w(px(52.)))
            .when(!compact, |rail| rail.min_w(px(172.)).max_w(px(240.)))
            .p(px(8.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .border_r_1()
            .border_color(rgb(theme.active));
        for (index, &source) in sources.iter().enumerate() {
            if source.is_cloud() && index > 0 && !sources[index - 1].is_cloud() {
                rail = if compact {
                    rail.child(div().my(px(6.)).h(px(1.)).bg(rgb(theme.active)))
                } else {
                    rail.child(
                        div()
                            .px(px(8.))
                            .pt(px(10.))
                            .pb(px(4.))
                            .text_color(rgb(theme.muted))
                            .child(fl!("add-device-cloud-heading")),
                    )
                };
            }
            let on = current == Some(source);
            let id = source.id();
            let icon = div()
                .flex_none()
                .size(px(24.))
                .rounded(px(crate::config::corners::SMALL))
                .bg(rgb(theme.active))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    svg()
                        .path(source.icon())
                        .size(px(14.))
                        .text_color(rgb(theme.foreground)),
                );
            let trailing = match self.source_state(source, cx) {
                State::None => None,
                State::Nearby(count) => Some(
                    div()
                        .px(px(6.))
                        .rounded_full()
                        .bg(rgb(theme.active))
                        .text_size(px(self.config.ui.size * 0.85))
                        .text_color(rgb(theme.subtext()))
                        .child(count.to_string()),
                ),
                #[cfg(feature = "cloud")]
                State::Ready => Some(
                    div()
                        .debug_selector(move || format!("{id}-ready"))
                        .size(px(7.))
                        .rounded_full()
                        .bg(rgb(super::super::colors::online(theme))),
                ),
            };
            rail = rail.child(
                div()
                    .id(id)
                    .debug_selector(move || id.into())
                    .p(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .cursor_pointer()
                    .when(on, |row| row.bg(rgb(theme.primary_wash())))
                    .when(!on, |row| row.hover(|s| s.bg(rgb(theme.active))))
                    .child(icon)
                    .when(!compact, |row| {
                        // Its own width counts, so the rail widens to fit it.
                        row.child(
                            div()
                                .flex_grow(1.)
                                .min_w_0()
                                .truncate()
                                .child(source.name()),
                        )
                        .children(trailing.map(|trailing| div().flex_none().child(trailing)))
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_source(source, window, cx);
                    })),
            );
        }
        rail
    }

    /// A cloud provider whose machines are set up in Settings: what it
    /// offers, and a button that opens its Settings tab.
    #[cfg(feature = "cloud")]
    pub(super) fn render_add_cloud(
        &self,
        provider: crate::cloud::CloudProvider,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let name = crate::cloud::name(provider);
        let text = match provider {
            #[cfg(feature = "coder")]
            crate::cloud::CloudProvider::Coder if !self.cloud_providers().contains(&provider) => {
                fl!("add-device-coder-connect")
            }
            // The provider's own offer, as herdr-remote words it.
            _ => format!("{}.", crate::cloud::offer(provider)), // i18n: data
        };
        let pane = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .p(px(16.))
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(
                                svg()
                                    .flex_none()
                                    .path(crate::cloud::icon(provider))
                                    .size(px(24.))
                                    .text_color(rgb(theme.foreground)),
                            )
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(name)),
                    )
                    .child(div().text_color(rgb(theme.subtext())).child(text))
                    .children(self.menu.error.clone().map(|error| {
                        div()
                            .debug_selector(|| "cloud-setup-error".into())
                            .text_color(super::super::danger(theme))
                            .child(error)
                    })),
            )
            .child(
                div()
                    .flex_none()
                    .p(px(16.))
                    .border_t_1()
                    .border_color(rgb(theme.active))
                    .flex()
                    .justify_end()
                    .child(
                        self.settings_button("cloud-setup-settings")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_provider_settings(provider, window, cx);
                            })),
                    ),
            );
        self.add_device_frame("cloud-setup-dialog", "cloud-setup-close", compact, pane, cx)
    }

    /// A pane's **Open Settings…** button, waiting for its click handler.
    #[cfg(feature = "cloud")]
    pub(super) fn settings_button(&self, id: &'static str) -> Stateful<Div> {
        let theme = &self.theme;
        div()
            .id(id)
            .debug_selector(move || id.into())
            .p(px(8.))
            .rounded(px(crate::config::corners::CONTROL))
            .bg(rgb(theme.active))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(theme.active).blend(rgba((theme.foreground << 8) | 0x20))))
            .child(fl!("add-device-open-settings"))
    }

    /// Enter opens the provider's Settings; Escape closes the dialog.
    #[cfg(feature = "cloud")]
    pub(in crate::menu) fn add_cloud_key(
        &mut self,
        provider: crate::cloud::CloudProvider,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.keystroke.key.as_str() {
            "enter" => self.open_provider_settings(provider, window, cx),
            "escape" => self.dismiss_menu(window, cx),
            _ => return,
        }
        cx.stop_propagation();
        window.prevent_default();
    }
}
