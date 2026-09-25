//! The app update dialog: the release the updater found, its progress, and
//! the one action the current state allows. Preview states are synthetic and
//! never reach the updater service.

use crate::{
    APP_VERSION, HerdrWindow,
    menu::{Page, listener},
    updater::State,
};
use gpui_kit::{
    component::{
        ActiveTheme as _,
        alert::Alert,
        button::{Button, ButtonVariants as _},
        description_list::DescriptionList,
        h_flex,
        progress::Progress,
        v_flex,
    },
    prelude::*,
    *,
};

/// How far along the update is, in percent, or `None` while the step has no
/// reliable measure and the bar shows activity instead.
fn update_progress(state: &State) -> Option<Option<f32>> {
    Some(match state {
        State::Downloading { received, total } if *total > 0 && received < total => {
            Some(*received as f32 / *total as f32 * 100.)
        }
        State::Ready { .. } | State::Restart { .. } => Some(100.),
        // Homebrew has no reliable overall percentage. A completed archive still
        // needs extraction and verification before it is ready to install.
        State::Checking
        | State::Downloading { .. }
        | State::Installing
        | State::Upgrading { .. }
        | State::Restarting
        | State::Cancelling => None,
        _ => return None,
    })
}

#[derive(Clone, Copy)]
enum UpdateAction {
    Check,
    Download,
    Install,
    Upgrade,
    Restart,
    Cancel,
}

impl HerdrWindow {
    pub(crate) fn open_update_progress_preview(
        &mut self,
        state: State,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.begin_menu(window, cx) {
            return;
        }
        self.update_preview = Some(state);
        self.show_app_update(window, cx);
    }

    pub(crate) fn open_app_update(
        &mut self,
        preview: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.begin_menu(window, cx) {
            return;
        }
        self.update_preview = preview.then(|| State::Available {
            version: "9999.0.0".into(),
        });
        self.show_app_update(window, cx);
    }

    fn show_app_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_dialog(Page::AppUpdate, window, cx, |this, dialog, weak, _, cx| {
            let state = this.update_preview.as_ref().unwrap_or(this.updater.state());
            let (message, action) = update_message(state);
            let latest = match state {
                State::Available { version }
                | State::Ready { version }
                | State::Homebrew { version }
                | State::Restart { version } => version.as_str(),
                State::Current => APP_VERSION,
                _ => "Not yet known",
            };
            let muted = cx.theme().muted_foreground;
            dialog
                .title("App Updates")
                .w(px(480.))
                .child(
                    v_flex()
                        .debug_selector(|| "app-update-body".into())
                        .gap_4()
                        .child(
                            DescriptionList::new()
                                .columns(1)
                                .item("Current version", APP_VERSION, 1)
                                .item("Latest version", latest.to_owned(), 1),
                        )
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(message))
                        .children(update_progress(state).map(|fraction| {
                            Progress::new("app-update-progress")
                                .loading(fraction.is_none())
                                .value(fraction.unwrap_or_default())
                        }))
                        .child(div().text_color(muted).child(
                            "Downloads are verified before installation. Your daemon and terminal sessions stay running.",
                        ))
                        .when(this.update_preview.is_some(), |body| {
                            body.child(Alert::info(
                                "app-update-preview",
                                "Synthetic update state only. No network or installation is performed. Close or Escape dismisses this preview.",
                            )
                            .title("QA preview"))
                        }),
                )
                .footer(
                    h_flex()
                        .w_full()
                        .flex_wrap()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("app-update-releases")
                                .ghost()
                                .label("Manual Releases")
                                .on_click(|_, _, cx| {
                                    cx.open_url("https://github.com/penso/herdr-gpui/releases")
                                }),
                        )
                        .children(action.map(|action| {
                            Button::new("app-update-action")
                                .primary()
                                .label(action.label(state))
                                .on_click(listener(weak, move |this, window, cx| {
                                    this.run_update_action(action, window, cx)
                                }))
                        })),
                )
        });
    }

    fn run_update_action(
        &mut self,
        action: UpdateAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Preview actions must never reach the service, including cancellation.
        if let Some(state) = self.update_preview.take() {
            match state {
                State::Available { version } | State::Homebrew { version } => {
                    self.update_preview = Some(State::Ready { version });
                }
                State::Ready { .. } => self.dismiss_menu(window, cx),
                _ => self.update_preview = Some(State::Idle),
            }
        } else {
            // A mailbox transition cannot turn a stale Download click into approval.
            match (action, self.updater.state()) {
                (UpdateAction::Check, State::Idle | State::Current | State::Error(_)) => {
                    self.updater.check()
                }
                (UpdateAction::Download, State::Available { .. }) => self.updater.download(),
                (UpdateAction::Install, State::Ready { .. }) => self.updater.install(),
                (UpdateAction::Upgrade, State::Homebrew { .. }) => self.updater.upgrade(),
                (UpdateAction::Restart, State::Restart { .. }) => self.updater.restart(),
                (
                    UpdateAction::Cancel,
                    State::Checking | State::Downloading { .. } | State::Installing,
                ) => self.updater.cancel(),
                _ => {}
            }
        }
        cx.notify();
    }
}

impl UpdateAction {
    fn label(self, state: &State) -> &'static str {
        match self {
            Self::Check if matches!(state, State::Error(_)) => "Retry",
            Self::Check => "Check for Updates",
            Self::Download => "Download",
            Self::Install => "Install and Restart",
            Self::Upgrade => "Update with Homebrew",
            Self::Restart => "Restart",
            Self::Cancel => "Cancel",
        }
    }
}

/// What the dialog says about `state`, and the action it offers.
fn update_message(state: &State) -> (String, Option<UpdateAction>) {
    match state {
        State::Disabled(reason) => (format!("In-app updates unavailable: {reason}"), None),
        State::Idle => (
            "Check GitHub for a new app release.".into(),
            Some(UpdateAction::Check),
        ),
        State::Checking => ("Checking for updates...".into(), Some(UpdateAction::Cancel)),
        State::Current => (
            "You are running the latest available release.".into(),
            Some(UpdateAction::Check),
        ),
        State::Available { .. } => (
            "A new app release is available.".into(),
            Some(UpdateAction::Download),
        ),
        State::Downloading { received, total } => (
            if *total == 0 {
                format!("Downloading: {received} bytes received (size unknown)")
            } else if received >= total {
                "Download complete. Extracting and verifying the update...".into()
            } else {
                format!(
                    "Downloading: {received} / {total} bytes ({:.0}%)",
                    (*received as f64 / *total as f64 * 100.).min(100.)
                )
            },
            Some(UpdateAction::Cancel),
        ),
        State::Ready { .. } => (
            "Download verified. Install and restart when you are ready.".into(),
            Some(UpdateAction::Install),
        ),
        State::Installing => (
            "Preparing installation and restart. Herdr will quit when the update is ready.".into(),
            Some(UpdateAction::Cancel),
        ),
        State::Homebrew { .. } => (
            "A new app release is available. Homebrew will install it, refreshing its package metadata if needed.".into(),
            Some(UpdateAction::Upgrade),
        ),
        State::Upgrading { detail } => (
            format!("Homebrew: {detail}"),
            // Homebrew is never interrupted mid-upgrade: see updater::cancel.
            None,
        ),
        State::Restart { version } => (
            format!("Homebrew installed {version}. Restart to finish; your daemon and terminal sessions stay running."),
            Some(UpdateAction::Restart),
        ),
        State::Restarting => ("Starting the updated app...".into(), None),
        State::Cancelling => (
            "Cancelling update... Waiting for current I/O to finish or time out.".into(),
            None,
        ),
        State::Error(error) => (format!("Update failed: {error}"), Some(UpdateAction::Check)),
    }
}
