//! What a click asks of a host: run a script now, pause or resume it or the
//! whole host, stop a run, start the daemon, approve a script. Each runs
//! `herdr-automate` on the background executor; the host is read again
//! once it answers, and a refusal is shown in the window's language, with
//! what the program said kept as it worded it.

use super::{Error, HostState, Machine, fetch};
use crate::{
    HerdrWindow,
    i18n::{Localize, fl},
    window::Flash,
};
use gpui::{prelude::*, *};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    /// Approve exactly the content whose hash the dialog showed.
    Approve {
        path: String,
        hash: String,
    },
    Pause {
        path: String,
    },
    PauseAll,
    Resume {
        path: String,
    },
    ResumeAll,
    RunNow {
        path: String,
    },
    Start,
    Stop {
        run: i64,
    },
}

impl Action {
    /// The command line after `herdr-automate`, before `--json`.
    pub(crate) fn args(&self) -> Vec<String> {
        let words: Vec<&str> = match self {
            Self::Approve { path, hash } => vec!["approve", path, "--hash", hash],
            Self::Pause { path } => vec!["pause", path],
            Self::PauseAll => vec!["pause", "--all"],
            Self::Resume { path } => vec!["resume", path],
            Self::ResumeAll => vec!["resume", "--all"],
            Self::RunNow { path } => vec!["run-now", path],
            Self::Start => vec!["start"],
            Self::Stop { run } => return vec!["stop".to_owned(), run.to_string()],
        };
        words.into_iter().map(str::to_owned).collect()
    }

    /// What the button says while the host works on it.
    pub(crate) fn doing(&self) -> String {
        match self {
            Self::Approve { .. } => fl!("automations-doing-approve"),
            Self::Pause { .. } | Self::PauseAll => fl!("automations-doing-pause"),
            Self::Resume { .. } | Self::ResumeAll => fl!("automations-doing-resume"),
            Self::RunNow { .. } => fl!("automations-doing-run"),
            Self::Start => fl!("automations-doing-start"),
            Self::Stop { .. } => fl!("automations-doing-stop"),
        }
    }
}

impl HostState {
    /// Whether `action` is what the host is busy with.
    pub(crate) fn doing(&self, action: &Action) -> bool {
        self.busy.as_ref().is_some_and(|(busy, _)| busy == action)
    }
}

impl HerdrWindow {
    /// Asks `machine` to do `action`, unless it is busy with another.
    pub(crate) fn automation_action(
        &mut self,
        machine: &Machine,
        action: Action,
        cx: &mut Context<Self>,
    ) {
        let cancelled = self.automations.cancelled();
        let Some(host) = self.automations.host_mut(machine) else {
            return;
        };
        if host.busy.is_some() {
            return;
        }
        let script_host = host.host.clone();
        let args = action.args();
        let work = cx.background_executor().spawn(async move {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            fetch::act(&script_host, &args, &cancelled)
        });
        let machine = machine.clone();
        let label = host.label.clone();
        let task = cx.spawn(async move |this, cx| {
            let done = work.await;
            let _ = this.update(cx, |this, cx| {
                this.automation_acted(&machine, &label, done, cx);
            });
        });
        host.busy = Some((action, task));
        cx.notify();
    }

    fn automation_acted(
        &mut self,
        machine: &Machine,
        label: &str,
        done: Result<herdr_automation::report::Done, Error>,
        cx: &mut Context<Self>,
    ) {
        if let Some(host) = self.automations.host_mut(machine) {
            host.busy = None;
            // What changed shows at once rather than at the next tick.
            host.last = None;
        }
        match done {
            Ok(done) if !done.message.trim().is_empty() => {
                self.show_flash(Flash::success(format!("{label}: {}", done.message)), cx);
            }
            Ok(_) => {}
            Err(error) => self.automation_failed(label, &error, cx),
        }
        self.poll_automations(cx);
        cx.notify();
    }

    /// Says what went wrong on `label`.
    pub(crate) fn automation_failed(&mut self, label: &str, error: &Error, cx: &mut Context<Self>) {
        tracing::warn!(category = "automations", host = label, %error, "automation command failed");
        self.show_flash(Flash::warning(failure(label, error)), cx);
    }
}

/// The warning for a command that failed on the host named `label`.
pub(super) fn failure(label: &str, error: &Error) -> String {
    fl!(
        "automations-action-failed",
        host = label,
        error = error.localize()
    )
}
