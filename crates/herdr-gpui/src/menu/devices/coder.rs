//! "Add Coder Workspace": sign in to the configured deployment, pick a
//! template and preset (or an existing workspace), wait for it, and, after
//! explicit approval, install Herdr there. Jobs run on a named worker thread;
//! the dialog drains their bounded mailbox from a foreground task, and closing
//! the dialog cancels the job and discards anything it still sends.

use super::super::Page;
use crate::{
    HerdrWindow,
    coder::{
        Preset, Progress, SavedWorkspace, Settings, Template, Workspace,
        setup::{self, Account, Ready, Source, Step},
    },
    search_input::SearchInput,
};
use gpui::{prelude::*, *};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

const MAILBOX: usize = 16;
const DRAIN: Duration = Duration::from_millis(100);
const SESSION: &str = "default";

enum Update {
    SignedIn(crate::coder::Result<bool>),
    Opened(String),
    Account(crate::coder::Result<Account>),
    Presets(String, crate::coder::Result<Vec<Preset>>),
    Step(Step),
    Provisioned(crate::coder::Result<(Ready, bool)>),
    Installed(crate::coder::Result<()>),
    Saved(crate::coder::Result<SavedWorkspace>),
    SignedOut(crate::coder::Result<()>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Checking,
    SignedOut,
    SigningIn,
    Loading,
    Choose,
    Working,
    NeedsInstall,
    Installing,
    Done,
}

/// What the user picked as the new device's source.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Choice {
    Template(String),
    Existing(String),
}

pub(in crate::menu) struct Wizard {
    settings: Settings,
    phase: Phase,
    account: Option<Account>,
    choice: Option<Choice>,
    /// Presets of the chosen template; `None` while loading.
    presets: Option<Vec<Preset>>,
    preset: Option<String>,
    name: Entity<SearchInput>,
    label: Entity<SearchInput>,
    ready: Option<Ready>,
    status: Option<String>,
    saved: Option<SavedWorkspace>,
    cancel: Arc<AtomicBool>,
    job: Option<Task<()>>,
}

impl Drop for Wizard {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}

fn step_text(step: &Step) -> String {
    match step {
        Step::Creating => "Creating the workspace…".into(),
        Step::Waiting(Progress::Starting) => "Starting the workspace…".into(),
        Step::Waiting(Progress::Building(status)) => format!(
            "Waiting for the workspace ({})…",
            format!("{status:?}").to_lowercase()
        ),
        Step::CheckingHerdr => "Checking for Herdr in the workspace…".into(),
    }
}

impl HerdrWindow {
    /// Whether the Coder row belongs in the device list at all.
    pub(super) fn coder_configured(&self) -> bool {
        self.config.coder.url.is_some() || std::env::var_os("HERDR_CODER_URL").is_some()
    }

    pub(super) fn open_coder_setup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = match self.config.coder.settings() {
            Ok(Some(settings)) => settings,
            Ok(None) => return,
            Err(error) => {
                self.menu.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let input = |placeholder: &str, cx: &mut Context<Self>| {
            let input = cx.new(SearchInput::new);
            input.update(cx, |input, cx| {
                input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
                input.set_placeholder(placeholder, cx);
            });
            input
        };
        let name = input("Workspace name", cx);
        let label = input("Device name (optional)", cx);
        window.focus(&self.menu.focus.clone(), cx);
        self.menu.error = None;
        self.menu.coder = Some(Wizard {
            settings,
            phase: Phase::Checking,
            account: None,
            choice: None,
            presets: None,
            preset: None,
            name,
            label,
            ready: None,
            status: None,
            saved: None,
            cancel: Arc::new(AtomicBool::new(false)),
            job: None,
        });
        self.menu.page = Some(Page::AddCoder);
        self.coder_job(cx, |settings, _, send| {
            send(Update::SignedIn(setup::signed_in(settings)));
        });
        cx.notify();
    }

    /// A signed-in dialog with fixed choices and no job, for headless layout tests.
    #[cfg(test)]
    pub(crate) fn open_coder_fixture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #![allow(clippy::unwrap_used)]
        let account = Account {
            user: serde_json::from_str(r#"{"id":"u1","username":"fixture-user"}"#).unwrap(),
            templates: serde_json::from_str(
                r#"[{"id":"t1","name":"docker","display_name":"Docker with a very long display name that must truncate","organization_name":"acme","active_version_id":"v1"}]"#,
            )
            .unwrap(),
            workspaces: serde_json::from_str(
                r#"[{"id":"w1","name":"herdr-box","template_name":"docker","latest_build":{"status":"running","transition":"start"}}]"#,
            )
            .unwrap(),
            saved: vec!["w1".into()],
        };
        let input = |placeholder: &str, cx: &mut Context<Self>| {
            let input = cx.new(SearchInput::new);
            input.update(cx, |input, cx| {
                input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
                input.set_placeholder(placeholder, cx);
            });
            input
        };
        let name = input("Workspace name", cx);
        let label = input("Device name (optional)", cx);
        self.menu.coder = Some(Wizard {
            settings: crate::coder::tests::settings(),
            phase: Phase::Choose,
            account: Some(account),
            choice: Some(Choice::Template("t1".into())),
            presets: Some(
                serde_json::from_str(r#"[{"ID":"p1","Name":"Large","Default":true}]"#).unwrap(),
            ),
            preset: Some("p1".into()),
            name,
            label,
            ready: None,
            status: None,
            saved: None,
            cancel: Arc::new(AtomicBool::new(false)),
            job: None,
        });
        self.menu.page = Some(Page::AddCoder);
        window.focus(&self.menu.focus.clone(), cx);
        cx.notify();
    }

    /// Run `work` on a worker thread with a fresh mailbox, replacing any job
    /// still running: the replaced job's cancellation flag is raised and its
    /// mailbox dropped, so nothing it sends can reach this dialog.
    fn coder_job(
        &mut self,
        cx: &mut Context<Self>,
        work: impl FnOnce(&Settings, &dyn Fn() -> bool, &dyn Fn(Update)) + Send + 'static,
    ) {
        let Some(wizard) = &mut self.menu.coder else {
            return;
        };
        wizard.cancel.store(true, Ordering::Release);
        let cancel = Arc::new(AtomicBool::new(false));
        wizard.cancel = cancel.clone();
        let settings = wizard.settings.clone();
        let (tx, rx) = mpsc::sync_channel(MAILBOX);
        let spawned = std::thread::Builder::new()
            .name("herdr-coder-setup".into())
            .spawn(move || {
                let cancelled = || cancel.load(Ordering::Acquire);
                // Progress is advisory and may be dropped; results block until
                // read or until the dialog goes away.
                let send = |update: Update| match update {
                    Update::Step(_) => {
                        let _ = tx.try_send(update);
                    }
                    update => {
                        let _ = tx.send(update);
                    }
                };
                work(&settings, &cancelled, &send);
            });
        if let Err(error) = spawned {
            tracing::error!(category = "coder_worker", error_kind = ?error.kind(), "Could not start Coder setup worker");
            self.menu.error = Some(crate::coder::Error::Worker("setup").to_string());
            return;
        }
        wizard.job = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(DRAIN).await;
                let mut updates = Vec::new();
                let closed = loop {
                    match rx.try_recv() {
                        Ok(update) => updates.push(update),
                        Err(mpsc::TryRecvError::Empty) => break false,
                        Err(mpsc::TryRecvError::Disconnected) => break true,
                    }
                };
                let alive = this.update(cx, |this, cx| {
                    for update in updates {
                        this.apply_coder(update, cx);
                    }
                    cx.notify();
                });
                if closed || alive.is_err() {
                    return;
                }
            }
        }));
    }

    fn apply_coder(&mut self, update: Update, cx: &mut Context<Self>) {
        let Some(wizard) = &mut self.menu.coder else {
            return;
        };
        let mut error = None;
        match update {
            Update::SignedIn(Ok(true)) => {
                wizard.phase = Phase::Loading;
                self.coder_job(cx, |settings, _, send| {
                    send(Update::Account(setup::account(settings)));
                });
                return;
            }
            Update::SignedIn(Ok(false)) => wizard.phase = Phase::SignedOut,
            Update::Opened(url) => {
                wizard.status = Some("Finish signing in in your browser…".into());
                cx.open_url(&url);
            }
            Update::Account(Ok(account)) => {
                wizard.phase = Phase::Choose;
                wizard.status = None;
                wizard.account = Some(account);
            }
            Update::Presets(template, result) => {
                if wizard.choice == Some(Choice::Template(template)) {
                    match result {
                        Ok(presets) => {
                            wizard.preset = presets.first().map(|preset| preset.id.clone());
                            wizard.presets = Some(presets);
                        }
                        Err(e) => {
                            wizard.presets = Some(Vec::new());
                            error = Some(e);
                        }
                    }
                }
            }
            Update::Step(step) => wizard.status = Some(step_text(&step)),
            Update::Provisioned(Ok((ready, installed))) => {
                wizard.ready = Some(ready);
                if installed {
                    self.save_coder(cx);
                    return;
                }
                wizard.phase = Phase::NeedsInstall;
                wizard.status = None;
            }
            Update::Installed(Ok(())) => {
                self.save_coder(cx);
                return;
            }
            Update::Saved(Ok(saved)) => {
                wizard.phase = Phase::Done;
                wizard.status = None;
                wizard.saved = Some(saved);
            }
            Update::SignedOut(result) => {
                wizard.phase = Phase::SignedOut;
                wizard.account = None;
                wizard.choice = None;
                wizard.status = None;
                error = result.err();
            }
            Update::SignedIn(Err(e)) | Update::Account(Err(e)) => {
                wizard.phase = Phase::SignedOut;
                wizard.status = None;
                error = Some(e);
            }
            Update::Provisioned(Err(e)) | Update::Saved(Err(e)) => {
                // Submitting again re-checks the workspace, so nothing is lost.
                wizard.phase = Phase::Choose;
                wizard.status = None;
                error = Some(e);
            }
            Update::Installed(Err(e)) => {
                wizard.phase = Phase::NeedsInstall;
                wizard.status = None;
                error = Some(e);
            }
        }
        if let Some(e) = error {
            self.menu.error = Some(e.to_string());
        }
    }

    fn save_coder(&mut self, cx: &mut Context<Self>) {
        let Some(wizard) = &mut self.menu.coder else {
            return;
        };
        let Some(ready) = wizard.ready.clone() else {
            return;
        };
        let label = match wizard.label.read(cx).text().trim() {
            "" => ready.name.clone(),
            label => label.to_owned(),
        };
        wizard.phase = Phase::Working;
        wizard.status = Some("Saving the device…".into());
        self.coder_job(cx, move |settings, _, send| {
            send(Update::Saved(setup::save(
                settings, &ready, &label, SESSION,
            )));
        });
    }

    fn coder_sign_in(&mut self, cx: &mut Context<Self>) {
        let Some(wizard) = &mut self.menu.coder else {
            return;
        };
        if wizard.phase != Phase::SignedOut {
            return;
        }
        wizard.phase = Phase::SigningIn;
        wizard.status = Some("Opening the Coder sign-in page…".into());
        self.menu.error = None;
        self.coder_job(cx, |settings, cancelled, send| {
            let result = setup::begin_sign_in(settings).and_then(|pending| {
                send(Update::Opened(pending.url.clone()));
                setup::finish_sign_in(settings, pending, cancelled)
            });
            send(Update::Account(result));
        });
    }

    fn coder_sign_out(&mut self, cx: &mut Context<Self>) {
        let Some(wizard) = &mut self.menu.coder else {
            return;
        };
        wizard.phase = Phase::Loading;
        self.menu.error = None;
        self.coder_job(cx, |settings, _, send| {
            send(Update::SignedOut(setup::sign_out(settings)));
        });
    }

    fn choose_coder_template(&mut self, template: Template, cx: &mut Context<Self>) {
        let Some(wizard) = &mut self.menu.coder else {
            return;
        };
        let prefix = wizard.settings.workspace_prefix.clone();
        wizard.choice = Some(Choice::Template(template.id.clone()));
        wizard.presets = None;
        wizard.preset = None;
        let suggestion = crate::coder::suggest_name(&prefix, &template.name);
        wizard
            .name
            .update(cx, |input, cx| input.set_text_selected(&suggestion, cx));
        self.menu.error = None;
        self.coder_job(cx, move |settings, _, send| {
            let id = template.id.clone();
            send(Update::Presets(id, setup::presets(settings, &template)));
        });
        cx.notify();
    }

    fn choose_coder_workspace(&mut self, workspace: &Workspace, cx: &mut Context<Self>) {
        let Some(wizard) = &mut self.menu.coder else {
            return;
        };
        wizard.choice = Some(Choice::Existing(workspace.id.clone()));
        wizard.presets = None;
        wizard.preset = None;
        self.menu.error = None;
        cx.notify();
    }

    fn submit_coder(&mut self, cx: &mut Context<Self>) {
        let Some(wizard) = &mut self.menu.coder else {
            return;
        };
        match wizard.phase {
            Phase::SignedOut => return self.coder_sign_in(cx),
            Phase::NeedsInstall => return self.install_coder(cx),
            Phase::Choose => {}
            _ => return,
        }
        let (Some(account), Some(choice)) = (&wizard.account, &wizard.choice) else {
            self.menu.error = Some("Choose a template or an existing workspace.".into());
            cx.notify();
            return;
        };
        if let Choice::Existing(id) = choice
            && account.saved.contains(id)
        {
            let id = id.clone();
            wizard.phase = Phase::Loading;
            wizard.choice = None;
            self.menu.error = None;
            self.coder_job(cx, move |settings, _, send| {
                send(Update::Account(setup::forget(settings, &id)));
            });
            cx.notify();
            return;
        }
        let source = match choice {
            Choice::Existing(id) => {
                let Some(workspace) = account.workspaces.iter().find(|w| &w.id == id) else {
                    return;
                };
                Source::Existing {
                    id: workspace.id.clone(),
                    name: workspace.name.clone(),
                }
            }
            Choice::Template(id) => {
                let Some(template) = account.templates.iter().find(|t| &t.id == id) else {
                    return;
                };
                let Some(presets) = &wizard.presets else {
                    return;
                };
                let name = wizard.name.read(cx).text().trim().to_owned();
                if !crate::coder::valid_name(&name) {
                    self.menu.error = Some(
                        "Workspace names use lowercase letters, digits, and single hyphens (at most 32)."
                            .into(),
                    );
                    cx.notify();
                    return;
                }
                Source::New {
                    name,
                    template: template.clone(),
                    preset: wizard
                        .preset
                        .as_ref()
                        .and_then(|id| presets.iter().find(|p| &p.id == id))
                        .cloned(),
                }
            }
        };
        wizard.phase = Phase::Working;
        wizard.ready = None;
        wizard.status = Some("Contacting Coder…".into());
        self.menu.error = None;
        self.coder_job(cx, move |settings, cancelled, send| {
            let result =
                setup::provision(settings, source, cancelled, |step| send(Update::Step(step)));
            send(Update::Provisioned(result));
        });
        cx.notify();
    }

    fn install_coder(&mut self, cx: &mut Context<Self>) {
        let Some(wizard) = &mut self.menu.coder else {
            return;
        };
        let Some(ready) = wizard.ready.clone() else {
            return;
        };
        wizard.phase = Phase::Installing;
        wizard.status = Some(format!("Installing Herdr in {}…", ready.name));
        self.menu.error = None;
        self.coder_job(cx, move |settings, cancelled, send| {
            send(Update::Installed(setup::install(
                settings, &ready, cancelled,
            )));
        });
        cx.notify();
    }

    fn coder_row(
        &self,
        id: impl Into<ElementId>,
        title: String,
        detail: String,
        checked: bool,
        cx: &mut Context<Self>,
        click: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        div()
            .id(id)
            .flex_none()
            .p(px(8.))
            .rounded(px(crate::config::corners::CONTROL))
            .cursor_pointer()
            .flex()
            .items_center()
            .gap(px(8.))
            .when(checked, |row| row.bg(rgb(theme.active)))
            .hover(|row| row.bg(rgb(theme.active)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(div().truncate().child(title))
                    .when(!detail.is_empty(), |column| {
                        column.child(
                            div()
                                .truncate()
                                .text_size(px(self.config.ui.size * 0.85))
                                .text_color(rgb(theme.muted))
                                .child(detail),
                        )
                    }),
            )
            .when(checked, |row| row.child(div().flex_none().child("✓")))
            .on_click(cx.listener(move |this, _, _, cx| click(this, cx)))
    }

    fn render_coder_choices(&self, wizard: &Wizard, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let heading = |text: &'static str| {
            div()
                .flex_none()
                .pt(px(4.))
                .text_color(rgb(theme.muted))
                .child(text)
        };
        let mut body = div().flex().flex_col().gap(px(4.));
        let Some(account) = &wizard.account else {
            return body;
        };
        body = body.child(heading("NEW WORKSPACE"));
        if account.templates.is_empty() {
            body = body.child(
                div()
                    .text_color(rgb(theme.muted))
                    .child("No templates are available to this account."),
            );
        }
        for (index, template) in account.templates.iter().enumerate() {
            let checked = wizard.choice.as_ref() == Some(&Choice::Template(template.id.clone()));
            let chosen = template.clone();
            body = body.child(self.coder_row(
                ("coder-template", index),
                template.label().to_owned(),
                template.organization_name.clone(),
                checked,
                cx,
                move |this, cx| this.choose_coder_template(chosen.clone(), cx),
            ));
            if checked {
                body = body.child(self.render_coder_template(wizard, cx));
            }
        }
        if !account.workspaces.is_empty() {
            body = body.child(heading("EXISTING WORKSPACE"));
        }
        for (index, workspace) in account.workspaces.iter().enumerate() {
            let checked = wizard.choice.as_ref() == Some(&Choice::Existing(workspace.id.clone()));
            let chosen = workspace.clone();
            let detail = if account.saved.contains(&workspace.id) {
                format!("{} · saved device", workspace.template_name)
            } else {
                workspace.template_name.clone()
            };
            body = body.child(self.coder_row(
                ("coder-workspace", index),
                workspace.name.clone(),
                detail,
                checked,
                cx,
                move |this, cx| this.choose_coder_workspace(&chosen, cx),
            ));
        }
        body
    }

    fn render_coder_template(&self, wizard: &Wizard, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let mut section = div()
            .flex_none()
            .ml(px(12.))
            .pl(px(8.))
            .border_l_1()
            .border_color(rgb(theme.active))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child("Workspace name")
                    .child(wizard.name.clone()),
            );
        match &wizard.presets {
            None => {
                section =
                    section.child(div().text_color(rgb(theme.muted)).child("Loading presets…"));
            }
            Some(presets) if presets.is_empty() => {}
            Some(presets) => {
                let mut list = div().flex().flex_col().gap(px(2.)).child("Preset");
                for (index, preset) in presets.iter().enumerate() {
                    let id = preset.id.clone();
                    list = list.child(self.coder_row(
                        ("coder-preset", index),
                        preset.name.clone(),
                        if preset.default {
                            "Default".into()
                        } else {
                            String::new()
                        },
                        wizard.preset.as_ref() == Some(&preset.id),
                        cx,
                        move |this, cx| {
                            if let Some(wizard) = &mut this.menu.coder {
                                wizard.preset = Some(id.clone());
                            }
                            cx.notify();
                        },
                    ));
                }
                section = section.child(list);
            }
        }
        section
    }

    pub(in crate::menu) fn render_add_coder(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let Some(wizard) = &self.menu.coder else {
            return div();
        };
        let signed_in = matches!(
            wizard.phase,
            Phase::Choose | Phase::Working | Phase::NeedsInstall | Phase::Installing | Phase::Done
        );
        let header = div()
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
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(self.config.ui.size * 1.35))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Add Coder Workspace"),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.config.ui.size * 0.85))
                            .text_color(rgb(theme.muted))
                            .child(match &wizard.account {
                                Some(account) => {
                                    format!("{} · {}", account.user.username, wizard.settings.base)
                                }
                                None => wizard.settings.base.clone(),
                            }),
                    ),
            )
            .when(signed_in && wizard.phase == Phase::Choose, |header| {
                header.child(
                    div()
                        .id("coder-sign-out")
                        .px_2()
                        .py_1()
                        .cursor_pointer()
                        .rounded(px(crate::config::corners::CONTROL))
                        .hover(|s| s.bg(rgb(theme.active)))
                        .child("Sign out")
                        .on_click(cx.listener(|this, _, _, cx| this.coder_sign_out(cx))),
                )
            })
            .child(
                div()
                    .id("coder-setup-close")
                    .px_2()
                    .py_1()
                    .cursor_pointer()
                    .rounded(px(crate::config::corners::CONTROL))
                    .hover(|s| s.bg(rgb(theme.active)))
                    .child("Close")
                    .on_click(cx.listener(|this, _, window, cx| this.dismiss_menu(window, cx))),
            );
        let mut body = div()
            .id("coder-setup-body")
            .min_h_0()
            .overflow_y_scroll()
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(12.));
        let text = |text: String| div().flex_none().text_color(rgb(theme.muted)).child(text);
        body = match wizard.phase {
            Phase::Checking | Phase::Loading => body.child(text("Checking your Coder account…".into())),
            Phase::SignedOut => body.child(text(
                "Sign in with your Coder account to create workspaces and add them as devices."
                    .into(),
            )),
            Phase::SigningIn => body,
            Phase::Choose => body
                .child(self.render_coder_choices(wizard, cx))
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child("Label")
                        .child(wizard.label.clone()),
                ),
            Phase::Working | Phase::Installing => body,
            Phase::NeedsInstall => body.child(text(format!(
                "Herdr is not installed in {}. Herdr can run its official installer there (curl -fsSL https://herdr.dev/install.sh | sh), which verifies the release checksum and installs to ~/.local/bin.",
                wizard.ready.as_ref().map_or("the workspace", |ready| ready.name.as_str())
            ))),
            Phase::Done => body.child(text(format!(
                "Added {}. It appears in the device picker and connects automatically.",
                wizard.saved.as_ref().map_or("the workspace", |saved| saved.label.as_str())
            ))),
        };
        if let Some(status) = &wizard.status {
            body = body.child(div().flex_none().child(status.clone()));
        }
        if let Some(error) = &self.menu.error {
            body = body.child(
                div()
                    .flex_none()
                    .text_color(super::super::danger(theme))
                    .child(error.clone()),
            );
        }
        let (label, ready) = match wizard.phase {
            Phase::SignedOut => ("Sign in with Coder", true),
            Phase::SigningIn => ("Waiting for browser…", false),
            Phase::Choose => (
                match &wizard.choice {
                    Some(Choice::Existing(id))
                        if wizard
                            .account
                            .as_ref()
                            .is_some_and(|account| account.saved.contains(id)) =>
                    {
                        "Remove device"
                    }
                    Some(Choice::Existing(_)) => "Add workspace",
                    _ => "Create workspace",
                },
                wizard.choice.is_some()
                    && !matches!(wizard.choice, Some(Choice::Template(_)) if wizard.presets.is_none()),
            ),
            Phase::NeedsInstall => ("Install Herdr", true),
            Phase::Installing => ("Installing…", false),
            Phase::Checking | Phase::Loading | Phase::Working => ("Working…", false),
            Phase::Done => ("Done", false),
        };
        div()
            .debug_selector(|| "coder-setup-dialog".into())
            .flex()
            .flex_col()
            .min_h_0()
            .child(header)
            .child(body)
            .child(
                div()
                    .flex_none()
                    .p(px(16.))
                    .border_t_1()
                    .border_color(rgb(theme.active))
                    .flex()
                    .justify_end()
                    .child(
                        div()
                            .id("coder-setup-submit")
                            .debug_selector(|| "coder-setup-submit".into())
                            .p(px(8.))
                            .rounded(px(crate::config::corners::CONTROL))
                            .bg(rgb(theme.active))
                            .when(ready, |button| {
                                button.cursor_pointer().hover(|s| {
                                    s.bg(rgb(theme.active)
                                        .blend(rgba((theme.foreground << 8) | 0x20)))
                                })
                            })
                            .when(!ready, |button| button.text_color(rgb(theme.muted)))
                            .child(label)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if this
                                    .menu
                                    .coder
                                    .as_ref()
                                    .is_some_and(|w| w.phase == Phase::Done)
                                {
                                    this.dismiss_menu(window, cx);
                                } else if ready {
                                    this.submit_coder(cx);
                                }
                            })),
                    ),
            )
    }

    pub(in crate::menu) fn coder_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(wizard) = &self.menu.coder else {
            return false;
        };
        if [&wizard.name, &wizard.label]
            .iter()
            .any(|field| field.read(cx).is_composing())
        {
            return false;
        }
        match event.keystroke.key.as_str() {
            "tab" if wizard.phase == Phase::Choose => {
                let next = if wizard.name.read(cx).focus.is_focused(window) {
                    wizard.label.read(cx).focus.clone()
                } else {
                    wizard.name.read(cx).focus.clone()
                };
                window.focus(&next, cx);
            }
            "enter" => self.submit_coder(cx),
            "escape" => self.dismiss_menu(window, cx),
            _ => return false,
        }
        true
    }
}
