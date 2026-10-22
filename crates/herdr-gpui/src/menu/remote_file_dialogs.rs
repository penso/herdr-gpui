//! The questions a save to a remote file's copy asks: whether to upload it,
//! and what to do when the host's file changed since it was synced. They
//! open only when no other page holds the window, and wait until then.

use super::Page;
use crate::{
    HerdrWindow,
    fonts::StyledFont,
    remote_files::{Answer, Prompt, PromptKind, Resolution, basename},
};
use gpui::{prelude::*, *};

impl HerdrWindow {
    /// The question the open page asks, if it is one of these.
    pub(crate) fn shown_remote_file_prompt(&self) -> Option<Prompt> {
        let kind = match self.menu.page? {
            Page::RemoteUpload => PromptKind::Upload,
            Page::RemoteConflict => PromptKind::Conflict,
            _ => return None,
        };
        Some(Prompt {
            key: self.menu.remote_file.clone()?,
            kind,
        })
    }

    /// Shows `prompt` unless another page holds the window. `true` when it
    /// opened.
    pub(crate) fn show_remote_file_prompt(
        &mut self,
        prompt: &Prompt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.menu.page.is_some() || !self.open_menu(window, cx) {
            return false;
        }
        self.menu.remote_file = Some(prompt.key.clone());
        self.menu.page = Some(match prompt.kind {
            PromptKind::Upload => Page::RemoteUpload,
            PromptKind::Conflict => Page::RemoteConflict,
        });
        true
    }

    /// Closes the open question without answering it: its copy no longer
    /// waits for the answer. The copy is forgotten first, so closing is not
    /// taken as a "Not now".
    pub(crate) fn close_remote_file_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu.remote_file = None;
        self.dismiss_menu(window, cx);
    }

    /// Closing the upload question without an answer is a "Not now".
    pub(in crate::menu) fn decline_remote_upload_on_dismiss(&mut self, cx: &mut Context<Self>) {
        if self.menu.page == Some(Page::RemoteUpload)
            && let Some(key) = self.menu.remote_file.take()
        {
            self.answer_remote_upload(key, Answer::NotNow, cx);
        }
    }

    pub(in crate::menu) fn confirm_remote_upload(
        &mut self,
        answer: Answer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = self.menu.remote_file.take();
        self.dismiss_menu(window, cx);
        if let Some(key) = key {
            self.answer_remote_upload(key, answer, cx);
        }
    }

    pub(in crate::menu) fn confirm_remote_conflict(
        &mut self,
        resolution: Resolution,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = self.menu.remote_file.take();
        self.dismiss_menu(window, cx);
        if let Some(key) = key {
            self.resolve_remote_conflict(key, resolution, cx);
        }
    }

    pub(in crate::menu) fn render_remote_upload(&self, cx: &mut Context<Self>) -> Div {
        let Some((target, path)) = &self.menu.remote_file else {
            return div();
        };
        let Some(copy) = self.remote_files.copy(target, path) else {
            return div();
        };
        let theme = &self.theme;
        div()
            .debug_selector(|| "remote-upload".into())
            .child(title(format!(
                "Upload your changes to {}?",
                basename(path, copy.shell)
            )))
            .child(
                div().px(px(8.)).child(
                    div()
                        .debug_selector(|| "remote-upload-path".into())
                        .rounded(px(crate::config::corners::CONTROL))
                        .bg(rgb(theme.active))
                        .px(px(10.))
                        .py(px(6.))
                        .overflow_hidden()
                        .text_font(&self.config.terminal)
                        .child(div().child(target.clone()))
                        .child(div().child(path.clone())),
                ),
            )
            .child(muted(
                theme.muted,
                "Your saved copy replaces the file on the host.",
            ))
            .child(
                buttons()
                    .child(self.upload_button("remote-upload-yes", "Upload", Answer::Upload, cx))
                    .child(self.upload_button(
                        "remote-upload-always",
                        "Upload future saves without asking",
                        Answer::Always,
                        cx,
                    ))
                    .child(
                        self.remote_file_button("remote-upload-no", "Not now", false)
                            .on_click(cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.dismiss_menu(window, cx);
                            })),
                    ),
            )
    }

    pub(in crate::menu) fn render_remote_conflict(&self, cx: &mut Context<Self>) -> Div {
        let Some((target, path)) = &self.menu.remote_file else {
            return div();
        };
        let Some(copy) = self.remote_files.copy(target, path) else {
            return div();
        };
        div()
            .debug_selector(|| "remote-conflict".into())
            .child(title(format!(
                "{} changed on {target}",
                basename(path, copy.shell)
            )))
            .child(muted(
                self.theme.muted,
                "It changed since it was fetched, so your save was not uploaded.",
            ))
            .child(
                buttons()
                    .child(self.conflict_button(
                        "remote-conflict-keep-both",
                        "Keep both",
                        Resolution::KeepBoth,
                        cx,
                    ))
                    .child(self.conflict_button(
                        "remote-conflict-overwrite",
                        "Overwrite anyway",
                        Resolution::Overwrite,
                        cx,
                    ))
                    .child(self.conflict_button(
                        "remote-conflict-refetch",
                        "Re-fetch",
                        Resolution::Refetch,
                        cx,
                    )),
            )
    }

    fn remote_file_button(
        &self,
        id: &'static str,
        label: &'static str,
        primary: bool,
    ) -> Stateful<Div> {
        let active = self.theme.active;
        div()
            .id(id)
            .debug_selector(move || id.into())
            .p(px(8.))
            .rounded(px(crate::config::corners::CONTROL))
            .when(primary, |button| button.bg(rgb(active)))
            .when(!primary, |button| button.hover(|s| s.bg(rgb(active))))
            .cursor_pointer()
            .child(label)
    }

    fn upload_button(
        &self,
        id: &'static str,
        label: &'static str,
        answer: Answer,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.remote_file_button(id, label, answer == Answer::Upload)
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.confirm_remote_upload(answer, window, cx);
            }))
    }

    fn conflict_button(
        &self,
        id: &'static str,
        label: &'static str,
        resolution: Resolution,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.remote_file_button(id, label, resolution == Resolution::KeepBoth)
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.confirm_remote_conflict(resolution, window, cx);
            }))
    }
}

fn title(text: String) -> Div {
    div()
        .p(px(8.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(text)
}

fn muted(color: u32, text: &'static str) -> Div {
    div().p(px(8.)).text_color(rgb(color)).child(text)
}

fn buttons() -> Div {
    div().flex().flex_wrap().gap(px(8.)).p(px(8.))
}
