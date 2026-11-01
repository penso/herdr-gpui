//! The app's own confirmations for what changes a host: approving a script,
//! which shows exactly the plan and content hash that will be approved, and
//! enabling or updating `herdr-automate`, which says what is copied where
//! before anything is. `herdr-gpui automation approve` only opens the first.

use super::{Action, Error, Machine, fetch, install};
use crate::{
    HerdrWindow,
    i18n::{Localize, fl},
    menu::Page,
    window::Flash,
};
use gpui::{prelude::*, *};
use herdr_automation::report::{ScriptCaps, ScriptState};
use std::sync::Arc;

/// The question open in the menu popup.
pub(crate) enum Dialog {
    Approve(Approve),
    Enable(Enable),
}

/// One script's approval, as the dialog shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Approve {
    pub(crate) machine: Machine,
    pub(crate) host: String,
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) hash: String,
    pub(crate) plan: Option<String>,
    pub(crate) caps: Option<ScriptCaps>,
}

/// Enabling or updating `herdr-automate` on a machine.
pub(crate) struct Enable {
    pub(crate) machine: Machine,
    pub(crate) host: String,
    pub(crate) update: bool,
    pub(crate) stage: Stage,
}

pub(crate) enum Stage {
    /// Finding the host's platform and this app's build for it.
    Preparing {
        _task: Task<()>,
    },
    Ready(install::Plan),
    Installing {
        _task: Task<()>,
    },
    /// Installed; what the host said.
    Done(String),
    Failed(Arc<Error>),
}

impl HerdrWindow {
    /// Opens the approval of `path` on `machine`, if the host lists it.
    pub(crate) fn review_automation(
        &mut self,
        machine: &Machine,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = self.ask_to_approve_automation(machine, path, window, cx) {
            let label = self
                .automations
                .host(machine)
                .map_or_else(String::new, |host| host.label.clone());
            self.automation_failed(&label, &error, cx);
        }
    }

    /// Opens the approval of `path` on `machine`; whether the dialog opened,
    /// which it does not while another page holds the window.
    pub(crate) fn ask_to_approve_automation(
        &mut self,
        machine: &Machine,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, Error> {
        let approve = self.automation_approval(machine, path)?;
        Ok(self.open_automation_dialog(Dialog::Approve(approve), window, cx))
    }

    /// What approving `path` on `machine` would approve, from the last read.
    pub(crate) fn automation_approval(
        &self,
        machine: &Machine,
        path: &str,
    ) -> Result<Approve, Error> {
        let host = self
            .automations
            .host(machine)
            .ok_or_else(|| Error::UnknownHost(machine.name()))?;
        let status = host.status().ok_or_else(|| Error::NotRead {
            host: host.label.clone(),
        })?;
        let script = status
            .scripts
            .iter()
            .find(|script| script.path == path)
            .ok_or_else(|| Error::NoScript {
                path: path.to_owned(),
                host: host.label.clone(),
            })?;
        if script.state != ScriptState::AwaitingApproval {
            return Err(Error::Approved {
                path: path.to_owned(),
                host: host.label.clone(),
            });
        }
        Ok(Approve {
            machine: machine.clone(),
            host: host.label.clone(),
            path: script.path.clone(),
            name: script.name.clone(),
            hash: script.hash.clone(),
            plan: script.plan.clone(),
            caps: script.caps,
        })
    }

    /// Opens enabling (or updating) `herdr-automate` on `machine`, and
    /// finds out what it would copy.
    pub(crate) fn ask_to_enable_automations(
        &mut self,
        machine: &Machine,
        update: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.automations.host(machine) else {
            return;
        };
        let script_host = host.host.clone();
        let label = host.label.clone();
        let shipped = self.shipped_automate_version().map(str::to_owned);
        let cancelled = self.automations.cancelled();
        let work = cx
            .background_executor()
            .spawn(async move { install::prepare(&script_host, shipped.as_deref(), &cancelled) });
        let task = cx.spawn(async move |this, cx| {
            let plan = work.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(Dialog::Enable(enable)) = &mut this.automations.dialog
                    && matches!(enable.stage, Stage::Preparing { .. })
                {
                    enable.stage = match plan {
                        Ok(plan) => Stage::Ready(plan),
                        Err(error) => Stage::Failed(Arc::new(error)),
                    };
                    cx.notify();
                }
            });
        });
        let dialog = Dialog::Enable(Enable {
            machine: machine.clone(),
            host: label,
            update,
            stage: Stage::Preparing { _task: task },
        });
        self.open_automation_dialog(dialog, window, cx);
    }

    /// Shows `dialog` in the menu popup, unless another page holds it.
    pub(crate) fn open_automation_dialog(
        &mut self,
        dialog: Dialog,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.menu.page.is_some() || !self.open_menu(window, cx) {
            return false;
        }
        self.automations.dialog = Some(dialog);
        self.menu.page = Some(Page::Automation);
        cx.notify();
        true
    }

    /// Whether an automation question holds the menu popup.
    pub(crate) fn automation_dialog_open(&self) -> bool {
        self.menu.page == Some(Page::Automation) && self.automations.dialog.is_some()
    }

    /// Drops a question whose popup closed unanswered.
    pub(super) fn forget_closed_automation_dialog(&mut self) {
        if self.menu.page != Some(Page::Automation) {
            self.automations.dialog = None;
        }
    }

    fn confirm_automation_approval(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::Approve(approve)) = self.automations.dialog.take() else {
            return;
        };
        if approve.plan.is_none() {
            return;
        }
        self.dismiss_menu(window, cx);
        self.automation_action(
            &approve.machine,
            Action::Approve {
                path: approve.path,
                hash: approve.hash,
            },
            cx,
        );
    }

    fn confirm_automation_enable(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::Enable(enable)) = &self.automations.dialog else {
            return;
        };
        let Stage::Ready(plan) = &enable.stage else {
            return;
        };
        let Some(host) = self.automations.host(&enable.machine) else {
            return;
        };
        let (plan, update, machine) = (plan.clone(), enable.update, enable.machine.clone());
        let script_host = host.host.clone();
        let label = host.label.clone();
        let cancelled = self.automations.cancelled();
        let work = cx
            .background_executor()
            .spawn(async move { install::install(&script_host, &plan, update, &cancelled) });
        let task = cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                let stage = match result {
                    Ok(message) => {
                        let done = if message.trim().is_empty() {
                            fl!("automations-enabled", host = label.as_str())
                        } else {
                            fl!(
                                "automations-enabled-said",
                                host = label.as_str(),
                                message = message.as_str()
                            )
                        };
                        this.show_flash(Flash::success(done.clone()), cx);
                        Stage::Done(done)
                    }
                    Err(error) => Stage::Failed(Arc::new(error)),
                };
                if let Some(Dialog::Enable(enable)) = &mut this.automations.dialog
                    && enable.machine == machine
                {
                    enable.stage = stage;
                }
                this.automations.refresh();
                this.poll_automations(cx);
                cx.notify();
            });
        });
        if let Some(Dialog::Enable(enable)) = &mut self.automations.dialog {
            enable.stage = Stage::Installing { _task: task };
        }
        cx.notify();
    }

    /// The open automation question's body, for the menu popup.
    pub(crate) fn automation_dialog_page(&self, cx: &mut Context<Self>) -> Option<Div> {
        if self.menu.page != Some(Page::Automation) {
            return None;
        }
        Some(match self.automations.dialog.as_ref()? {
            Dialog::Approve(approve) => self.render_automation_approval(approve, cx),
            Dialog::Enable(enable) => self.render_automation_enable(enable, cx),
        })
    }

    fn render_automation_approval(&self, approve: &Approve, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let mut facts = vec![
            fl!("automations-approve-host", host = approve.host.as_str()),
            fl!("automations-approve-file", path = approve.path.as_str()),
            fl!("automations-approve-content", hash = approve.hash.as_str()),
        ];
        if let Some(caps) = approve.caps {
            facts.push(super::model::caps(caps));
        }
        let plan = match &approve.plan {
            Some(plan) => self.automation_plan(plan, "automation-approve-plan"),
            None => div()
                .debug_selector(|| "automation-approve-no-plan".into())
                .text_color(rgb(theme.ink(theme.palette[3])))
                .child(fl!("automations-approve-no-plan")),
        };
        // Only what the user can read in full is offered for approval.
        let confirm = approve
            .plan
            .is_some()
            .then(|| fl!("automations-approve-confirm", host = approve.host.as_str()));
        self.automation_dialog_frame(
            "automation-approve",
            fl!(
                "automations-approve-title",
                name = approve.name.as_str(),
                host = approve.host.as_str()
            ),
            fl!("automations-approve-note"),
        )
        .children(facts.into_iter().map(|fact| {
            // Wrapped, never cut: the hash and path are what is approved.
            div().min_w_0().text_color(rgb(theme.subtext())).child(fact)
        }))
        .child(
            div()
                .id("automation-approve-plan-scroll")
                .max_h(px(360.))
                .overflow_y_scroll()
                .child(plan),
        )
        .child(self.automation_dialog_buttons(
            "automation-approve",
            confirm,
            cx.listener(|this, _, window, cx| {
                cx.stop_propagation();
                this.confirm_automation_approval(window, cx);
            }),
            cx,
        ))
    }

    fn render_automation_enable(&self, enable: &Enable, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let host = enable.host.as_str();
        let title = if enable.update {
            fl!("automations-update-title", host = host)
        } else {
            fl!("automations-enable-title", host = host)
        };
        let mut frame = self.automation_dialog_frame(
            "automation-enable",
            title,
            fl!("automations-enable-note"),
        );
        let mut confirm = None;
        match &enable.stage {
            Stage::Preparing { .. } => {
                frame = frame.child(
                    div()
                        .text_color(rgb(theme.muted))
                        .child(fl!("automations-enable-checking")),
                );
            }
            Stage::Installing { .. } => {
                frame = frame.child(
                    div()
                        .text_color(rgb(theme.muted))
                        .child(fl!("automations-enable-installing")),
                );
            }
            Stage::Done(message) => {
                frame = frame.child(
                    div()
                        .debug_selector(|| "automation-enable-done".into())
                        .text_color(rgb(theme.ink(theme.palette[2])))
                        .child(message.clone()),
                );
            }
            Stage::Failed(error) => {
                frame = frame.child(
                    div()
                        .debug_selector(|| "automation-enable-error".into())
                        .text_color(rgb(theme.ink(theme.palette[3])))
                        .child(error.localize()),
                );
            }
            Stage::Ready(plan) => {
                let short: String = plan.sha256.chars().take(12).collect();
                let lines = [
                    fl!(
                        "automations-enable-copies",
                        version = plan.version.as_str(),
                        platform = plan.platform.to_string(),
                        size = super::format::bytes(plan.size),
                        hash = short
                    ),
                    fl!(
                        "automations-enable-into",
                        version = plan.version.as_str(),
                        program = fetch::PROGRAM
                    ),
                    if enable.update {
                        fl!("automations-enable-then-update")
                    } else {
                        fl!("automations-enable-then-install")
                    },
                ];
                frame = frame.children(
                    lines
                        .into_iter()
                        .map(|line| div().text_color(rgb(theme.subtext())).child(line)),
                );
                confirm = Some(if enable.update {
                    fl!("automations-update-confirm", host = host)
                } else {
                    fl!("automations-enable-confirm", host = host)
                });
            }
        }
        frame.child(self.automation_dialog_buttons(
            "automation-enable",
            confirm,
            cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.confirm_automation_enable(cx);
            }),
            cx,
        ))
    }

    fn automation_dialog_frame(&self, selector: &'static str, title: String, note: String) -> Div {
        div()
            .debug_selector(move || selector.into())
            .flex()
            .flex_col()
            .min_h_0()
            .p(px(8.))
            .gap(px(10.))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
            .child(div().text_color(rgb(self.theme.subtext())).child(note))
    }

    fn automation_dialog_buttons(
        &self,
        prefix: &'static str,
        confirm: Option<String>,
        on_confirm: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        div()
            .flex()
            .justify_end()
            .gap(px(8.))
            .child(
                div()
                    .id(SharedString::from(format!("{prefix}-cancel")))
                    .debug_selector(move || format!("{prefix}-cancel"))
                    .p(px(6.))
                    .cursor_pointer()
                    .child(if confirm.is_some() {
                        fl!("automations-cancel")
                    } else {
                        fl!("automations-close")
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.automations.dialog = None;
                        this.dismiss_menu(window, cx);
                    })),
            )
            .children(confirm.map(|label| {
                div()
                    .id(SharedString::from(format!("{prefix}-submit")))
                    .debug_selector(move || format!("{prefix}-submit"))
                    .p(px(6.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .bg(rgb(theme.active))
                    .text_color(rgb(theme.primary()))
                    .cursor_pointer()
                    .child(label)
                    .on_click(on_confirm)
            }))
    }
}
