//! Add Device's Coder form: sign in to the configured deployment and pick a
//! template and preset (or an existing workspace). Creating hands the work to
//! a window-level job (see `cloud::Jobs`) and closes the dialog at once, so a
//! slow build never blocks the next one. The dialog's own jobs (sign-in,
//! listing, presets, removal) run through `cloud::worker` and stop with it.

use super::super::Page;
use crate::i18n::fl;
use crate::{
    HerdrWindow,
    cloud::{CloudProvider, worker},
    coder::{
        Preset, Settings, Template, Workspace,
        setup::{self, Account, AddRequest, Source},
    },
    search_input::SearchInput,
};
use gpui::{prelude::*, *};

enum Update {
    SignedIn(crate::coder::Result<bool>),
    Opened(String),
    Account(crate::coder::Result<Account>),
    Presets(String, crate::coder::Result<Vec<Preset>>),
    SignedOut(crate::coder::Result<()>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Checking,
    SignedOut,
    SigningIn,
    Loading,
    Choose,
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
    /// Run Herdr's installer when the workspace lacks it. On by default: the
    /// switch is the user's approval, given before the job starts.
    install: bool,
    /// Run the configured setup script once the device connects. Off for an
    /// existing workspace, which may well be set up already.
    setup: bool,
    status: Option<String>,
    /// Dropping the job cancels it, so closing the dialog stops its work.
    job: Option<worker::Worker>,
    /// Why the last step failed, kept with this form while another source
    /// of Add Device shows.
    error: Option<String>,
}

impl HerdrWindow {
    /// Set the Coder form's error. Before the form exists, the Settings
    /// pane that stands in for it shows the shared one.
    fn set_coder_error(&mut self, error: Option<String>) {
        match &mut self.menu.coder {
            Some(wizard) => wizard.error = error,
            None => self.menu.error = error,
        }
    }

    fn coder_input(
        &self,
        placeholder: fn() -> String,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Entity<SearchInput> {
        let input = cx.new(SearchInput::new);
        input.update(cx, |input, cx| {
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
            input.set_placeholder_with(placeholder, cx);
            if !text.is_empty() {
                input.set_text_selected(text, cx);
            }
        });
        input
    }

    fn new_wizard(&self, settings: Settings, phase: Phase, cx: &mut Context<Self>) -> Wizard {
        let name = crate::coder::random_name(&settings.workspace_prefix);
        Wizard {
            phase,
            account: None,
            choice: None,
            presets: None,
            preset: None,
            name: self.coder_input(|| fl!("devices-coder-name-placeholder"), &name, cx),
            label: self.coder_input(|| fl!("devices-coder-label-placeholder"), "", cx),
            install: true,
            setup: true,
            status: None,
            job: None,
            error: None,
            settings,
        }
    }

    pub(super) fn open_coder_setup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if crate::cloud::unavailable().is_some() {
            return;
        }
        let settings = match self.config.coder.settings() {
            Ok(Some(settings)) => settings,
            Ok(None) => return,
            Err(error) => {
                // No form yet: Coder's Settings pane shows why.
                self.menu.coder = None;
                self.menu.page = Some(Page::AddCloud(CloudProvider::Coder));
                self.set_coder_error(Some(error.to_string()));
                cx.notify();
                return;
            }
        };
        window.focus(&self.menu.focus.clone(), cx);
        self.set_coder_error(None);
        self.menu.coder = Some(self.new_wizard(settings, Phase::Checking, cx));
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
                // i18n: data
                r#"[{"id":"t1","name":"docker","display_name":"Docker with a very long display name that must truncate","organization_name":"acme","active_version_id":"v1"}]"#,
            )
            .unwrap(),
            workspaces: serde_json::from_str(
                r#"[{"id":"w1","name":"herdr-box","template_name":"docker","latest_build":{"status":"running","transition":"start"}}]"#,
            )
            .unwrap(),
            saved: vec!["w1".into()],
        };
        let mut wizard = self.new_wizard(crate::coder::test_support::settings(), Phase::Choose, cx);
        wizard.account = Some(account);
        wizard.choice = Some(Choice::Template("t1".into()));
        wizard.presets =
            Some(serde_json::from_str(r#"[{"ID":"p1","Name":"Large","Default":true}]"#).unwrap());
        wizard.preset = Some("p1".into());
        self.menu.coder = Some(wizard);
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
        // Dropping the old worker cancels it and discards its mailbox.
        wizard.job = None;
        let settings = wizard.settings.clone();
        let spawned = worker::spawn(
            "herdr-coder-setup",
            cx,
            move |cancelled, send| work(&settings, cancelled, send),
            |this: &mut Self, update, cx| this.apply_coder(update, cx),
        );
        match spawned {
            Ok(job) => {
                if let Some(wizard) = &mut self.menu.coder {
                    wizard.job = Some(job);
                }
            }
            Err(error) => {
                tracing::error!(category = "coder_worker", error_kind = ?error.kind(), "Could not start Coder setup worker");
                self.set_coder_error(Some(crate::coder::Error::Worker("setup").to_string()));
            }
        }
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
                wizard.status = Some(fl!("devices-coder-finish-sign-in"));
                cx.open_url(&url);
            }
            Update::Account(Ok(account)) => {
                wizard.phase = Phase::Choose;
                wizard.status = None;
                // Preselect the first template, so creating needs one click.
                let first = account.templates.first().cloned();
                wizard.account = Some(account);
                if let Some(template) = first.filter(|_| wizard.choice.is_none()) {
                    self.choose_coder_template(template, cx);
                    return;
                }
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
        }
        if let Some(e) = error {
            self.set_coder_error(Some(e.to_string()));
        }
    }

    fn coder_sign_in(&mut self, cx: &mut Context<Self>) {
        let Some(wizard) = &mut self.menu.coder else {
            return;
        };
        if wizard.phase != Phase::SignedOut {
            return;
        }
        wizard.phase = Phase::SigningIn;
        wizard.status = Some(fl!("devices-coder-opening-sign-in"));
        self.set_coder_error(None);
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
        self.set_coder_error(None);
        self.coder_job(cx, |settings, _, send| {
            send(Update::SignedOut(setup::sign_out(settings)));
        });
    }

    fn choose_coder_template(&mut self, template: Template, cx: &mut Context<Self>) {
        let Some(wizard) = &mut self.menu.coder else {
            return;
        };
        wizard.choice = Some(Choice::Template(template.id.clone()));
        wizard.presets = None;
        wizard.preset = None;
        wizard.setup = true;
        self.set_coder_error(None);
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
        wizard.setup = false;
        self.set_coder_error(None);
        cx.notify();
    }

    fn submit_coder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(wizard) = &mut self.menu.coder else {
            return;
        };
        match wizard.phase {
            Phase::SignedOut => return self.coder_sign_in(cx),
            Phase::Choose => {}
            _ => return,
        }
        let (Some(account), Some(choice)) = (&wizard.account, &wizard.choice) else {
            self.set_coder_error(Some(fl!("devices-coder-choose")));
            cx.notify();
            return;
        };
        if let Choice::Existing(id) = choice
            && account.saved.contains(id)
        {
            let id = id.clone();
            wizard.phase = Phase::Loading;
            wizard.choice = None;
            self.set_coder_error(None);
            self.coder_job(cx, move |settings, _, send| {
                send(Update::Account(setup::forget(settings, &id)));
            });
            cx.notify();
            return;
        }
        let (name, source) = match choice {
            Choice::Existing(id) => {
                let Some(workspace) = account.workspaces.iter().find(|w| &w.id == id) else {
                    return;
                };
                (
                    workspace.name.clone(),
                    Source::Existing {
                        id: workspace.id.clone(),
                        name: workspace.name.clone(),
                    },
                )
            }
            Choice::Template(id) => {
                let Some(template) = account.templates.iter().find(|t| &t.id == id) else {
                    return;
                };
                let Some(presets) = &wizard.presets else {
                    return;
                };
                let name = wizard.name.read(cx).text().trim().to_owned();
                let problem = if !crate::coder::valid_name(&name) {
                    Some(fl!("devices-coder-invalid-name"))
                } else if account.workspaces.iter().any(|w| w.name == name) {
                    Some(fl!("devices-coder-name-taken"))
                } else {
                    None
                };
                if let Some(problem) = problem {
                    self.set_coder_error(Some(problem));
                    cx.notify();
                    return;
                }
                (
                    name.clone(),
                    Source::New {
                        name,
                        template: template.clone(),
                        preset: wizard
                            .preset
                            .as_ref()
                            .and_then(|id| presets.iter().find(|p| &p.id == id))
                            .cloned(),
                    },
                )
            }
        };
        if self.cloud_jobs.contains(CloudProvider::Coder, &name) {
            self.set_coder_error(Some(fl!(
                "devices-coder-already-adding",
                name = name.as_str()
            )));
            cx.notify();
            return;
        }
        let request = AddRequest {
            settings: wizard.settings.clone(),
            source,
            label: wizard.label.read(cx).text().trim().to_owned(),
            install: wizard.install,
        };
        let script = wizard
            .setup
            .then(|| {
                self.config
                    .cloud
                    .script(CloudProvider::Coder)
                    .ok()
                    .flatten()
            })
            .flatten();
        let work = move |cancelled: &dyn Fn() -> bool, report: &dyn Fn(crate::cloud::Step)| {
            setup::add_device(request, cancelled, report)
        };
        match self.start_cloud_job(CloudProvider::Coder, name, script, work, cx) {
            // The footer's count and the picker's Adding list show progress.
            Ok(()) => self.dismiss_menu(window, cx),
            Err(error) => {
                self.set_coder_error(Some(error.to_string()));
                cx.notify();
            }
        }
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
                        // Wraps: the install row's detail is a sentence.
                        column.child(
                            div()
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
        let heading = |text: String| {
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
        body = body.child(heading(fl!("devices-coder-new-workspace")));
        if account.templates.is_empty() {
            body = body.child(
                div()
                    .text_color(rgb(theme.muted))
                    .child(fl!("devices-coder-no-templates")),
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
            body = body.child(heading(fl!("devices-coder-existing-workspace")));
        }
        for (index, workspace) in account.workspaces.iter().enumerate() {
            let checked = wizard.choice.as_ref() == Some(&Choice::Existing(workspace.id.clone()));
            let chosen = workspace.clone();
            let detail = if account.saved.contains(&workspace.id) {
                fl!(
                    "devices-coder-saved",
                    template = workspace.template_name.as_str()
                )
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
                    .child(fl!("devices-coder-name"))
                    .child(wizard.name.clone()),
            );
        match &wizard.presets {
            None => {
                section = section.child(
                    div()
                        .text_color(rgb(theme.muted))
                        .child(fl!("devices-coder-loading-presets")),
                );
            }
            Some(presets) if presets.is_empty() => {}
            Some(presets) => {
                let mut list = div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(fl!("devices-coder-preset"));
                for (index, preset) in presets.iter().enumerate() {
                    let id = preset.id.clone();
                    list = list.child(self.coder_row(
                        ("coder-preset", index),
                        preset.name.clone(),
                        if preset.default {
                            fl!("devices-coder-default")
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

    pub(in crate::menu) fn render_add_coder(
        &self,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = &self.theme;
        let Some(wizard) = &self.menu.coder else {
            return self.add_device_frame(
                "coder-setup-dialog",
                "coder-setup-close",
                compact,
                div(),
                cx,
            );
        };
        // The deployment, and the account once signed in, head the pane now
        // that the dialog's title is Add Device's own.
        let account = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(theme.muted))
                    .child(match &wizard.account {
                        Some(account) => {
                            format!("{} · {}", account.user.username, wizard.settings.base)
                        }
                        None => wizard.settings.base.clone(),
                    }),
            )
            .when(wizard.phase == Phase::Choose, |header| {
                header.child(
                    div()
                        .id("coder-sign-out")
                        .flex_none()
                        .px_2()
                        .py_1()
                        .cursor_pointer()
                        .rounded(px(crate::config::corners::CONTROL))
                        .hover(|s| s.bg(rgb(theme.active)))
                        .child(fl!("devices-coder-sign-out"))
                        .on_click(cx.listener(|this, _, _, cx| this.coder_sign_out(cx))),
                )
            });
        let mut body = div()
            .id("coder-setup-body")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(account);
        let removing = matches!(
            (&wizard.choice, &wizard.account),
            (Some(Choice::Existing(id)), Some(account)) if account.saved.contains(id)
        );
        let text = |text: String| div().flex_none().text_color(rgb(theme.muted)).child(text);
        let has_script = matches!(self.config.cloud.script(CloudProvider::Coder), Ok(Some(_)));
        body = match wizard.phase {
            Phase::Checking | Phase::Loading => body.child(text(fl!("devices-coder-checking"))),
            Phase::SignedOut => body.child(text(fl!("devices-coder-sign-in-intro"))),
            Phase::SigningIn => body,
            // Removing a saved device names and installs nothing.
            Phase::Choose => {
                body.child(self.render_coder_choices(wizard, cx))
                    .when(!removing, |body| {
                        body.child(
                            div()
                                .flex_none()
                                .flex()
                                .flex_col()
                                .gap(px(6.))
                                .child(fl!("devices-coder-label"))
                                .child(wizard.label.clone()),
                        )
                        .child(self.coder_row(
                            "coder-install",
                            fl!("devices-coder-install"),
                            fl!("devices-coder-install-detail"),
                            wizard.install,
                            cx,
                            |this, cx| {
                                if let Some(wizard) = &mut this.menu.coder {
                                    wizard.install = !wizard.install;
                                }
                                cx.notify();
                            },
                        ))
                        .when(has_script, |body| {
                            body.child(self.coder_row(
                                "coder-run-setup",
                                fl!("devices-coder-run-setup"),
                                crate::cloud_setup::summary(
                                    &self.config.cloud,
                                    CloudProvider::Coder,
                                ),
                                wizard.setup,
                                cx,
                                |this, cx| {
                                    if let Some(wizard) = &mut this.menu.coder {
                                        wizard.setup = !wizard.setup;
                                    }
                                    cx.notify();
                                },
                            ))
                        })
                    })
            }
        };
        if let Some(status) = &wizard.status {
            body = body.child(div().flex_none().child(status.clone()));
        }
        if let Some(error) = &wizard.error {
            body = body.child(
                div()
                    .flex_none()
                    .text_color(super::super::danger(theme))
                    .child(error.clone()),
            );
        }
        let (label, ready) = match wizard.phase {
            Phase::SignedOut => (fl!("devices-coder-sign-in"), true),
            Phase::SigningIn => (fl!("devices-coder-waiting"), false),
            Phase::Choose => (
                match &wizard.choice {
                    _ if removing => fl!("devices-coder-remove"),
                    Some(Choice::Existing(_)) => fl!("devices-coder-add"),
                    _ => fl!("devices-coder-create"),
                },
                wizard.choice.is_some()
                    && !matches!(wizard.choice, Some(Choice::Template(_)) if wizard.presets.is_none()),
            ),
            Phase::Checking | Phase::Loading => (fl!("devices-coder-working"), false),
        };
        let pane = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
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
                                if ready {
                                    this.submit_coder(window, cx);
                                }
                            })),
                    ),
            );
        self.add_device_frame("coder-setup-dialog", "coder-setup-close", compact, pane, cx)
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
            "enter" => self.submit_coder(window, cx),
            "escape" => self.dismiss_menu(window, cx),
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests;
