use super::*;
use sheprd_client::protocol::{FrameData, PaneSurfaceFrame};

fn surface(snapshot: &ClientShellSnapshot) -> Arc<PaneSurfaceFrame> {
    Arc::new(PaneSurfaceFrame {
        boot_id: snapshot.boot_id.clone(),
        projection_revision: snapshot.revision,
        surface_revision: 1,
        frame: FrameData {
            cells: vec![],
            width: 0,
            height: 0,
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

/// The agent changed status in metadata that waits for its surface, so the
/// displayed snapshot still says it is working.
#[test]
fn staged_agent_status_validates_delayed_sounds_once() {
    for (kind, status) in [
        (Kind::NeedsAttention, AgentStatus::Blocked),
        (Kind::Finished, AgentStatus::Done),
    ] {
        let now = Instant::now();
        let mut state = live(AgentStatus::Working);
        let displayed = state.snapshot.clone().unwrap();
        state.apply(ClientEvent::Surface(surface(&displayed)));
        let mut staged = (*displayed).clone();
        staged.revision += 1;
        staged.agents[0].agent_status = status;
        let staged = Arc::new(staged);
        state.apply(ClientEvent::Snapshot(staged.clone()));
        assert!(Arc::ptr_eq(state.snapshot.as_ref().unwrap(), &displayed));
        state.sound_events.push_back((now, event(kind)));
        let mut policy = Policy::default();
        let mut sounds = 0;
        for at in [now, now + Duration::from_secs(1)] {
            policy.poll(&mut state, Some(1), false, at, |_| sounds += 1);
        }
        assert_eq!(sounds, 1, "{kind:?}");
        state.apply(ClientEvent::Surface(surface(&staged)));
        assert!(Arc::ptr_eq(state.snapshot.as_ref().unwrap(), &staged));
        policy.poll(
            &mut state,
            Some(1),
            false,
            now + Duration::from_secs(5),
            |_| panic!("replayed sound"),
        );
    }
}
