//! Application mouse gestures take precedence over local selection and links.
//! Shift keeps a gesture local; a forwarded drag stays with its pressed target.
//! A pane's right-click reaches the application only when Herdr routes it there,
//! or when it is held with Herdr's `ui.right_click_passthrough_modifier`.

use super::HerdrWindow;
use crate::{
    connection::ConnectionBridge,
    navigation::NavigationTarget,
    terminal::{InputTarget, Scrollbar, WheelTarget, splits, wheel_target},
};
use gpui::{
    Context, CursorStyle, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, Window,
};
use herdr_client::{
    Method,
    protocol::{ClientMouseButton, ClientMouseKind, PaneSurfaceSplit},
};
use serde_json::json;

pub(crate) struct Gesture {
    hit: WheelTarget,
    button: MouseButton,
    /// The application was sent motion to another cell: the press became a
    /// drag, which may have been meant as a selection.
    dragged: bool,
    /// Modifiers the application never sees for this gesture: the
    /// passthrough combination that sent a right-click to it.
    strip: Modifiers,
    boot: String,
    epoch: u64,
    generation: u64,
}

/// A drag of a pane's native scrollbar. `grab` keeps the pointer's place on
/// the thumb; `want` is the latest offset not yet sent to the daemon.
pub(crate) struct ScrollbarDrag {
    pane: String,
    boot: String,
    grab: f32,
    held: bool,
    want: Option<u64>,
}

/// A drag of the border between panes. The split is kept as pressed: its
/// area, which the ratio divides, does not move with its own border. `grab`
/// keeps the pointer's place on the border; `want` is the latest ratio not yet
/// sent to the daemon.
pub(crate) struct SplitDrag {
    split: PaneSurfaceSplit,
    boot: String,
    tab: String,
    topology: u64,
    grab: f32,
    held: bool,
    want: Option<f32>,
    sent: Option<f32>,
}

/// `held` without the modifiers in `strip`.
fn without(held: Modifiers, strip: Modifiers) -> Modifiers {
    Modifiers {
        control: held.control && !strip.control,
        alt: held.alt && !strip.alt,
        shift: held.shift && !strip.shift,
        platform: held.platform && !strip.platform,
        function: held.function && !strip.function,
    }
}

fn button(button: MouseButton) -> Option<ClientMouseButton> {
    match button {
        MouseButton::Left => Some(ClientMouseButton::Left),
        MouseButton::Right => Some(ClientMouseButton::Right),
        MouseButton::Middle => Some(ClientMouseButton::Middle),
        _ => None,
    }
}

impl HerdrWindow {
    pub(crate) fn mouse_focus_pending(&self) -> bool {
        self.terminal_mouse.as_ref().is_some_and(|gesture| {
            matches!(&gesture.hit.target, InputTarget::Pane(id)
                if self.live.snapshot.as_ref().and_then(|snapshot| snapshot.focused_pane_id.as_ref()) != Some(id))
        })
    }

    /// Retire only an input already sent to this connection. This is cleanup,
    /// not new input: a menu or resize must not leave the application dragging.
    pub(crate) fn cancel_terminal_mouse(&mut self, cx: &mut Context<Self>) {
        let Some(gesture) = self.terminal_mouse.take() else {
            return;
        };
        if gesture.epoch != self.selection_epoch
            || gesture.generation != self.selected_generation
            || self
                .live
                .snapshot
                .as_ref()
                .is_none_or(|snapshot| snapshot.boot_id != gesture.boot)
        {
            return;
        }
        if let Some(handle) = &self.endpoints[self.selected_endpoint].connection.handle
            && let Some(button) = button(gesture.button)
            && let Err(error) = ConnectionBridge::send_input(
                handle,
                &gesture.boot,
                &gesture.hit.target,
                gesture
                    .hit
                    .mouse_event(ClientMouseKind::Up(button), Modifiers::default()),
            )
        {
            self.local_error = Some(format!("Mouse release not sent: {error}"));
            cx.notify();
        }
    }

    pub(crate) fn terminal_mouse_at(&self, position: Point<Pixels>) -> Option<WheelTarget> {
        if self.menu.page.is_some()
            || !self.live.surface_ready()
            || !self.bounds.contains(&position)
        {
            return None;
        }
        let (x, y) = self.grid_position(position);
        wheel_target(
            self.live.surface.as_deref()?,
            x,
            y,
            self.cell_width,
            self.config.terminal.line_height(),
        )
    }

    fn send_mouse(
        &mut self,
        hit: &WheelTarget,
        kind: ClientMouseKind,
        modifiers: Modifiers,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.menu.page.is_some() || !self.input_ready() {
            return false;
        }
        let (Some(handle), Some(snapshot)) = (
            &self.endpoints[self.selected_endpoint].connection.handle,
            &self.live.snapshot,
        ) else {
            return false;
        };
        // `handle` borrows the window, so the field is set directly. Hover
        // motion is not counted here: `note_pointer` counts it only when the pointer moved.
        if kind != ClientMouseKind::Moved {
            self.last_input = Some(std::time::Instant::now());
        }
        match ConnectionBridge::send_input(
            handle,
            &snapshot.boot_id,
            &hit.target,
            hit.mouse_event(kind, modifiers),
        ) {
            Ok(()) => true,
            Err(error) => {
                self.local_error = Some(format!("Mouse input not sent: {error}"));
                cx.notify();
                false
            }
        }
    }

    /// The modifiers a right-click loses on its way to the application, or
    /// `None` when it opens the pane menu instead. A pane's right-click opens
    /// its menu unless Herdr routes that pane's right-clicks to the
    /// application, as every client of the daemon does; otherwise a modifier
    /// always reaches the menu, so a routed pane can be switched back. Holding
    /// exactly the configured `right_click_passthrough_modifier` sends it to
    /// the application without that modifier, as Herdr's TUI does. A popup
    /// has no menu, so a mouse-aware one keeps its right-clicks.
    fn right_click_to_application(
        &self,
        target: &InputTarget,
        modifiers: Modifiers,
    ) -> Option<Modifiers> {
        if self
            .settings
            .shared
            .as_ref()
            .is_some_and(|shared| shared.right_click_passthrough.matches(modifiers))
        {
            return Some(modifiers);
        }
        let InputTarget::Pane(id) = target else {
            return Some(Modifiers::default());
        };
        (!modifiers.modified()
            && self.live.snapshot.as_ref().is_some_and(|snapshot| {
                snapshot
                    .panes
                    .iter()
                    .any(|pane| pane.pane_id == *id && pane.right_click_passthrough)
            }))
        .then_some(Modifiers::default())
    }

    /// Returns ownership, not queue success: an unavailable application must not
    /// turn a click into an unexpected clipboard write or browser launch.
    pub(crate) fn terminal_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        // A press over the panes is use even when the frame is not ready or
        // the pane ignores the mouse.
        self.note_input();
        self.cancel_terminal_mouse(cx);
        let Some((hit, strip)) = self
            .terminal_mouse_at(event.position)
            .filter(|hit| hit.mouse_reporting)
            // Shift keeps this press local, so its user needs no hint.
            .inspect(|_| {
                if event.button == MouseButton::Left && event.modifiers.shift {
                    super::select_hint::learned(cx);
                }
            })
            .and_then(|hit| {
                let strip = if event.button == MouseButton::Right {
                    self.right_click_to_application(&hit.target, event.modifiers)?
                } else {
                    Modifiers::default()
                };
                // The configured passthrough is decided before links, as in
                // Herdr's TUI: a right-click opens no link, so its modifier
                // may be the link modifier. It never includes Shift.
                (strip.modified() || !self.link_modifier_held(event.position, event.modifiers))
                    .then_some((hit, strip))
            })
        else {
            return false;
        };
        self.pressed_terminal_link = None;
        self.selection = None;
        window.focus(&self.focus, cx);
        cx.stop_propagation();
        if let Some(button) = button(event.button)
            && !cx.has_active_drag()
            && self.send_mouse(
                &hit,
                ClientMouseKind::Down(button),
                without(event.modifiers, strip),
                cx,
            )
            && let Some(snapshot) = &self.live.snapshot
        {
            self.terminal_mouse = Some(Gesture {
                hit,
                button: event.button,
                dragged: false,
                strip,
                boot: snapshot.boot_id.clone(),
                epoch: self.selection_epoch,
                generation: self.selected_generation,
            });
        }
        true
    }

    fn gesture_hit(&self, gesture: &Gesture, position: Point<Pixels>) -> Option<WheelTarget> {
        if self.menu.page.is_some()
            || !self.input_ready()
            || gesture.epoch != self.selection_epoch
            || gesture.generation != self.selected_generation
            || self.live.snapshot.as_ref()?.boot_id != gesture.boot
        {
            return None;
        }
        let bounds = gesture.hit.bounds;
        let local = position - self.bounds.origin;
        let x = f32::from(local.x).clamp(
            f32::from(bounds.left()),
            f32::from(bounds.right()).next_down(),
        );
        let y = f32::from(local.y).clamp(
            f32::from(bounds.top()),
            f32::from(bounds.bottom()).next_down(),
        );
        let hit = wheel_target(
            self.live.surface.as_deref()?,
            x,
            y,
            self.cell_width,
            self.config.terminal.line_height(),
        )?;
        (hit.target == gesture.hit.target && hit.bounds == bounds && hit.mouse_reporting)
            .then_some(hit)
    }

    pub(crate) fn terminal_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if cx.has_active_drag() {
            self.cancel_terminal_mouse(cx);
            self.selection = None;
            self.pressed_terminal_link = None;
            return false;
        }
        if let Some(gesture) = &self.terminal_mouse {
            if event.pressed_button != Some(gesture.button) {
                self.cancel_terminal_mouse(cx);
                return false;
            }
            let hit = self.gesture_hit(gesture, event.position);
            let waits = hit.is_none() && self.gesture_waits(gesture);
            let kind = button(gesture.button).map(ClientMouseKind::Drag);
            if let (Some(hit), Some(kind)) = (hit, kind) {
                // Like a terminal's button-event tracking, motion is reported
                // only when it reaches another cell, or another pixel for an
                // application that asked for pixels.
                if hit == gesture.hit {
                    return true;
                }
                let modifiers = without(event.modifiers, gesture.strip);
                // Pixel motion within one cell is forwarded but selects no
                // text, so only reaching another cell makes it a drag.
                let moved_cell = hit.cell() != gesture.hit.cell();
                if self.send_mouse(&hit, kind, modifiers, cx)
                    && let Some(gesture) = &mut self.terminal_mouse
                {
                    gesture.hit = hit;
                    gesture.dragged |= moved_cell;
                }
            } else if !waits {
                self.cancel_terminal_mouse(cx);
            }
            return true;
        }
        false
    }

    /// Whether a gesture's frame is only catching up: the surface for the
    /// latest snapshot has not arrived, or the frame is sized for another
    /// client. It is still the same press on the same connection, so the
    /// application keeps its button until the next coherent frame decides,
    /// rather than taking an early release as the end of its selection.
    fn gesture_waits(&self, gesture: &Gesture) -> bool {
        self.menu.page.is_none()
            && gesture.epoch == self.selection_epoch
            && gesture.generation == self.selected_generation
            && self
                .live
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.boot_id == gesture.boot)
            && (!self.live.surface_ready() || !self.surface_matches_options())
    }

    pub(crate) fn terminal_mouse_hover(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        // Hover belongs to the hit-tested element, unlike an owned drag.
        if event.pressed_button.is_none()
            && !cx.has_active_drag()
            && !event.modifiers.shift
            && let Some(hit) = self
                .terminal_mouse_at(event.position)
                .filter(|hit| hit.mouse_reporting)
        {
            self.send_mouse(&hit, ClientMouseKind::Moved, event.modifiers, cx);
        }
    }

    /// The pointer moved over the panes: updates the split and link cursors,
    /// reports hover to the pane's program and lets focus follow the mouse.
    pub(crate) fn pane_pointer_moved(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        // Pointing at a pane is use even when its program ignores the mouse.
        let moved = self.note_pointer(event.position);
        let split_cursor = self.split_cursor_at(event.position);
        if split_cursor != self.split_cursor {
            self.split_cursor = split_cursor;
            cx.notify();
        }
        // A border is not the application's to hover. A move event for a pointer that did not
        // move (Windows sends them) is not motion: no report, and focus does not follow it.
        if split_cursor.is_none() && moved {
            self.terminal_mouse_hover(event, cx);
            self.focus_hovered_pane(event, window, cx);
        }
        let hovered = self.terminal_link_hovered(event.position, event.modifiers);
        if hovered != self.hovered_terminal_link {
            self.hovered_terminal_link = hovered;
            cx.notify();
        }
    }

    pub(crate) fn terminal_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if cx.has_active_drag() {
            self.cancel_terminal_mouse(cx);
            self.selection = None;
            self.pressed_terminal_link = None;
            return false;
        }
        if self
            .terminal_mouse
            .as_ref()
            .is_none_or(|gesture| gesture.button != event.button)
        {
            return false;
        }
        let hit = self
            .terminal_mouse
            .as_ref()
            .and_then(|gesture| self.gesture_hit(gesture, event.position));
        let (strip, dragged) = self
            .terminal_mouse
            .as_ref()
            .map_or((Modifiers::default(), false), |gesture| {
                (gesture.strip, gesture.dragged)
            });
        if hit.is_none() {
            self.cancel_terminal_mouse(cx);
            return true;
        }
        self.terminal_mouse = None;
        if let Some(hit) = hit
            && let Some(button) = button(event.button)
            && self.send_mouse(
                &hit,
                ClientMouseKind::Up(button),
                without(event.modifiers, strip),
                cx,
            )
        {
            // A left drag the application took is where a selection was
            // most likely expected and did not happen.
            if dragged && event.button == MouseButton::Left {
                self.hint_shift_select(cx);
            }
            let InputTarget::Pane(id) = &hit.target else {
                return true;
            };
            // Finish targeted input before navigation fences the surface. This
            // also lets the first click in an inactive split reach its app.
            self.focus_clicked_pane(id, cx);
        }
        true
    }

    pub(crate) fn focus_clicked_pane(&mut self, id: &str, cx: &mut Context<Self>) {
        if self
            .live
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.focused_pane_id.as_deref())
            != Some(id)
        {
            self.navigate(NavigationTarget::Pane(id), cx);
        }
    }

    pub(crate) fn focus_hovered_pane(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !self.config.focus_follows_mouse
            || !window.is_window_active()
            || !self.focus.is_focused(window)
            || !self.input_ready()
            || event.pressed_button.is_some()
            || event.modifiers.modified()
            || cx.has_active_drag()
            || self.terminal_mouse.is_some()
            || self.scrollbar_drag.is_some()
            || self.split_drag.is_some()
            || !self.marked.is_empty()
            || self
                .selection
                .as_ref()
                .is_some_and(|selection| selection.dragging())
            || self.split_at(event.position).is_some()
        {
            return;
        }
        let Some(surface) = self.live.surface.as_deref() else {
            return;
        };
        // A popup owns input even when the pointer is over an uncovered pane.
        if surface.popup.is_some() || surface.panes.len() < 2 {
            return;
        }
        if let Some(hit) = self.terminal_mouse_at(event.position)
            && let InputTarget::Pane(id) = hit.target
        {
            // Reuse the click navigation fence: repeated motion cannot queue
            // more focus requests or send typing to the old pane while waiting.
            self.focus_clicked_pane(&id, cx);
        }
    }

    fn scrollbar(&self, pane: &str) -> Option<Scrollbar> {
        let pane = self
            .live
            .surface
            .as_deref()?
            .panes
            .iter()
            .find(|p| p.pane_id == pane)?;
        Scrollbar::new(pane, self.cell_width, self.config.terminal.line_height())
    }

    /// Grabs the thumb where pressed, or jumps it under the pointer when the
    /// press lands on the track.
    pub(crate) fn scrollbar_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.menu.page.is_some() || !self.input_ready() {
            return false;
        }
        let (Some(surface), Some(snapshot)) = (self.live.surface.as_deref(), &self.live.snapshot)
        else {
            return false;
        };
        if surface.popup.is_some() {
            return false;
        }
        let local = event.position - self.bounds.origin;
        let cell_height = self.config.terminal.line_height();
        let Some((pane, bar)) = surface.panes.iter().find_map(|pane| {
            Scrollbar::new(pane, self.cell_width, cell_height)
                .filter(|bar| bar.track.contains(&local))
                .map(|bar| (pane.pane_id.clone(), bar))
        }) else {
            return false;
        };
        let grab = if bar.thumb.contains(&local) {
            f32::from(local.y - bar.thumb.top())
        } else {
            f32::from(bar.thumb.size.height) / 2.
        };
        self.scrollbar_drag = Some(ScrollbarDrag {
            pane,
            boot: snapshot.boot_id.clone(),
            grab,
            held: true,
            want: None,
        });
        self.drag_scrollbar(event.position, cx);
        cx.stop_propagation();
        true
    }

    pub(crate) fn scrollbar_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(drag) = self.scrollbar_drag.as_mut().filter(|drag| drag.held) else {
            return false;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            drag.held = false;
            return false;
        }
        self.drag_scrollbar(event.position, cx);
        true
    }

    pub(crate) fn scrollbar_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(drag) = self
            .scrollbar_drag
            .as_mut()
            .filter(|drag| drag.held && event.button == MouseButton::Left)
        else {
            return false;
        };
        drag.held = false;
        self.flush_scrollbar(cx);
        true
    }

    fn drag_scrollbar(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(drag) = &self.scrollbar_drag else {
            return;
        };
        // A snapshot ahead of its surface leaves no bar for a moment: the drag
        // waits it out, and the next move (macOS repeats a held one) resumes.
        let Some(bar) = self.scrollbar(&drag.pane) else {
            return;
        };
        let top = f32::from(position.y - self.bounds.origin.y) - drag.grab;
        if let Some(drag) = &mut self.scrollbar_drag {
            drag.want = Some(bar.offset_at(top));
        }
        self.flush_scrollbar(cx);
    }

    /// Sends the latest wanted offset once the previous request has answered,
    /// and retires a released drag when nothing is left to send.
    pub(crate) fn flush_scrollbar(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = &mut self.scrollbar_drag else {
            return;
        };
        if self.live.drag_request.is_some() {
            return;
        }
        let Some(offset) = drag.want.take() else {
            if !drag.held {
                self.scrollbar_drag = None;
            }
            return;
        };
        let connection = &self.endpoints[self.selected_endpoint].connection;
        let (Some(handle), Some(snapshot)) = (&connection.handle, &self.live.snapshot) else {
            self.scrollbar_drag = None;
            return;
        };
        if snapshot.boot_id != drag.boot {
            self.scrollbar_drag = None;
            return;
        }
        // Registered under the inbox lock so the response cannot be applied
        // before the request is known, which would leave the drag waiting.
        // The UI thread never waits on the worker; a busy inbox retries next tick.
        let Ok(mut state) = connection.inbox.try_lock() else {
            drag.want = Some(offset);
            return;
        };
        match handle.request(
            &drag.boot,
            Method::PaneScroll,
            json!({"pane_id": drag.pane, "offset_from_bottom": offset}),
        ) {
            Ok(request) => {
                state.drag_request = Some(request.clone());
                self.live.drag_request = Some(request);
            }
            Err(error) => {
                drop(state);
                self.local_error = Some(format!("Scroll not sent: {error}"));
                cx.notify();
            }
        }
    }

    /// The resize cursor for a held split drag, or for the border under the
    /// pointer that a press would pick up.
    pub(crate) fn split_cursor_at(&self, position: Point<Pixels>) -> Option<CursorStyle> {
        if let Some(drag) = self.split_drag.as_ref().filter(|drag| drag.held) {
            return Some(splits::cursor(drag.split.direction));
        }
        self.split_at(position)
            .map(|split| splits::cursor(split.direction))
    }

    fn split_at(&self, position: Point<Pixels>) -> Option<&PaneSurfaceSplit> {
        if self.menu.page.is_some()
            || !self.input_ready()
            || !self.live.surface_ready()
            || !self.bounds.contains(&position)
        {
            return None;
        }
        let local = position - self.bounds.origin;
        splits::split_at(
            self.live.surface.as_deref()?,
            f32::from(local.x),
            f32::from(local.y),
            self.cell_width,
            self.config.terminal.line_height(),
        )
    }

    /// Picks up the border under the pointer. Nothing is sent until it moves,
    /// so a click on a border leaves the layout as it was.
    pub(crate) fn split_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(split) = self.split_at(event.position).cloned() else {
            return false;
        };
        let (Some(snapshot), Some(surface)) = (&self.live.snapshot, self.live.surface.as_deref())
        else {
            return false;
        };
        let Some(tab) = snapshot.focused_tab_id.clone() else {
            return false;
        };
        let local = event.position - self.bounds.origin;
        let pointer = splits::along(split.direction, f32::from(local.x), f32::from(local.y));
        let grab =
            splits::edge(&split, self.cell_width, self.config.terminal.line_height()) - pointer;
        self.split_drag = Some(SplitDrag {
            boot: snapshot.boot_id.clone(),
            tab,
            topology: splits::topology(surface),
            split,
            grab,
            held: true,
            want: None,
            sent: None,
        });
        self.cancel_terminal_mouse(cx);
        self.selection = None;
        self.pressed_terminal_link = None;
        cx.stop_propagation();
        cx.notify();
        true
    }

    pub(crate) fn split_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let cell_height = self.config.terminal.line_height();
        let Some(drag) = self.split_drag.as_mut().filter(|drag| drag.held) else {
            return false;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            drag.held = false;
            cx.notify();
            return false;
        }
        let local = event.position - self.bounds.origin;
        let pointer = splits::along(drag.split.direction, f32::from(local.x), f32::from(local.y));
        let ratio = splits::ratio(
            &drag.split,
            pointer + drag.grab,
            self.cell_width,
            cell_height,
        );
        if let Some(ratio) = ratio.filter(|ratio| drag.sent != Some(*ratio)) {
            drag.want = Some(ratio);
        }
        self.flush_split(cx);
        true
    }

    pub(crate) fn split_mouse_up(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self
            .split_drag
            .as_mut()
            .filter(|drag| drag.held && event.button == MouseButton::Left)
        else {
            return false;
        };
        drag.held = false;
        self.flush_split(cx);
        cx.notify();
        true
    }

    /// Sends the latest wanted ratio once the previous drag request has
    /// answered, and retires a released drag when nothing is left to send.
    /// The drag ends once its tab or panes change: its path could then name a
    /// different border.
    pub(crate) fn flush_split(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = &mut self.split_drag else {
            return;
        };
        if self.live.drag_request.is_some() {
            return;
        }
        let Some(ratio) = drag.want else {
            if !drag.held {
                self.split_drag = None;
            }
            return;
        };
        let connection = &self.endpoints[self.selected_endpoint].connection;
        let (Some(handle), Some(snapshot)) = (&connection.handle, &self.live.snapshot) else {
            self.split_drag = None;
            return;
        };
        if snapshot.boot_id != drag.boot {
            self.split_drag = None;
            return;
        }
        // A snapshot ahead of its surface is waited out, not taken as a change.
        let Some(surface) = self
            .live
            .surface
            .as_deref()
            .filter(|_| self.live.surface_ready())
        else {
            return;
        };
        if snapshot.focused_tab_id.as_deref() != Some(drag.tab.as_str())
            || splits::topology(surface) != drag.topology
        {
            self.split_drag = None;
            return;
        }
        // Registered under the inbox lock for the same reason as the scrollbar.
        let Ok(mut state) = connection.inbox.try_lock() else {
            return;
        };
        match handle.request(
            &drag.boot,
            Method::LayoutSetSplitRatio,
            json!({"tab_id": drag.tab, "path": drag.split.path, "ratio": ratio}),
        ) {
            Ok(request) => {
                drag.want = None;
                drag.sent = Some(ratio);
                state.drag_request = Some(request.clone());
                self.live.drag_request = Some(request);
            }
            Err(error) => {
                drop(state);
                self.split_drag = None;
                self.local_error = Some(format!("Pane resize not sent: {error}"));
                cx.notify();
            }
        }
    }
}
