//! Runs only in the existing isolated, daemon-free native menu fixture.
use super::*;
use anyhow::{Result, ensure};
use gpui::{AsyncApp, WindowHandle, size};
use std::time::{Duration, Instant};

pub(crate) async fn verify(handle: WindowHandle<SheprdWindow>, cx: &mut AsyncApp) -> Result<()> {
    let info = crate::controls::COMMANDS
        .iter()
        .find(|info| info.name == "new_tab")
        .ok_or_else(|| anyhow::anyhow!("missing New Tab command"))?;
    for width in [900., 360.] {
        handle.update(cx, |view, window, cx| {
            view.dismiss_menu(window, cx);
            window.resize(size(px(width), px(600.)));
            view.open_keybinds(window, cx);
            view.edit_shortcut(info, window, cx);
        })?;
        cx.background_executor()
            .timer(Duration::from_millis(100))
            .await;
        handle.update(cx, |view, window, cx| -> Result<()> {
            let editor = view
                .menu
                .shortcut_editor
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("missing editor"))?;
            ensure!(
                editor.input.read(cx).focus.is_focused(window),
                "editor lost focus"
            );
            ensure!(
                editor.input.read(cx).text() == "ctrl-shift-t",
                "wrong Linux default"
            );
            ensure!(
                matches!(view.menu.cover.get(), crate::menu::Cover::All),
                "shortcut panel not painted"
            );
            Ok(())
        })??;
        for (edit, expected) in [
            (Edit::Save, "ctrl-shift-y"),
            (Edit::Unbind, ""),
            (Edit::Reset, "ctrl-shift-t"),
        ] {
            handle.update(cx, |view, window, cx| {
                view.edit_shortcut(info, window, cx);
                if let Some(editor) = &view.menu.shortcut_editor {
                    editor
                        .input
                        .update(cx, |input, cx| input.set_text_selected("ctrl-shift-y", cx));
                }
                view.save_shortcut_edit(edit, window, cx);
            })?;
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let done = handle.update(cx, |view, _, _| -> Result<bool> {
                    if let Some(error) = view
                        .menu
                        .shortcut_editor
                        .as_ref()
                        .and_then(|editor| editor.error.as_ref())
                    {
                        anyhow::bail!("save failed: {error}");
                    }
                    Ok(view.menu.shortcut_editor.is_none()
                        && view.keymap().primary(info.command) == expected)
                })??;
                if done {
                    break;
                }
                ensure!(
                    Instant::now() < deadline,
                    "shortcut save/reload did not complete"
                );
                cx.background_executor()
                    .timer(Duration::from_millis(20))
                    .await;
            }
        }
    }
    handle.update(cx, |view, window, cx| view.dismiss_menu(window, cx))?;
    Ok(())
}
