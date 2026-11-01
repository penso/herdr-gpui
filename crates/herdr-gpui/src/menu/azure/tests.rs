//! The account menu's Azure DevOps page. The fixture window is headless, so
//! nothing here reads the keyring, runs `az`, or reaches Azure DevOps.
#![allow(clippy::unwrap_used)]
use super::{Lookup, OrganizationStatus};
use crate::{HerdrWindow, menu::Page};
use gpui::{Entity, TestAppContext, VisualTestContext, px, size};

mod page;
mod status;

/// Text without Fluent's isolation marks around names.
fn plain(text: String) -> String {
    text.replace(['\u{2068}', '\u{2069}'], "")
}

fn page(cx: &mut TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.simulate_resize(size(px(1200.), px(900.)));
    view.update_in(cx, |view, window, cx| {
        assert!(view.open_menu(window, cx));
        view.open_azure_page(window, cx);
    });
    draw(cx);
    (view, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let point = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_click(point, Default::default());
    draw(cx);
}

fn status(lookup: Lookup) -> OrganizationStatus {
    OrganizationStatus {
        organization: "contoso".into(),
        saved: true,
        lookup,
    }
}
