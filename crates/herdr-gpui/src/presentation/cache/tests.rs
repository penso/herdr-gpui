#![allow(clippy::unwrap_used)]
use super::*;

fn snapshot(tab: usize) -> ClientShellSnapshot {
    let mut snapshot = crate::sidebar::layout_tests::snapshot(1);
    snapshot.tabs[0].tab_id = tab.to_string();
    snapshot.focused_tab_id = Some(tab.to_string());
    snapshot
}

fn surface() -> Arc<PaneSurfaceFrame> {
    Arc::new(PaneSurfaceFrame {
        boot_id: snapshot(0).boot_id,
        projection_revision: 1,
        surface_revision: 1,
        frame: herdr_client::protocol::FrameData {
            cells: vec![],
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
    })
}

#[test]
fn oldest_screens_are_evicted_and_revisits_replace_them() {
    let mut cache = Cache::default();
    let first = surface();
    let weak = Arc::downgrade(&first);
    cache.remember(&snapshot(0), &first);
    drop(first);
    for tab in 1..=MAX_SCREENS {
        cache.remember(&snapshot(tab), &surface());
    }
    assert!(weak.upgrade().is_none());
    let replacement = surface();
    for _ in 0..MAX_SCREENS {
        cache.remember(&snapshot(1), &replacement);
    }
    assert_eq!(
        cache
            .screens
            .iter()
            .filter(|screen| screen.tab == "1")
            .count(),
        1
    );
    assert!(cache.screens.len() <= MAX_SCREENS);
    assert!(Arc::ptr_eq(
        &cache.screens.front().unwrap().frame,
        &replacement
    ));
}

#[test]
fn byte_budget_counts_allocations_and_evicts_old_screens() {
    let mut cache = Cache::default();
    let mut frame = surface();
    Arc::make_mut(&mut frame)
        .frame
        .cells
        .reserve(MAX_BYTES / size_of::<CellData>() / 2);
    let weak = Arc::downgrade(&frame);
    cache.remember(&snapshot(0), &frame);
    drop(frame);
    let mut next = surface();
    Arc::make_mut(&mut next)
        .frame
        .cells
        .reserve(MAX_BYTES / size_of::<CellData>() / 2);
    cache.remember(&snapshot(1), &next);
    assert!(weak.upgrade().is_none());
    assert!(
        cache
            .screens
            .iter()
            .map(|screen| screen.bytes)
            .sum::<usize>()
            <= MAX_BYTES
    );
    Arc::make_mut(&mut next)
        .frame
        .cells
        .reserve(MAX_BYTES / size_of::<CellData>() + 1);
    cache.remember(&snapshot(1), &next);
    assert!(cache.screens.is_empty());
}

#[test]
fn graphics_and_popups_replace_old_previews_without_retaining_them() {
    let mut cache = Cache::default();
    cache.remember(&snapshot(0), &surface());
    let mut graphical = surface();
    Arc::make_mut(&mut graphical).frame.graphics = vec![1];
    cache.remember(&snapshot(0), &graphical);
    assert!(cache.screens.is_empty());
    let mut popup = surface();
    let frame = Arc::make_mut(&mut popup);
    frame.popup = Some(Box::new(herdr_client::protocol::ClientShellPopupSurface {
        terminal_id: "popup".into(),
        title: "popup".into(),
        width: None,
        height: None,
        frame: frame.frame.clone(),
        mouse_reporting: false,
        sgr_pixel_mouse: false,
        pixel_width: 0,
        pixel_height: 0,
    }));
    cache.remember(&snapshot(0), &popup);
    assert!(cache.screens.is_empty());
}
