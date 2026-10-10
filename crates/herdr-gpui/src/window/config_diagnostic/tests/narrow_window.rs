use super::*;
use herdr_client::protocol::{CellData, FrameData, PaneSurfaceFrame};

fn inside(card: &'static str, cx: &mut VisualTestContext) {
    let bounds = cx.debug_bounds(card).unwrap();
    assert!(
        bounds.left() >= px(0.) && bounds.right() <= px(320.),
        "{card}: {bounds:?}"
    );
}

#[gpui::test]
fn a_long_line_shrinks_with_a_narrow_window(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(320.), px(400.)));
    // Wider than the window on its own; the card truncates it instead of
    // keeping its width and leaving through the left edge.
    let long = "config.toml: ignoring unknown keys ".repeat(4);
    set_diagnostic(&view, 0, Some(&long), cx);
    inside("config-diagnostic", cx);
}

#[gpui::test]
fn stacked_cards_and_the_command_chip_stay_inside_a_narrow_window(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(320.), px(600.)));
    let icon = PaneSurfaceFrame {
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
    };
    view.update(cx, |view, cx| {
        view.gui_config_diagnostic.sync(Some(
            "config-gpui.local.toml: ignoring unknown keys a, b, c",
        ));
        view.icon_font_notice.observe(true, Some(&icon));
        cx.notify();
    });
    draw(cx);
    inside("gui-config-diagnostic", cx);
    inside("icon-font-notice", cx);
    // The cards still stack, flush right.
    let warning = cx.debug_bounds("gui-config-diagnostic").unwrap();
    let notice = cx.debug_bounds("icon-font-notice").unwrap();
    assert!(warning.bottom() <= notice.top());
    assert_eq!(warning.right(), notice.right());
    // The command truncates with the card; its copy icon stays on screen.
    if crate::icon_font_notice::COMMAND.is_some() {
        inside("diagnostic-command", cx);
        inside("diagnostic-command-copy", cx);
    }
}
