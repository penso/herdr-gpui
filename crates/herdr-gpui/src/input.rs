use super::{HerdrWindow, terminal::input_cursor_bounds};
use gpui::*;
use herdr_client::protocol::{ClientPaneInputEvent, ClientShellSnapshot, PaneSurfaceFrame};
use std::ops::Range;

#[derive(PartialEq, Eq)]
pub(super) struct TerminalBinding {
    endpoint_id: String,
    boot_id: String,
    target: crate::terminal::InputTarget,
}

impl TerminalBinding {
    pub fn new(
        endpoint_id: &str,
        snapshot: Option<&ClientShellSnapshot>,
        surface: Option<&PaneSurfaceFrame>,
    ) -> Option<Self> {
        let (snapshot, surface) = (snapshot?, surface?);
        if snapshot.boot_id != surface.boot_id || snapshot.revision != surface.projection_revision {
            return None;
        }
        let target = if let Some(popup) = &surface.popup {
            crate::terminal::InputTarget::Popup(popup.terminal_id.clone())
        } else {
            crate::terminal::InputTarget::Pane(snapshot.focused_pane_id.clone()?)
        };
        Some(Self {
            endpoint_id: endpoint_id.to_owned(),
            boot_id: snapshot.boot_id.clone(),
            target,
        })
    }
}

pub(super) fn terminal_handler(
    bounds: Bounds<Pixels>,
    view: Entity<HerdrWindow>,
    binding: Option<TerminalBinding>,
    epoch: u64,
    selection_epoch: u64,
    generation: u64,
) -> impl InputHandler {
    crate::input_guard::GuardedInputHandler::new(
        bounds,
        view,
        move |view: &HerdrWindow, window: &Window, _: &App| {
            view.terminal_input_enabled(window)
                && view.selection_epoch == selection_epoch
                && view.endpoints[view.selected_endpoint].generation == generation
                && view.terminal_input_epoch == epoch
                && binding.is_some()
                && TerminalBinding::new(
                    &view.endpoints[view.selected_endpoint].id,
                    view.live.snapshot.as_deref(),
                    view.live.surface.as_deref(),
                ) == binding
                && view.endpoints[view.selected_endpoint]
                    .connection
                    .inbox
                    .try_lock()
                    .ok()
                    .is_some_and(|state| {
                        state.status.is_connected()
                            && state.surface_ready()
                            && TerminalBinding::new(
                                &view.endpoints[view.selected_endpoint].id,
                                state.snapshot.as_deref(),
                                state.surface.as_deref(),
                            ) == binding
                    })
        },
    )
}

impl HerdrWindow {
    pub(super) fn invalidate_terminal_input(&mut self) {
        self.terminal_input_epoch = self.terminal_input_epoch.wrapping_add(1);
        self.marked.clear();
    }

    fn terminal_input_enabled(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
            && !self.menu.is_open()
            && self.navigation_fence.is_none()
            && self.input_ready()
    }
}

impl EntityInputHandler for HerdrWindow {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        if !self.terminal_input_enabled(window) {
            return None;
        }
        let text: Vec<u16> = self.marked.encode_utf16().collect();
        let range = range.start.min(text.len())..range.end.min(text.len());
        *adjusted = Some(range.clone());
        Some(String::from_utf16_lossy(&text[range]))
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        if !self.terminal_input_enabled(window) {
            return None;
        }
        let end = self.marked.encode_utf16().count();
        Some(UTF16Selection {
            range: end..end,
            reversed: false,
        })
    }
    fn marked_text_range(
        &self,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        (self.terminal_input_enabled(window) && !self.marked.is_empty())
            .then(|| 0..self.marked.encode_utf16().count())
    }
    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.terminal_input_enabled(window) {
            return;
        }
        self.marked.clear();
        cx.notify();
    }
    fn replace_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.terminal_input_enabled(window) {
            return;
        }
        self.marked.clear();
        if !text.is_empty() {
            #[cfg(feature = "integration-test")]
            {
                self.input_probe.text += 1;
            }
            self.send(ClientPaneInputEvent::TextCommit(text.into()), cx);
        }
        cx.notify();
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.terminal_input_enabled(window) {
            return;
        }
        self.marked = text.into();
        cx.notify();
    }
    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: Bounds<Pixels>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if !self.terminal_input_enabled(window) {
            return None;
        }
        Some(input_cursor_bounds(
            self.live.surface.as_deref(),
            self.bounds.origin,
            self.cell_width,
            self.config.terminal.line_height(),
        ))
    }
    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}
