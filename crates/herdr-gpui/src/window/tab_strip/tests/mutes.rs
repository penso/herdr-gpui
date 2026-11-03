use super::*;
use crate::{
    config::TabLayoutMode,
    notification_mutes::{Daemon, Mutes, Scope},
};

/// Mutes `scope` `id` on the strip's host, as its menu would.
fn mute(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, scope: Scope, id: &str) {
    view.update(cx, |view, cx| {
        let daemon = Daemon::of(&view.endpoints[0]);
        let snapshot = view.live.snapshot.clone().unwrap();
        assert!(Mutes::update(cx, |mutes| mutes.set(
            &daemon,
            &snapshot,
            scope,
            id,
            true,
            std::time::SystemTime::now()
        )));
        cx.notify();
    });
    for _ in 0..2 {
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        view.update(cx, |view, _| view.settle_tabs());
    }
}

fn bounds(cx: &mut VisualTestContext, selector: String) -> Option<Bounds<Pixels>> {
    cx.debug_bounds(Box::leak(selector.into_boxed_str()))
}

/// A muted tab carries the mark after its title, after a zoom mark, inside
/// the width measured for it, in every layout. Compact names only the chosen
/// tab, so as with the zoom mark only that one shows it.
#[gpui::test]
fn every_layout_draws_a_muted_tab_as_wide_as_it_measures_it(cx: &mut TestAppContext) {
    for &mode in TabLayoutMode::ALL {
        let (view, cx) = layouts::strip(cx, mode);
        // Every layout's window shares the app, and so its mutes.
        cx.update(|_, cx| cx.set_global(Mutes::default()));
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        assert!(bounds(cx, "tab-muted-zoomy".into()).is_none(), "{mode}");
        mute(&view, cx, Scope::Tab, "zoomy");
        mute(&view, cx, Scope::Tab, "w1:t1");
        let near = |a: Pixels, b: Pixels| (a - b).abs() <= px(0.5);
        let measured: Vec<(String, f32)> = cx.update(|window, cx| {
            let view = view.read(cx);
            let indicators = Indicators::new(
                None,
                crate::config::StatusIcons::default(),
                false,
                false,
                &view.theme,
            );
            let snapshot = view.live.snapshot.as_ref().unwrap();
            snapshot
                .tabs
                .iter()
                .map(|tab| {
                    let shape = TabShape {
                        lead: herdr_mark(snapshot, tab).lead(),
                        zoomed: tab.zoomed,
                        muted: tab.tab_id != "plain",
                        selected: tab.focused,
                    };
                    let label = SharedString::from(tab.label.clone());
                    let width = view.tab_width(&label, shape, indicators, window);
                    (tab.tab_id.clone(), width)
                })
                .collect()
        });
        for (id, width) in measured {
            let tab = bounds(cx, format!("tab-{id}")).unwrap();
            assert!(
                near(tab.size.width, px(width)),
                "{mode}: {id} drew {tab:?}, measured {width}"
            );
            let mark = bounds(cx, format!("tab-muted-{id}"));
            let named = mode != TabLayoutMode::Compact || id == "w1:t1";
            assert_eq!(mark.is_some(), named && id != "plain", "{mode}: {id}");
            let Some(mark) = mark else { continue };
            assert_eq!(mark.size, size(px(ZOOM_ICON), px(ZOOM_ICON)), "{mode}");
            assert!(
                mark.left() >= tab.left() && mark.right() <= tab.right(),
                "{mode}: {mark:?} in {tab:?}"
            );
            if let Some(close) = bounds(cx, format!("close-tab-{id}")) {
                assert!(
                    mark.right() <= close.left(),
                    "{mode}: {mark:?} before {close:?}"
                );
            }
            if let Some(zoom) = bounds(cx, format!("tab-zoom-{id}")) {
                assert!(
                    zoom.right() <= mark.left(),
                    "{mode}: zoom {zoom:?}, mute {mark:?}"
                );
            }
        }
    }
}

/// Muting the workspace marks every tab in it, and the strip's accessible
/// names say so.
#[gpui::test]
fn a_muted_workspace_marks_all_its_tabs(cx: &mut TestAppContext) {
    let (view, cx) = layouts::strip(cx, TabLayoutMode::Classic);
    mute(&view, cx, Scope::Workspace, "w1");
    for id in ["w1:t1", "plain", "zoomy"] {
        assert!(bounds(cx, format!("tab-muted-{id}")).is_some(), "{id}");
    }
    // A page is this app's own tab, never muted.
    let pages = cx.update(|_, cx| view.read(cx).browser_tab_labels(cx));
    for (pick, _) in pages {
        let Pick::Page(id) = pick else { continue };
        assert!(bounds(cx, format!("browser-tab-muted-{id}")).is_none());
    }
}
