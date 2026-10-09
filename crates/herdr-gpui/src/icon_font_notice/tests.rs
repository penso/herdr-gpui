use super::*;
use crate::sidebar::layout_tests::fixture_window;
use gpui::{TestAppContext, VisualTestContext};
use herdr_client::protocol::CellData;

fn cell(symbol: &str) -> CellData {
    CellData {
        symbol: symbol.into(),
        fg: 0,
        bg: 0,
        modifier: 0,
        skip: false,
        hyperlink: None,
    }
}

fn frame(revision: u64, symbols: &[&str]) -> PaneSurfaceFrame {
    PaneSurfaceFrame {
        boot_id: "boot".into(),
        projection_revision: 1,
        surface_revision: revision,
        frame: FrameData {
            cells: symbols.iter().copied().map(cell).collect(),
            width: symbols.len() as u16,
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

#[test]
fn private_use_icons_need_a_font_but_painted_separators_do_not() {
    // Folder, thin separator, and a supplementary-plane Material icon.
    for icon in ["\u{f07c}", "\u{e0b1}", "\u{f0001}"] {
        assert!(shows_icon(&frame(1, &["a", icon]).frame), "{icon:?}");
    }
    let drawable = [
        "A", "\u{2714}", "\u{256d}", "\u{e0b0}", "\u{e0b2}", "\u{e0b4}", "\u{e0b6}",
    ];
    assert!(!shows_icon(&frame(1, &drawable).frame));
    // A wide character's continuation cell draws nothing.
    let mut hidden = frame(1, &["\u{f07c}"]);
    hidden.frame.cells[0].skip = true;
    assert!(!shows_icon(&hidden.frame));
}

#[test]
fn appears_only_when_an_icon_font_is_missing_and_an_icon_is_shown() {
    let mut notice = IconFontNotice::default();
    let icon = frame(1, &["\u{f179}"]);
    assert!(!notice.observe(false, Some(&icon)));
    assert!(!notice.observe(true, Some(&frame(2, &["a"]))));
    assert!(!notice.observe(true, None));
    assert!(notice.observe(true, Some(&frame(3, &["\u{f179}"]))));
    assert_eq!(notice.visible().map(|lines| lines.len()), Some(3));
    // The prompt scrolling away keeps the explanation.
    assert!(!notice.observe(true, Some(&frame(4, &["a"]))));
    assert!(notice.visible().is_some());
    // A reload that finds a Nerd Font clears it.
    assert!(notice.observe(false, Some(&frame(4, &["a"]))));
    assert!(notice.visible().is_none());
}

#[test]
fn a_new_boot_with_the_same_revisions_is_scanned() {
    let mut notice = IconFontNotice::default();
    notice.observe(true, Some(&frame(1, &["a"])));
    let mut restarted = frame(1, &["\u{f179}"]);
    restarted.boot_id = "another boot".into();
    assert!(notice.observe(true, Some(&restarted)));
}

#[test]
fn dismissal_holds_across_a_reload_that_hides_the_card() {
    let mut notice = IconFontNotice::default();
    notice.observe(true, Some(&frame(1, &["\u{f179}"])));
    let lines = notice.visible().cloned();
    assert!(lines.is_some_and(|lines| notice.dismiss(&lines)));
    // `fallback = []` added, then removed again.
    assert!(!notice.observe(false, Some(&frame(1, &["\u{f179}"]))));
    assert!(!notice.observe(true, Some(&frame(2, &["\u{f179}"]))));
    assert!(notice.visible().is_none());
}

#[test]
fn a_frame_is_scanned_once_and_dismissal_holds() {
    let mut notice = IconFontNotice::default();
    let plain = frame(1, &["a"]);
    notice.observe(true, Some(&plain));
    // Same revision: not rescanned, even if the cells were different.
    assert!(!notice.observe(true, Some(&frame(1, &["\u{f179}"]))));
    assert!(notice.observe(true, Some(&frame(2, &["\u{f179}"]))));
    let lines = notice.visible().cloned();
    assert!(lines.is_some_and(|lines| notice.dismiss(&lines)));
    assert!(!notice.observe(true, Some(&frame(3, &["\u{f179}"]))));
    assert!(notice.visible().is_none());
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

#[gpui::test]
fn the_card_names_the_fix_and_dismisses(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    draw(cx);
    assert!(cx.debug_bounds("icon-font-notice").is_none());
    view.update(cx, |view, cx| {
        view.icon_font_notice
            .observe(true, Some(&frame(1, &["\u{f179}"])));
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("icon-font-notice").is_some());
    let headline = view.read_with(cx, |view, _| {
        view.icon_font_notice
            .visible()
            .map(|lines| lines[0].clone())
    });
    assert!(headline.is_some_and(|line| line.contains("Nerd Font")));
    let dismiss = cx.debug_bounds("icon-font-notice-dismiss");
    assert!(dismiss.is_some());
    if let Some(dismiss) = dismiss {
        cx.simulate_click(dismiss.center(), Default::default());
    }
    draw(cx);
    assert!(cx.debug_bounds("icon-font-notice").is_none());
}
