//! The dialog that writes a reply to one Overview row or several selected
//! ones. It is a menu page so its field gets the menu's focus, composition,
//! and paste handling; the text then waits in the Overview's queue.

use super::Page;
use crate::{SheprdWindow, dialog_input::DialogInput, i18n::fl, overview::MAX_REPLY};
use gpui::{prelude::*, *};

/// Turns the just opened menu into the reply dialog.
pub(crate) fn open_overview_reply(view: &mut SheprdWindow) {
    view.menu.page = Some(Page::OverviewReply);
    let mut input = DialogInput::default();
    input.label = Some(fl!("overview-reply-field").into());
    view.menu.input = Some(input);
}

impl SheprdWindow {
    pub(super) fn submit_overview_reply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = self
            .menu
            .input
            .as_ref()
            .filter(|input| input.marked.is_none())
        else {
            return;
        };
        let text = input.text.clone();
        if text.trim().is_empty() {
            return;
        }
        // Refused here, with the text still in the field, rather than cut.
        if text.trim().len() > MAX_REPLY {
            self.menu.error = Some(fl!("overview-reply-too-long"));
            cx.notify();
            return;
        }
        self.dismiss_menu(window, cx);
        self.queue_overview_replies(&text, cx);
    }

    pub(super) fn render_overview_reply(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let targets = self.overview_reply_targets();
        let name = targets.first().map_or("", |row| row.title.as_str());
        let armed = self
            .menu
            .input
            .as_ref()
            .is_some_and(|input| !input.text.trim().is_empty());
        let mut body = div()
            .debug_selector(|| "overview-reply-dialog".into())
            .flex()
            .flex_col()
            .gap(px(10.))
            .px(px(16.))
            .py(px(12.))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .truncate()
                    .child(fl!(
                        "overview-reply-title",
                        count = targets.len(),
                        name = name
                    )),
            )
            .child(
                div()
                    .text_color(rgb(theme.muted))
                    .child(fl!("overview-reply-note")),
            );
        if self.menu.input.is_some() {
            body = body.child(self.render_dialog_input(cx));
        }
        if let Some(error) = &self.menu.error {
            body = body.child(
                div()
                    .debug_selector(|| "overview-reply-error".into())
                    .text_color(rgb(theme.muted))
                    .child(error.clone()),
            );
        }
        div().flex().flex_col().child(body).child(
            self.pr_footer()
                .child(
                    self.pr_button(
                        "overview-reply-dismiss",
                        fl!("overview-reply-cancel-dialog"),
                        false,
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.dismiss_menu(window, cx);
                    })),
                )
                .child(
                    self.pr_button("overview-reply-submit", fl!("overview-reply-send"), armed)
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.submit_overview_reply(window, cx);
                        })),
                ),
        )
    }
}
