use super::*;

fn ready() -> LiveState {
    let snapshot = snapshot();
    let mut state = LiveState::default();
    state.apply(ClientEvent::Snapshot(snapshot.clone()));
    state.apply(ClientEvent::Surface(surface(&snapshot)));
    assert!(state.surface_ready());
    state
}

#[test]
fn metadata_gaps_keep_one_coherent_pair_until_latest_surface_commits() {
    let mut state = ready();
    let old = state.snapshot.clone().unwrap();
    let old_surface = state.surface.clone().unwrap();
    for revision in old.revision + 1..old.revision + 40 {
        let mut next = (*old).clone();
        next.revision = revision;
        next.agents[0].title = Some(format!("spinner {revision}"));
        state.apply(ClientEvent::Snapshot(Arc::new(next)));
        assert!(state.surface_ready());
        assert!(Arc::ptr_eq(state.snapshot.as_ref().unwrap(), &old));
        assert!(Arc::ptr_eq(state.surface.as_ref().unwrap(), &old_surface));
        assert_eq!(state.pending_snapshot.as_ref().unwrap().revision, revision);
    }
    let latest = state.pending_snapshot.clone().unwrap();
    state.apply(ClientEvent::Surface(surface(&old)));
    assert!(Arc::ptr_eq(state.snapshot.as_ref().unwrap(), &old));
    state.apply(ClientEvent::Surface(surface(&latest)));
    assert!(state.surface_ready());
    assert!(Arc::ptr_eq(state.snapshot.as_ref().unwrap(), &latest));
    assert_eq!(
        state.surface.as_ref().unwrap().projection_revision,
        latest.revision
    );
    assert!(state.pending_snapshot.is_none());
}

#[test]
fn focus_topology_and_boot_changes_invalidate_input_immediately() {
    type Change = fn(&mut ClientShellSnapshot);
    for change in [
        |s: &mut ClientShellSnapshot| s.focused_pane_id = Some("different".into()),
        |s: &mut ClientShellSnapshot| s.focused_tab_id = Some("different".into()),
        |s: &mut ClientShellSnapshot| s.focused_workspace_id = Some("different".into()),
        |s: &mut ClientShellSnapshot| s.tabs[0].zoomed = !s.tabs[0].zoomed,
        |s: &mut ClientShellSnapshot| {
            s.panes[0].right_click_passthrough = !s.panes[0].right_click_passthrough
        },
        |s: &mut ClientShellSnapshot| {
            s.panes.remove(0);
        },
        |s: &mut ClientShellSnapshot| s.boot_id = "reboot".into(),
        |s: &mut ClientShellSnapshot| s.server_keybindings_toml = Some("changed".into()),
        |s: &mut ClientShellSnapshot| s.commands.clear(),
    ] as [Change; 9]
    {
        let mut state = ready();
        let mut next = (*state.snapshot.clone().unwrap()).clone();
        next.revision += 1;
        change(&mut next);
        state.apply(ClientEvent::Snapshot(Arc::new(next)));
        assert!(!state.surface_ready());
        assert!(state.surface.is_none());
        assert!(state.pending_snapshot.is_none());
    }
}

#[test]
fn stale_input_changing_snapshot_cannot_replace_newer_staged_metadata() {
    let mut state = ready();
    let old = state.snapshot.clone().unwrap();
    let mut latest = (*old).clone();
    latest.revision += 2;
    state.apply(ClientEvent::Snapshot(Arc::new(latest.clone())));
    let mut stale = (*old).clone();
    stale.revision += 1;
    stale.focused_pane_id = Some("retired".into());
    state.apply(ClientEvent::Snapshot(Arc::new(stale)));
    assert!(state.surface_ready());
    assert!(Arc::ptr_eq(state.snapshot.as_ref().unwrap(), &old));
    assert_eq!(
        state.pending_snapshot.as_ref().unwrap().revision,
        latest.revision
    );
    state.apply(ClientEvent::Surface(surface(&latest)));
    assert!(state.surface_ready());
    assert_eq!(state.snapshot.as_ref().unwrap().revision, latest.revision);
}

#[test]
fn staged_metadata_cannot_satisfy_changed_focus_surface_barrier_in_either_ack_order() {
    for ack_first in [false, true] {
        let mut state = ready();
        let old = state.snapshot.clone().unwrap();
        let mut pending = (*old).clone();
        pending.revision += 1;
        state.apply(ClientEvent::Snapshot(Arc::new(pending.clone())));
        state.activation = Some(SurfaceActivation {
            request: "focus".into(),
            boot: old.boot_id.clone(),
            revision: None,
            failed: false,
            focus: Some(crate::NavigationTarget::Pane("destination".into())),
            active: true,
        });
        state.surface = None;
        let ack = ClientEvent::Response {
            request_id: "focus".into(),
            response: serde_json::json!({"result": {
                "type": "client_shell_surface_set", "active": true,
                "projection_revision": pending.revision + 1
            }}),
        };
        let frame = ClientEvent::Surface(surface(&pending));
        for event in if ack_first {
            [ack, frame]
        } else {
            [frame, ack]
        } {
            state.apply(event);
            assert!(!state.surface_ready());
            assert!(state.activation.as_ref().unwrap().focus.is_some());
        }
        let mut destination = pending;
        destination.revision += 1;
        destination.focused_pane_id = Some("destination".into());
        state.apply(ClientEvent::Snapshot(Arc::new(destination.clone())));
        assert!(!state.surface_ready());
        state.apply(ClientEvent::Surface(surface(&destination)));
        assert!(state.surface_ready());
        assert!(state.activation.as_ref().unwrap().focus.is_none());
    }
}

#[test]
fn staged_metadata_cannot_satisfy_same_target_surface_activation_below_its_floor() {
    for ack_first in [false, true] {
        let mut state = ready();
        let old = state.snapshot.clone().unwrap();
        let mut pending = (*old).clone();
        pending.revision += 1;
        state.apply(ClientEvent::Snapshot(Arc::new(pending.clone())));
        let mut activated = pending.clone();
        activated.revision += 1;
        state.activation = Some(SurfaceActivation {
            request: "activate".into(),
            boot: old.boot_id.clone(),
            revision: None,
            failed: false,
            focus: old
                .focused_pane_id
                .clone()
                .map(crate::NavigationTarget::Pane),
            active: true,
        });
        state.surface = None;
        let ack = ClientEvent::Response {
            request_id: "activate".into(),
            response: serde_json::json!({"result": {
                "type": "client_shell_surface_set", "active": true,
                "projection_revision": activated.revision
            }}),
        };
        let frame = ClientEvent::Surface(surface(&pending));
        for event in if ack_first {
            [ack, frame]
        } else {
            [frame, ack]
        } {
            state.apply(event);
            assert!(!state.surface_ready());
        }
        state.apply(ClientEvent::Snapshot(Arc::new(activated.clone())));
        assert!(!state.surface_ready());
        state.apply(ClientEvent::Surface(surface(&activated)));
        assert!(state.surface_ready());
    }
}

#[test]
fn image_catalog_changes_commit_with_their_surface_and_do_not_survive_reboot() {
    use sheprd_client::protocol::{
        SurfaceGraphicsAsset, SurfaceGraphicsAssetKey, SurfaceGraphicsFormat,
        SurfaceGraphicsSource, SurfaceGraphicsTarget,
    };
    let mut state = ready();
    let key = SurfaceGraphicsAssetKey {
        source: SurfaceGraphicsSource::Terminal {
            target: SurfaceGraphicsTarget::Pane {
                pane_id: "w1:p1".into(),
            },
            image_id: 1,
        },
        image_width: 1,
        image_height: 1,
        format: SurfaceGraphicsFormat::Rgba,
        data_len: 4,
        data_fingerprint: 1,
    };
    state.surface_images = Arc::new(
        [SurfaceGraphicsAsset {
            key: key.clone(),
            data: vec![1, 2, 3, 4],
        }]
        .into_iter()
        .collect(),
    );
    Arc::make_mut(state.surface.as_mut().unwrap())
        .graphics
        .retained_assets
        .push(key.clone());
    let old_images = state.surface_images.clone();
    let mut next = (*state.snapshot.clone().unwrap()).clone();
    next.revision += 1;
    state.apply(ClientEvent::Snapshot(Arc::new(next.clone())));
    let new_images = Arc::new(SurfaceImages::default());
    state.apply(ClientEvent::SurfaceImages(new_images.clone()));
    assert!(Arc::ptr_eq(&state.surface_images, &old_images));
    assert!(state.surface_images.get(&key).is_some());
    assert_eq!(
        state.surface.as_ref().unwrap().graphics.retained_assets,
        std::slice::from_ref(&key)
    );
    assert!(Arc::ptr_eq(
        state.pending_surface_images.as_ref().unwrap(),
        &new_images
    ));
    state.apply(ClientEvent::Surface(surface(&next)));
    assert!(Arc::ptr_eq(&state.surface_images, &new_images));
    assert!(state.surface_images.get(&key).is_none());
    assert!(state.pending_surface_images.is_none());
    next.boot_id = "reboot".into();
    state.apply(ClientEvent::Snapshot(Arc::new(next)));
    assert!(!Arc::ptr_eq(&state.surface_images, &new_images));
    assert!(state.surface_images.is_empty());
    assert!(state.pending_surface_images.is_none());
}

#[test]
fn disconnect_discards_staged_pair_and_surface_images() {
    let mut state = ready();
    let mut next = (*state.snapshot.clone().unwrap()).clone();
    next.revision += 1;
    state.apply(ClientEvent::Snapshot(Arc::new(next)));
    state.apply(ClientEvent::SurfaceImages(Arc::default()));
    state.apply(ClientEvent::Disconnected {
        reason: "closed".into(),
        ssh: None,
    });
    assert!(state.snapshot.is_none() && state.surface.is_none());
    assert!(state.pending_snapshot.is_none() && state.pending_surface_images.is_none());
    assert!(!state.surface_ready());
}

#[test]
fn faster_metadata_bursts_release_superseded_snapshots_and_keep_surfaces_advancing() {
    let mut state = ready();
    for _ in 0..100 {
        let previous = state.snapshot.as_ref().unwrap().revision;
        let mut staged = Vec::new();
        for revision in previous + 1..=previous + 32 {
            let mut next = (**state.snapshot.as_ref().unwrap()).clone();
            next.revision = revision;
            next.agents[0].title = Some(format!("metadata {revision}"));
            let next = Arc::new(next);
            staged.push(Arc::downgrade(&next));
            state.apply(ClientEvent::Snapshot(next));
            assert!(state.surface_ready());
            assert_eq!(state.snapshot.as_ref().unwrap().revision, previous);
        }
        assert_eq!(
            staged
                .iter()
                .filter(|snapshot| snapshot.strong_count() > 0)
                .count(),
            1
        );
        let latest = state.pending_snapshot.clone().unwrap();
        state.apply(ClientEvent::Surface(surface(&latest)));
        assert_eq!(state.snapshot.as_ref().unwrap().revision, previous + 32);
        assert!(state.pending_snapshot.is_none());
        assert!(state.surface_ready());
    }
}
