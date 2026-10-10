use super::*;
use crate::icon_font_notice::COMMAND;
use herdr_client::protocol::{CellData, FrameData, PaneSurfaceFrame};

/// A frame showing one Private Use Area icon, as a prompt draws it.
fn icon_frame() -> PaneSurfaceFrame {
    PaneSurfaceFrame {
        boot_id: "boot".into(),
        projection_revision: 1,
        surface_revision: 1,
        frame: FrameData {
            cells: vec![CellData {
                symbol: "\u{f179}".into(),
                fg: 0,
                bg: 0,
                modifier: 0,
                skip: false,
                hyperlink: None,
            }],
            width: 1,
            height: 1,
            cursor: None,
            hyperlinks: vec![],
            graphics: vec![],
        },
        panes: vec![],
        splits: vec![],
        popup: None,
        graphics: Default::default(),
    }
}

fn show_notice(view: &gpui::Entity<crate::HerdrWindow>, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| {
        view.icon_font_notice.observe(true, Some(&icon_frame()));
        cx.notify();
    });
    draw(cx);
}

#[gpui::test]
fn the_icon_font_notice_draws_its_command_on_a_chip_under_the_text(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(1000.), px(600.)));
    show_notice(&view, cx);
    let card = cx.debug_bounds("icon-font-notice").unwrap();
    let chip = cx.debug_bounds("diagnostic-command");
    // Only where one command installs the font is there one to draw.
    assert_eq!(chip.is_some(), COMMAND.is_some());
    let Some(chip) = chip else {
        return;
    };
    let text = cx.debug_bounds("icon-font-notice-line-1").unwrap();
    assert!(chip.top() >= text.bottom(), "{chip:?} under {text:?}");
    assert_eq!(chip.left(), text.left());
    assert!(chip.right() <= card.right() && chip.bottom() <= card.bottom());
    // The copy icon sits inside the chip, at its trailing end.
    let icon = cx.debug_bounds("diagnostic-command-copy").unwrap();
    assert!(icon.size.width > px(0.) && icon.size.height > px(0.));
    assert!(icon.left() > chip.center().x && icon.right() <= chip.right());
    assert!(icon.top() >= chip.top() && icon.bottom() <= chip.bottom());
}

#[gpui::test]
fn clicking_the_command_or_its_icon_copies_it_and_keeps_the_card(cx: &mut TestAppContext) {
    let Some(command) = COMMAND else {
        return;
    };
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(1000.), px(600.)));
    show_notice(&view, cx);
    for control in ["diagnostic-command", "diagnostic-command-copy"] {
        cx.update(|_, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string("before".into()));
        });
        view.update(cx, |view, _| view.flash = None);
        let bounds = cx.debug_bounds(control).unwrap();
        cx.simulate_click(bounds.center(), Default::default());
        draw(cx);
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some(command.to_owned()),
            "{control}"
        );
        // The gesture is answered, and the card stays for the restart step.
        view.read_with(cx, |view, _| assert!(view.flash.is_some(), "{control}"));
        assert!(cx.debug_bounds("icon-font-notice").is_some(), "{control}");
    }
}

#[gpui::test]
fn cards_that_name_no_command_draw_no_chip(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(1000.), px(600.)));
    view.update(cx, |view, cx| {
        view.gui_config_diagnostic
            .sync(Some("config-gpui.local.toml: ignoring unknown keys a"));
        cx.notify();
    });
    set_diagnostic(&view, 0, Some("config.toml invalid; using defaults"), cx);
    assert!(cx.debug_bounds("gui-config-diagnostic").is_some());
    assert!(cx.debug_bounds("config-diagnostic").is_some());
    assert!(cx.debug_bounds("diagnostic-command").is_none());
}
