use super::{
    HerdrWindow,
    terminal::{input_area, input_cursor_bounds},
};
use gpui::*;
use herdr_client::protocol::ClientPaneInputEvent;
use std::ops::Range;

impl EntityInputHandler for HerdrWindow {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        if self.menu.page.is_some() {
            let input = self.menu.input.as_ref()?;
            let range = input.range_from_utf16(range);
            *adjusted = Some(input.to_utf16(range.clone()));
            return Some(input.text[range].to_owned());
        }
        let text: Vec<u16> = self.marked.encode_utf16().collect();
        let range = range.start.min(text.len())..range.end.min(text.len());
        *adjusted = Some(range.clone());
        Some(String::from_utf16_lossy(&text[range]))
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        if self.menu.page.is_some() {
            let input = self.menu.input.as_ref()?;
            return Some(UTF16Selection {
                range: input.to_utf16(input.selection.clone()),
                reversed: input.reversed,
            });
        }
        let end = self.marked.encode_utf16().count();
        Some(UTF16Selection {
            range: end..end,
            reversed: false,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        if self.menu.page.is_some() {
            let input = self.menu.input.as_ref()?;
            return input.marked.clone().map(|range| input.to_utf16(range));
        }
        (!self.marked.is_empty()).then(|| 0..self.marked.encode_utf16().count())
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(input) = self.menu.input.as_mut() {
            input.marked = None;
        }
        self.marked.clear();
        cx.notify();
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu.page.is_some() {
            if let Some(input) = self.menu.input.as_mut() {
                input.replace(range, text, false, None);
                cx.notify();
            }
            return;
        }
        #[cfg(feature = "integration-test")]
        {
            self.input_probe.text += 1;
        }
        self.marked.clear();
        if !text.is_empty() {
            self.send(ClientPaneInputEvent::TextCommit(text.into()), cx);
        }
        cx.notify();
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu.page.is_some() {
            if let Some(input) = self.menu.input.as_mut() {
                input.replace(range, text, true, selected);
                cx.notify();
            }
            return;
        }
        if !self.input_ready() {
            return;
        }
        self.marked = text.into();
        cx.notify();
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if self.menu.page.is_some() {
            return self.menu.input.as_ref()?.range_bounds(range);
        }
        let surface = self.live.surface.as_deref();
        let cell_height = self.config.terminal.line_height();
        let cursor = input_cursor_bounds(surface, self.bounds.origin, self.cell_width, cell_height);
        Some(self.painter.borrow().composition_bounds(
            &self.marked,
            range,
            cursor,
            input_area(surface, self.bounds, self.cell_width, cell_height),
            &self.config.terminal.font(),
            window,
        ))
    }
    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        if self.menu.page.is_some() {
            let input = self.menu.input.as_ref()?;
            let index = input.index_at(point)?;
            return Some(input.to_utf16(index..index).start);
        }
        None
    }
}

/// The terminal's platform input handler. A terminal repeats a held key, so
/// it opts out of macOS press-and-hold, which would otherwise swallow the
/// repeats and open the accent picker. Menu text fields keep the picker.
pub(crate) struct TerminalInputHandler {
    inner: ElementInputHandler<HerdrWindow>,
    press_and_hold: bool,
}

impl TerminalInputHandler {
    pub(crate) fn new(bounds: Bounds<Pixels>, view: Entity<HerdrWindow>, menu_open: bool) -> Self {
        Self {
            inner: ElementInputHandler::new(bounds, view),
            press_and_hold: menu_open,
        }
    }
}

impl InputHandler for TerminalInputHandler {
    fn selected_text_range(
        &mut self,
        ignore_disabled_input: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<UTF16Selection> {
        self.inner
            .selected_text_range(ignore_disabled_input, window, cx)
    }
    fn marked_text_range(&mut self, window: &mut Window, cx: &mut App) -> Option<Range<usize>> {
        self.inner.marked_text_range(window, cx)
    }
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<String> {
        self.inner
            .text_for_range(range_utf16, adjusted_range, window, cx)
    }
    fn replace_text_in_range(
        &mut self,
        replacement_range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.inner
            .replace_text_in_range(replacement_range, text, window, cx);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.inner.replace_and_mark_text_in_range(
            range_utf16,
            new_text,
            new_selected_range,
            window,
            cx,
        );
    }
    fn unmark_text(&mut self, window: &mut Window, cx: &mut App) {
        self.inner.unmark_text(window, cx);
    }
    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        self.inner.bounds_for_range(range_utf16, window, cx)
    }
    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<usize> {
        self.inner.character_index_for_point(point, window, cx)
    }
    fn apple_press_and_hold_enabled(&mut self) -> bool {
        self.press_and_hold
    }
}
