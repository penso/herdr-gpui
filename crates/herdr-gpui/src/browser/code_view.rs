//! What a VS Code tab shows until its page is there: why it is empty, the
//! license to accept, or that VS Code is starting.
use super::code::Reach;
use crate::{
    HerdrWindow,
    code_server::{Launcher, Startup},
};
use gpui::{prelude::*, *};

impl HerdrWindow {
    /// Why the VS Code tab's page is not there yet: `failure` says why it
    /// could not be created.
    pub(super) fn render_code_status(
        &self,
        failure: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let startup = Launcher::startup(cx, &self.config.code);
        let text: SharedString = if !super::EMBEDDED {
            "This build cannot show pages in the window.".into()
        } else if let Some(message) = failure {
            format!("Could not show this page: {message}").into()
        } else {
            match startup {
                Startup::Address if self.config.code.url.is_none() => {
                    "Set the VS Code server in Settings to show VS Code here.".into()
                }
                Startup::Finding => "Looking for VS Code\u{2026}".into(),
                Startup::Missing => "VS Code is not installed. Install it, or set the address \
                                     of a server you run in Settings."
                    .into(),
                Startup::Consent => return self.render_code_consent(cx),
                Startup::Idle | Startup::Starting { .. } => {
                    "Starting VS Code\u{2026} (the first run downloads it)".into()
                }
                Startup::Failed { error, .. } => {
                    return self.render_code_unreachable(error.to_string().into());
                }
                Startup::Address | Startup::Ready { .. } => match &self.browser.code_server.state {
                    Reach::Failed { message, .. } => {
                        return self.render_code_unreachable(message.clone());
                    }
                    Reach::Ready { server, .. } => {
                        format!("Loading VS Code {}\u{2026}", server.short_commit()).into()
                    }
                    Reach::Unknown | Reach::Asking => "Connecting to VS Code\u{2026}".into(),
                },
            }
        };
        centered(
            div()
                .debug_selector(|| "code-placeholder".into())
                .text_color(rgb(self.theme.muted))
                .child(text),
        )
    }

    /// Why the server did not answer, and that it is asked again.
    fn render_code_unreachable(&self, message: SharedString) -> AnyElement {
        let theme = &self.theme;
        centered(
            div()
                .debug_selector(|| "code-unreachable".into())
                .flex()
                .flex_col()
                .items_center()
                .gap(px(8.))
                .text_center()
                .child(
                    svg()
                        .path("icons/error.svg")
                        .size(px(32.))
                        .text_color(rgb(theme.ink(theme.palette[1]))),
                )
                .child(
                    div()
                        .text_color(rgb(theme.foreground))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Cannot reach the VS Code server"),
                )
                .child(div().text_color(rgb(theme.muted)).child(message))
                .child(
                    div()
                        .text_color(rgb(theme.muted))
                        .text_sm()
                        .child("Retrying\u{2026}"),
                ),
        )
    }

    /// Whether VS Code is offered, in each group's "…" menu and as Open
    /// VS Code: once a server is set, or the app starts VS Code, in a build
    /// that can show pages.
    pub(crate) fn code_offered(&self, cx: &App) -> bool {
        super::EMBEDDED && Launcher::startup(cx, &self.config.code).offered(&self.config.code)
    }
}

/// `content` in the middle of the tab, both ways.
pub(super) fn centered(content: impl IntoElement) -> AnyElement {
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .items_center()
        .justify_center()
        .px_4()
        .child(content)
        .into_any_element()
}
