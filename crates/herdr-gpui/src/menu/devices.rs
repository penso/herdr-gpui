//! Device scope is presentation state; connection ownership stays in `endpoint`.
mod host_menu;
mod setup;

pub(crate) use host_menu::HostMenu;

use super::{Page, colors};
use crate::{Command, HerdrWindow, NavigationTarget, search_input::SearchInput};
use gpui::{prelude::*, *};
use herdr_client::{
    ConnectTarget, HostProbe, Method,
    protocol::{ClientKeyCode, ClientKeyKind, ClientPaneInputEvent},
};

pub(super) const MENU_GAP: f32 = 12.;
pub(super) const MENU_WIDTH: f32 = 280.;

/// How tall an anchored list above the sidebar footer may be, measured from the
/// origin of the button that opened it. The device picker and the session list
/// clamp at the same place, so neither grows over the terminal.
pub(super) fn list_height(anchor_y: Pixels) -> Pixels {
    let chrome = crate::titlebar::HEIGHT
        + crate::worktree_banner::reserved(env!("HERDR_BUILD_WORKTREE") == "1");
    (anchor_y - px(chrome + MENU_GAP + super::MENU_MARGIN + 12.))
        .max(px(48.))
        .min(px(420.))
}

pub(super) struct Setup {
    fields: [Entity<SearchInput>; 3],
    step: Step,
    /// This process's claim on the host being added, from the first check
    /// until the device is saved or the dialog closes.
    claim: Option<setup::Claim>,
    task: Option<Task<()>>,
}

/// Where adding a device stands. Each step after `Form` belongs to the request
/// that was submitted, not to whatever the fields hold now.
enum Step {
    Form,
    Checking(setup::Request),
    /// Herdr is present, so the CLI saves the device without a terminal. A
    /// stopped server is started by that same command.
    Saving(setup::Request, HostProbe),
    /// Saved; the next frame closes the dialog (see `poll_device_setup`).
    Saved,
    /// Setup needs prompts, so the user decides whether to run it locally.
    Confirm(setup::Request, Offer),
    /// Checking the catalog on disk again before opening the setup workspace.
    Verifying(setup::Request),
    /// Waiting for the local daemon to create the setup workspace.
    Opening(setup::Request, LocalSpace),
}

/// Why setup has to continue in a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Offer {
    Install,
    Update,
    /// SSH needs a prompt, or saving without a terminal failed.
    Terminal,
}

impl Offer {
    /// `None` when Herdr is present, so the device is saved without a terminal.
    fn for_probe(probe: HostProbe) -> Option<Self> {
        match probe {
            HostProbe::Running | HostProbe::Stopped => None,
            HostProbe::Missing => Some(Self::Install),
            HostProbe::Outdated => Some(Self::Update),
            HostProbe::SshFailed => Some(Self::Terminal),
        }
    }

    fn question(self, target: &str) -> String {
        match self {
            Self::Install => {
                format!("Herdr was not detected on {target}. Should we install it?")
            }
            Self::Update => {
                format!("The Herdr on {target} is too old for this app. Should we update it?")
            }
            Self::Terminal => format!(
                "Setting up {target} needs your input, such as an SSH password or host key. Continue in a local terminal?"
            ),
        }
    }

    fn action(self) -> &'static str {
        match self {
            Self::Install => "Install",
            Self::Update => "Update",
            Self::Terminal => "Open terminal",
        }
    }
}

/// The local workspace request whose root pane will run the setup command.
struct LocalSpace {
    request: String,
    boot: String,
    command: String,
}

struct SettingsHint {
    text: SharedString,
    foreground: u32,
    surface: u32,
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
    pub(crate) fn device_visible(&self, id: &str) -> bool {
        self.device_filter
            .as_deref()
            .is_none_or(|filter| filter == id)
    }

    pub(super) fn device_setup_unavailable(&self) -> Option<&'static str> {
        match &self.endpoints[0].connection.target {
            ConnectTarget::Socket(_) => {
                Some("Device setup is unavailable with an explicit socket.")
            }
            ConnectTarget::Session {
                development: true, ..
            } => Some("Device setup is unavailable with a development catalog."),
            _ if cfg!(windows) => Some("Saved SSH devices are unavailable on Windows."),
            _ => None,
        }
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
            .first()
            .map_or_else(
                || "Settings".to_owned(),
                |shortcut| format!("Settings ({shortcut})"),
            )
            .into();
        let foreground = self.theme.foreground;
        let surface = self.theme.surface;
        let label = self
            .device_filter
            .as_ref()
            .and_then(|id| self.endpoints.iter().find(|endpoint| &endpoint.id == id))
            .map_or("All Devices", |endpoint| endpoint.label.as_str());
        let connected = self.endpoints.iter().any(|endpoint| {
            self.device_visible(&endpoint.id) && endpoint.live.status.is_connected()
        });
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
                        svg()
                            .path("icons/devices.svg")
                            .size(px(16.))
                            .flex_none()
                            .text_color(rgb(self.theme.foreground)),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(label.to_owned()))
                    .child(
                        div()
                            .size(px(6.))
                            .flex_none()
                            .rounded_full()
                            .bg(rgb(if connected {
                                colors::ONLINE
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
                            text: "Sessions".into(),
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
            .child(
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
    }

    pub(super) fn render_devices(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let connected = self
            .endpoints
            .iter()
            .filter(|e| e.live.status.is_connected())
            .count();
        let count = self.endpoints.len();
        let mut rows = vec![(
            "All Devices".to_owned(),
            format!(
                "{count} {} · {connected} connected",
                if count == 1 { "device" } else { "devices" }
            ),
            self.device_filter.is_none(),
            true,
        )];
        rows.extend(self.endpoints.iter().map(|endpoint| {
            let detail = match &endpoint.connection.target {
                ConnectTarget::Ssh { target, session } => format!("{target} · {session}"),
                ConnectTarget::Socket(path) => path.display().to_string(),
                ConnectTarget::Session { name, .. } => format!("This device · {name}"),
                ConnectTarget::Local => "This device".into(),
            };
            (
                endpoint.label.clone(),
                format!("{detail} · {}", endpoint.status()),
                self.device_filter.as_ref() == Some(&endpoint.id),
                endpoint.enabled,
            )
        }));
        rows.push((
            "Add Device…".into(),
            self.device_setup_unavailable()
                .unwrap_or("Set up a remote host over SSH")
                .into(),
            false,
            self.device_setup_unavailable().is_none(),
        ));
        let mut view = div()
            .id("devices-list")
            .max_h(list_height(self.menu.anchor.y))
            .overflow_y_scroll()
            .track_scroll(&self.menu.devices_scroll)
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .p(px(8.))
                    .text_color(rgb(self.theme.muted))
                    .child("DEVICES"),
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
                                    .when(index == self.endpoints.len() + 1, |row| {
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
                                    .child(div().flex_1().min_w_0().truncate().child(label)),
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
                                    colors::ONLINE
                                } else {
                                    self.theme.muted
                                })),
                        )
                    })
                    .when(checked, |row| {
                        row.child(
                            div()
                                .debug_selector(move || format!("device-check-{index}"))
                                .flex_none()
                                .child("✓"),
                        )
                    })
                    .on_hover(cx.listener(move |this, hovered, _, cx| {
                        if *hovered {
                            this.menu.selected = Some(index);
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.choose_device(index, window, cx);
                    })),
            );
        }
        view
    }

    fn choose_device(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index == self.endpoints.len() + 1 {
            if self.device_setup_unavailable().is_some() {
                return;
            }
            let fields = std::array::from_fn(|index| {
                let input = cx.new(SearchInput::new);
                input.update(cx, |input, cx| {
                    input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
                    input.set_placeholder(
                        ["user@hostname or SSH alias", "The SSH target", "default"][index],
                        cx,
                    );
                });
                input
            });
            window.focus(&fields[0].read(cx).focus.clone(), cx);
            self.menu.device_setup = Some(Setup {
                fields,
                step: Step::Form,
                claim: None,
                task: None,
            });
            self.menu.page = Some(Page::AddDevice);
        } else {
            let filter = if index == 0 {
                None
            } else {
                let Some(endpoint) = self.endpoints.get(index - 1).filter(|e| e.enabled) else {
                    return;
                };
                Some(endpoint.id.clone())
            };
            self.dismiss_menu(window, cx);
            if let Some(id) = &filter
                && !self.select_endpoint(id, cx)
            {
                return;
            }
            self.device_filter = filter;
            for scroll in &self.sidebar_scroll {
                scroll.set_offset(Point::default());
            }
            for revealed in &self.sidebar_revealed {
                revealed.set(None);
            }
        }
        cx.notify();
    }

    pub(super) fn render_add_device(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let view = div()
            .debug_selector(|| "device-setup-dialog".into())
            .flex()
            .flex_col()
            .min_h_0()
            .child(
                div()
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
                            .child("Add Device"),
                    )
                    .child(
                        div()
                            .id("device-setup-close")
                            .debug_selector(|| "device-setup-close".into())
                            .px_2()
                            .py_1()
                            .cursor_pointer()
                            .rounded(px(crate::config::corners::CONTROL))
                            .hover(|s| s.bg(rgb(theme.active)))
                            .child("Close")
                            .on_click(
                                cx.listener(|this, _, window, cx| this.dismiss_menu(window, cx)),
                            ),
                    ),
            );
        let Some(setup) = &self.menu.device_setup else {
            return view;
        };
        let mut body = div().id("device-setup-body").debug_selector(|| "device-setup-body".into())
            .min_h_0().overflow_y_scroll().p(px(16.)).flex().flex_col().gap(px(12.))
            .child(div().flex_none().text_color(rgb(theme.muted))
                .child("Herdr checks the host over SSH and saves the device. If Herdr is missing or SSH needs your input, setup continues in a local workspace."));
        for (label, field) in [
            "SSH target",
            "Label (optional)",
            "Remote session (optional)",
        ]
        .into_iter()
        .zip(&setup.fields)
        {
            body = body.child(
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(label)
                    .child(field.clone()),
            );
        }
        if let Some(error) = &self.menu.error {
            body = body.child(
                div()
                    .flex_none()
                    .text_color(super::danger(theme))
                    .child(error.clone()),
            );
        }
        let status = match &setup.step {
            Step::Form => None,
            Step::Checking(request) => Some(format!("Checking Herdr on {}…", request.target())),
            Step::Saving(request, HostProbe::Stopped) => Some(format!(
                "Herdr is installed on {} but not running. Starting it…",
                request.target()
            )),
            Step::Saving(request, _) => Some(format!(
                "Herdr is running on {}. Saving the device…",
                request.target()
            )),
            Step::Saved => None,
            Step::Confirm(request, offer) => Some(offer.question(request.target())),
            Step::Verifying(request) => {
                Some(format!("Checking {} is not saved yet…", request.target()))
            }
            Step::Opening(..) => Some("Opening a local workspace…".into()),
        };
        if let Some(status) = status {
            body = body.child(
                div()
                    .debug_selector(|| "device-setup-status".into())
                    .flex_none()
                    .text_color(rgb(theme.muted))
                    .child(status),
            );
        }
        let button = |id: &'static str, label: SharedString, enabled: bool| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .p(px(8.))
                .rounded(px(crate::config::corners::CONTROL))
                .bg(rgb(theme.active))
                .when(enabled, |button| {
                    button.cursor_pointer().hover(|s| {
                        s.bg(rgb(theme.active).blend(rgba((theme.foreground << 8) | 0x20)))
                    })
                })
                .when(!enabled, |button| button.text_color(rgb(theme.muted)))
                .child(label)
        };
        let mut footer = div()
            .debug_selector(|| "device-setup-footer".into())
            .flex_none()
            .p(px(16.))
            .border_t_1()
            .border_color(rgb(theme.active))
            .flex()
            .justify_end()
            .gap(px(8.));
        footer = match &setup.step {
            Step::Confirm(_, offer) => footer
                .child(
                    button("device-setup-cancel", "Cancel".into(), true).on_click(cx.listener(
                        |this, _, _, cx| {
                            if let Some(setup) = &mut this.menu.device_setup {
                                setup.step = Step::Form;
                                this.menu.error = None;
                                cx.notify();
                            }
                        },
                    )),
                )
                .child(
                    button("device-setup-submit", offer.action().into(), true)
                        .on_click(cx.listener(|this, _, _, cx| this.open_setup_space(cx))),
                ),
            step => {
                let ready = matches!(step, Step::Form);
                footer.child(
                    button(
                        "device-setup-submit",
                        if ready { "Add device" } else { "Checking…" }.into(),
                        ready,
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.submit_device_setup(cx))),
                )
            }
        };
        view.child(body).child(footer)
    }

    /// Refuse a host already in the catalog. Checked again before each step
    /// that saves, since another window or the CLI can add it meanwhile. A
    /// device is matched by its saved entry: the sessions list may have attached
    /// it to another session, and that does not add one to the catalog.
    fn ensure_new_device(&self, request: &setup::Request) -> crate::Result<()> {
        match self.endpoints.iter().find(|endpoint| {
            endpoint
                .saved_ssh()
                .is_some_and(|(target, session)| request.same_host(target, session))
        }) {
            Some(endpoint) => Err(crate::Error::DeviceExists(endpoint.label.clone())),
            None => Ok(()),
        }
    }

    fn submit_device_setup(&mut self, cx: &mut Context<Self>) {
        let Some(form) = &mut self.menu.device_setup else {
            return;
        };
        match &form.step {
            Step::Form => {}
            Step::Confirm(..) => return self.open_setup_space(cx),
            _ => return,
        }
        let request = setup::Request::new(
            form.fields[0].read(cx).text(),
            form.fields[1].read(cx).text(),
            form.fields[2].read(cx).text(),
        );
        let request = match request.and_then(|request| {
            self.ensure_new_device(&request)?;
            Ok(request)
        }) {
            Ok(request) => request,
            Err(error) => {
                self.menu.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let Some(form) = &mut self.menu.device_setup else {
            return;
        };
        form.step = Step::Checking(request.clone());
        self.menu.error = None;
        // Claim before probing: a second add of this host, from any window of
        // this process, is refused from here on rather than after a slow probe.
        let background = cx.background_executor().spawn(async move {
            let claim = setup::claim(&request)?;
            Ok::<_, crate::Error>((claim, request.probe()))
        });
        form.task = Some(cx.spawn(async move |this, cx| {
            let result = background.await;
            let _ = this.update(cx, |this, cx| this.device_probed(result, cx));
        }));
        cx.notify();
    }

    /// Return to the form when the host is already saved or being added:
    /// offering a terminal would only add it again.
    fn refuse_duplicate(&mut self, error: &crate::Error, cx: &mut Context<Self>) -> bool {
        if !matches!(
            error,
            crate::Error::DeviceExists(_) | crate::Error::DeviceAdding
        ) {
            return false;
        }
        if let Some(form) = &mut self.menu.device_setup {
            form.step = Step::Form;
            form.claim = None;
        }
        self.menu.error = Some(error.to_string());
        cx.notify();
        true
    }

    fn device_probed(
        &mut self,
        result: crate::Result<(setup::Claim, crate::Result<HostProbe>)>,
        cx: &mut Context<Self>,
    ) {
        let Some(Setup {
            step: Step::Checking(request),
            ..
        }) = &self.menu.device_setup
        else {
            return;
        };
        let request = request.clone();
        let (claim, probe) = match result {
            Ok(result) => result,
            Err(error) => {
                if !self.refuse_duplicate(&error, cx) {
                    self.menu.error = Some(format!("Check {}: {error}", request.target()));
                    if let Some(form) = &mut self.menu.device_setup {
                        form.step = Step::Confirm(request, Offer::Terminal);
                    }
                    cx.notify();
                }
                return;
            }
        };
        let offer = match probe {
            Ok(probe) => match Offer::for_probe(probe) {
                None => return self.save_device(request, probe, claim, cx),
                Some(offer) => offer,
            },
            Err(error) => {
                self.menu.error = Some(format!("Check {}: {error}", request.target()));
                Offer::Terminal
            }
        };
        if let Some(form) = &mut self.menu.device_setup {
            form.step = Step::Confirm(request, offer);
            form.claim = Some(claim);
        }
        cx.notify();
    }

    fn save_device(
        &mut self,
        request: setup::Request,
        probe: HostProbe,
        claim: setup::Claim,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = &mut self.menu.device_setup else {
            return;
        };
        form.step = Step::Saving(request.clone(), probe);
        // The claim travels with the save and comes back, so a failure that
        // offers a terminal still holds the host.
        let background = cx.background_executor().spawn(async move {
            let result = setup::save(&request, &claim);
            (result, claim)
        });
        form.task = Some(cx.spawn(async move |this, cx| {
            let (result, claim) = background.await;
            let _ = this.update(cx, |this, cx| {
                let Some(Setup {
                    step: Step::Saving(request, _),
                    ..
                }) = &this.menu.device_setup
                else {
                    return;
                };
                let request = request.clone();
                let error = match result {
                    Ok(()) => {
                        // Saved: the catalog now refuses this host by itself.
                        if let Some(form) = &mut this.menu.device_setup {
                            form.step = Step::Saved;
                        }
                        cx.notify();
                        return;
                    }
                    Err(error) => error,
                };
                if this.refuse_duplicate(&error, cx) {
                    return;
                }
                this.menu.error = Some(error.to_string());
                if let Some(form) = &mut this.menu.device_setup {
                    form.step = Step::Confirm(request, Offer::Terminal);
                    form.claim = Some(claim);
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// Check the catalog on disk once more, then ask the local daemon for a
    /// workspace whose root pane runs the setup (see `poll_device_setup`).
    fn open_setup_space(&mut self, cx: &mut Context<Self>) {
        let Some(form) = &mut self.menu.device_setup else {
            return;
        };
        let Step::Confirm(request, _) = &form.step else {
            return;
        };
        let request = request.clone();
        let claim = form.claim.take();
        form.step = Step::Verifying(request.clone());
        self.menu.error = None;
        let background = cx.background_executor().spawn(async move {
            // A failed probe left no claim; take one now.
            match claim {
                Some(claim) => setup::verify_unsaved(&request, &claim).map(|()| claim),
                None => setup::claim(&request),
            }
        });
        form.task = Some(cx.spawn(async move |this, cx| {
            let result = background.await;
            let _ = this.update(cx, |this, cx| this.setup_space_verified(result, cx));
        }));
        cx.notify();
    }

    fn setup_space_verified(
        &mut self,
        result: crate::Result<setup::Claim>,
        cx: &mut Context<Self>,
    ) {
        let Some(Setup {
            step: Step::Verifying(request),
            ..
        }) = &self.menu.device_setup
        else {
            return;
        };
        let request = request.clone();
        let claim = match result {
            Ok(claim) => claim,
            Err(error) => {
                if !self.refuse_duplicate(&error, cx) {
                    self.menu.error = Some(format!("Open a local workspace: {error}"));
                    if let Some(form) = &mut self.menu.device_setup {
                        form.step = Step::Confirm(request, Offer::Terminal);
                    }
                    cx.notify();
                }
                return;
            }
        };
        let local = &self.endpoints[0];
        let result = (|| {
            let command = setup::terminal_command(&request)?;
            let boot = local
                .live
                .snapshot
                .as_ref()
                .filter(|_| local.live.status.is_connected())
                .map(|snapshot| snapshot.boot_id.clone())
                .ok_or(crate::Error::NotConnected)?;
            let id = local.connection.request_dialog(
                &boot,
                Method::WorkspaceCreate,
                serde_json::json!({"focus": true, "label": format!("Set up {}", request.label())}),
            )?;
            Ok::<_, crate::Error>(LocalSpace {
                request: id,
                boot,
                command,
            })
        })();
        let Some(form) = &mut self.menu.device_setup else {
            return;
        };
        form.claim = Some(claim);
        match result {
            Ok(space) => form.step = Step::Opening(request, space),
            Err(error) => {
                form.step = Step::Confirm(request, Offer::Terminal);
                self.menu.error = Some(format!("Open a local workspace: {error}"));
            }
        }
        cx.notify();
    }

    /// Runs the setup command in the workspace the local daemon created, then
    /// shows it. The command is typed into the pane's shell: the endpoint API
    /// has no method that starts a command in a pane.
    pub(crate) fn poll_device_setup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A saved device appears in the sidebar and picker by itself, so the
        // dialog has nothing left to say. Closing here, where the window is
        // at hand, returns focus to the terminal.
        if self
            .menu
            .device_setup
            .as_ref()
            .is_some_and(|form| matches!(form.step, Step::Saved))
        {
            return self.dismiss_menu(window, cx);
        }
        let Some(Setup {
            step: Step::Opening(_, space),
            ..
        }) = &self.menu.device_setup
        else {
            return;
        };
        let local = &self.endpoints[0];
        let fail = |this: &mut Self, error: String, cx: &mut Context<Self>| {
            if let Some(form) = &mut this.menu.device_setup
                && let Step::Opening(request, _) = &form.step
            {
                form.step = Step::Confirm(request.clone(), Offer::Terminal);
            }
            this.menu.error = Some(format!("Open a local workspace: {error}"));
            cx.notify();
        };
        let current = local.live.status.is_connected()
            && local
                .live
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.boot_id == space.boot);
        if !current {
            return fail(self, crate::Error::StaleConnection.to_string(), cx);
        }
        let Some((id, Some(result))) = &local.live.dialog_response else {
            return;
        };
        if id != &space.request {
            return;
        }
        let response = match result {
            Ok(response) => response,
            Err(error) => return fail(self, error.to_string(), cx),
        };
        if let Some(error) = response.get("error") {
            let (code, message) = super::endpoint_error(error);
            return fail(self, format!("{code}: {message}"), cx);
        }
        let result = &response["result"];
        let created = (result["type"] == "workspace_created")
            .then(|| {
                Some((
                    result["workspace"]["workspace_id"].as_str()?,
                    result["root_pane"]["pane_id"].as_str()?,
                ))
            })
            .flatten()
            .filter(|(workspace, pane)| !workspace.is_empty() && !pane.is_empty());
        let Some((workspace, pane)) = created else {
            return fail(self, "Unexpected daemon response".into(), cx);
        };
        let (workspace, pane) = (workspace.to_owned(), pane.to_owned());
        let sent = local
            .connection
            .handle
            .as_ref()
            .ok_or(crate::Error::NotConnected)
            .and_then(|handle| {
                Ok(handle.send_input(
                    &space.boot,
                    &pane,
                    [
                        ClientPaneInputEvent::TextCommit(space.command.clone()),
                        enter(),
                    ],
                )?)
            });
        if let Err(error) = sent {
            return fail(self, error.to_string(), cx);
        }
        // The terminal's `machine add` outlives this dialog; keep the host
        // claimed so this process cannot start a second add meanwhile.
        if let Some(claim) = self
            .menu
            .device_setup
            .as_mut()
            .and_then(|form| form.claim.take())
        {
            claim.hold();
        }
        let local = local.id.clone();
        self.dismiss_menu(window, cx);
        self.navigate_endpoint(&local, NavigationTarget::Workspace(&workspace), cx);
    }

    pub(super) fn devices_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        if self.menu.page == Some(Page::AddDevice) {
            let Some(form) = &self.menu.device_setup else {
                return;
            };
            if form
                .fields
                .iter()
                .any(|field| field.read(cx).is_composing())
            {
                return;
            }
            match key {
                "tab" => {
                    let index = form
                        .fields
                        .iter()
                        .position(|field| field.read(cx).focus.is_focused(window))
                        .unwrap_or(0);
                    let next = (index
                        + if event.keystroke.modifiers.shift {
                            2
                        } else {
                            1
                        })
                        % 3;
                    window.focus(&form.fields[next].read(cx).focus.clone(), cx);
                }
                "enter" => self.submit_device_setup(cx),
                "escape" => self.dismiss_menu(window, cx),
                _ => return,
            }
        } else {
            let count = self.endpoints.len() + 2;
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
                "escape" => self.dismiss_menu(window, cx),
                _ => {}
            }
        }
        cx.stop_propagation();
        window.prevent_default();
    }
}

fn enter() -> ClientPaneInputEvent {
    ClientPaneInputEvent::Key {
        code: ClientKeyCode::Enter,
        modifiers: 0,
        kind: ClientKeyKind::Press,
        repeat_count: 1,
        shifted_codepoint: None,
        generated_text: None,
        tracks_release: false,
        physical_key_id: None,
        windows_record: None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{LocalSpace, Offer, Page, Setup, Step, enter, setup};
    use crate::{
        HerdrWindow, NavigationTarget, search_input::SearchInput,
        sidebar::layout_tests::fixture_window, state::ConnectionStatus,
    };
    use gpui::{AppContext, Context};
    use herdr_client::protocol::{ClientMessage, ClientShellSnapshot};
    use herdr_client::{HostProbe, protocol::ClientPaneInputEvent};
    use std::sync::Arc;

    #[test]
    fn only_a_present_herdr_is_saved_without_a_terminal() {
        assert_eq!(Offer::for_probe(HostProbe::Running), None);
        assert_eq!(Offer::for_probe(HostProbe::Stopped), None);
        assert_eq!(Offer::for_probe(HostProbe::Missing), Some(Offer::Install));
        assert_eq!(Offer::for_probe(HostProbe::Outdated), Some(Offer::Update));
        assert_eq!(
            Offer::for_probe(HostProbe::SshFailed),
            Some(Offer::Terminal)
        );
        assert_eq!(
            Offer::Install.question("dev@box"),
            "Herdr was not detected on dev@box. Should we install it?"
        );
    }

    fn open_form(view: &mut HerdrWindow, step: Step, cx: &mut Context<HerdrWindow>) {
        view.menu.device_setup = Some(Setup {
            fields: std::array::from_fn(|_| cx.new(SearchInput::new)),
            step,
            claim: None,
            task: None,
        });
        view.menu.page = Some(Page::AddDevice);
    }

    fn step(view: &HerdrWindow) -> &Step {
        &view.menu.device_setup.as_ref().unwrap().step
    }

    #[gpui::test]
    fn a_saved_device_closes_the_dialog_by_itself(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(fixture_window);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                open_form(view, Step::Saved, cx);
                view.poll_device_setup(window, cx);
                assert!(view.menu.page.is_none());
                assert!(view.menu.device_setup.is_none());
                assert!(view.focus.is_focused(window));
            });
        });
    }

    #[gpui::test]
    fn install_needs_a_connected_local_daemon(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(fixture_window);
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                let request = setup::Request::new("dev@box", "Box", "").unwrap();
                view.endpoints[0].live.status = ConnectionStatus::Disconnected;
                open_form(view, Step::Verifying(request), cx);
                view.setup_space_verified(Ok(setup::Claim::fixture("disconnected.test")), cx);
                assert!(matches!(step(view), Step::Confirm(_, Offer::Terminal)));
                // The claim stays with the dialog for a retry.
                assert!(view.menu.device_setup.as_ref().unwrap().claim.is_some());
                assert!(
                    view.menu
                        .error
                        .as_deref()
                        .unwrap()
                        .starts_with("Open a local workspace:")
                );
            });
        });
    }

    /// The sessions list can attach a saved device to another of its sessions.
    /// The catalog still holds the entry it was saved with, so adding that entry
    /// again is refused, and the session it now shows was never saved.
    #[gpui::test]
    fn a_device_on_another_session_is_still_matched_by_its_saved_entry(
        cx: &mut gpui::TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(fixture_window);
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.reconcile_catalog(
                    vec![herdr_client::SavedHost {
                        id: "0123456789abcdef0123456789abcdef".into(),
                        label: "m5max-ms".into(),
                        target: "penso@box".into(),
                        session: "default".into(),
                        enabled: true,
                    }],
                    cx,
                );
                // What choosing another session from the list does to the target.
                view.endpoints[1].connection.target = herdr_client::ConnectTarget::Ssh {
                    target: "penso@box".into(),
                    session: "work".into(),
                };
                let saved = setup::Request::new("penso@box", "Again", "").unwrap();
                assert!(matches!(
                    view.ensure_new_device(&saved),
                    Err(crate::Error::DeviceExists(label)) if label == "m5max-ms"
                ));
                let work = setup::Request::new("penso@box", "Work", "work").unwrap();
                assert!(view.ensure_new_device(&work).is_ok());
            });
        });
    }

    #[gpui::test]
    fn a_host_already_saved_or_being_added_returns_to_the_form(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(fixture_window);
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.endpoints.push(crate::endpoint::Endpoint::new(
                    "0123456789abcdef0123456789abcdef".into(),
                    "m5max-ms".into(),
                    herdr_client::ConnectTarget::Ssh {
                        target: "penso@box".into(),
                        session: "default".into(),
                    },
                    true,
                ));
                // The window's own list refuses the exact spelling at once.
                let again = setup::Request::new("penso@box", "Again", "").unwrap();
                assert!(matches!(
                    view.ensure_new_device(&again),
                    Err(crate::Error::DeviceExists(label)) if label == "m5max-ms"
                ));
                let other = setup::Request::new("penso@box", "Work", "work").unwrap();
                assert!(view.ensure_new_device(&other).is_ok());

                // Background checks catch other spellings and concurrent adds.
                // Neither may fall through to offering a terminal.
                for (error, message) in [
                    (
                        crate::Error::DeviceExists("m5max-ms".into()),
                        "This host and session are already saved as \u{201c}m5max-ms\u{201d}.",
                    ),
                    (
                        crate::Error::DeviceAdding,
                        "This host is already being added.",
                    ),
                ] {
                    open_form(view, Step::Checking(again.clone()), cx);
                    view.device_probed(Err(error), cx);
                    assert!(matches!(step(view), Step::Form));
                    assert_eq!(view.menu.error.as_deref(), Some(message));
                }
                open_form(view, Step::Verifying(again.clone()), cx);
                view.setup_space_verified(Err(crate::Error::DeviceExists("m5max-ms".into())), cx);
                assert!(matches!(step(view), Step::Form));
            });
        });
    }

    #[gpui::test]
    fn created_space_runs_setup_in_its_root_pane(cx: &mut gpui::TestAppContext) {
        let mut peer = crate::window::MockPeer::new();
        let (view, cx) = cx.add_window_view(fixture_window);
        let snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
            "../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
        ))
        .unwrap();
        let boot = snapshot.boot_id.clone();
        let space = |request: &str| {
            let request = request.to_owned();
            let boot = boot.clone();
            move || LocalSpace {
                request,
                boot,
                command: "'herdr' 'machine' 'add'".into(),
            }
        };
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let local = &mut view.endpoints[0];
                local.connection.handle = Some(peer.client.handle.clone());
                local.live.snapshot = Some(Arc::new(snapshot));
                local.live.status = ConnectionStatus::Connected;
                let request = setup::Request::new("dev@box", "Box", "").unwrap();

                // A refusal returns to the question with the daemon's reason.
                open_form(view, Step::Opening(request.clone(), space("create")()), cx);
                view.endpoints[0].live.dialog_response = Some((
                    "create".into(),
                    Some(Ok(serde_json::json!({"error":{"code":"denied","message":"no"}}))),
                ));
                view.poll_device_setup(window, cx);
                assert!(matches!(step(view), Step::Confirm(_, Offer::Terminal)));
                assert!(view.menu.error.as_deref().unwrap().ends_with("denied: no"));

                // Another dialog's response is not this workspace.
                let created = serde_json::json!({"result":{"type":"workspace_created","workspace":{"workspace_id":"w9"},"tab":{"tab_id":"t9"},"root_pane":{"pane_id":"w9:p1"}}});
                open_form(view, Step::Opening(request, space("create")()), cx);
                view.endpoints[0].live.dialog_response =
                    Some(("other".into(), Some(Ok(created.clone()))));
                view.poll_device_setup(window, cx);
                assert!(matches!(step(view), Step::Opening(..)));

                view.endpoints[0].live.dialog_response =
                    Some(("create".into(), Some(Ok(created))));
                view.poll_device_setup(window, cx);
                assert!(view.menu.page.is_none());
                assert_eq!(
                    view.pending_navigation,
                    Some(NavigationTarget::Workspace("w9".into()))
                );
            });
        });
        assert_eq!(
            peer.receive(),
            ClientMessage::ClientShellPaneInput {
                pane_id: "w9:p1".into(),
                events: vec![
                    ClientPaneInputEvent::TextCommit("'herdr' 'machine' 'add'".into()),
                    enter(),
                ],
            }
        );
    }
}
