#![allow(clippy::unwrap_used)]

//! An empty field under `en-XB` keeps its caret where text starts, on the
//! right, however long the placeholder beside it.

use super::*;
use gpui::VisualTestContext;
use herdr_i18n::Pseudo;

/// A field narrower than its placeholder.
struct Narrow(gpui::Entity<SearchInput>);

impl Render for Narrow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(120.)).child(self.0.clone())
    }
}

fn field(cx: &mut gpui::TestAppContext) -> (gpui::Entity<SearchInput>, &mut VisualTestContext) {
    let (narrow, cx) = cx.add_window_view(|_, cx| {
        Narrow(cx.new(|cx| {
            let mut input = SearchInput::new(cx);
            input.set_placeholder("Search every theme, font, and setting by name", cx);
            input
        }))
    });
    let input = narrow.read_with(cx, |narrow, _| narrow.0.clone());
    cx.update(|window, cx| {
        window.focus(&input.focus_handle(cx), cx);
        window.draw(cx).clear(cx);
    });
    (input, cx)
}

#[gpui::test]
fn an_empty_field_waits_at_its_right_edge(cx: &mut gpui::TestAppContext) {
    crate::i18n::show(&Pseudo::Bidi.locale());
    let (input, cx) = field(cx);
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        input.update(cx, |input, cx| {
            let bounds = input.bounds.unwrap();
            let layout = input.layout.clone().unwrap();
            let end = layout.x(input.placeholder().len());
            assert!(end > bounds.size.width, "the placeholder fits: {end:?}");
            // The drawn caret follows the placeholder's end, which is set
            // against the field's right edge.
            let caret = bounds.left() - input.scroll + end;
            assert_eq!(caret, bounds.right() - px(1.));
            let place = input
                .bounds_for_range(0..0, Bounds::default(), window, cx)
                .unwrap();
            assert_eq!(place.left(), caret);
            assert_eq!(place.right(), caret);
        })
    });
    crate::i18n::show(&herdr_i18n::Locale::FALLBACK);
}
