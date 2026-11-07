use super::*;
use crate::chat::view::layout::Side;
use crate::config::{ChatLayoutMode, mix};
use core::prelude::v1::test;

const THEMES: [&str; 2] = ["Default", "Catppuccin Latte"];

fn theme(name: &str) -> Theme {
    let Some(theme) = Theme::builtin(name) else {
        panic!("missing {name}")
    };
    theme
}

fn link() -> Style {
    Style {
        link: Some("https://example.org".into()),
        ..Style::default()
    }
}

/// The fills drawn under a message, from each layout's row.
fn fills(theme: &Theme, mode: ChatLayoutMode, side: Side) -> Option<u32> {
    let tinted = |percent| Some(mix(theme.background, theme.foreground, percent));
    match (mode, side) {
        (ChatLayoutMode::Bubbles, Side::User) => tinted(9),
        (ChatLayoutMode::Cards, _) => tinted(3),
        (ChatLayoutMode::IMessage, Side::User) => None,
        (ChatLayoutMode::IMessage, Side::Agent) => tinted(14),
        (ChatLayoutMode::OpenCode, Side::User) => tinted(10),
        (ChatLayoutMode::Terminal, Side::User) => tinted(7),
        _ => Some(theme.background),
    }
}

#[gpui::test]
fn every_layout_reports_the_ground_its_rows_draw(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    for name in THEMES {
        view.update(cx, |view, _| {
            view.theme = theme(name);
            for &mode in ChatLayoutMode::ALL {
                for side in [Side::User, Side::Agent] {
                    assert_eq!(
                        layout_for(mode).ground(side, &view.chat_look()),
                        fills(&view.theme, mode, side),
                        "{name} {mode:?} {side:?}"
                    );
                }
            }
        });
    }
}

#[test]
fn accents_read_as_text_on_every_ground_a_message_can_show() {
    for name in THEMES {
        let theme = theme(name);
        for &mode in ChatLayoutMode::ALL {
            for side in [Side::User, Side::Agent] {
                let Some(ground) = fills(&theme, mode, side) else {
                    continue;
                };
                let accents = Accents::on(&theme, ground);
                for ink in [accents.emphasis, accents.heading, accents.link] {
                    // The message, a table's rows, and a quote's tint.
                    for surface in [ground, theme.background, theme.surface] {
                        assert!(
                            crate::contrast::ratio(ink, surface) >= 4.5,
                            "{name} {mode:?} {side:?}: {ink:06x} on {surface:06x}"
                        );
                    }
                }
            }
        }
    }
}

#[gpui::test]
fn accents_color_emphasis_and_links_and_leave_plain_text_alone(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    for name in THEMES {
        view.update(cx, |view, _| {
            view.theme = theme(name);
            let accents = Accents::on(&view.theme, view.theme.background);
            let color = |style: &Style| view.rich_style(style, Some(accents)).color;
            let emphasis = Some(rgb(accents.emphasis).into());
            assert_eq!(color(&Style::default()), None, "{name}: plain text");
            for style in [
                Style {
                    bold: true,
                    ..Style::default()
                },
                Style {
                    italic: true,
                    ..Style::default()
                },
            ] {
                assert_eq!(color(&style), emphasis, "{name}");
            }
            let bold_link = Style {
                bold: true,
                ..link()
            };
            let shown = view.rich_style(&bold_link, Some(accents));
            assert_eq!(shown.color, Some(rgb(accents.link).into()));
            assert!(shown.underline.is_some(), "{name}: a link stays underlined");
        });
    }
}

#[gpui::test]
fn a_colored_fill_keeps_its_ink_but_inline_code_keeps_its_own(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    for name in THEMES {
        view.update(cx, |view, _| {
            view.theme = theme(name);
            for style in [
                Style {
                    bold: true,
                    ..Style::default()
                },
                Style {
                    italic: true,
                    ..Style::default()
                },
                link(),
            ] {
                assert_eq!(view.rich_style(&style, None).color, None, "{name}");
            }
            let ink = code_ink(&view.theme);
            assert!(crate::contrast::ratio(ink, view.theme.active) >= 4.5);
            let on_ground = Accents::on(&view.theme, view.theme.background);
            for accents in [None, Some(on_ground)] {
                let code = view.rich_style(
                    &Style {
                        code: true,
                        ..link()
                    },
                    accents,
                );
                assert_eq!(code.color, Some(rgb(ink).into()), "{name}");
                assert_eq!(code.background_color, Some(rgb(view.theme.active).into()));
                assert!(code.underline.is_some(), "{name}: code in a link");
            }
        });
    }
}

#[gpui::test]
fn headings_take_the_accent_and_quotes_own_a_readable_surface(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    for name in THEMES {
        view.update(cx, |view, cx| {
            view.theme = theme(name);
            let visuals = Visuals::default();
            let document = crate::chat::selection::Document::default();
            let selected = crate::chat::selection::Selection::default();
            let geometry = crate::chat::view::selection::Geometry::default();
            let selection = Scope::new(
                "fixture",
                sheprd_chat_protocol::EntryId(0),
                crate::chat::selection::Region::Body(0),
                &document,
                &selected,
                &geometry,
            );
            let content = [Inline::Text("A styled paragraph".into(), Style::default())];
            let size = view.chat_text_size() / 2.;
            let on_ground = Accents::on(&view.theme, view.theme.background);
            for accents in [None, Some(on_ground)] {
                let mut context = RichContext {
                    accents,
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
                    accents.map(|accents| rgb(accents.heading).into()),
                    "{name}: a heading in a colored fill keeps its ink"
                );
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
                assert_eq!(
                    quote.style().border_color,
                    Some(rgb(view.theme.ink(view.theme.palette[4])).into())
                );
                assert!(crate::contrast::ratio(view.theme.foreground, view.theme.surface) >= 4.5);
            }
        });
    }
}
