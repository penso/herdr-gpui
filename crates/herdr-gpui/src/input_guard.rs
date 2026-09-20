use gpui::{
    App, Bounds, ElementInputHandler, Entity, EntityInputHandler, InputHandler, Pixels, Point,
    UTF16Selection, Window,
};
use std::ops::Range;

/// Revalidate registration-time state before every native callback, including
/// callbacks delivered after focus or the entity's input session has changed.
pub(super) struct GuardedInputHandler<V, F> {
    entity: Entity<V>,
    inner: ElementInputHandler<V>,
    predicate: F,
}

impl<V: EntityInputHandler, F: Fn(&V, &Window, &App) -> bool + 'static> GuardedInputHandler<V, F> {
    pub(super) fn new(bounds: Bounds<Pixels>, entity: Entity<V>, predicate: F) -> Self {
        Self {
            inner: ElementInputHandler::new(bounds, entity.clone()),
            entity,
            predicate,
        }
    }
}

impl<V: EntityInputHandler, F: Fn(&V, &Window, &App) -> bool + 'static> InputHandler
    for GuardedInputHandler<V, F>
{
    fn selected_text_range(
        &mut self,
        ignore_disabled_input: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<UTF16Selection> {
        if !(self.predicate)(self.entity.read(cx), window, cx) {
            return None;
        }
        self.inner
            .selected_text_range(ignore_disabled_input, window, cx)
    }

    fn marked_text_range(&mut self, window: &mut Window, cx: &mut App) -> Option<Range<usize>> {
        if !(self.predicate)(self.entity.read(cx), window, cx) {
            return None;
        }
        self.inner.marked_text_range(window, cx)
    }

    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<String> {
        if !(self.predicate)(self.entity.read(cx), window, cx) {
            return None;
        }
        self.inner.text_for_range(range, adjusted_range, window, cx)
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        if (self.predicate)(self.entity.read(cx), window, cx) {
            self.inner.replace_text_in_range(range, text, window, cx);
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selection: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if (self.predicate)(self.entity.read(cx), window, cx) {
            self.inner
                .replace_and_mark_text_in_range(range, text, selection, window, cx);
        }
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut App) {
        if (self.predicate)(self.entity.read(cx), window, cx) {
            self.inner.unmark_text(window, cx);
        }
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        if !(self.predicate)(self.entity.read(cx), window, cx) {
            return None;
        }
        self.inner.bounds_for_range(range, window, cx)
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<usize> {
        if !(self.predicate)(self.entity.read(cx), window, cx) {
            return None;
        }
        self.inner.character_index_for_point(point, window, cx)
    }
}
