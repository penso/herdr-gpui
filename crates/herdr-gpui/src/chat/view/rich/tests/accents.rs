use super::*;
use core::prelude::v1::test;

#[gpui::test]
fn prose_accents_read_on_panels_and_reply_bubbles_but_user_ink_is_inherited(
    cx: &mut TestAppContext,
) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    for name in ["Default", "Catppuccin Latte"] {
        for layout in [
            ChatLayoutMode::Bubbles,
            ChatLayoutMode::Document,
            ChatLayoutMode::IMessage,
        ] {
            view.update(cx, |view, _| {
                let Some(theme) = Theme::builtin(name) else {
                    panic!("missing {name}")
                };
                view.theme = theme;
                view.config.chat.layout = layout;
                let reply = crate::config::mix(view.theme.background, view.theme.foreground, 14);
                for palette in [4, 6] {
                    for surface in [
                        view.theme.background,
                        view.theme.surface,
                        view.theme.active,
                        crate::config::mix(view.theme.background, view.theme.foreground, 9),
                        if layout == ChatLayoutMode::IMessage {
                            reply
                        } else {
                            view.theme.background
                        },
                    ] {
                        assert!(
                            crate::contrast::ratio(view.prose_ink(palette), surface) >= 4.5,
                            "{name} {layout:?}: colored prose must remain readable"
                        );
                    }
                }
                for style in [
                    Style::default(),
                    Style {
                        bold: true,
                        ..Style::default()
                    },
                    Style {
                        italic: true,
                        ..Style::default()
                    },
                    Style {
                        link: Some("https://example.org".into()),
                        ..Style::default()
                    },
                ] {
                    assert_eq!(
                        view.rich_style(&style, true).color,
                        None,
                        "{name}: blue user and answered-question bubbles retain inherited ink"
                    );
                }
                let strong = view.rich_style(
                    &Style {
                        bold: true,
                        ..Style::default()
                    },
                    false,
                );
                let link = view.rich_style(
                    &Style {
                        link: Some("https://example.org".into()),
                        ..Style::default()
                    },
                    false,
                );
                assert!(strong.color.is_some());
                assert_eq!(strong.font_weight, Some(FontWeight::BOLD));
                assert!(link.color.is_some() && link.underline.is_some());
                assert_eq!(view.rich_style(&Style::default(), false).color, None);
                let ink = super::super::super::text_ink(view.theme.palette[5], view.theme.active);
                assert!(crate::contrast::ratio(ink, view.theme.active) >= 4.5);
                // Code owns its background even inside a blue bubble or a link.
                for inherit in [true, false] {
                    let code = view.rich_style(
                        &Style {
                            code: true,
                            bold: true,
                            link: Some("https://example.org".into()),
                            ..Style::default()
                        },
                        inherit,
                    );
                    assert_eq!(code.color, Some(rgb(ink).into()));
                    assert_eq!(code.background_color, Some(rgb(view.theme.active).into()));
                    assert!(code.underline.is_some());
                }
            });
        }
    }
}

#[gpui::test]
fn headings_keep_bubble_ink_and_quotes_own_a_readable_surface(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    for name in ["Default", "Catppuccin Latte"] {
        view.update(cx, |view, cx| {
            let Some(theme) = Theme::builtin(name) else {
                panic!("missing {name}")
            };
            view.theme = theme;
            view.config.chat.layout = ChatLayoutMode::IMessage;
            let visuals = Visuals::default();
            let document = crate::chat::selection::Document::default();
            let selected = crate::chat::selection::Selection::default();
            let geometry = crate::chat::view::selection::Geometry::default();
            let selection = Scope::new(
                "fixture",
                herdr_chat_protocol::EntryId(0),
                crate::chat::selection::Region::Body(0),
                &document,
                &selected,
                &geometry,
            );
            let content = [Inline::Text("A styled paragraph".into(), Style::default())];
            let size = view.chat_text_size() / 2.;
            for inherit_ink in [false, true] {
                let mut context = RichContext {
                    inherit_ink,
                    visuals: &visuals,
                    links: Links::new(view, "fixture", cx),
                    selection: &selection,
                    size,
                    cx,
                };
                let mut heading = view.rich_paragraph(
                    Kind::Heading(HeadingLevel::H2),
                    0,
                    &content,
                    0,
                    &mut context,
                );
                assert_eq!(
                    heading.text_style().color,
                    (!inherit_ink).then(|| rgb(view.prose_ink(4)).into())
                );
                assert_eq!(heading.text_style().font_weight, Some(FontWeight::BOLD));
                assert_eq!(
                    heading.text_style().font_size,
                    Some(px(size * rhythm::heading_scale(HeadingLevel::H2)).into()),
                    "headings follow their message's text size"
                );
                let mut quote = view.rich_paragraph(Kind::Quote, 0, &content, 0, &mut context);
                assert_eq!(
                    quote.style().background,
                    Some(rgb(view.theme.surface).into())
                );
                assert_eq!(
                    quote.text_style().color,
                    Some(rgb(view.theme.foreground).into())
                );
                assert!(crate::contrast::ratio(view.theme.foreground, view.theme.surface) >= 4.5);
            }
        });
    }
}
