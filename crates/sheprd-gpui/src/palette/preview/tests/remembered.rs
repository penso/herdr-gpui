use super::*;

/// What the daemon streams for the tab whose only pane is `pane`.
fn streaming(boot: &str, pane: &str) -> Arc<PaneSurfaceFrame> {
    let mut surface = Arc::unwrap_or_clone(surface(boot));
    surface.panes[0].pane_id = pane.into();
    Arc::new(surface)
}

/// `snapshot()` plus tabs `w1:t3`.. each holding one pane `w1:p3`...
fn many_tabs(count: usize) -> ClientShellSnapshot {
    let mut snapshot = snapshot();
    for n in 3..3 + count {
        let mut tab = snapshot.tabs[0].clone();
        tab.tab_id = format!("w1:t{n}");
        snapshot.tabs.push(tab);
        let mut pane = snapshot.panes[1].clone();
        pane.pane_id = format!("w1:p{n}");
        pane.tab_id = format!("w1:t{n}");
        snapshot.panes.push(pane);
    }
    snapshot
}

fn seen_at(preview: Option<Preview>) -> Option<Option<u64>> {
    match preview? {
        Preview::Screen { seen_at, .. } => Some(seen_at),
        Preview::Facts { .. } => None,
    }
}

#[test]
fn a_tab_left_behind_previews_its_last_frame_with_its_age() {
    let snapshot = snapshot();
    let agent = surface("boot-v1");
    let logs = streaming("boot-v1", "w1:p2");
    let mut seen = LastSeen::default();
    seen.observe(
        (Some(&snapshot), Some(&agent)),
        (Some(&snapshot), Some(&logs)),
        100,
    );
    let look =
        |seen: &LastSeen, target| preview(Some(&snapshot), Some(&logs), seen, "boot-v1", target);
    // The agent's tab is no longer streamed, but its last frame is on hand.
    assert_eq!(
        shown(look(&seen, NavigationTarget::Pane("w1:p1"))),
        Some(INNER)
    );
    assert_eq!(
        seen_at(look(&seen, NavigationTarget::Pane("w1:p1"))),
        Some(Some(100))
    );
    assert_eq!(
        seen_at(look(&seen, NavigationTarget::Tab("w1:t1"))),
        Some(Some(100))
    );
    // The streamed tab is live, never a remembered frame.
    assert_eq!(
        seen_at(look(&seen, NavigationTarget::Pane("w1:p2"))),
        Some(None)
    );
    // Going back forgets the frame: live cells take over.
    seen.observe(
        (Some(&snapshot), Some(&logs)),
        (Some(&snapshot), Some(&agent)),
        200,
    );
    assert_eq!(
        seen.frames("boot-v1").map(|(_, at)| at).collect::<Vec<_>>(),
        [200]
    );
    assert_eq!(
        seen_at(preview(
            Some(&snapshot),
            Some(&agent),
            &seen,
            "boot-v1",
            NavigationTarget::Pane("w1:p1")
        )),
        Some(None)
    );
}

#[test]
fn patches_within_one_tab_remember_nothing() {
    let snapshot = snapshot();
    let before = surface("boot-v1");
    let after = surface("boot-v1");
    let mut seen = LastSeen::default();
    seen.observe(
        (Some(&snapshot), Some(&before)),
        (Some(&snapshot), Some(&after)),
        100,
    );
    assert_eq!(seen.frames("boot-v1").count(), 0);
}

#[test]
fn a_new_daemon_boot_forgets_every_frame() {
    let snapshot = snapshot();
    let mut seen = LastSeen::default();
    seen.observe(
        (Some(&snapshot), Some(&surface("boot-v1"))),
        (Some(&snapshot), Some(&streaming("boot-v1", "w1:p2"))),
        100,
    );
    let mut restarted = snapshot.clone();
    restarted.boot_id = "boot-v2".into();
    seen.observe((Some(&snapshot), None), (Some(&restarted), None), 200);
    assert_eq!(seen.frames("boot-v1").count(), 0);
    assert_eq!(seen.frames("boot-v2").count(), 0);
}

#[test]
fn closed_panes_and_tabs_are_never_previewed_from_memory() {
    let snapshot = snapshot();
    let agent = surface("boot-v1");
    let mut seen = LastSeen::default();
    seen.observe(
        (Some(&snapshot), Some(&agent)),
        (Some(&snapshot), None),
        100,
    );
    let mut closed = snapshot.clone();
    closed.panes.retain(|pane| pane.pane_id != "w1:p1");
    assert!(
        preview(
            Some(&closed),
            None,
            &seen,
            "boot-v1",
            NavigationTarget::Pane("w1:p1")
        )
        .is_none()
    );
    // A tab already gone when it is left is not worth a slot.
    let mut gone = snapshot.clone();
    gone.tabs.retain(|tab| tab.tab_id != "w1:t1");
    let mut fresh = LastSeen::default();
    fresh.observe((Some(&snapshot), Some(&agent)), (Some(&gone), None), 100);
    assert_eq!(fresh.frames("boot-v1").count(), 0);
}

/// A frame for the tab whose only pane is `pane`, holding 1.5 MiB of cells.
fn heavy(pane: &str) -> Arc<PaneSurfaceFrame> {
    let mut surface = Arc::unwrap_or_clone(streaming("boot-v1", pane));
    let cell = sheprd_client::protocol::CellData {
        symbol: String::new(),
        fg: 0,
        bg: 0,
        modifier: 0,
        skip: false,
        hyperlink: None,
    };
    surface.frame.cells = vec![cell; (3 << 19) / size_of::<sheprd_client::protocol::CellData>()];
    Arc::new(surface)
}

/// Leaves the tab of each of `panes` in turn, at times 1, 2, ...
fn leave_each(snapshot: &ClientShellSnapshot, panes: &[&str]) -> LastSeen {
    let mut seen = LastSeen::default();
    for (at, pane) in (1..).zip(panes) {
        seen.observe(
            (Some(snapshot), Some(&heavy(pane))),
            (Some(snapshot), None),
            at,
        );
    }
    seen
}

fn ages(seen: &LastSeen) -> Vec<u64> {
    seen.frames("boot-v1").map(|(_, at)| at).collect()
}

#[test]
fn a_host_keeps_its_newest_frames_within_its_memory_budget() {
    let snapshot = many_tabs(3);
    // Two 1.5 MiB frames fit in 4 MiB; a third pushes out the oldest.
    let seen = leave_each(&snapshot, &["w1:p3", "w1:p4", "w1:p5"]);
    assert_eq!(ages(&seen), [3, 2]);
    // Small frames are bounded by count instead.
    let snapshot = many_tabs(80);
    let mut seen = LastSeen::default();
    for n in 3..83u64 {
        let pane = format!("w1:p{n}");
        seen.observe(
            (Some(&snapshot), Some(&streaming("boot-v1", &pane))),
            (Some(&snapshot), None),
            n,
        );
    }
    assert_eq!(ages(&seen), (19..83).rev().collect::<Vec<_>>());
}

#[test]
fn hosts_together_keep_their_newest_frames_within_the_total_budget() {
    let snapshot = many_tabs(6);
    // Three hosts with 3 MiB each hold 9 MiB; the oldest frame of any goes.
    let mut a = leave_each(&snapshot, &["w1:p3", "w1:p4"]);
    let mut b = leave_each(&snapshot, &["w1:p5", "w1:p6"]);
    let mut c = leave_each(&snapshot, &["w1:p7", "w1:p8"]);
    b.observe(
        (Some(&snapshot), Some(&heavy("w1:p6"))),
        (Some(&snapshot), None),
        10,
    );
    LastSeen::trim([&mut a, &mut b, &mut c]);
    assert_eq!(
        (ages(&a), ages(&b), ages(&c)),
        (vec![2], vec![10, 1], vec![2, 1])
    );
}

#[test]
fn popups_and_images_count_against_the_budget() {
    use sheprd_client::protocol::{
        ClientShellPopupSurface, SurfaceGraphicsAsset, SurfaceGraphicsAssetKey,
        SurfaceGraphicsFormat, SurfaceGraphicsSource, SurfaceGraphicsTarget,
    };
    let snapshot = many_tabs(4);
    let mut seen = LastSeen::default();
    let leave = |seen: &mut LastSeen, surface: PaneSurfaceFrame, at| {
        seen.observe(
            (Some(&snapshot), Some(&Arc::new(surface))),
            (Some(&snapshot), None),
            at,
        );
    };
    // Each frame's grid is empty; a popup or an image holds 3 MiB.
    let mut popup = Arc::unwrap_or_clone(streaming("boot-v1", "w1:p3"));
    let mut grid = Arc::unwrap_or_clone(heavy("w1:p3")).frame;
    grid.cells.extend(grid.cells.clone());
    popup.popup = Some(Box::new(ClientShellPopupSurface {
        terminal_id: "popup".into(),
        title: "Popup".into(),
        width: None,
        height: None,
        frame: grid,
        mouse_reporting: false,
        sgr_pixel_mouse: false,
        pixel_width: 0,
        pixel_height: 0,
    }));
    let mut image = Arc::unwrap_or_clone(streaming("boot-v1", "w1:p4"));
    image.graphics.assets.push(SurfaceGraphicsAsset {
        key: SurfaceGraphicsAssetKey {
            source: SurfaceGraphicsSource::Terminal {
                target: SurfaceGraphicsTarget::Pane {
                    pane_id: "w1:p4".into(),
                },
                image_id: 1,
            },
            image_width: 1024,
            image_height: 768,
            format: SurfaceGraphicsFormat::Rgba,
            data_len: 3 << 20,
            data_fingerprint: 0,
        },
        data: vec![0; 3 << 20],
    });
    leave(&mut seen, popup, 1);
    assert_eq!(ages(&seen), [1]);
    // Two 3 MiB frames exceed 4 MiB, so only the newest stays.
    leave(&mut seen, image, 2);
    assert_eq!(ages(&seen), [2]);
}
