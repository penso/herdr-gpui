//! Drawing the VS Code panel to the right of the editor groups, its draggable edge,
//! and the titlebar button that shows and hides it.
use super::{code::Reach, store::Place, view::store};
use crate::{HerdrWindow, panel_resize::PanelDrag};
use gpui::{prelude::*, *};

/// The narrowest Herdr realm that still holds a dialog beside VS Code; a
/// narrower one gives Herdr's menus the whole window again.
const MIN_REALM: f32 = 480.;

impl HerdrWindow {
    /// The width of the Herdr realm, the window less the VS Code column,
    /// while the column shows. Herdr's menus and dialogs stay in it: the
    /// VS Code page draws above GPUI, so it would hide them, and it belongs
    /// to VS Code, which decides for itself what to dim. `None` when the
    /// realm is the whole window, as it is when too narrow for a dialog: the
    /// page then steps aside for an open menu.
    pub(crate) fn herdr_realm(&self) -> Option<Pixels> {
        self.beside_code().filter(|realm| *realm >= px(MIN_REALM))
    }

    /// The width left of the VS Code column while it shows, however narrow.
    /// Toasts and cards keep to it: no menu is open to make the page step
    /// aside for them, so the page would hide them anywhere else.
    pub(crate) fn beside_code(&self) -> Option<Pixels> {
        self.shown_code()
            .then(|| px(self.viewport_width - self.code_width.width(self.viewport_width)))
    }

    /// `groups`, beside the focused workspace's VS Code panel when it shows.
    pub(crate) fn render_beside_code(
        &mut self,
        groups: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !self.shown_code() {
            return groups;
        }
        // The page draws above everything GPUI paints, so the edge to drag
        // hangs just outside the column, over the Herdr side of the line.
        let column = div()
            .id("code")
            .debug_selector(|| "code".into())
            .flex_none()
            .flex()
            .flex_col()
            .min_h_0()
            .border_l_1()
            .border_color(rgb(self.theme.active))
            .bg(rgb(self.theme.background))
            .child(self.render_code_body(cx));
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(div().flex().flex_1().min_w_0().min_h_0().child(groups))
            .child(self.resizable_panel(column, "code-resize", PanelDrag::Code, None, cx))
            .into_any_element()
    }

    /// The page, or why there is none yet.
    fn render_code_body(&self, cx: &App) -> AnyElement {
        // A tab moved to the groups draws there, not here.
        let tab = self
            .browser_key()
            .and_then(|(scope, workspace)| store(cx)?.code_tab(&scope, &workspace).cloned())
            .filter(|tab| tab.place == Place::Code);
        let failure = tab
            .as_ref()
            .and_then(|tab| self.browser.failed.get(&tab.id).cloned());
        #[cfg(any(target_os = "macos", windows))]
        if let Some(tab) = &tab
            && failure.is_none()
            && let Some(page) = self.browser.pages.page(tab.id).cloned()
        {
            return self.page_area(tab.id, page).into_any_element();
        }
        self.render_code_status(failure)
    }

    /// Why the VS Code page is not there yet, in the panel or in a group:
    /// `failure` says why it could not be created.
    pub(super) fn render_code_status(&self, failure: Option<SharedString>) -> AnyElement {
        let text: SharedString = if !super::EMBEDDED {
            "This build cannot show pages in the window.".into()
        } else if self.config.code.url.is_none() {
            "Set the VS Code server in Settings to show VS Code here.".into()
        } else if let Some(message) = failure {
            format!("Could not show this page: {message}").into()
        } else {
            match &self.browser.code_server.state {
                Reach::Failed { message, .. } => {
                    return self.render_code_unreachable(message.clone());
                }
                Reach::Ready { server, .. } => {
                    format!("Loading VS Code {}\u{2026}", server.short_commit()).into()
                }
                Reach::Unknown | Reach::Asking => "Connecting to VS Code\u{2026}".into(),
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

    /// The titlebar button showing and hiding the VS Code panel. It shows
    /// only once a server is set, in a build that can show pages.
    pub(crate) fn render_code_toggle(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        if !super::EMBEDDED || self.config.code.url.is_none() {
            return None;
        }
        // Lit while VS Code shows, in the panel or in a group.
        let shown = self.shown_code()
            || self
                .grouped_code_tab(cx)
                .is_some_and(|id| self.group_shows_page(id, cx));
        let color = if shown {
            self.theme.foreground
        } else {
            self.theme.muted
        };
        Some(
            div()
                .id("toggle-code")
                .debug_selector(|| "toggle-code".into())
                .flex_none()
                .self_center()
                .mx(px(4.))
                .size(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(crate::config::corners::CONTROL))
                .cursor_pointer()
                .hover(|style| style.bg(rgb(self.theme.active)))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    this.command(crate::controls::Command::ToggleCode, window, cx);
                }))
                .child(
                    svg()
                        .path("icons/vscode.svg")
                        .size(px(16.))
                        .text_color(rgb(color)),
                ),
        )
    }
}

/// `content` in the middle of the panel, both ways.
fn centered(content: impl IntoElement) -> AnyElement {
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
