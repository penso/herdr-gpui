use super::*;
use gpui::{ScrollDelta, ScrollWheelEvent};

/// Release notes far past every parser bound: unbreakable words, an endless
/// link, a code line wider than any window, and more lines than are kept.
fn huge_notes() -> String {
    let mut notes = format!("# {}\n\n", "Heading ".repeat(200));
    notes.push_str(&format!("{}\n", "x".repeat(5_000)));
    notes.push_str(&format!("https://example.com/{}\n", "a".repeat(900)));
    notes.push_str(&format!("```\n{}\n```\n", "code ".repeat(1_000)));
    notes.push_str(&format!("- **{}**\n", "bold".repeat(500)));
    for line in 1..=2_000 {
        notes.push_str(&format!("- Change number {line}\n"));
    }
    notes
}

fn offered(notes: String) -> crate::updater::State {
    crate::updater::State::Available {
        version: "20261008.1".into(),
        notes,
    }
}

fn fixture(cx: &mut gpui::TestAppContext) -> (Entity<HerdrWindow>, &mut gpui::VisualTestContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    (view, cx)
}

#[gpui::test]
fn huge_release_notes_stay_inside_the_dialog_and_scroll(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let notes = huge_notes();
    let lines = cx.update(|_, cx| view.read(cx).app_update_notes.lines(&notes).len());
    let last: &'static str = format!("app-update-notes-line-{}", lines - 1).leak();
    for (width, height) in [
        (320., 360.),
        (900., 480.),
        (320., 600.),
        (800., 600.),
        (1400., 900.),
    ] {
        cx.simulate_resize(size(px(width), px(height)));
        let (panel, action) = draw_update_state(cx, &view, &offered(notes.clone()));
        assert!(
            panel.left() >= px(0.) && panel.right() <= px(width),
            "{width}x{height}: {panel:?}"
        );
        assert!(
            panel.top() >= px(0.) && panel.bottom() <= px(height),
            "{width}x{height}: {panel:?}"
        );
        let body = cx.debug_bounds("app-update-notes-body").unwrap();
        assert!(
            body.left() >= panel.left() && body.right() <= panel.right(),
            "{width}x{height}: notes {body:?} overflow {panel:?}"
        );
        // The section is never squeezed below its scroll box, which would
        // draw the notes over the text that follows them.
        let section = cx.debug_bounds("app-update-notes").unwrap();
        assert!(
            body.bottom() <= section.bottom() + px(1.),
            "{width}x{height}: notes {body:?} spill out of {section:?}"
        );
        for index in 0..6 {
            let line = cx
                .debug_bounds(format!("app-update-notes-line-{index}").leak())
                .unwrap();
            assert!(
                line.right() <= body.right() + px(1.),
                "{width}x{height}: line {index} {line:?} is wider than {body:?}"
            );
        }
        let cap = cx.update(|_, cx| px(view.read(cx).config.ui.line_height() * 14.));
        assert!(body.size.height <= cap + px(1.), "{body:?} > {cap:?}");

        // The way out of a long excerpt stays on screen: the footer is never
        // pushed below the panel by the notes.
        let footer = cx.debug_bounds("app-update-footer").unwrap();
        assert!(footer.bottom() <= panel.bottom() + px(1.), "{footer:?}");
        if height < 400. {
            // Header and wrapped footer leave less than the notes' minimum, so
            // only the dialog's own scroll can reach them; containment is all.
            cx.update(|window, cx| {
                view.update(cx, |view, cx| view.dismiss_menu(window, cx));
                full_draw(window, cx).clear(cx);
            });
            continue;
        }
        assert!(
            body.bottom() <= footer.top() + px(1.),
            "{width}x{height}: notes {body:?} run under the footer {footer:?}"
        );
        for button in [
            action.unwrap(),
            cx.debug_bounds("app-update-notes-page").unwrap(),
        ] {
            assert!(
                button.left() >= panel.left()
                    && button.right() <= panel.right()
                    && button.top() >= footer.top()
                    && button.bottom() <= footer.bottom(),
                "{width}x{height}: {button:?} outside {footer:?}"
            );
        }

        // Scrolling the notes reaches the last kept line.
        cx.simulate_event(ScrollWheelEvent {
            position: body.center(),
            delta: ScrollDelta::Pixels(point(px(0.), px(-1_000_000.))),
            ..Default::default()
        });
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        let body = cx.debug_bounds("app-update-notes-body").unwrap();
        let end = cx.debug_bounds(last).unwrap();
        assert!(
            end.bottom() <= body.bottom() + px(1.) && end.top() >= body.top(),
            "{width}x{height}: last line {end:?} not scrolled into {body:?}"
        );
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.dismiss_menu(window, cx));
            full_draw(window, cx).clear(cx);
        });
    }
}

#[gpui::test]
fn release_notes_open_the_release_page_in_the_browser(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    cx.simulate_resize(size(px(800.), px(600.)));
    // Notes or not, the page is offered: an empty body still has a release.
    for notes in ["", "- Change"] {
        draw_update_state(cx, &view, &offered(notes.into()));
        let button = cx.debug_bounds("app-update-notes-page").unwrap();
        cx.simulate_click(button.center(), Modifiers::default());
        assert_eq!(
            cx.opened_url().as_deref(),
            Some("https://github.com/penso/herdr-gpui/releases/tag/v20261008.1")
        );
        // Opening the page is not a menu action: the dialog stays.
        cx.update(|_, cx| assert_eq!(view.read(cx).menu.page, Some(crate::menu::Page::AppUpdate)));
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.dismiss_menu(window, cx));
            full_draw(window, cx).clear(cx);
        });
    }
}
