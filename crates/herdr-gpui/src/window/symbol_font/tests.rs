use super::{COMMAND, NOTICE, SymbolFontNotice};
use crate::HerdrWindow;
use crate::sidebar::layout_tests::{fixture_window, snapshot};
use gpui::{TestAppContext, VisualTestContext, px, size};
use herdr_client::protocol::{CellData, FrameData, PaneSurfaceFrame};
use std::sync::Arc;

const PLAIN: &[&str] = &["~", " ", "$"];
/// A folder icon and a thin separator, as a Powerlevel10k prompt draws them.
const ICONS: &[&str] = &["\u{f07c}", "~", "\u{e0b1}"];

fn frame(symbols: &[&str]) -> FrameData {
    FrameData {
        width: u16::try_from(symbols.len()).unwrap(),
        height: 1,
        cells: symbols
            .iter()
            .map(|symbol| CellData {
                symbol: (*symbol).into(),
                fg: 0,
                bg: 0,
                modifier: 0,
                skip: false,
                hyperlink: None,
            })
            .collect(),
        cursor: None,
        hyperlinks: Vec::new(),
        graphics: Vec::new(),
    }
}

fn shown(notice: &SymbolFontNotice) -> Option<Vec<String>> {
    notice.card.visible().map(|lines| lines.to_vec())
}

#[test]
fn the_card_waits_for_a_missing_font_and_a_drawn_icon() {
    let mut notice = SymbolFontNotice::default();
    // An icon font is installed: the icons draw, so there is nothing to say.
    assert!(!notice.observe(false, Some(&frame(ICONS))));
    assert_eq!(shown(&notice), None);
    // None is installed, but nothing on screen needs one.
    assert!(!notice.observe(true, None));
    assert!(!notice.observe(true, Some(&frame(PLAIN))));
    assert_eq!(shown(&notice), None);
    // The first icon brings the card, every line of it intact.
    assert!(notice.observe(true, Some(&frame(ICONS))));
    let lines: Vec<String> = NOTICE.lines().map(str::to_owned).collect();
    assert_eq!(shown(&notice), Some(lines));
    // It stays once the icon scrolls away, and reports no further change.
    assert!(!notice.observe(true, Some(&frame(PLAIN))));
    assert!(!notice.observe(true, None));
    assert!(shown(&notice).is_some());
}

#[test]
fn dismissal_holds_until_a_font_is_installed_and_removed_again() {
    let mut notice = SymbolFontNotice::default();
    assert!(notice.observe(true, Some(&frame(ICONS))));
    let lines = notice.card.visible().unwrap().clone();
    assert!(notice.card.dismiss(&lines));
    assert!(!notice.observe(true, Some(&frame(ICONS))));
    assert_eq!(shown(&notice), None);
    // A reload that finds an icon font clears the notice for good.
    assert!(!notice.observe(false, Some(&frame(ICONS))));
    assert_eq!(shown(&notice), None);
    // Losing the font again is news, without waiting for another icon.
    assert!(notice.observe(true, None));
    assert!(shown(&notice).is_some());
    assert!(notice.observe(false, None));
    assert_eq!(shown(&notice), None);
}

/// Hands the selected endpoint a frame the way its socket worker does.
fn deliver(view: &mut HerdrWindow, symbols: &[&str]) {
    let snapshot = snapshot(1);
    let surface = PaneSurfaceFrame {
        boot_id: snapshot.boot_id.clone(),
        projection_revision: snapshot.revision,
        surface_revision: 1,
        frame: frame(symbols),
        panes: Vec::new(),
        splits: Vec::new(),
        popup: None,
        graphics: Default::default(),
    };
    let mut state = view.endpoints[view.selected_endpoint]
        .connection
        .inbox
        .lock()
        .unwrap();
    state.snapshot = Some(Arc::new(snapshot));
    state.surface = Some(Arc::new(surface));
    state.dirty = true;
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn window(cx: &mut TestAppContext) -> (gpui::Entity<HerdrWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(1000.), px(600.)));
    view.update(cx, |view, _| {
        // This fixture has no transport; polling must not start one.
        for endpoint in &mut view.endpoints {
            endpoint.enabled = false;
        }
    });
    (view, cx)
}

#[gpui::test]
fn a_polled_icon_frame_shows_the_card_only_without_a_font(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    view.update(cx, |view, cx| {
        deliver(view, ICONS);
        view.poll_endpoints(cx);
    });
    draw(cx);
    assert!(cx.debug_bounds("symbol-font-notice").is_none());

    view.update(cx, |view, cx| {
        view.config.symbol_font_missing = true;
        deliver(view, PLAIN);
        view.poll_endpoints(cx);
    });
    draw(cx);
    assert!(cx.debug_bounds("symbol-font-notice").is_none());

    view.update(cx, |view, cx| {
        deliver(view, ICONS);
        view.poll_endpoints(cx);
    });
    draw(cx);
    let card = cx.debug_bounds("symbol-font-notice").unwrap();
    assert!(card.left() >= px(0.) && card.right() <= px(1000.));
    assert_eq!(NOTICE.lines().count(), 2);
    // The text, then the command chip, all inside the card.
    let mut above = card.top();
    for part in [
        "symbol-font-notice-line-0",
        "symbol-font-notice-line-1",
        "symbol-font-command",
    ] {
        let bounds = cx.debug_bounds(part).unwrap();
        assert!(bounds.top() >= above, "{part}: {bounds:?}");
        assert!(
            bounds.left() >= card.left() && bounds.right() <= card.right(),
            "{part}: {bounds:?}"
        );
        assert!(bounds.bottom() <= card.bottom(), "{part}: {bounds:?}");
        above = bounds.bottom();
    }
    // The copy icon sits inside the chip, at its trailing end.
    let chip = cx.debug_bounds("symbol-font-command").unwrap();
    let icon = cx.debug_bounds("symbol-font-copy").unwrap();
    assert!(icon.size.width > px(0.) && icon.size.height > px(0.));
    assert!(icon.left() > chip.center().x && icon.right() <= chip.right());
    assert!(icon.top() >= chip.top() && icon.bottom() <= chip.bottom());

    // Dismissed, it stays away while the prompt keeps drawing icons.
    let dismiss = cx.debug_bounds("symbol-font-notice-dismiss").unwrap();
    cx.simulate_click(dismiss.center(), Default::default());
    view.update(cx, |view, cx| {
        deliver(view, ICONS);
        view.poll_endpoints(cx);
    });
    draw(cx);
    assert!(cx.debug_bounds("symbol-font-notice").is_none());
}

#[gpui::test]
fn clicking_the_command_or_its_icon_copies_it_and_keeps_the_card(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    view.update(cx, |view, cx| {
        view.config.symbol_font_missing = true;
        deliver(view, ICONS);
        view.poll_endpoints(cx);
    });
    draw(cx);
    for control in ["symbol-font-command", "symbol-font-copy"] {
        cx.update(|_, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string("before".into()));
        });
        view.update(cx, |view, _| view.flash = None);
        let bounds = cx.debug_bounds(control).unwrap();
        cx.simulate_click(bounds.center(), Default::default());
        draw(cx);
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some(COMMAND.to_owned()),
            "{control}"
        );
        // The gesture is answered, and the card stays for the restart step.
        view.read_with(cx, |view, _| assert!(view.flash.is_some(), "{control}"));
        assert!(cx.debug_bounds("symbol-font-notice").is_some(), "{control}");
    }
}

#[gpui::test]
fn detection_finishing_after_the_prompt_is_drawn_shows_the_card(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    // The first frame arrives under the unresolved first-paint config.
    view.update(cx, |view, cx| {
        deliver(view, ICONS);
        view.poll_endpoints(cx);
    });
    draw(cx);
    assert!(cx.debug_bounds("symbol-font-notice").is_none());

    let load = |missing: bool| {
        move || {
            let config = crate::config::Config {
                symbol_font_missing: missing,
                ..Default::default()
            };
            Ok((config, Default::default()))
        }
    };
    view.update(cx, |view, cx| view.load_gui_config_with(load(true), cx));
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("symbol-font-notice").is_some());

    // A later load that finds an icon font takes it away again.
    view.update(cx, |view, cx| view.load_gui_config_with(load(false), cx));
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("symbol-font-notice").is_none());
}

#[gpui::test]
fn the_card_stacks_below_a_config_warning_and_fits_a_narrow_window(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    view.update(cx, |view, cx| {
        view.config.symbol_font_missing = true;
        view.gui_config_diagnostic
            .sync(Some("config-gpui.local.toml: ignoring unknown keys a"));
        deliver(view, ICONS);
        view.poll_endpoints(cx);
    });
    draw(cx);
    let warning = cx.debug_bounds("gui-config-diagnostic").unwrap();
    let card = cx.debug_bounds("symbol-font-notice").unwrap();
    assert!(warning.bottom() <= card.top());
    assert_eq!(warning.right(), card.right());

    cx.simulate_resize(size(px(320.), px(600.)));
    draw(cx);
    // All carry text wider than the window; none may leave it, and the
    // copy icon stays on screen while the command beside it truncates.
    for card in [
        "gui-config-diagnostic",
        "symbol-font-notice",
        "symbol-font-command",
        "symbol-font-copy",
    ] {
        let bounds = cx.debug_bounds(card).unwrap();
        assert!(
            bounds.left() >= px(0.) && bounds.right() <= px(320.),
            "{card}: {bounds:?}"
        );
    }
}
