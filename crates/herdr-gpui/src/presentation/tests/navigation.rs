use super::*;
use herdr_client::protocol::{CellData, PaneSurfacePane, SurfaceRect};

fn visited(index: usize) -> LiveState {
    let mut live = connected("boot", index as u64 + 1);
    let snapshot = Arc::make_mut(live.snapshot.as_mut().unwrap());
    let template: ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap();
    snapshot.workspaces.clear();
    snapshot.tabs.clear();
    snapshot.panes.clear();
    for i in 0..3 {
        let mut workspace = template.workspaces[0].clone();
        workspace.workspace_id = format!("w{i}");
        workspace.active_tab_id = format!("t{i}");
        workspace.focused = i == index;
        snapshot.workspaces.push(workspace);
        let mut tab = template.tabs[0].clone();
        tab.workspace_id = format!("w{i}");
        tab.tab_id = format!("t{i}");
        tab.focused = i == index;
        snapshot.tabs.push(tab);
        let mut pane = template.panes[0].clone();
        pane.workspace_id = format!("w{i}");
        pane.tab_id = format!("t{i}");
        pane.pane_id = format!("p{i}");
        pane.focused = i == index;
        snapshot.panes.push(pane);
    }
    snapshot.focused_workspace_id = Some(format!("w{index}"));
    snapshot.focused_tab_id = Some(format!("t{index}"));
    snapshot.focused_pane_id = Some(format!("p{index}"));
    let surface = Arc::make_mut(live.surface.as_mut().unwrap());
    surface.frame.width = 1;
    surface.frame.height = 1;
    surface.frame.cells = vec![CellData {
        symbol: index.to_string(),
        fg: 0,
        bg: 0,
        modifier: 0,
        skip: false,
        hyperlink: None,
    }];
    let rect = SurfaceRect {
        x: 0,
        y: 0,
        width: 1,
        height: 1,
    };
    surface.panes.push(PaneSurfacePane {
        pane_id: format!("p{index}"),
        content_revision: 1,
        rect,
        inner_rect: rect,
        scrollbar_rect: None,
        scroll: None,
        focused: true,
        mouse_reporting: false,
        sgr_pixel_mouse: false,
        alternate_screen_active: false,
        pixel_width: 0,
        pixel_height: 0,
    });
    live
}

fn preview(
    presentation: &mut Presentation,
    live: &LiveState,
    target: &str,
) -> Arc<PaneSurfaceFrame> {
    presentation
        .navigation_picture(live, Some(NavigationTarget::Workspace(target)), (1, 1))
        .unwrap()
        .frame
}

#[test]
fn revisiting_a_worktree_draws_its_screen_without_waiting_for_any_response() {
    let mut presentation = Presentation::default();
    let a = visited(0);
    let b = visited(1);
    presentation.remember(&a, true);
    presentation.remember(&b, true);
    let mut waiting = b.clone();
    waiting.surface = None;
    assert!(Arc::ptr_eq(
        &preview(&mut presentation, &waiting, "w0"),
        a.surface.as_ref().unwrap()
    ));
    assert!(!waiting.surface_ready());
    for target in [NavigationTarget::Tab("t0"), NavigationTarget::Pane("p0")] {
        let picture = presentation
            .navigation_picture(&waiting, Some(target), (1, 1))
            .unwrap();
        assert!(Arc::ptr_eq(&picture.frame, a.surface.as_ref().unwrap()));
    }
    assert_eq!(
        waiting
            .snapshot
            .as_ref()
            .unwrap()
            .focused_workspace_id
            .as_deref(),
        Some("w1")
    );
    // A preview never becomes the authoritative last presented frame.
    assert!(Arc::ptr_eq(
        &presentation.frame(&waiting).unwrap(),
        b.surface.as_ref().unwrap()
    ));
    // Once coherent, the fresh destination wins over its old picture.
    let mut fresh = a.clone();
    Arc::make_mut(fresh.surface.as_mut().unwrap()).frame.cells[0].symbol = "fresh".into();
    assert!(Arc::ptr_eq(
        &preview(&mut presentation, &fresh, "w0"),
        fresh.surface.as_ref().unwrap()
    ));
}

#[test]
fn rapid_navigation_previews_the_latest_destination_and_misses_keep_the_last_screen() {
    let mut presentation = Presentation::default();
    for index in 0..3 {
        presentation.remember(&visited(index), true);
    }
    let mut waiting = visited(2);
    waiting.surface = None;
    for (target, text) in [("w0", "0"), ("w1", "1"), ("unknown", "2")] {
        assert_eq!(
            preview(&mut presentation, &waiting, target).frame.cells[0].symbol,
            text
        );
    }
}

#[test]
fn previews_reject_resize_reboot_closed_panes_and_changed_zoom() {
    let mut presentation = Presentation::default();
    presentation.remember(&visited(0), true);
    let b = visited(1);
    presentation.remember(&b, true);
    let mut waiting = b.clone();
    waiting.surface = None;
    let resized = presentation
        .navigation_picture(&waiting, Some(NavigationTarget::Workspace("w0")), (2, 1))
        .unwrap();
    assert!(Arc::ptr_eq(&resized.frame, b.surface.as_ref().unwrap()));
    for changed in ["closed", "zoomed", "tab", "boot"] {
        let mut live = waiting.clone();
        let snapshot = Arc::make_mut(live.snapshot.as_mut().unwrap());
        match changed {
            "closed" => snapshot.panes.retain(|pane| pane.pane_id != "p0"),
            "zoomed" => snapshot.tabs[0].zoomed = true,
            "tab" => snapshot.workspaces[0].active_tab_id = "another".into(),
            _ => snapshot.boot_id = "another-boot".into(),
        }
        let picture =
            presentation.navigation_picture(&live, Some(NavigationTarget::Workspace("w0")), (1, 1));
        if changed == "boot" {
            assert!(picture.is_none());
        } else {
            assert!(Arc::ptr_eq(
                &picture.unwrap().frame,
                b.surface.as_ref().unwrap()
            ));
        }
    }
}

#[test]
fn reconnect_and_clear_forget_other_worktrees() {
    for hold in [true, false] {
        let mut presentation = Presentation::default();
        presentation.remember(&visited(0), true);
        let b = visited(1);
        presentation.remember(&b, true);
        if hold {
            presentation.hold();
            presentation.resume();
        } else {
            presentation.clear();
        }
        presentation.frame(&b);
        let mut waiting = b.clone();
        waiting.surface = None;
        assert!(Arc::ptr_eq(
            &preview(&mut presentation, &waiting, "w0"),
            b.surface.as_ref().unwrap()
        ));
    }
}
