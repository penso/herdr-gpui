//! Editing GUI overrides from the shortcut reference. The field uses GPUI's
//! key notation, so aliases are editable without capturing desktop shortcuts.
// i18n: migrated
use super::Page;
use crate::i18n::fl;
use crate::{
    HerdrWindow,
    config::Config,
    controls::CommandInfo,
    keymap::{Binding, DaemonKeys, Keymap},
    search_input::SearchInput,
};
use gpui::{Context, Div, Entity, FontWeight, KeyDownEvent, Window, div, prelude::*, px, rgb};

pub(super) struct Editor {
    info: &'static CommandInfo,
    input: Entity<SearchInput>,
    saving: bool,
    error: Option<String>,
}

#[derive(Clone, Copy)]
enum Edit {
    Save,
    Unbind,
    Reset,
}

impl HerdrWindow {
    pub(super) fn edit_shortcut(
        &mut self,
        info: &'static CommandInfo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .menu
            .shortcut_editor
            .as_ref()
            .is_some_and(|editor| editor.saving)
        {
            return;
        }
        let keys = self
            .keymap()
            .bindings()
            .filter(|(command, _)| *command == info.command)
            .map(|(_, key)| key)
            .collect::<Vec<_>>()
            .join(" ");
        let input = cx.new(SearchInput::new);
        input.update(cx, |input, cx| {
            input.set_placeholder_with(
                || fl!("shortcut-editor-placeholder", example = "ctrl-shift-t"),
                cx,
            );
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
            input.set_text_selected(&keys, cx);
            window.focus(&input.focus, cx);
        });
        self.menu.shortcut_editor = Some(Editor {
            info,
            input,
            saving: false,
            error: None,
        });
        self.menu.keybinds_scroll.set_offset(Default::default());
        cx.notify();
    }

    fn cancel_shortcut_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu.shortcut_editor = None;
        if let Some(search) = &self.menu.keybinds_search {
            window.focus(&search.read(cx).focus.clone(), cx);
        }
        cx.notify();
    }

    pub(super) fn shortcut_editor_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(editor) = self
            .menu
            .shortcut_editor
            .as_ref()
            .filter(|_| self.menu.page == Some(Page::Keybinds))
        else {
            return false;
        };
        let input = editor.input.read(cx);
        if !input.focus.is_focused(window) {
            return false;
        }
        if input.is_composing() {
            return true;
        }
        match event.keystroke.key.as_str() {
            "escape" => self.cancel_shortcut_edit(window, cx),
            "enter" => self.save_shortcut_edit(Edit::Save, window, cx),
            _ => return true, // The field owns typing and native text editing.
        }
        cx.stop_propagation();
        window.prevent_default();
        true
    }

    fn save_shortcut_edit(&mut self, edit: Edit, window: &mut Window, cx: &mut Context<Self>) {
        self.save_shortcut_with(edit, Config::save_keybinding, window, cx);
    }

    fn save_shortcut_with(
        &mut self,
        edit: Edit,
        save: impl FnOnce(&'static str, Option<Binding>) -> crate::Result<()> + Send + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self
            .menu
            .shortcut_editor
            .as_mut()
            .filter(|editor| !editor.saving)
        else {
            return;
        };
        if editor.input.read(cx).is_composing() {
            return;
        }
        let binding = match edit {
            Edit::Reset => None,
            Edit::Unbind => Some(Binding::Many(Vec::new())),
            Edit::Save => Some(Binding::Many(
                editor
                    .input
                    .read(cx)
                    .text()
                    .split_whitespace()
                    .take(9) // The keymap rejects more than eight aliases.
                    .map(str::to_owned)
                    .collect(),
            )),
        };
        let name = editor.info.name;
        let mut overrides = self.config.keybinding_overrides.clone();
        if let Some(binding) = &binding {
            overrides.insert(name.into(), binding.clone());
        } else {
            overrides.remove(name);
        }
        if let Err(error) =
            Keymap::with_overrides(&overrides, &self.config.pane_keys, &DaemonKeys::default())
        {
            editor.error = Some(error.to_string());
            cx.notify();
            return;
        }
        editor.error = None;
        editor.saving = true;
        let input = editor.input.clone();
        let saved = cx
            .background_executor()
            .spawn(async move { save(name, binding) });
        cx.spawn_in(window, async move |this, cx| {
            let result = saved.await;
            let _ = this.update_in(cx, |this, window, cx| {
                let current = this
                    .menu
                    .shortcut_editor
                    .as_ref()
                    .is_some_and(|editor| editor.input == input);
                match result {
                    Ok(()) => {
                        if current {
                            this.cancel_shortcut_edit(window, cx);
                        }
                        this.load_gui_config(cx);
                    }
                    Err(error) => {
                        if current && let Some(editor) = &mut this.menu.shortcut_editor {
                            editor.saving = false;
                            editor.error = Some(error.to_string());
                        } else {
                            this.local_error = Some(fl!(
                                "shortcut-editor-save-failed",
                                error = error.to_string()
                            ));
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn render_shortcut_editor(&self, cx: &mut Context<Self>) -> Option<Div> {
        let editor = self.menu.shortcut_editor.as_ref()?;
        let theme = &self.theme;
        let buttons = [
            ("shortcut-save", fl!("shortcut-editor-save"), Edit::Save),
            (
                "shortcut-unbind",
                fl!("shortcut-editor-unbind"),
                Edit::Unbind,
            ),
            ("shortcut-reset", fl!("shortcut-editor-reset"), Edit::Reset),
        ]
        .into_iter()
        .map(|(id, label, edit)| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .px(px(8.))
                .py(px(4.))
                .rounded(px(crate::config::corners::CONTROL))
                .bg(rgb(theme.active))
                .when(!editor.saving, |button| {
                    button
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.save_shortcut_edit(edit, window, cx);
                        }))
                })
                .child(label)
        });
        let panel = div()
            .debug_selector(|| "shortcut-editor".into())
            .flex_none()
            .py(px(8.))
            .border_b_1()
            .border_color(rgb(theme.active))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(fl!(
                "shortcut-editor-title",
                command = crate::i18n::command(editor.info)
            )))
            .child(editor.input.clone())
            .child(
                div()
                    .text_color(rgb(theme.muted))
                    .child(fl!("shortcut-editor-hint")),
            )
            .child(
                div()
                    .text_color(rgb(theme.muted))
                    .child(fl!("shortcut-editor-note")),
            )
            .when_some(editor.error.clone(), |panel, error| {
                panel.child(
                    div()
                        .debug_selector(|| "shortcut-error".into())
                        .text_color(super::danger(theme))
                        .child(error),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .mt(px(6.))
                    .children(buttons)
                    .child(
                        div()
                            .id("shortcut-cancel")
                            .px(px(8.))
                            .py(px(4.))
                            .cursor_pointer()
                            .child(fl!("shortcut-editor-cancel"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.cancel_shortcut_edit(window, cx)
                            })),
                    )
                    .when(editor.saving, |row| {
                        row.child(fl!("shortcut-editor-saving"))
                    }),
            );
        Some(panel)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

#[cfg(all(feature = "integration-test", target_os = "linux"))]
mod native;
#[cfg(all(feature = "integration-test", target_os = "linux"))]
pub(crate) use native::verify as verify_native;
