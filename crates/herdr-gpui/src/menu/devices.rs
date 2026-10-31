// i18n: migrated
//! The sidebar's device footer and the device picker that scopes the sidebar
//! to one device, several, or all of them, or opens the Add Device dialog. Device scope is presentation
//! state; connection ownership stays in `endpoint`.
mod activity_card;
mod add_device;
#[cfg(feature = "cloud")]
mod catalog;
#[cfg(feature = "coder")]
pub(super) mod coder;
pub(crate) use add_device::enter;
mod discover;
mod error;
mod host_key;
mod host_key_choice;
mod host_menu;
mod local_network;
pub(super) mod rail;
mod saved_warning;
mod scope;
mod setup;
mod wsl;

pub(super) use add_device::Setup;
pub(crate) use error::Error;
pub(crate) use host_menu::HostMenu;
pub(crate) use scope::DeviceScope;
pub(super) use wsl::WslSetup;

use super::{Page, colors};
use crate::i18n::fl;
use crate::{Command, HerdrWindow};
use gpui::{prelude::*, *};
use herdr_client::ConnectTarget;
use rail::add_device_offer;

pub(super) const MENU_GAP: f32 = 12.;

/// How the picker says several devices can be listed at once.
fn multi_select_hint() -> String {
    if cfg!(target_os = "macos") {
        fl!("devices-multi-select-macos")
    } else {
        fl!("devices-multi-select")
    }
}

/// Why Add Device cannot open in this window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SetupUnavailable {
    /// The window talks to an explicit socket, which has no device catalog.
    Socket,
    /// The window shows a development catalog.
    Development,
}

impl SetupUnavailable {
    pub(crate) fn text(self) -> String {
        match self {
            Self::Socket => fl!("devices-unavailable-socket"),
            Self::Development => fl!("devices-unavailable-development"),
        }
    }
}

impl From<SetupUnavailable> for SharedString {
    fn from(reason: SetupUnavailable) -> Self {
        reason.text().into()
    }
}
pub(super) const MENU_WIDTH: f32 = 280.;

/// How tall an anchored list above the sidebar footer may be, measured from the
/// origin of the button that opened it. The device picker and the session list
/// clamp at the same place, so neither grows over the terminal.
pub(super) fn list_height(anchor_y: Pixels) -> Pixels {
    room_above(anchor_y).max(px(48.)).min(px(420.))
}

/// The device picker's list: the same cap, less the room the activity card
/// above it takes when the window is too short for both.
fn device_list_height(anchor_y: Pixels) -> Pixels {
    (room_above(anchor_y) - px(activity_card::ROOM))
        .max(px(48.))
        .min(px(420.))
}

fn room_above(anchor_y: Pixels) -> Pixels {
    let chrome = crate::titlebar::HEIGHT;
    anchor_y - px(chrome + MENU_GAP + super::MENU_MARGIN + 12.)
}

pub(crate) struct SettingsHint {
    pub(crate) text: SharedString,
    pub(crate) foreground: u32,
    pub(crate) surface: u32,
}

impl Render for SettingsHint {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.))
            .py(px(4.))
            .rounded(px(crate::config::corners::CONTROL))
            .shadow_md()
            .text_size(px(12.))
            .text_color(rgb(self.foreground))
            .bg(rgb(self.surface))
            .child(self.text.clone())
    }
}

impl HerdrWindow {
    /// The cloud providers set up in the config or environment, in rail order.
    #[cfg(feature = "cloud")]
    pub(super) fn cloud_providers(&self) -> Vec<crate::cloud::CloudProvider> {
        self.cloud_providers_with_env(|name| std::env::var_os(name))
    }

    #[cfg(feature = "cloud")]
    // Providers that need no setting never read the environment.
    #[cfg_attr(
        not(any(
            feature = "azure",
            feature = "boat",
            feature = "coder",
            feature = "daytona"
        )),
        allow(unused_variables)
    )]
    fn cloud_providers_with_env(
        &self,
        var: impl Fn(&str) -> Option<std::ffi::OsString>,
    ) -> Vec<crate::cloud::CloudProvider> {
        if crate::cloud::unavailable().is_some() {
            return Vec::new();
        }
        crate::cloud::CloudProvider::ALL
            .iter()
            .copied()
            .filter(|provider| match provider {
                #[cfg(feature = "azure")]
                crate::cloud::CloudProvider::Azure => self.config.azure.configured(&var),
                #[cfg(feature = "boat")]
                crate::cloud::CloudProvider::Boat => {
                    self.config.boat.api_url.is_some() || var("HERDR_BOAT_API_URL").is_some()
                }
                #[cfg(feature = "coder")]
                crate::cloud::CloudProvider::Coder => {
                    self.config.coder.url.is_some() || var("HERDR_CODER_URL").is_some()
                }
                #[cfg(feature = "daytona")]
                crate::cloud::CloudProvider::Daytona => {
                    self.config.daytona.api_url.is_some() || var("HERDR_DAYTONA_API_URL").is_some()
                }
                #[cfg(feature = "opensandbox")]
                crate::cloud::CloudProvider::OpenSandbox => {
                    self.config.opensandbox.api_url.is_some()
                        || var("HERDR_OPENSANDBOX_API_URL").is_some()
                }
                #[cfg(feature = "sprites")]
                crate::cloud::CloudProvider::Sprites => {
                    self.config.sprites.api_url.is_some() || var("HERDR_SPRITES_API_URL").is_some()
                }
                // A local engine has no account; it is set up in Settings.
                #[cfg(feature = "container")]
                crate::cloud::CloudProvider::Container => false,
                // Has no account to configure; it is set up in Settings.
                #[cfg(feature = "railway")]
                crate::cloud::CloudProvider::Railway => false,
            })
            .collect()
    }

    /// Whether the open page is a cloud provider's pane of Add Device.
    pub(in crate::menu) fn cloud_dialog_open(&self) -> bool {
        #[cfg(feature = "coder")]
        if self.menu.page == Some(Page::AddCoder) {
            return true;
        }
        #[cfg(feature = "cloud")]
        if matches!(self.menu.page, Some(Page::AddCloud(_) | Page::AddProvider)) {
            return true;
        }
        false
    }

    /// The open cloud provider's pane of Add Device, if one is open.
    pub(in crate::menu) fn render_cloud_dialog(
        &self,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        #[cfg(feature = "coder")]
        if self.menu.page == Some(Page::AddCoder) {
            return Some(self.render_add_coder(compact, cx).into_any_element());
        }
        #[cfg(feature = "cloud")]
        if self.menu.page == Some(Page::AddProvider) {
            return Some(self.render_add_provider(compact, cx).into_any_element());
        }
        #[cfg(feature = "cloud")]
        if let Some(Page::AddCloud(provider)) = self.menu.page {
            return Some(
                self.render_add_cloud(provider, compact, cx)
                    .into_any_element(),
            );
        }
        let _ = (compact, cx);
        None
    }

    /// How many cloud machines are still being added.
    fn cloud_jobs_pending(&self) -> usize {
        #[cfg(feature = "cloud")]
        return self.cloud_jobs.len();
        #[cfg(not(feature = "cloud"))]
        0
    }

    pub(crate) fn device_visible(&self, id: &str) -> bool {
        self.device_filter.shows(id)
    }

    /// Lists `scope`'s devices in the sidebar, from the top: the rows a scroll
    /// or reveal kept belong to the old list.
    pub(super) fn set_device_scope(&mut self, scope: DeviceScope) {
        self.device_filter = scope;
        for scroll in &self.sidebar_scroll {
            scroll.set_offset(Point::default());
        }
        for revealed in &self.sidebar_revealed {
            revealed.set(None);
        }
        // The row a reveal left for the next frame is in the old list.
        self.sidebar_pin_reveal.set(None);
    }

    pub(crate) fn device_setup_unavailable(&self) -> Option<SetupUnavailable> {
        match &self.endpoints[0].connection.target {
            ConnectTarget::Socket(_) => Some(SetupUnavailable::Socket),
            ConnectTarget::Session {
                development: true, ..
            } => Some(SetupUnavailable::Development),
            _ => None,
        }
    }

    /// Whether the sidebar draws the device footer, per `[sidebar]
    /// device_footer`.
    pub(crate) fn device_footer_shown(&self) -> bool {
        self.config
            .sidebar_style
            .device_footer
            .shown(self.endpoints.len(), self.cloud_jobs_pending())
    }

    /// Where the Sessions list opens while the footer is hidden: the sidebar's
    /// bottom-left corner, where the footer's button would sit.
    pub(crate) fn render_sessions_anchor(&self) -> impl IntoElement {
        let anchor = self.sessions_anchor.clone();
        canvas(
            |_, _, _| (),
            move |bounds, _, _, _| {
                anchor.set(point(bounds.left() + px(8.), bounds.bottom()));
            },
        )
        .absolute()
        .inset_0()
        .size_full()
    }

    pub(crate) fn render_device_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let button_bounds = std::rc::Rc::new(std::cell::Cell::new(Bounds::<Pixels>::default()));
        let painted_bounds = button_bounds.clone();
        // The window owns this cell, so the shortcut and the click anchor the
        // list in the same place and it outlives this frame's rebuild.
        let painted_sessions = self.sessions_anchor.clone();
        let hint: SharedString = self
            .config
            .keybindings
            .shortcuts(Command::Settings)
            .next()
            .map_or_else(
                || fl!("devices-settings"),
                |shortcut| fl!("devices-settings-shortcut", shortcut = shortcut.to_string()),
            )
            .into();
        let foreground = self.theme.foreground;
        let surface = self.theme.surface;
        let picked = self.device_filter.picked().count();
        let label = match self.device_filter.picked().next() {
            Some(id) if picked == 1 => self
                .endpoints
                .iter()
                .find(|endpoint| endpoint.id == id)
                .map_or_else(|| fl!("devices-all"), |e| e.label.clone()),
            Some(_) => fl!("devices-picked", count = picked),
            None => fl!("devices-all"),
        };
        let connected = self.endpoints.iter().any(|endpoint| {
            self.device_visible(&endpoint.id) && endpoint.live.status.is_connected()
        });
        // Beside the activity bar, which carries Settings, the picker leads
        // with the device's initials and says how many hosts there are.
        let bar = self.activity_bar_shown();
        let hosts = self.endpoints.len();
        let initials: String = label
            .split_whitespace()
            .filter_map(|word| word.chars().next())
            .take(2)
            .flat_map(char::to_uppercase)
            .collect();
        div()
            .id("device-footer")
            .debug_selector(|| "device-footer".into())
            .h(px(crate::sidebar::DEVICE_FOOTER_HEIGHT))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(8.))
            .border_t_1()
            .border_color(rgb(self.theme.active))
            .child(
                div()
                    .id("device-picker")
                    .role(Role::Button)
                    .aria_label(if connected {
                        fl!("devices-picker-connected", device = label.as_str())
                    } else {
                        fl!("devices-picker-disconnected", device = label.as_str())
                    })
                    .map(|picker| {
                        self.chrome_stop(crate::window::chrome_focus::Region::Sidebar, picker)
                    })
                    .relative()
                    .debug_selector(|| "device-picker".into())
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .p(px(6.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(self.theme.active)))
                    .child(
                        div()
                            .relative()
                            .flex_none()
                            .map(|icon| {
                                if bar {
                                    icon.child(
                                        div()
                                            .debug_selector(|| "device-footer-initials".into())
                                            .size(px(22.))
                                            .rounded_full()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .bg(rgb(self.theme.active))
                                            .text_size(px(10.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(initials),
                                    )
                                } else {
                                    icon.child(
                                        svg()
                                            .path("icons/devices.svg")
                                            .size(px(16.))
                                            .text_color(rgb(self.theme.foreground)),
                                    )
                                }
                            })
                            // How many Coder workspaces are still being added,
                            // on the icon's corner so the label keeps its room.
                            .when(self.cloud_jobs_pending() > 0, |icon| {
                                icon.child(
                                    div()
                                        .debug_selector(|| "device-footer-adding".into())
                                        .absolute()
                                        .top(px(-6.))
                                        .right(px(-8.))
                                        .min_w(px(13.))
                                        .h(px(13.))
                                        .px(px(3.))
                                        .rounded_full()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_size(px(9.))
                                        .bg(colors::accent(&self.theme))
                                        .text_color(rgb(self.theme.background))
                                        .child(self.cloud_jobs_pending().to_string()),
                                )
                            }),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(label.to_owned()))
                    .when(bar && hosts > 1, |picker| {
                        picker.child(
                            div()
                                .flex_none()
                                .text_size(px(11.))
                                .text_color(rgb(self.theme.muted))
                                .child(fl!("devices-hosts", count = hosts)),
                        )
                    })
                    .child(
                        div()
                            .size(px(6.))
                            .flex_none()
                            .rounded_full()
                            .bg(rgb(if connected {
                                colors::online(&self.theme)
                            } else {
                                self.theme.muted
                            })),
                    )
                    .child(
                        svg()
                            .path("icons/chevron-up.svg")
                            .size(px(12.))
                            .flex_none()
                            .text_color(rgb(self.theme.muted)),
                    )
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |bounds, _, _, _| {
                                painted_bounds.set(bounds);
                            },
                        )
                        .absolute()
                        .inset_0()
                        .size_full(),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        if this.open_menu(window, cx) {
                            // Anchor to the control, not the pointer: every click
                            // position leaves the same clear gap above the button.
                            this.menu.anchor = button_bounds.get().origin;
                            this.menu.page = Some(Page::Devices);
                            this.menu.selected = Some(0);
                        }
                    })),
            )
            .child(
                div()
                    .id("device-sessions")
                    .role(Role::Button)
                    .aria_label(fl!("devices-sessions"))
                    .map(|button| {
                        self.chrome_stop(crate::window::chrome_focus::Region::Sidebar, button)
                    })
                    .relative()
                    .debug_selector(|| "device-sessions".into())
                    .size(px(28.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(crate::config::corners::CONTROL))
                    .cursor_pointer()
                    .tooltip(move |_, cx| {
                        cx.new(|_| SettingsHint {
                            text: fl!("devices-sessions").into(),
                            foreground,
                            surface,
                        })
                        .into()
                    })
                    .hover(|s| s.bg(rgb(self.theme.active)))
                    .child(
                        svg()
                            .path("icons/sessions.svg")
                            .size(px(18.))
                            .text_color(rgb(self.theme.foreground)),
                    )
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |bounds, _, _, _| {
                                painted_sessions.set(bounds.origin);
                            },
                        )
                        .absolute()
                        .inset_0()
                        .size_full(),
                    )
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.open_sessions(this.sessions_anchor.get(), window, cx);
                    })),
            )
            .when(!bar, |footer| {
                footer.child(
                    div()
                        .id("device-settings")
                        .debug_selector(|| "device-settings".into())
                        .size(px(28.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(crate::config::corners::CONTROL))
                        .cursor_pointer()
                        .tooltip(move |_, cx| {
                            cx.new(|_| SettingsHint {
                                text: hint.clone(),
                                foreground,
                                surface,
                            })
                            .into()
                        })
                        .hover(|s| s.bg(rgb(self.theme.active)))
                        .child(
                            svg()
                                .path("icons/settings.svg")
                                .size(px(18.))
                                .text_color(rgb(self.theme.foreground)),
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.command(Command::Settings, window, cx);
                        })),
                )
            })
    }

    pub(super) fn render_devices(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let connected = self
            .endpoints
            .iter()
            .filter(|e| e.live.status.is_connected())
            .count();
        let count = self.endpoints.len();
        let mut rows = vec![(
            fl!("devices-all"),
            fl!("devices-all-detail", count = count, connected = connected),
            self.device_filter == DeviceScope::All,
            true,
        )];
        rows.extend(self.endpoints.iter().map(|endpoint| {
            let detail = match &endpoint.connection.target {
                // Names: the host, machine, or distribution, and the session.
                // i18n: data
                ConnectTarget::Ssh { target, session } => format!("{target} · {session}"),
                #[cfg(feature = "cloud")]
                ConnectTarget::Cloud {
                    provider,
                    machine,
                    session,
                    ..
                } => format!("{} {machine} · {session}", crate::cloud::name(*provider)),
                // i18n: data
                ConnectTarget::Wsl { distro, session } => format!("WSL {distro} · {session}"),
                ConnectTarget::Socket(path) => path.display().to_string(),
                ConnectTarget::Session { name, .. } => {
                    fl!("devices-this-device-session", session = name.as_str())
                }
                ConnectTarget::Local => fl!("devices-this-device"),
            };
            (
                endpoint.label.clone(),
                fl!(
                    "devices-row-detail",
                    place = detail,
                    state = endpoint.status()
                ),
                self.device_filter.picks(&endpoint.id),
                endpoint.enabled,
            )
        }));
        rows.push((
            fl!("devices-add"),
            self.device_setup_unavailable().map_or_else(
                || add_device_offer(self.cloud_offered()),
                SetupUnavailable::text,
            ),
            false,
            self.device_setup_unavailable().is_none(),
        ));
        // The footer's Sessions button is an icon with no label, so the
        // picker, whose Local row already names the session, offers it too.
        rows.push((
            fl!("devices-sessions-row"),
            fl!("devices-sessions-detail"),
            false,
            true,
        ));
        let mut view = div()
            .id("devices-list")
            .max_h(device_list_height(self.menu.anchor.y))
            .overflow_y_scroll()
            .track_scroll(&self.menu.devices_scroll)
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .p(px(8.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.))
                    .text_color(rgb(self.theme.muted))
                    .child(div().flex_none().child(fl!("devices-heading")))
                    .child(
                        // Wraps rather than cutting a longer language short.
                        div()
                            .min_w_0()
                            .text_size(px(self.config.ui.size * 0.85))
                            .child(multi_select_hint()),
                    ),
            );
        for (index, (label, detail, checked, enabled)) in rows.into_iter().enumerate() {
            let endpoint = index.checked_sub(1).and_then(|i| self.endpoints.get(i));
            view = view.child(
                div()
                    .id(("device-row", index))
                    .debug_selector(move || format!("device-row-{index}"))
                    .p(px(8.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .when(self.menu.selected == Some(index), |s| {
                        s.bg(rgb(self.theme.active))
                    })
                    // The current scope is highlighted as the session list marks
                    // its current session, rather than with a trailing check.
                    .when(checked, |s| s.bg(rgb(self.theme.primary_wash())))
                    .when(enabled, |s| s.cursor_pointer())
                    .text_color(rgb(if enabled {
                        self.theme.foreground
                    } else {
                        self.theme.muted
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(8.))
                                    .when(index == 0, |row| {
                                        row.child(
                                            svg()
                                                .path("icons/devices.svg")
                                                .size(px(16.))
                                                .flex_none()
                                                .text_color(rgb(self.theme.foreground)),
                                        )
                                    })
                                    .when(index == self.session_list_row(), |row| {
                                        row.child(
                                            svg()
                                                .path("icons/sessions.svg")
                                                .size(px(16.))
                                                .flex_none()
                                                .text_color(rgb(self.theme.foreground)),
                                        )
                                    })
                                    .when(index == self.add_device_row(), |row| {
                                        row.child(
                                            svg()
                                                .path("icons/plus.svg")
                                                .size(px(16.))
                                                .flex_none()
                                                .text_color(rgb(if enabled {
                                                    self.theme.foreground
                                                } else {
                                                    self.theme.muted
                                                })),
                                        )
                                    })
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .when(checked, |label| {
                                                label
                                                    .debug_selector(move || {
                                                        format!("device-current-{index}")
                                                    })
                                                    .font_weight(FontWeight::SEMIBOLD)
                                            })
                                            .child(label),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(self.config.ui.size * 0.85))
                                    .text_color(rgb(self.theme.muted))
                                    .child(detail),
                            ),
                    )
                    .when_some(endpoint, |row, endpoint| {
                        row.child(
                            div()
                                .debug_selector(move || format!("device-dot-{index}"))
                                .size(px(7.))
                                .flex_none()
                                .rounded_full()
                                .bg(rgb(if endpoint.live.status.is_connected() {
                                    colors::online(&self.theme)
                                } else {
                                    self.theme.muted
                                })),
                        )
                    })
                    .on_hover(cx.listener(move |this, hovered, _, cx| {
                        if *hovered {
                            this.menu.selected = Some(index);
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                        if event.modifiers().secondary() {
                            this.toggle_listed_device(index, window, cx);
                        } else {
                            this.choose_device(index, window, cx);
                        }
                    })),
            );
        }
        // Workspaces still being added: progress only, not selectable rows,
        // so keyboard navigation keeps its indices.
        #[cfg(feature = "cloud")]
        {
            view = self.cloud_job_rows(view);
        }
        // The card stays put above the list, which scrolls on its own.
        div()
            .flex()
            .flex_col()
            .child(self.render_activity_card(cx))
            .child(view)
    }

    /// Machines still being added: progress only, not selectable rows, so
    /// keyboard navigation keeps its indices.
    #[cfg(feature = "cloud")]
    fn cloud_job_rows(&self, mut view: Stateful<Div>) -> Stateful<Div> {
        if !self.cloud_jobs.is_empty() {
            view = view.child(
                div()
                    .p(px(8.))
                    .text_color(rgb(self.theme.muted))
                    .child(fl!("devices-adding")),
            );
        }
        for (index, provision) in self.cloud_jobs.iter().enumerate() {
            view = view.child(
                div()
                    .debug_selector(move || format!("device-adding-{index}"))
                    .p(px(8.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(div().truncate().child(provision.name.clone()))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.config.ui.size * 0.85))
                                    .text_color(rgb(self.theme.muted))
                                    .child(format!(
                                        "{} · {}",
                                        crate::cloud::name(provision.provider),
                                        provision.status
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .size(px(7.))
                            .flex_none()
                            .rounded_full()
                            .bg(colors::accent(&self.theme)),
                    ),
            );
        }
        view
    }

    /// Shared by the picker and overview, including isolated-window guards.
    pub(crate) fn open_device_setup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(reason) = self.device_setup_unavailable() {
            tracing::info!(category = "device_setup", ?reason, "Add Device unavailable");
            return;
        }
        tracing::info!(category = "device_setup", "Opening Add Device");
        self.open_add_device(window, cx);
        cx.notify();
    }

    /// Opens Settings > Cloud devices on `provider`'s tab.
    #[cfg(feature = "cloud")]
    pub(super) fn open_provider_settings(
        &mut self,
        provider: crate::cloud::CloudProvider,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_menu(window, cx);
        #[cfg(not(test))]
        crate::settings_window::open_cloud(cx.weak_entity(), Some(provider), cx);
        #[cfg(test)]
        {
            self.menu.settings_opened = Some(provider);
        }
    }

    /// The picker's rows after the devices: Add Device, then Sessions.
    fn add_device_row(&self) -> usize {
        self.endpoints.len() + 1
    }

    fn session_list_row(&self) -> usize {
        self.endpoints.len() + 2
    }

    fn choose_device(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index == self.session_list_row() {
            // Where the picker opened, so the list replaces it in place.
            self.open_sessions(self.menu.anchor, window, cx);
        } else if index > self.add_device_row() {
            return;
        } else if index == self.add_device_row() {
            self.open_device_setup(window, cx);
        } else {
            let id = if index == 0 {
                None
            } else {
                let Some(endpoint) = self.endpoints.get(index - 1).filter(|e| e.enabled) else {
                    return;
                };
                Some(endpoint.id.clone())
            };
            self.dismiss_menu(window, cx);
            if let Some(id) = &id
                && !self.select_endpoint(id, cx)
            {
                return;
            }
            self.set_device_scope(id.map_or(DeviceScope::All, DeviceScope::only));
        }
        cx.notify();
    }

    /// Adds a device to the sidebar's list or takes it away, leaving the picker
    /// open to pick more. Taking away the device on screen selects the first
    /// one still listed, so the terminal always belongs to a listed device;
    /// like any switch of device, that closes the picker.
    fn toggle_listed_device(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        // All Devices, Add Device, and Sessions have nothing to add.
        let Some(endpoint) = index
            .checked_sub(1)
            .and_then(|index| self.endpoints.get(index))
        else {
            return self.choose_device(index, window, cx);
        };
        if !endpoint.enabled {
            return;
        }
        let enabled = self
            .endpoints
            .iter()
            .filter(|endpoint| endpoint.enabled)
            .map(|endpoint| endpoint.id.as_str());
        let Some(scope) = self.device_filter.toggled(
            &endpoint.id,
            &self.endpoints[self.selected_endpoint].id,
            enabled,
        ) else {
            return;
        };
        if !scope.shows(&self.endpoints[self.selected_endpoint].id) {
            let Some(next) = self
                .displayed_devices()
                .into_iter()
                .find(|(_, endpoint)| endpoint.enabled && scope.shows(&endpoint.id))
                .map(|(_, endpoint)| endpoint.id.clone())
            else {
                return;
            };
            if !self.select_endpoint(&next, cx) {
                return;
            }
        }
        self.set_device_scope(scope);
        cx.notify();
    }

    pub(super) fn devices_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu.page == Some(Page::AddDevice) {
            if !self.add_device_key(event, window, cx) {
                return;
            }
        } else {
            let key = event.keystroke.key.as_str();
            let count = self.session_list_row() + 1;
            match key {
                "up" | "down" => {
                    let index = self.menu.selected.unwrap_or(0).min(count - 1);
                    self.menu.selected =
                        Some((index + if key == "up" { count - 1 } else { 1 }) % count);
                    if let Some(index) = self.menu.selected {
                        self.menu.devices_scroll.scroll_to_item(index + 1);
                    }
                    cx.notify();
                }
                "enter" => self.choose_device(self.menu.selected.unwrap_or(0), window, cx),
                "space" => self.toggle_listed_device(self.menu.selected.unwrap_or(0), window, cx),
                "escape" => self.dismiss_menu(window, cx),
                _ => {}
            }
        }
        cx.stop_propagation();
        window.prevent_default();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
