//! Toggle Menu Bar: the bar gives its row to the window, F10 still opens the
//! menus over it, and the window controls move to the title bar.

use super::*;
use crate::controls::Command;
use gpui::{point, px, size};

fn toggle(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::ToggleMenuBar, window, cx)
        })
    });
    draw(cx);
}

fn press_f10(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_application_menu(Some(0), window, cx)
        })
    });
    draw(cx);
}

#[gpui::test]
fn hiding_gives_the_row_to_the_window_and_showing_takes_it_back(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.simulate_resize(size(px(1000.), px(600.)));
    draw(cx);
    let bar = cx.debug_bounds("application-menu-bar").unwrap();
    let body = cx.debug_bounds("window-body").unwrap();
    assert!(bar.size.height > px(0.));

    toggle(&view, cx);
    assert_eq!(
        cx.debug_bounds("application-menu-bar").unwrap().size.height,
        px(0.)
    );
    let hidden = cx.debug_bounds("window-body").unwrap();
    assert_eq!(hidden.top(), body.top() - bar.size.height);
    assert_eq!(hidden.bottom(), body.bottom());

    toggle(&view, cx);
    assert_eq!(cx.debug_bounds("application-menu-bar").unwrap(), bar);
    assert_eq!(cx.debug_bounds("window-body").unwrap(), body);
}

/// The menus open from the first frame at the bar's full height and over the
/// window, so the terminal below never resizes for them.
#[gpui::test]
fn f10_opens_the_menus_over_the_window_while_hidden(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.simulate_resize(size(px(1000.), px(600.)));
    draw(cx);
    let shown = cx.debug_bounds("application-menu-bar").unwrap();
    toggle(&view, cx);
    let body = cx.debug_bounds("window-body").unwrap();

    press_f10(&view, cx);
    let panel = cx.debug_bounds("application-menu-panel").unwrap();
    assert!((panel.top() - shown.bottom()).abs() <= px(1.));
    assert_eq!(cx.debug_bounds("window-body").unwrap(), body);
    assert!(!view.read_with(cx, |view, _| view.menu_bar_visible));
}

/// Headings measured while hidden stay under the overlay's, so a later
/// heading's menu opens beneath it rather than at the window's edge.
#[gpui::test]
fn a_later_headings_menu_hangs_under_it_while_hidden(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.simulate_resize(size(px(1200.), px(700.)));
    draw(cx);
    toggle(&view, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_application_menu(Some(2), window, cx)
        })
    });
    draw(cx);
    let measured = view.read_with(cx, |view, _| view.menu.application_bar.buttons.borrow()[2]);
    let panel = cx.debug_bounds("application-menu-panel").unwrap();
    assert!(measured.left() > px(0.));
    assert!((panel.left() - measured.left()).abs() <= px(1.));
}

/// With the bar gone, nothing else on screen would close the window.
#[gpui::test]
fn the_window_controls_follow_into_the_title_bar(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    view.update(cx, |view, cx| {
        view.config.show_window_controls = true;
        cx.notify();
    });
    cx.simulate_resize(size(px(1000.), px(600.)));
    draw(cx);
    let bar = cx.debug_bounds("application-menu-bar").unwrap();
    let controls = cx.debug_bounds("window-controls").unwrap();
    assert!(bar.contains(&point(controls.left(), controls.center().y)));

    toggle(&view, cx);
    let controls = cx.debug_bounds("window-controls").unwrap();
    let header = cx
        .debug_bounds("titlebar")
        .or_else(|| cx.debug_bounds("strip-titlebar-trailing"))
        .unwrap();
    assert!(header.contains(&point(controls.left(), controls.center().y)));
}

#[gpui::test]
fn the_toggle_lasts_for_the_session_and_leaves_the_setting_alone(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    toggle(&view, cx);
    assert!(!view.read_with(cx, |view, _| view.menu_bar_visible));
    assert!(view.read_with(cx, |view, _| view.config.show_menu_bar));
}
