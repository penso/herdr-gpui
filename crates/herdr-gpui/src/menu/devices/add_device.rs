//! The Add Device dialog: its form, the host check and save that run off the
//! UI thread, and the local workspace that carries a setup needing prompts.

use super::{Page, discover::Discovery, setup};
use crate::{
    HerdrWindow, NavigationTarget,
    search_input::{Changed, SearchInput},
};
use gpui::{prelude::*, *};
use herdr_client::{
    HostProbe, Method, NewHostKey, SshFailure,
    protocol::{ClientKeyCode, ClientKeyKind, ClientPaneInputEvent},
};

pub(in crate::menu) struct Setup {
    pub(super) fields: [Entity<SearchInput>; 3],
    pub(super) step: Step,
    /// This process's claim on the host being added, from the first check
    /// until the device is saved or the dialog closes.
    pub(super) claim: Option<setup::Claim>,
    pub(super) task: Option<Task<()>>,
    /// The search for hosts to suggest, from when the form opens.
    pub(super) discovery: Option<Discovery>,
    /// The label a suggestion filled in, until the user edits it. Only that
    /// label is replaced when another suggestion is chosen.
    pub(super) suggested_label: Option<String>,
    /// Clears `suggested_label` once the label field holds anything else, and
    /// refilters the suggestions as the target changes.
    pub(super) _edits: Vec<Subscription>,
    /// The suggestion Up and Down moved to, by the names and addresses that
    /// identify its machine, with the target text it was chosen for: typing
    /// anything else drops it.
    pub(super) highlight: Option<(String, Vec<String>)>,
    /// Why the last check or save failed. Each source of Add Device keeps
    /// its own, so a job finishing while another source shows stays here.
    pub(super) error: Option<String>,
    pub(super) suggestion_scroll: ScrollHandle,
}

/// Where adding a device stands. Each step after `Form` belongs to the request
/// that was submitted, not to whatever the fields hold now.
pub(super) enum Step {
    Form,
    Checking(setup::Request),
    /// macOS refused the host check the local network; waiting for the user
    /// to allow Herdr, then checking again (see `local_network`).
    AwaitingLocalNetwork(setup::Request),
    /// Herdr is present, so the CLI saves the device without a terminal. A
    /// stopped server is started by that same command.
    Saving(setup::Request, HostProbe),
    /// Saved; the next frame closes the dialog (see `poll_device_setup`).
    Saved,
    /// Already saved; keep the recovery advice readable until dismissed.
    SavedWarning(&'static str),
    /// SSH refused an unknown host key; fetching it for the user to approve.
    FetchingKey(setup::Request),
    /// The user compares the host key's fingerprint before trusting it.
    TrustKey(setup::Request, NewHostKey),
    /// Setup needs prompts, so the user decides whether to run it locally.
    Confirm(setup::Request, Offer),
    /// Checking the catalog on disk again before opening the setup workspace
    /// for the offer the user accepted.
    Verifying(setup::Request, Offer),
    /// Waiting for the window to show this computer's Herdr, whose surface
    /// must be active before it creates a workspace (see `create_setup_space`).
    Activating(setup::Request, setup::TerminalSetup, Offer),
    /// Waiting for the local daemon to create the setup workspace.
    Opening(setup::Request, LocalSpace),
}

/// Why setup has to continue in a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Offer {
    Install,
    Update,
    /// SSH needs a prompt, or saving without a terminal failed.
    Terminal,
    /// Herdr restarts the host's server before saving, once confirmed.
    Restart,
    /// SSH accepted none of the user's keys: install one with the password,
    /// so the device then connects without a prompt.
    InstallKey,
}

impl Offer {
    /// `None` when Herdr is present, so the device is saved without a terminal.
    pub(super) fn for_probe(probe: HostProbe) -> Option<Self> {
        match probe {
            HostProbe::Running | HostProbe::Stopped => None,
            HostProbe::Missing => Some(Self::Install),
            HostProbe::Outdated => Some(Self::Update),
            HostProbe::SshFailed(SshFailure::Auth) => Some(Self::InstallKey),
            HostProbe::SshFailed(_) => Some(Self::Terminal),
        }
    }

    /// `user` is the login `ssh` resolved for the target, when known: a target
    /// without one logs in as the local user, which may not be the account
    /// the user's keys are installed for.
    pub(super) fn question(self, target: &str, user: Option<&str>) -> String {
        if cfg!(windows) {
            return self.terminal_instructions(target, user);
        }
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
            Self::Restart => format!(
                "Restart the Herdr server on {target}? A local terminal asks you to confirm."
            ),
            Self::InstallKey => match implied_user(target, user) {
                Some(user) => format!(
                    "{target} accepted none of your SSH keys for the user {user}. If you log in there as someone else, change the target to {} and press Enter to check it. Otherwise, install your key for {user}? A local terminal runs ssh-copy-id, which asks for the password once, then saves the device.",
                    with_user(target, "name")
                ),
                None => format!(
                    "{target} accepted none of your SSH keys. Install your key there? A local terminal runs ssh-copy-id, which asks for the password once, then saves the device."
                ),
            },
        }
    }

    fn action(self) -> &'static str {
        match self {
            Self::Install => "Install",
            Self::Update => "Update",
            Self::Terminal | Self::Restart => "Open terminal",
            Self::InstallKey => "Set up key login",
        }
    }

    /// Windows cannot run a setup on a local terminal yet (see
    /// `setup::terminal_setup`), so the user runs Herdr's own command in one.
    fn terminal_instructions(self, target: &str, user: Option<&str>) -> String {
        let reason = match self {
            Self::Install => "Herdr was not detected there".into(),
            Self::Update => "its Herdr is too old for this app".into(),
            Self::Terminal => "SSH needs your input, such as a password or host key".into(),
            Self::Restart => "its Herdr server has to restart".into(),
            Self::InstallKey => match implied_user(target, user) {
                Some(user) => format!(
                    "it accepted none of your SSH keys for the user {user} (use {} to log in as someone else)",
                    with_user(target, "name")
                ),
                None => "it accepted none of your SSH keys".into(),
            },
        };
        format!(
            "Setting up {target} needs a terminal: {reason}. Run `herdr machine add {target}` in a terminal, then add the device again."
        )
    }
}

/// The resolved login, only when `target` does not name one itself: the
/// dialog then has not shown which account SSH tried.
fn implied_user<'a>(target: &str, user: Option<&'a str>) -> Option<&'a str> {
    let named = target
        .strip_prefix("ssh://")
        .unwrap_or(target)
        .contains('@');
    user.filter(|_| !named)
}

/// `target` logging in as `user`; an `ssh://` URI takes the user inside it.
fn with_user(target: &str, user: &str) -> String {
    match target.strip_prefix("ssh://") {
        Some(rest) => format!("ssh://{user}@{rest}"),
        None => format!("{user}@{target}"),
    }
}

/// The local workspace request whose root pane will run the setup command.
pub(super) struct LocalSpace {
    pub(super) request: String,
    pub(super) boot: String,
    /// The relay command the workspace's shell runs, and the setup it shows.
    pub(super) setup: setup::TerminalSetup,
    /// What the user accepted, so a retry after a failure runs it again.
    pub(super) offer: Offer,
}

impl HerdrWindow {
    /// Set the SSH form's error, which stays with that form.
    pub(super) fn set_device_error(&mut self, error: Option<String>) {
        if let Some(setup) = &mut self.menu.device_setup {
            setup.error = error;
        }
    }

    /// Show an empty Add Device form with the target field focused.
    pub(in crate::menu) fn open_add_device(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        let label_edits = cx.subscribe(&fields[1], |this, field, _: &Changed, cx| {
            if let Some(setup) = &mut this.menu.device_setup
                && setup.suggested_label.as_deref() != Some(field.read(cx).text())
            {
                setup.suggested_label = None;
            }
        });
        // An offer runs for the target that was checked, so changing the
        // target withdraws it: the next submit checks the new target.
        let target_edits = cx.subscribe(&fields[0], |this, field, _: &Changed, cx| {
            if let Some(setup) = &mut this.menu.device_setup
                && let Step::Confirm(request, _) = &setup.step
                && request.target() != field.read(cx).text().trim()
            {
                setup.step = Step::Form;
                setup.claim = None;
                setup.error = None;
            }
            cx.notify();
        });
        self.menu.device_setup = Some(Setup {
            fields,
            step: Step::Form,
            claim: None,
            task: None,
            discovery: None,
            suggested_label: None,
            _edits: vec![label_edits, target_edits],
            highlight: None,
            suggestion_scroll: ScrollHandle::new(),
            error: None,
        });
        self.menu.page = Some(Page::AddDevice);
        self.start_device_discovery(cx);
    }

    /// Route a key to the Add Device form. Returns `false` when the key is
    /// left to propagate: during composition, or when the form ignores it.
    pub(super) fn add_device_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(form) = &self.menu.device_setup else {
            return false;
        };
        if matches!(form.step, Step::SavedWarning(_)) {
            if matches!(event.keystroke.key.as_str(), "enter" | "escape") {
                self.dismiss_menu(window, cx);
            }
            // The completed form has no editable fields or further setup action.
            return true;
        }
        if form
            .fields
            .iter()
            .any(|field| field.read(cx).is_composing())
        {
            return false;
        }
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        let in_target = form.fields[0].read(cx).focus.is_focused(window);
        let plain = !(modifiers.shift || modifiers.platform || modifiers.control || modifiers.alt);
        if in_target && plain && self.complete_device_target(key, window, cx) {
            return true;
        }
        let Some(form) = &self.menu.device_setup else {
            return false;
        };
        match key {
            "tab" => {
                let index = form
                    .fields
                    .iter()
                    .position(|field| field.read(cx).focus.is_focused(window))
                    .unwrap_or(0);
                let next = (index + if modifiers.shift { 2 } else { 1 }) % 3;
                window.focus(&form.fields[next].read(cx).focus.clone(), cx);
            }
            "enter" => self.submit_device_setup(cx),
            "escape" => self.dismiss_menu(window, cx),
            _ => return false,
        }
        true
    }

    pub(in crate::menu) fn render_add_device(
        &self,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = &self.theme;
        let frame = |pane: Div, cx: &mut Context<Self>| {
            self.add_device_frame(
                "device-setup-dialog",
                "device-setup-close",
                compact,
                pane,
                cx,
            )
        };
        let Some(setup) = &self.menu.device_setup else {
            return frame(div(), cx);
        };
        if let Step::SavedWarning(warning) = setup.step {
            return frame(self.render_device_saved_warning(warning, cx), cx);
        }
        let ghost = matches!(setup.step, Step::Form)
            .then(|| {
                let text = setup.fields[0].read(cx).text().to_owned();
                self.device_completion(setup, &text).ghost(&text)
            })
            .flatten();
        setup.fields[0].update(cx, |field, cx| field.set_completion(ghost, cx));
        let mut body = div().id("device-setup-body").debug_selector(|| "device-setup-body".into())
            .flex_1()
            .min_h_0().overflow_y_scroll().p(px(16.)).flex().flex_col().gap(px(12.))
            .child(div().flex_none().text_color(rgb(theme.subtext()))
                .child("Herdr checks the host over SSH and saves the device. If Herdr is missing or SSH needs your input, setup continues in a local workspace."));
        for (index, (label, field)) in [
            "SSH target",
            "Label (optional)",
            "Remote session (optional)",
        ]
        .into_iter()
        .zip(&setup.fields)
        .enumerate()
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
            // Suggestions fill the target, so they sit right under it, and
            // only while the form can still take one.
            if index == 0
                && matches!(setup.step, Step::Form)
                && let Some(discovery) = &setup.discovery
            {
                body = body.child(self.render_device_suggestions(setup, discovery, cx));
            }
        }
        if let Some(error) = &setup.error {
            body = body.child(
                div()
                    .flex_none()
                    .text_color(crate::menu::danger(theme))
                    .child(error.clone()),
            );
        }
        let status = match &setup.step {
            Step::Form => None,
            Step::Checking(request) => Some(format!("Checking Herdr on {}…", request.target())),
            Step::AwaitingLocalNetwork(request) => {
                Some(super::local_network::waiting_status(request.target()))
            }
            Step::Saving(request, HostProbe::Stopped) => Some(format!(
                "Herdr is installed on {} but not running. Starting it…",
                request.target()
            )),
            Step::Saving(request, _) => Some(format!(
                "Herdr is running on {}. Saving the device…",
                request.target()
            )),
            Step::Saved | Step::SavedWarning(_) => None,
            Step::FetchingKey(request) => {
                Some(format!("Fetching the host key of {}…", request.target()))
            }
            Step::TrustKey(request, key) => {
                body = body.child(self.render_host_key(request.target(), key));
                None
            }
            Step::Confirm(request, offer) => Some(offer.question(
                request.target(),
                setup.claim.as_ref().map(setup::Claim::user),
            )),
            Step::Verifying(request, _) => {
                Some(format!("Checking {} is not saved yet…", request.target()))
            }
            Step::Activating(..) | Step::Opening(..) => Some("Opening a local workspace…".into()),
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
        let cancel = button("device-setup-cancel", "Cancel".into(), true)
            .on_click(cx.listener(|this, _, _, cx| this.cancel_device_step(cx)));
        footer = match &setup.step {
            Step::Confirm(..) if cfg!(windows) => footer.child(cancel),
            Step::Confirm(_, offer) => footer.child(cancel).child(
                button("device-setup-submit", offer.action().into(), true)
                    .on_click(cx.listener(|this, _, _, cx| this.open_setup_space(cx))),
            ),
            Step::AwaitingLocalNetwork(_) => footer
                .children(self.local_network_settings_button("device-setup-settings", cx))
                .child(cancel),
            // Enter does not trust a key: only this button does.
            Step::TrustKey(..) => footer.child(cancel).child(
                button("device-setup-trust", "Trust key".into(), true)
                    .on_click(cx.listener(|this, _, _, cx| this.trust_host_key(cx))),
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
        frame(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(body)
                .child(footer),
            cx,
        )
    }

    /// Refuse a host already in the catalog. Checked again before each step
    /// that saves, since another window or the CLI can add it meanwhile. A
    /// device is matched by its saved entry: the sessions list may have attached
    /// it to another session, and that does not add one to the catalog.
    pub(super) fn ensure_new_device(&self, request: &setup::Request) -> crate::Result<()> {
        match self.endpoints.iter().find(|endpoint| {
            endpoint
                .saved_ssh()
                .is_some_and(|(target, session)| request.same_host(target, session))
        }) {
            Some(endpoint) => Err(super::Error::Exists(endpoint.label.clone()).into()),
            None => Ok(()),
        }
    }

    fn submit_device_setup(&mut self, cx: &mut Context<Self>) {
        let Some(form) = &mut self.menu.device_setup else {
            return;
        };
        match &form.step {
            Step::Form => {}
            // Windows offers no terminal setup; the question says what to run.
            Step::Confirm(..) if cfg!(windows) => return,
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
                tracing::info!(category = "device_setup", "Device form refused");
                self.set_device_error(Some(error.to_string()));
                cx.notify();
                return;
            }
        };
        let Some(form) = &mut self.menu.device_setup else {
            return;
        };
        tracing::info!(category = "device_setup", "Checking device");
        form.step = Step::Checking(request.clone());
        form.error = None;
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
            crate::Error::Devices(super::Error::Exists(_) | super::Error::Adding)
        ) {
            return false;
        }
        if let Some(form) = &mut self.menu.device_setup {
            form.step = Step::Form;
            form.claim = None;
        }
        self.set_device_error(Some(error.to_string()));
        cx.notify();
        true
    }

    pub(super) fn device_probed(
        &mut self,
        result: crate::Result<(setup::Claim, crate::Result<HostProbe>)>,
        cx: &mut Context<Self>,
    ) {
        let (request, waited) = match &self.menu.device_setup {
            Some(Setup {
                step: Step::Checking(request),
                ..
            }) => (request.clone(), false),
            Some(Setup {
                step: Step::AwaitingLocalNetwork(request),
                ..
            }) => (request.clone(), true),
            _ => return,
        };
        let (claim, probe) = match result {
            Ok(result) => result,
            Err(error) => {
                tracing::warn!(category = "device_setup", "Could not claim device");
                if !self.refuse_duplicate(&error, cx) {
                    self.set_device_error(Some(format!("Check {}: {error}", request.target())));
                    if let Some(form) = &mut self.menu.device_setup {
                        form.step = Step::Confirm(request, Offer::Terminal);
                    }
                    cx.notify();
                }
                return;
            }
        };
        match &probe {
            Ok(probe) => tracing::info!(
                category = "device_setup",
                probe = ?probe,
                "Device probed"
            ),
            Err(_) => tracing::warn!(category = "device_setup", "Device probe failed"),
        }
        // An unknown key can be approved here; a changed one is refused while
        // fetching it, and the terminal is offered instead.
        if let Ok(HostProbe::SshFailed(SshFailure::HostKey)) = probe {
            return self.fetch_host_key(request, claim, cx);
        }
        // A terminal cannot help: the permission belongs to this app. The
        // first refusal can be the system still asking, so wait for the user;
        // a refusal after the wait returns to the form, and the claim drops.
        if let Ok(HostProbe::SshFailed(failure @ SshFailure::LocalNetworkDenied)) = probe {
            if !waited {
                return self.await_local_network(request, claim, cx);
            }
            self.set_device_error(Some(failure.to_string()));
            if let Some(form) = &mut self.menu.device_setup {
                form.step = Step::Form;
            }
            cx.notify();
            return;
        }
        let offer = match probe {
            Ok(probe) => match Offer::for_probe(probe) {
                None => return self.save_device(request, probe, claim, cx),
                Some(offer) => {
                    // Say what SSH refused, since the terminal may not need to.
                    // The key offer already says it, without the advice
                    // about agents that does not apply to a new host.
                    if let HostProbe::SshFailed(failure) = probe
                        && offer != Offer::InstallKey
                    {
                        self.set_device_error(Some(format!(
                            "SSH to {}: {failure}",
                            request.target()
                        )));
                    }
                    offer
                }
            },
            Err(error) => {
                self.set_device_error(Some(format!("Check {}: {error}", request.target())));
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
                match &result {
                    Ok(_) => tracing::info!(category = "device_setup", "Device saved"),
                    Err(_) => tracing::warn!(category = "device_setup", "Device save failed"),
                }
                let error = match result {
                    Ok(outcome) => {
                        this.device_save_succeeded(outcome, cx);
                        return;
                    }
                    Err(error) => error,
                };
                if this.refuse_duplicate(&error, cx) {
                    return;
                }
                this.set_device_error(Some(error.to_string()));
                let offer = match error {
                    crate::Error::Devices(super::Error::ServerRestart) => Offer::Restart,
                    _ => Offer::Terminal,
                };
                if let Some(form) = &mut this.menu.device_setup {
                    form.step = Step::Confirm(request, offer);
                    form.claim = Some(claim);
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// Back to the form, giving up the host: the next submit, for this host or
    /// another, claims it again.
    pub(super) fn cancel_device_step(&mut self, cx: &mut Context<Self>) {
        if let Some(setup) = &mut self.menu.device_setup {
            setup.step = Step::Form;
            setup.claim = None;
            // Ends a wait for Local Network permission, which holds a claim.
            setup.task = None;
            setup.error = None;
            cx.notify();
        }
    }

    /// Check the catalog on disk once more, then ask the local daemon for a
    /// workspace whose root pane runs the setup (see `poll_device_setup`).
    fn open_setup_space(&mut self, cx: &mut Context<Self>) {
        let Some(form) = &mut self.menu.device_setup else {
            return;
        };
        let Step::Confirm(request, offer) = &form.step else {
            return;
        };
        let (request, offer) = (request.clone(), *offer);
        let claim = form.claim.take();
        form.step = Step::Verifying(request.clone(), offer);
        form.error = None;
        let background = cx.background_executor().spawn(async move {
            // A failed probe left no claim; take one now.
            let claim = match claim {
                Some(claim) => setup::verify_unsaved(&request, &claim).map(|()| claim),
                None => setup::claim(&request),
            }?;
            // The setup runs here, with the user's login environment, which
            // can block the first time it is read.
            let terminal = setup::terminal_setup(&request, offer == Offer::InstallKey);
            Ok::<_, crate::Error>((claim, terminal))
        });
        form.task = Some(cx.spawn(async move |this, cx| {
            let result = background.await;
            let _ = this.update(cx, |this, cx| this.setup_space_verified(result, cx));
        }));
        cx.notify();
    }

    pub(super) fn setup_space_verified(
        &mut self,
        result: crate::Result<(setup::Claim, crate::Result<setup::TerminalSetup>)>,
        cx: &mut Context<Self>,
    ) {
        let Some(Setup {
            step: Step::Verifying(request, offer),
            ..
        }) = &self.menu.device_setup
        else {
            return;
        };
        let (request, offer) = (request.clone(), *offer);
        let (claim, terminal) = match result {
            Ok(verified) => verified,
            Err(error) => {
                tracing::warn!(
                    category = "device_setup",
                    "Device setup verification failed"
                );
                if !self.refuse_duplicate(&error, cx) {
                    self.set_device_error(Some(format!("Open a local workspace: {error}")));
                    if let Some(form) = &mut self.menu.device_setup {
                        form.step = Step::Confirm(request, offer);
                    }
                    cx.notify();
                }
                return;
            }
        };
        let Some(form) = &mut self.menu.device_setup else {
            return;
        };
        form.claim = Some(claim);
        let setup = match terminal {
            Ok(setup) => setup,
            Err(error) => return self.fail_setup_space(request, offer, &error, cx),
        };
        // Herdr refuses commands from a connection whose surface is inactive,
        // as Local's is while the window shows another device, so the window
        // moves to Local first; it ends there anyway once the setup opens.
        // Switching closes every dialog, so this one is put back over Local.
        let (page, form) = (self.menu.page, self.menu.device_setup.take());
        let selected = self.select_endpoint(crate::endpoint::LOCAL, cx);
        (self.menu.page, self.menu.device_setup) = (page, form);
        if !selected {
            return self.fail_setup_space(request, offer, &crate::Error::NotConnected, cx);
        }
        if let Some(form) = &mut self.menu.device_setup {
            form.step = Step::Activating(request, setup, offer);
        }
        self.create_setup_space(cx);
    }

    /// Asks the local daemon for the setup workspace once the window has
    /// queued Local's surface activation. The request follows it on the same
    /// ordered connection, so Herdr handles it with the surface active.
    pub(super) fn create_setup_space(&mut self, cx: &mut Context<Self>) {
        let Some(Setup {
            step: Step::Activating(..),
            ..
        }) = &self.menu.device_setup
        else {
            return;
        };
        let local = &self.endpoints[0];
        let connected = local.live.status.is_connected();
        if self.selected_endpoint == 0 && connected && !local.surface_requested() {
            // The next poll activates the surface.
            return;
        }
        let result = (|| {
            if self.selected_endpoint != 0 {
                return Err(crate::Error::StaleConnection);
            }
            let boot = local
                .live
                .snapshot
                .as_ref()
                .filter(|_| connected)
                .map(|snapshot| snapshot.boot_id.clone())
                .ok_or(crate::Error::NotConnected)?;
            let Some(Setup {
                step: Step::Activating(request, ..),
                ..
            }) = &self.menu.device_setup
            else {
                return Err(crate::Error::StaleConnection);
            };
            local
                .connection
                .request_dialog(
                    &boot,
                    Method::WorkspaceCreate,
                    serde_json::json!({
                        "focus": true,
                        "label": format!("Set up {}", request.label()),
                    }),
                )
                .map(|id| (id, boot))
        })();
        let Some(form) = &mut self.menu.device_setup else {
            return;
        };
        let Step::Activating(request, setup, offer) = std::mem::replace(&mut form.step, Step::Form)
        else {
            return;
        };
        match result {
            Ok((id, boot)) => {
                form.step = Step::Opening(
                    request,
                    LocalSpace {
                        request: id,
                        boot,
                        setup,
                        offer,
                    },
                );
                cx.notify();
            }
            Err(error) => self.fail_setup_space(request, offer, &error, cx),
        }
    }

    /// Back to the question, keeping the claim for a retry.
    fn fail_setup_space(
        &mut self,
        request: setup::Request,
        offer: Offer,
        error: &crate::Error,
        cx: &mut Context<Self>,
    ) {
        tracing::warn!(category = "device_setup", "Setup workspace failed");
        if let Some(form) = &mut self.menu.device_setup {
            form.step = Step::Confirm(request, offer);
        }
        self.set_device_error(Some(format!("Open a local workspace: {error}")));
        cx.notify();
    }

    /// Runs the setup command in the workspace the local daemon created, then
    /// shows it. The command is typed into the pane's shell: the endpoint API
    /// has no method that starts a command in a pane.
    pub(crate) fn poll_device_setup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.poll_saved_device(window, cx) {
            return;
        }
        self.create_setup_space(cx);
        let Some(Setup {
            step: Step::Opening(_, space),
            ..
        }) = &self.menu.device_setup
        else {
            return;
        };
        let local = &self.endpoints[0];
        let fail = |this: &mut Self, error: String, cx: &mut Context<Self>| {
            tracing::warn!(category = "device_setup", "Setup workspace failed");
            if let Some(form) = &mut this.menu.device_setup
                && let Step::Opening(request, space) = &form.step
            {
                form.step = Step::Confirm(request.clone(), space.offer);
            }
            this.set_device_error(Some(format!("Open a local workspace: {error}")));
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
            let (code, message) = crate::menu::endpoint_error(error);
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
                        ClientPaneInputEvent::TextCommit(space.setup.command.clone()),
                        enter(),
                    ],
                )?)
            });
        if let Err(error) = sent {
            return fail(self, error.to_string(), cx);
        }
        // The setup outlives this dialog: its relay keeps waiting, and keeps
        // the host claimed until the setup ends, however it ends, so this
        // process cannot start a second add meanwhile, nor stay locked out.
        if let Some(form) = self.menu.device_setup.as_mut()
            && let Step::Opening(_, space) = &mut form.step
            && let Some(claim) = form.claim.take()
        {
            space.setup.relay.keep(claim);
        }
        let local = local.id.clone();
        self.dismiss_menu(window, cx);
        self.navigate_endpoint(&local, NavigationTarget::Workspace(&workspace), cx);
    }
}

/// The Enter key, which submits a line typed into a pane's shell.
pub(crate) fn enter() -> ClientPaneInputEvent {
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
