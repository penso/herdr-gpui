//! Semantic input on its way to a pane or popup: keystrokes, paste, and wheel
//! deltas. Nothing here synthesizes terminal keys for an operation that has an
//! endpoint API method, and nothing is sent while a menu page holds input.

use super::HerdrWindow;
use crate::{
    connection::ConnectionBridge,
    terminal::{InputTarget, WheelAccumulator, key_input, wheel_target},
};
use gpui_kit::{Context, KeyDownEvent, ScrollWheelEvent, Window};
use herdr_client::protocol::ClientPaneInputEvent;

impl HerdrWindow {
    pub(crate) fn open_terminal_link(
        &mut self,
        event: &gpui_kit::ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pressed = self.pressed_terminal_link.take();
        let gpui_kit::ClickEvent::Mouse(event) = event else {
            return;
        };
        if event.down.button != gpui_kit::MouseButton::Left
            || event.down.click_count != 1
            || (event.up.position.x - event.down.position.x).abs() > gpui_kit::px(4.)
            || (event.up.position.y - event.down.position.y).abs() > gpui_kit::px(4.)
        {
            return;
        }
        if let Some(url) = self.terminal_link_at(event.up.position)
            && pressed
                .as_ref()
                .is_some_and(|(destination, _)| destination == &url)
        {
            cx.stop_propagation();
            cx.open_url(&url);
        }
    }

    /// Whether the modifiers keep a press here from a mouse-reporting
    /// application. Shift keeps any gesture local; the platform link modifier
    /// (cmd on macOS, ctrl elsewhere) claims only a press on a link, so the
    /// application still receives it everywhere else.
    pub(crate) fn link_modifier_held(
        &self,
        position: gpui_kit::Point<gpui_kit::Pixels>,
        modifiers: gpui_kit::Modifiers,
    ) -> bool {
        modifiers.shift || (modifiers.secondary() && self.terminal_link_at(position).is_some())
    }

    /// Whether a left click here would open a link, which the pointer shows.
    pub(crate) fn terminal_link_hovered(
        &self,
        position: gpui_kit::Point<gpui_kit::Pixels>,
        modifiers: gpui_kit::Modifiers,
    ) -> bool {
        self.terminal_link_at(position).is_some()
            && (modifiers.secondary()
                || modifiers.shift
                || self
                    .terminal_mouse_at(position)
                    .is_none_or(|hit| !hit.mouse_reporting))
    }

    pub(crate) fn terminal_link_at(
        &self,
        position: gpui_kit::Point<gpui_kit::Pixels>,
    ) -> Option<String> {
        if self.menu.page.is_some()
            || !self.live.surface_ready()
            || !self.bounds.contains(&position)
        {
            return None;
        }
        crate::terminal::link_at(
            self.live.surface.as_deref()?,
            f32::from(position.x - self.bounds.origin.x),
            f32::from(position.y - self.bounds.origin.y),
            self.cell_width,
            self.config.terminal.line_height(),
        )
    }

    pub(crate) fn send(&mut self, event: ClientPaneInputEvent, cx: &mut Context<Self>) {
        if self.menu.page.is_some() || !self.input_ready() || self.mouse_focus_pending() {
            return;
        }
        if let (Some(handle), Some(snapshot), Some(surface)) = (
            &self.endpoints[self.selected_endpoint].connection.handle,
            &self.live.snapshot,
            &self.live.surface,
        ) {
            let target = if let Some(popup) = &surface.popup {
                InputTarget::Popup(popup.terminal_id.clone())
            } else if let Some(pane) = &snapshot.focused_pane_id {
                InputTarget::Pane(pane.clone())
            } else {
                return;
            };
            if let Err(error) =
                ConnectionBridge::send_input(handle, &snapshot.boot_id, &target, event)
            {
                self.local_error = Some(format!("Input not sent: {error}"));
                cx.notify();
            }
        }
    }

    pub(crate) fn scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu.page.is_some() || !self.input_ready() {
            return;
        }
        let (Some(handle), Some(snapshot), Some(surface)) = (
            &self.endpoints[self.selected_endpoint].connection.handle,
            &self.live.snapshot,
            &self.live.surface,
        ) else {
            return;
        };
        let x = (event.position.x - self.bounds.origin.x).to_f64() as f32;
        let y = (event.position.y - self.bounds.origin.y).to_f64() as f32;
        let cell_height = self.config.terminal.line_height();
        let Some(target) = wheel_target(surface, x, y, self.cell_width, cell_height) else {
            self.wheel = WheelAccumulator::default();
            return;
        };
        let lines = self.wheel.lines(&target.target, event, cell_height);
        cx.stop_propagation();
        if lines == 0 {
            return;
        }
        let input = target.event(lines, event.modifiers);
        let result = ConnectionBridge::send_input(handle, &snapshot.boot_id, &target.target, input);
        if let Err(error) = result {
            self.local_error = Some(format!("Wheel input not sent: {error}"));
            cx.notify();
        }
    }

    /// Cmd-V and Edit > Paste into the focused pane or popup.
    pub(crate) fn paste(&mut self, cx: &mut Context<Self>) {
        // GPUI has no text-only Linux clipboard API. Preserve its native
        // ordinary paste (which needs no helper executable); explicit Ctrl-V
        // image acquisition still uses the bounded background reader.
        if self.accepts_clipboard_images() && !cfg!(target_os = "linux") {
            self.paste_native_clipboard(false, None, cx);
        } else if let Some(item) = cx.read_from_clipboard() {
            self.paste_terminal_clipboard(item, false, cx);
        }
    }

    pub(crate) fn key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(feature = "integration-test")]
        {
            self.input_probe.keys += 1;
        }
        let alt_keys = self
            .config
            .option_as_alt
            .sends_alt(cx.keyboard_layout().id());
        if event.keystroke.key == "escape" && self.cancel_workspace_drag(cx) {
            cx.stop_propagation();
            window.prevent_default();
        } else if event.keystroke.modifiers.platform && event.keystroke.key == "v" {
            self.paste(cx);
            cx.stop_propagation();
            window.prevent_default();
        } else if event.keystroke.key == "v"
            && event.keystroke.modifiers.control
            && !event.keystroke.modifiers.alt
            && !event.keystroke.modifiers.shift
            // Local agents read the shared clipboard themselves on Ctrl-V.
            && self.accepts_remote_images()
        {
            self.paste_native_clipboard(true, key_input(event, alt_keys), cx);
            cx.stop_propagation();
            window.prevent_default();
        } else if self.marked.is_empty()
            && let Some(input) = key_input(event, alt_keys)
        {
            self.send(input, cx);
            cx.stop_propagation();
            window.prevent_default();
        }
    }
}
