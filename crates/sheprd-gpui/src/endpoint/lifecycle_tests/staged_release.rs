use super::*;

#[gpui::test]
fn releasing_a_surface_publishes_metadata_that_no_longer_needs_matching_cells(
    cx: &mut gpui::TestAppContext,
) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (origin, _origin_server) = connected_endpoint("release-origin");
    let (destination, _destination_server) = connected_endpoint("release-destination");
    view.update(cx, |view, cx| {
        prepare_mouse(view, origin, cx);
        view.endpoints.push(destination);
        let mut next = (**view.live.snapshot.as_ref().unwrap()).clone();
        next.revision += 1;
        next.agents[0].title = Some("latest metadata".into());
        let revision = next.revision;
        let inbox = view.endpoints[1].connection.inbox.clone();
        inbox
            .lock()
            .unwrap()
            .apply(ClientEvent::Snapshot(Arc::new(next)));
        project_until(view, cx, "staged metadata", |view| {
            view.live.pending_snapshot.is_some()
        });
        assert!(view.select_endpoint("release-destination", cx));
        let state = inbox.lock().unwrap();
        assert_eq!(state.snapshot.as_ref().unwrap().revision, revision);
        assert!(state.pending_snapshot.is_none());
        assert!(state.surface.is_none());
        assert!(!state.surface_ready());
        assert_eq!(
            view.endpoints[1].live.snapshot.as_ref().unwrap().revision,
            revision
        );
    });
}
