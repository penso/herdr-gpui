#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use core::prelude::v1::test;

struct AlignedText {
    align: TextAlign,
    layout: Option<TextLayout>,
}

const WRAPPED: &str = "naïve α café 中文 wrapped words final";

impl Render for AlignedText {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let text = StyledText::new(WRAPPED);
        self.layout = Some(text.layout().clone());
        div()
            .w(px(110.))
            .text_size(px(16.))
            .line_height(px(24.))
            .text_align(self.align)
            .child(text)
    }
}

#[gpui::test]
fn aligned_backgrounds_cover_the_actual_wrapped_unicode_rows(cx: &mut TestAppContext) {
    for align in [TextAlign::Center, TextAlign::Right] {
        let (view, cx) = cx.add_window_view(|_, _| AlignedText {
            align,
            layout: None,
        });
        cx.update(|window, cx| {
            crate::sidebar::layout_tests::full_draw(window, cx).clear(cx);
        });
        view.read_with(cx, |view, _| {
            let layout = view.layout.as_ref().expect("painted text");
            let all = background_rects(layout, align, &(0..WRAPPED.len()));
            assert!(all.len() > 1, "the text must wrap in this test");
            for rect in &all {
                assert!(rect.size.width > px(0.));
                assert!(rect.size.width <= layout.bounds().size.width);
                let (actual, expected) = match align {
                    TextAlign::Center => (rect.center().x, layout.bounds().center().x),
                    _ => (rect.right(), layout.bounds().right()),
                };
                assert!((f32::from(actual - expected)).abs() < 0.01);
            }
            let partial = background_rects(layout, align, &(0..WRAPPED.len() - 2));
            assert_eq!(partial.len(), all.len());
            assert!(partial.last().unwrap().size.width < all.last().unwrap().size.width);
        });
    }
}

#[test]
fn aligned_backgrounds_retain_inline_code_and_selection_colors() {
    let mut transcript = herdr_chat_protocol::Transcript::new("test");
    transcript.entries.push(herdr_chat_protocol::Entry {
        id: EntryId(1),
        kind: herdr_chat_protocol::EntryKind::Agent {
            text: "code text".into(),
        },
    });
    let mut prepared = crate::chat::prepared::Prepared::default();
    prepared.update(&transcript, &[(EntryId(1), 1)]);
    let document = prepared.text();
    let key = document
        .key(EntryId(1), Region::Body(0), Cell::default())
        .unwrap();
    let mut selection = Selection::default();
    selection.press(document, Caret { key, offset: 5 }, 1, false);
    selection.drag(document, Caret { key, offset: 9 });
    let geometry = Geometry::default();
    let scope = Scope::new(
        "test",
        EntryId(1),
        Region::Body(0),
        document,
        &selection,
        &geometry,
    );
    let fragment = scope.fragment(Cell::default()).unwrap().aligned();
    let theme = Theme::default();
    let _text = fragment.styled(
        "code text",
        [(
            0..4,
            HighlightStyle {
                background_color: Some(rgb(theme.active).into()),
                ..Default::default()
            },
        )],
        &theme,
    );
    let backgrounds = fragment.backgrounds.as_ref().unwrap().borrow();
    assert_eq!(
        backgrounds
            .iter()
            .map(|mark| mark.range.clone())
            .collect::<Vec<_>>(),
        vec![0..4, 5..9]
    );
    assert_eq!(backgrounds[0].color, Hsla::from(rgb(theme.active)));
    assert_eq!(backgrounds[1].color, Hsla::from(rgb(theme.primary_wash())));
}

#[test]
fn selecting_even_one_byte_blocks_link_activation_until_the_next_plain_click() {
    let mut transcript = herdr_chat_protocol::Transcript::new("test");
    transcript.entries.push(herdr_chat_protocol::Entry {
        id: EntryId(1),
        kind: herdr_chat_protocol::EntryKind::Agent {
            text: "[label](https://example.org)".into(),
        },
    });
    let mut prepared = crate::chat::prepared::Prepared::default();
    prepared.update(&transcript, &[(EntryId(1), 1)]);
    let geometry = Geometry::default();
    let selection = Selection::default();
    let scope = Scope::new(
        "test",
        EntryId(1),
        Region::Body(0),
        prepared.text(),
        &selection,
        &geometry,
    );
    let Some(fragment) = scope.fragment(Cell::default()) else {
        panic!("link text missing")
    };
    let caret = Caret {
        key: fragment.key,
        offset: 0,
    };
    let origin = point(px(10.), px(10.));
    geometry.press(caret, origin, 1, false);
    geometry.moved(origin + point(px(2.), px(0.)));
    assert!(
        fragment.allows_link(),
        "two pixels alone are below the drag threshold"
    );
    geometry.dragged(Caret { offset: 1, ..caret });
    geometry.release();
    assert!(
        !fragment.allows_link(),
        "a selected character must never open the link"
    );
    geometry.press(caret, origin, 2, false);
    assert!(!fragment.allows_link(), "word selection must not navigate");
    geometry.press(caret, origin, 3, false);
    assert!(
        !fragment.allows_link(),
        "paragraph selection must not navigate"
    );
    geometry.press(caret, origin, 1, true);
    assert!(
        !fragment.allows_link(),
        "extending selection must not navigate"
    );
    geometry.press(caret, origin, 1, false);
    assert!(
        fragment.allows_link(),
        "a fresh plain click still opens a link"
    );
    geometry.clear();
    assert!(
        !fragment.allows_link(),
        "replaced geometry cannot activate a stale link"
    );
}

#[test]
fn selection_splits_styles_without_losing_emphasis_or_link_decoration() {
    let theme = Theme::default();
    let bold = HighlightStyle {
        font_weight: Some(FontWeight::BOLD),
        ..Default::default()
    };
    let underline = HighlightStyle {
        underline: Some(UnderlineStyle::default()),
        ..Default::default()
    };
    let marks = highlights(
        "éabcdef",
        vec![(0..4, bold), (4..8, underline)],
        Some(2..6),
        &theme,
    );
    assert_eq!(
        marks
            .iter()
            .map(|(range, _)| range.clone())
            .collect::<Vec<_>>(),
        vec![0..2, 2..4, 4..6, 6..8]
    );
    assert_eq!(marks[1].1.font_weight, bold.font_weight);
    assert_eq!(marks[2].1.underline, underline.underline);
    assert!(marks[0].1.background_color.is_none());
    assert!(marks[1].1.background_color.is_some());
    assert!(marks[2].1.background_color.is_some());
    assert!(marks[3].1.background_color.is_none());
}
