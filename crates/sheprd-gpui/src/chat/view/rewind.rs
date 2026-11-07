//! "Edit from here" beside a prompt's Copy, and the bar over the composer
//! that says what sending the edit does to the checkout's files.

use super::super::{
    chats::ChatPane,
    rewind::{Checkpoint, Files, Stage},
};
use crate::{SheprdWindow, config::corners, i18n::fl, window::chrome_focus::Region};
use gpui::{prelude::*, *};
use sheprd_chat_protocol::EntryId;

impl SheprdWindow {
    /// A prompt's "Edit from here", which is only a pencil: the tooltip names
    /// it, or says why it can't be used now. It sits inside the Copy button,
    /// so its clicks stop here.
    pub(super) fn render_chat_rewind_button(
        &self,
        chat: &ChatPane,
        id: EntryId,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let session = chat.session.transcript().session.as_ref();
        let supported = session.is_some_and(|session| session.rewind);
        let available = supported && !chat.rewind_submitted();
        let label = SharedString::from(if available {
            fl!("chat-rewind-edit")
        } else if supported {
            fl!("chat-rewind-busy")
        } else {
            fl!("chat-rewind-unavailable")
        });
        let color = if available {
            theme.muted
        } else {
            theme.dimmed(theme.muted)
        };
        let (foreground, surface) = (theme.foreground, theme.surface);
        let pane_id = chat.target.pane_id.clone();
        let button = div()
            .id(("chat-rewind", id.0))
            .debug_selector(move || format!("chat-rewind-{}", id.0))
            .flex()
            .items_center()
            .justify_center()
            .size(px(16.))
            .rounded(px(corners::SMALL))
            .child(
                svg()
                    .path("icons/pencil.svg")
                    .size(px(11.))
                    .text_color(rgb(color)),
            )
            .tooltip(move |_, cx| {
                cx.new(|_| crate::menu::SettingsHint {
                    text: label.clone(),
                    foreground,
                    surface,
                })
                .into()
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                cx.stop_propagation();
                if available {
                    this.begin_chat_rewind(&pane_id, id, window, cx);
                }
            }));
        if !available {
            return button;
        }
        let button = button
            .cursor_pointer()
            .hover(|button| button.bg(rgb(theme.surface)));
        self.chrome_stop(Region::Chat, button)
    }

    /// The warning that an agent may keep changing the files, as the
    /// checkpoints dialog gives it.
    fn render_chat_rewind_warning(&self, workspace: Option<&str>) -> Option<Div> {
        workspace
            .is_some_and(|workspace| self.workspace_agent_working(workspace))
            .then(|| {
                self.render_agent_working_warning()
                    .debug_selector(|| "chat-rewind-agent-working".into())
            })
    }

    /// The bar over the composer while an edit waits to be sent.
    pub(super) fn render_chat_rewind(
        &self,
        chat: &ChatPane,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let rewind = chat.rewind.as_ref()?;
        let theme = &self.theme;
        let small = px((self.chat_text_size() - 2.).max(10.));
        let editing = rewind.stage == Stage::Editing;
        let restorable = matches!(
            rewind.checkpoint,
            Checkpoint::Looking | Checkpoint::Found(_)
        );
        let ring = rgb(theme.primary());
        let choice = |files: Files,
                      label: String,
                      id: &'static str,
                      enabled: bool,
                      cx: &mut Context<Self>| {
            let chosen = rewind.files == files;
            let pane_id = chat.target.pane_id.clone();
            let focus = &rewind.focus[usize::from(files == Files::Restore)];
            let button = div()
                .id(id)
                .debug_selector(move || id.into())
                .px(px(8.))
                .py(px(3.))
                .rounded(px(corners::CONTROL))
                .border_1()
                .border_color(rgb(if chosen {
                    theme.primary()
                } else {
                    theme.active
                }))
                .when(chosen, |button| button.bg(rgb(theme.primary_wash())))
                .text_color(rgb(if enabled {
                    theme.foreground
                } else {
                    theme.muted
                }))
                .child(label);
            if !enabled {
                return button;
            }
            button
                .track_focus(focus)
                .focus_visible(move |style| {
                    style.shadow(vec![BoxShadow {
                        color: ring.into(),
                        offset: point(px(0.), px(0.)),
                        blur_radius: px(0.),
                        spread_radius: px(2.),
                        inset: true,
                    }])
                })
                .cursor_pointer()
                .hover(|button| button.bg(rgb(theme.active)))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    this.choose_chat_rewind_files(&pane_id, files, window, cx);
                }))
        };
        let note = match (&rewind.stage, rewind.files, &rewind.checkpoint) {
            (Stage::Restoring { .. }, _, _) => fl!("chat-rewind-restoring"),
            (Stage::Sending, _, _) => fl!("chat-rewind-sending"),
            (_, Files::Restore, Checkpoint::Found(_)) => fl!("chat-rewind-restore-note"),
            (_, _, Checkpoint::Looking) => fl!("chat-rewind-checkpoint-looking"),
            (_, _, Checkpoint::Missing) => fl!("chat-rewind-checkpoint-missing"),
            (_, _, Checkpoint::Unreachable) => fl!("chat-rewind-checkpoint-unreachable"),
            _ => fl!("chat-rewind-keep-note"),
        };
        let keep = choice(
            Files::Keep,
            fl!("chat-rewind-keep"),
            "chat-rewind-keep",
            editing,
            cx,
        );
        let restore = choice(
            Files::Restore,
            fl!("chat-rewind-restore"),
            "chat-rewind-restore",
            editing && restorable,
            cx,
        );
        let pane_id = chat.target.pane_id.clone();
        let cancel = div()
            .id("chat-rewind-cancel")
            .debug_selector(|| "chat-rewind-cancel".into())
            .flex_none()
            .px(px(8.))
            .py(px(3.))
            .rounded(px(corners::CONTROL))
            .text_color(rgb(theme.subtext()))
            .when(editing, |button| {
                button
                    .cursor_pointer()
                    .hover(|button| button.bg(rgb(theme.active)))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.cancel_chat_rewind(&pane_id, window, cx);
                    }))
            })
            .child(fl!("chat-rewind-cancel"));
        Some(
            div()
                .debug_selector(|| "chat-rewind".into())
                .flex()
                .flex_col()
                .gap(px(6.))
                .text_size(small)
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .justify_between()
                        .gap(px(6.))
                        .child(
                            div()
                                .min_w_0()
                                .text_color(rgb(theme.foreground))
                                .child(fl!("chat-rewind-title")),
                        )
                        .child(cancel),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(6.))
                        .child(keep)
                        .child(restore),
                )
                .child(
                    div()
                        .debug_selector(|| "chat-rewind-note".into())
                        .min_w_0()
                        .text_color(rgb(theme.subtext()))
                        .child(note),
                )
                .when(rewind.lost_files, |bar| {
                    bar.child(
                        div()
                            .debug_selector(|| "chat-rewind-files-lost".into())
                            .min_w_0()
                            .text_color(rgb(theme.subtext()))
                            .child(fl!("chat-rewind-files-lost")),
                    )
                })
                .children(self.render_chat_rewind_warning(rewind.workspace.as_deref())),
        )
    }
}
