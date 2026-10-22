//! Semantic input on its way to a pane or popup: keystrokes, paste, and wheel
//! deltas. Nothing here synthesizes terminal keys for an operation that has an
//! endpoint API method, and nothing is sent while a menu page holds input.

use super::HerdrWindow;
use crate::{
    connection::ConnectionBridge,
    terminal::{
        InputTarget, WheelAccumulator, cursor_offset, key_input, pane_key_input, wheel_rows,
        wheel_target,
    },
};
use gpui::{
    Bounds, Context, KeyDownEvent, KeyUpEvent, Pixels, Point, ScrollWheelEvent, Window, point, px,
    size,
};

impl HerdrWindow {
    pub(crate) fn open_terminal_link(
        &mut self,
        event: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pressed = self.pressed_terminal_link.take();
        let gpui::ClickEvent::Mouse(event) = event else {
            return;
        };
        if event.down.button != gpui::MouseButton::Left
            || event.down.click_count != 1
            || (event.up.position.x - event.down.position.x).abs() > px(4.)
            || (event.up.position.y - event.down.position.y).abs() > px(4.)
        {
            return;
        }
        let Some(pressed) = pressed else {
            return;
        };
        let in_tab = (self.config.open_links_in == crate::config::LinkTarget::BrowserTab)
            != event.down.modifiers.alt;
        if self.activate_terminal_link(
            &pressed,
            event.up.position,
            event.down.modifiers,
            in_tab,
            window,
            cx,
        ) {
            cx.stop_propagation();
        }
    }

    /// Whether the modifiers keep a press here from a mouse-reporting
    /// application. Shift keeps any gesture local, for selecting; the platform
    /// link modifier (cmd on macOS, ctrl elsewhere) claims only a press on a
    /// verified link, so the application still receives it everywhere else,
    /// a printed path still being looked for on disk included.
    pub(crate) fn link_modifier_held(
        &self,
        position: Point<Pixels>,
        modifiers: gpui::Modifiers,
    ) -> bool {
        modifiers.shift
            || (modifiers.secondary()
                && (self.terminal_link_at(position).is_some()
                    || self.daemon_link_at(position).is_some()
                    || self
                        .terminal_link_press(position, modifiers)
                        .is_some_and(|press| self.verified_press(&press))))
    }

    /// Whether the pointer shows a link here: only while the link modifier is
    /// held, when a click would act on what is under it. That is every
    /// explicit hyperlink, which says it is a link, and any other text once
    /// it is verified as one.
    pub(crate) fn terminal_link_hovered(
        &self,
        position: Point<Pixels>,
        modifiers: gpui::Modifiers,
    ) -> bool {
        modifiers.secondary()
            && (self.terminal_link_at(position).is_some()
                || self.daemon_link_at(position).is_some()
                || self.terminal_hyperlink_at(position)
                || (self.hovered_local_link().is_some() && self.local_link_at(position).is_some()))
    }

    /// Whether wheel scrolling may draw panes between rows. Off in settings,
    /// it never does. Images paint with the whole grid, which never slides,
    /// a dragged thumb places the content exactly where the pointer put it,
    /// and a popup stays on its rows.
    pub(crate) fn slides_allowed(&self) -> bool {
        self.config.smooth_scroll
            && self.scrollbar_drag.is_none()
            && (self.live.surface.as_ref()).is_none_or(|surface| {
                surface.graphics.placements.is_empty() && surface.popup.is_none()
            })
    }

    /// The pane drawn mid-row after a wheel scroll, in pixels from the grid's
    /// origin, and how far below its grid position its content is drawn.
    fn scroll_shift(&self) -> Option<(Bounds<Pixels>, Pixels)> {
        let (rect, offset) = self.presentation.scroll.offset()?;
        let (cell_width, cell_height) = (self.cell_width, self.config.terminal.line_height());
        let pane = Bounds::new(
            point(
                px(f32::from(rect.x) * cell_width),
                px(f32::from(rect.y) * cell_height),
            ),
            size(
                px(f32::from(rect.width) * cell_width),
                px(f32::from(rect.height) * cell_height),
            ),
        );
        Some((pane, px(offset * cell_height)))
    }

    /// How far below its grid position the content at grid pixel `cell` is
    /// drawn, with the pane that clips it.
    pub(crate) fn shift_at(&self, cell: Point<Pixels>) -> Option<(Bounds<Pixels>, Pixels)> {
        self.scroll_shift().filter(|(pane, _)| pane.contains(&cell))
    }

    /// `position` in the terminal grid's pixels where the content drawn there
    /// sits, and whether the live surface holds it: the sliver uncovered at a
    /// pane's edge shows a row from an earlier frame, mapped to the edge row.
    fn drawn_at(&self, position: Point<Pixels>) -> ((f32, f32), bool) {
        let mut grid = position - self.bounds.origin;
        let mut held = true;
        if let Some((pane, shift)) = self.shift_at(grid) {
            grid.y -= shift;
            held = pane.contains(&grid);
            grid.y = grid.y.clamp(pane.top(), pane.bottom() - px(1.));
        }
        ((f32::from(grid.x), f32::from(grid.y)), held)
    }

    /// `position` in the terminal grid's pixels, for clicks and selection.
    pub(crate) fn grid_position(&self, position: Point<Pixels>) -> (f32, f32) {
        self.drawn_at(position).0
    }

    /// `position` in the terminal grid's pixels, or `None` over a row the live
    /// surface does not hold, so links never resolve against another row.
    pub(crate) fn drawn_position(&self, position: Point<Pixels>) -> Option<(f32, f32)> {
        let (grid, held) = self.drawn_at(position);
        held.then_some(grid)
    }

    /// How far below its grid cell the input cursor is drawn, which an IME
    /// composition and its candidate window follow.
    pub(crate) fn ime_shift(&self) -> Pixels {
        let Some(cursor) = (self.live.surface.as_deref()).and_then(|s| s.frame.cursor.as_ref())
        else {
            return px(0.);
        };
        let cell = cursor_offset(cursor, self.cell_width, self.config.terminal.line_height());
        self.shift_at(cell).map_or(px(0.), |(_, shift)| shift)
    }

    pub(crate) fn terminal_link_at(&self, position: Point<Pixels>) -> Option<String> {
        if self.menu.page.is_some()
            || !self.live.surface_ready()
            || !self.bounds.contains(&position)
        {
            return None;
        }
        let (x, y) = self.drawn_position(position)?;
        crate::terminal::link_at(
            self.live.surface.as_deref()?,
            x,
            y,
            self.cell_width,
            self.config.terminal.line_height(),
        )
    }

    /// Herdr's shared `ui.mouse_scroll_lines`, its default until the shared
    /// config loads.
    fn mouse_scroll_lines(&self) -> std::num::NonZeroU16 {
        self.settings.shared.as_ref().map_or(
            crate::herdr_settings::DEFAULT_MOUSE_SCROLL_LINES,
            |shared| shared.mouse_scroll_lines,
        )
    }

    pub(crate) fn scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.shift_taps.cancel();
        if self.menu.page.is_some() {
            return;
        }
        // A wheel over the panes is use even before the frame is ready, so the
        // window takes back a frame another client sized.
        self.note_input();
        if !self.input_ready() {
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
        let notch_lines = self.mouse_scroll_lines();
        let mut steps = self
            .wheel
            .steps(&target, event, self.cell_width, cell_height, notch_lines);
        // Scrollback follows the OS's motion exactly; anything else, such as an
        // application reading the wheel, gets whole steps as they accumulate.
        let smooth = match &target.target {
            InputTarget::Pane(id) if self.slides_allowed() => self
                .presentation
                .wheel(id, wheel_rows(event, cell_height, notch_lines)),
            InputTarget::Pane(_) | InputTarget::Popup(_) => None,
        };
        if let Some(lines) = smooth {
            steps.lines = lines;
            // The motion is spent here; the whole-line path must not count it again.
            self.wheel.drop_lines();
        }
        cx.stop_propagation();
        for input in target.wheel_events(steps, event.modifiers) {
            let result =
                ConnectionBridge::send_input(handle, &snapshot.boot_id, &target.target, input);
            if let Err(error) = result {
                self.local_error = Some(format!("Wheel input not sent: {error}"));
                // The rows asked for never left, so nothing may wait on them.
                self.presentation.scroll.clear();
                cx.notify();
                break;
            }
        }
        if smooth.is_some() {
            // The drawing moves with every delta, before any surface lands.
            self.redraw_terminal(cx);
        }
    }

    /// Cmd-V and Edit > Paste into the focused pane or popup.
    pub(crate) fn paste(&mut self, cx: &mut Context<Self>) {
        if self.copy_mode_active() {
            return;
        }
        self.paste_or_hold(false, None, cx);
    }

    pub(crate) fn key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A remapped function key has its name from here on, so the pane key
        // looked up, the key sent, and the key held for its release agree.
        let named =
            super::function_keys::named(&event.keystroke, self.key_press).map(|keystroke| {
                KeyDownEvent {
                    keystroke,
                    ..event.clone()
                }
            });
        let event = named.as_ref().unwrap_or(event);
        // A new press goes wherever this decides; only the pane branch at the
        // end holds it again for its release.
        self.held_keys.forget(&event.keystroke.key);
        // A keystroke bubbling out of the find field, the terminal note field,
        // or the copy-mode search prompt is the field's: an unhandled one is
        // still on its way to the field's IME.
        if self.find_focused(window, cx)
            || self.terminal_note_focused(window, cx)
            || self.copy_search_focused(window, cx)
        {
            return;
        }
        // Copy mode owns the keyboard: its keys run and nothing else is typed.
        if self.copy_mode_active() {
            self.copy_mode_key(event, window, cx);
            if !event.keystroke.modifiers.platform {
                cx.stop_propagation();
                window.prevent_default();
            }
            return;
        }
        #[cfg(feature = "integration-test")]
        {
            self.input_probe.keys += 1;
        }
        let modifiers = event.keystroke.modifiers;
        let option_keys = if modifiers.alt {
            crate::input::held_option_keys()
        } else {
            crate::config::OptionKeys::LEFT
        };
        let alt_keys = self
            .config
            .option_as_alt
            .sends_alt(cx.keyboard_layout().id(), option_keys);
        // Cmd-C copies a selection that is still highlighted. Ctrl-C does too
        // only while Herdr's `copy_on_select` is off, as in Herdr, where the
        // highlight is waiting for that copy; a selection the release already
        // copied must not stop Ctrl-C from interrupting the pane.
        let copy = if modifiers.platform {
            !modifiers.control && !modifiers.shift
        } else {
            modifiers.control && (modifiers.shift || !self.copy_on_select())
        };
        if event.keystroke.key.eq_ignore_ascii_case("c")
            && copy
            && !modifiers.alt
            && self.copy_retained_selection(cx)
        {
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        // Cmd-A, or Ctrl-Shift-A on Linux and Windows, where plain Ctrl-A
        // stays the pane's, selects the focused pane's history and screen.
        let select_all = if modifiers.platform {
            !modifiers.control && !modifiers.shift
        } else {
            crate::controls::CTRL_SHORTCUTS && modifiers.control && modifiers.shift
        };
        if event.keystroke.key.eq_ignore_ascii_case("a")
            && select_all
            && !modifiers.alt
            && self.select_all(cx)
        {
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        self.clear_retained_selection(cx);
        if event.keystroke.key == "escape"
            && (self.cancel_workspace_drag(cx)
                | self.cancel_device_drag(cx)
                | self.cancel_tab_drag(cx))
        {
            cx.stop_propagation();
            window.prevent_default();
        } else if (event.keystroke.modifiers.platform
            || (event.keystroke.modifiers.control && event.keystroke.modifiers.shift)
            || (event.keystroke.modifiers.shift && event.keystroke.key == "insert"))
            && (event.keystroke.key.eq_ignore_ascii_case("v") || event.keystroke.key == "insert")
        {
            self.paste(cx);
            cx.stop_propagation();
            window.prevent_default();
        } else if event.keystroke.key == "v"
            && event.keystroke.modifiers.control
            && !event.keystroke.modifiers.alt
            && !event.keystroke.modifiers.shift
            // Local agents read the shared clipboard themselves on Ctrl-V. A
            // remote pane's Ctrl-V waits out a projection gap as a paste.
            && self.selected_is_remote()
            && (self.accepts_remote_images() || !self.input_ready())
        {
            self.paste_or_hold(true, key_input(event, alt_keys), cx);
            cx.stop_propagation();
            window.prevent_default();
        } else if self.marked.is_empty()
            && let Some(input) = match self.keymap().pane_key(&event.keystroke) {
                Some(sent) => pane_key_input(event, sent),
                None => key_input(event, alt_keys),
            }
        {
            let input =
                self.held_keys
                    .press(&event.keystroke.key, input, self.live.keyboard_report_all);
            self.send(input, cx);
            cx.stop_propagation();
            window.prevent_default();
        }
    }

    /// Releases a key the pane received while Herdr reported that its focused
    /// pane wants every key event, as the TUI does by switching its outer
    /// terminal to report all keys. Text stays with the input handler, so
    /// only keys sent as key events are released.
    pub(crate) fn key_up(&mut self, event: &KeyUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        // Held under the name `key_down` gave it.
        let named = super::function_keys::named(&event.keystroke, self.key_press);
        let key = &named.as_ref().unwrap_or(&event.keystroke).key;
        let Some(release) = self.held_keys.release(key) else {
            return;
        };
        if self.live.keyboard_report_all && self.marked.is_empty() {
            self.send(release, cx);
            cx.stop_propagation();
        }
    }
}
