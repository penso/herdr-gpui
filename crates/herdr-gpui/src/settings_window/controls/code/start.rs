//! The Code page's choice between starting VS Code and using an address,
//! and, when the app starts it, its license and how it is doing. Nothing
//! here waits: the launcher finds and runs VS Code off the UI thread, and
//! this page shows what it last reported.
use super::*;
use crate::code_server::{
    Launcher, Startup,
    launcher::{self, Cli, LICENSE_URL, PRIVACY_URL},
};

impl SettingsWindow {
    /// The mode in effect, or `None` while VS Code is still being looked for.
    pub(super) fn code_mode(&self, cx: &App) -> Option<CodeMode> {
        if !crate::browser::EMBEDDED {
            return Some(CodeMode::Address);
        }
        launcher::mode(
            &self.config.code,
            Launcher::cli(cx).unwrap_or(&Cli::Missing),
        )
    }

    pub(in crate::settings_window) fn choose_code_mode(
        &mut self,
        mode: CodeMode,
        cx: &mut Context<Self>,
    ) {
        if self.code_mode(cx) == Some(mode) && self.config.code.mode == Some(mode) {
            return;
        }
        // Shown chosen at once; the reload after the save confirms it, and
        // the windows stop or start VS Code as their config then says.
        self.config.code.mode = Some(mode);
        self.queue_code_edit(CodeEdit::Mode(mode), cx);
        cx.notify();
    }

    pub(in crate::settings_window) fn accept_code_license(&mut self, cx: &mut Context<Self>) {
        self.config.code.license_accepted = true;
        self.queue_code_edit(CodeEdit::AcceptLicense, cx);
        cx.notify();
    }

    /// The two ways to get a server. Starting VS Code needs it installed,
    /// in a build that shows pages.
    pub(super) fn render_code_mode(&self, mode: Option<CodeMode>, cx: &mut Context<Self>) -> Div {
        let cli = Launcher::cli(cx);
        let can_start = crate::browser::EMBEDDED && matches!(cli, Some(Cli::Found(_)));
        let choice = |id: &'static str, label: String, choice: CodeMode, enabled: bool| {
            self.control_choice(id, label, mode == Some(choice), enabled)
                .debug_selector(move || id.into())
                .when(enabled, |button| {
                    button.on_click(cx.listener(move |this, _, _, cx| {
                        this.choose_code_mode(choice, cx);
                    }))
                })
        };
        let note = match cli {
            _ if !crate::browser::EMBEDDED => fl!("settings-code-no-pages"),
            Some(Cli::Unknown | Cli::Finding) => fl!("settings-code-finding"),
            Some(Cli::Found(program)) => {
                fl!("settings-code-found", path = program.display().to_string())
            }
            Some(Cli::Missing) | None => fl!("settings-code-missing"),
        };
        self.control_card(&fl!("settings-code-vscode"))
            .relative()
            .debug_selector(|| "code-mode".into())
            .child(crate::settings_window::search::track_bounds(
                "code-mode".into(),
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(12.))
                    .child(choice(
                        "settings-code-start",
                        fl!("settings-code-start"),
                        CodeMode::Start,
                        can_start,
                    ))
                    .child(choice(
                        "settings-code-address",
                        fl!("settings-code-address"),
                        CodeMode::Address,
                        true,
                    )),
            )
            .child(self.control_note(note))
    }

    /// The server the app starts: its license until accepted, then how it is.
    pub(super) fn render_code_start(&self, cx: &mut Context<Self>) -> Div {
        let card = self
            .control_card(&fl!("settings-code-server"))
            .child(self.control_note(fl!("settings-code-start-note")));
        match Launcher::startup(cx, &self.config.code) {
            Startup::Consent => card.child(self.render_code_license(cx)),
            Startup::Idle => card.child(self.control_note(fl!("settings-code-idle"))),
            Startup::Starting { .. } => {
                card.child(self.control_note(fl!("settings-code-starting")))
            }
            Startup::Ready { url } => card.child(self.render_code_state(
                "icons/pass.svg",
                self.theme.palette[2],
                fl!("settings-code-running", address = url.address().to_string()).into(),
            )),
            Startup::Failed { error, .. } => card.child(self.render_code_state(
                "icons/error.svg",
                self.theme.palette[1],
                fl!("settings-code-failed", error = error.to_string()).into(),
            )),
            // The mode card says why.
            Startup::Address | Startup::Finding | Startup::Missing => card,
        }
    }

    /// The terms to accept before the app may start VS Code.
    fn render_code_license(&self, cx: &mut Context<Self>) -> Div {
        let link = |id: &'static str, label: String, url: &'static str| {
            div()
                .id(id)
                .text_color(crate::menu::accent(&self.theme))
                .cursor_pointer()
                .hover(|style| style.underline())
                .on_click(move |_, _, cx| cx.open_url(url))
                .child(label)
        };
        let accept = self
            .control_choice(
                "settings-code-accept",
                fl!("settings-code-accept"),
                false,
                true,
            )
            .debug_selector(|| "settings-code-accept".into())
            .flex_none()
            .on_click(cx.listener(|this, _, _, cx| this.accept_code_license(cx)));
        div()
            .debug_selector(|| "settings-code-license".into())
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(div().min_w_0().child(fl!("settings-code-license-terms")))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(16.))
                    .child(link(
                        "settings-code-license-link",
                        fl!("settings-code-license-link"),
                        LICENSE_URL,
                    ))
                    .child(link(
                        "settings-code-privacy-link",
                        fl!("settings-code-privacy-link"),
                        PRIVACY_URL,
                    )),
            )
            .child(div().flex().child(accept))
    }

    fn render_code_state(&self, icon: &'static str, color: u32, text: SharedString) -> Div {
        div()
            .debug_selector(|| "settings-code-state".into())
            .flex()
            .items_center()
            .gap(px(8.))
            .min_w_0()
            .child(
                svg()
                    .path(icon)
                    .flex_none()
                    .size(px(16.))
                    .text_color(rgb(self.theme.ink(color))),
            )
            .child(div().min_w_0().child(text))
    }
}
