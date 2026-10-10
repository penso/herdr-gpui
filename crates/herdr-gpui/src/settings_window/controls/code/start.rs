//! The Code page's choice between starting VS Code and using an address,
//! and, when the app starts it, its license and how it is doing. Nothing
//! here waits: the launcher finds and runs VS Code off the UI thread, and
//! this page shows what it last reported.
use super::*;
use crate::code_server::{
    Launcher, Startup,
    launcher::{self, Cli, LICENSE_TERMS, LICENSE_URL, PRIVACY_URL},
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
        let choice = |id: &'static str, label: &'static str, choice: CodeMode, enabled: bool| {
            self.control_choice(id, label, mode == Some(choice), enabled)
                .debug_selector(move || id.into())
                .when(enabled, |button| {
                    button.on_click(cx.listener(move |this, _, _, cx| {
                        this.choose_code_mode(choice, cx);
                    }))
                })
        };
        let note = match cli {
            _ if !crate::browser::EMBEDDED => {
                Some("This build shows no pages, so it starts no VS Code.".into())
            }
            Some(Cli::Unknown | Cli::Finding) => Some("Looking for VS Code\u{2026}".into()),
            Some(Cli::Found(program)) => Some(SharedString::from(format!(
                "VS Code: {}",
                program.display()
            ))),
            Some(Cli::Missing) | None => Some(
                "VS Code was not found on your PATH or in Applications. Install it to have \
                 the app start it, or use the address of a server you run."
                    .into(),
            ),
        };
        self.control_card("VS Code")
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(12.))
                    .child(choice(
                        "settings-code-start",
                        "Start VS Code automatically",
                        CodeMode::Start,
                        can_start,
                    ))
                    .child(choice(
                        "settings-code-address",
                        "Use an address",
                        CodeMode::Address,
                        true,
                    )),
            )
            .children(note.map(|note| self.control_note(note)))
    }

    /// The server the app starts: its license until accepted, then how it is.
    pub(super) fn render_code_start(&self, cx: &mut Context<Self>) -> Div {
        let card = self.control_card("Server").child(self.control_note(
            "The app runs `code serve-web` on 127.0.0.1 when a VS Code tab first needs it, \
             and stops it when it quits.",
        ));
        match Launcher::startup(cx, &self.config.code) {
            Startup::Consent => card.child(self.render_code_license(cx)),
            Startup::Idle => card.child(self.control_note("Not started yet.")),
            Startup::Starting { .. } => card
                .child(self.control_note("Starting VS Code\u{2026} (the first run downloads it)")),
            Startup::Ready { url } => card.child(self.render_code_state(
                "icons/pass.svg",
                self.theme.palette[2],
                format!("Running on {}", url.address()).into(),
            )),
            Startup::Failed { error, .. } => card.child(self.render_code_state(
                "icons/error.svg",
                self.theme.palette[1],
                format!("{error} Retrying\u{2026}").into(),
            )),
            // The mode card says why.
            Startup::Address | Startup::Finding | Startup::Missing => card,
        }
    }

    /// The terms to accept before the app may start VS Code.
    fn render_code_license(&self, cx: &mut Context<Self>) -> Div {
        let link = |id: &'static str, label: &'static str, url: &'static str| {
            div()
                .id(id)
                .text_color(crate::menu::accent(&self.theme))
                .cursor_pointer()
                .hover(|style| style.underline())
                .on_click(move |_, _, cx| cx.open_url(url))
                .child(label)
        };
        let accept = self
            .control_choice("settings-code-accept", "Accept and start", false, true)
            .debug_selector(|| "settings-code-accept".into())
            .flex_none()
            .on_click(cx.listener(|this, _, _, cx| this.accept_code_license(cx)));
        div()
            .debug_selector(|| "settings-code-license".into())
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(div().min_w_0().child(LICENSE_TERMS))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(16.))
                    .child(link(
                        "settings-code-license-link",
                        "License Terms",
                        LICENSE_URL,
                    ))
                    .child(link(
                        "settings-code-privacy-link",
                        "Privacy Statement",
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
