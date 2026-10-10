//! Opt-in checks on a real Linux window, driven through GPUI's event dispatch.
//! This exercises native layout/painting, not compositor-delivered input or IME.

use super::*;
use anyhow::{Context as _, Result, ensure};
use std::time::Duration;

async fn settle(cx: &mut AsyncApp) {
    cx.background_executor()
        .timer(Duration::from_millis(100))
        .await;
}

fn key(handle: WindowHandle<HerdrWindow>, key: &str, cx: &mut AsyncApp) -> Result<()> {
    AnyWindowHandle::from(handle).update(cx, |_, window, cx| {
        let event = KeyDownEvent {
            keystroke: Keystroke::parse(key)?,
            is_held: false,
            prefer_character_input: false,
        };
        window.dispatch_event(PlatformInput::KeyDown(event), cx);
        Ok(())
    })?
}

pub(crate) async fn run(handle: WindowHandle<HerdrWindow>, cx: &mut AsyncApp) -> Result<()> {
    for width in [1200., 360.] {
        handle.update(cx, |view, window, cx| {
            view.dismiss_menu(window, cx);
            view.sidebar_visible = false;
            window.resize(size(px(width), px(500.)));
            view.focus.focus(window, cx);
            cx.notify();
        })?;
        settle(cx).await;
        let button = handle.update(cx, |view, window, _| -> Result<_> {
            let bar = view.menu.application_bar.bar.get();
            ensure!(bar.size.width > px(0.), "menu bar was not painted");
            ensure!(
                bar.right() <= window.viewport_size().width,
                "menu bar overflow"
            );
            let buttons = view.menu.application_bar.buttons.borrow();
            ensure!(
                buttons.len()
                    == if width < 560. {
                        1
                    } else {
                        super::super::menus(view.config.layout).len()
                    }
            );
            for button in buttons.iter() {
                ensure!(
                    button.left() >= bar.left() && button.right() <= bar.right(),
                    "heading clipped"
                );
            }
            buttons.first().copied().context("menu button missing")
        })??;
        AnyWindowHandle::from(handle).update(cx, |_, window, cx| {
            let position = button.center();
            window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    position,
                    button: MouseButton::Left,
                    modifiers: Modifiers::none(),
                    click_count: 1,
                    first_mouse: false,
                }),
                cx,
            );
            window.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    position,
                    button: MouseButton::Left,
                    modifiers: Modifiers::none(),
                    click_count: 1,
                }),
                cx,
            );
        })?;
        settle(cx).await;
        if width < 560. {
            key(handle, "enter", cx)?;
            settle(cx).await;
        }
        handle.update(cx, |view, window, _| -> Result<()> {
            ensure!(
                view.menu.page == Some(Page::Application),
                "button did not open menu"
            );
            ensure!(view.menu.focus.is_focused(window), "menu lost focus");
            let crate::menu::Cover::Panel(panel) = view.menu.cover.get() else {
                anyhow::bail!("menu did not paint")
            };
            // The cover includes the eight-pixel shadow margin.
            ensure!(
                panel.right() <= window.viewport_size().width + px(8.),
                "dropdown horizontal overflow: {panel:?}, viewport {:?}, bar {:?}",
                window.viewport_size(),
                view.menu.application_bar.bar.get()
            );
            ensure!(
                panel.bottom() <= window.viewport_size().height + px(8.),
                "dropdown vertical overflow: {panel:?}, viewport {:?}, bar {:?}",
                window.viewport_size(),
                view.menu.application_bar.bar.get()
            );
            Ok(())
        })??;
        let before = handle.update(cx, |view, _, _| view.input_probe)?;
        key(handle, "a", cx)?;
        key(handle, "down", cx)?;
        key(handle, "up", cx)?;
        key(handle, "enter", cx)?; // About, from the shared native menu action.
        settle(cx).await;
        handle.update(cx, |view, window, cx| -> Result<()> {
            ensure!(
                view.menu.page == Some(Page::About),
                "menu action did not dispatch"
            );
            ensure!(
                view.input_probe.text == before.text && view.input_probe.keys == before.keys,
                "menu leaked terminal input"
            );
            view.dismiss_menu(window, cx);
            Ok(())
        })??;
        settle(cx).await;
        key(handle, "f10", cx)?;
        settle(cx).await;
        key(handle, "escape", cx)?;
        handle.update(cx, |view, window, _| -> Result<()> {
            ensure!(
                view.menu.page.is_none() && view.focus.is_focused(window),
                "dismissal lost focus"
            );
            Ok(())
        })??;
    }
    Ok(())
}
