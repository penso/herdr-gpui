use super::*;

/// A resize that settles while Cmd-T's `tab.create` is in flight must not
/// claim the tab the snapshot still names: requests go one at a time, so
/// the claim would reach the daemon after the new tab and focus the old one
/// again, leaving the new tab hidden.
#[gpui::test]
fn a_resize_during_a_focus_change_does_not_claim_the_tab_being_left(cx: &mut gpui::TestAppContext) {
    let (endpoint, mut server) = connected_endpoint("resize-claim-fence");
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.read_with(cx, |fixture, _| fixture.0.clone());
    let start = Instant::now();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.endpoints = vec![endpoint];
            view.options = ConnectOptions::default();
            view.reset_selected(cx);
            view.activation_deadline = None;
            view.last_queued_options = Some(view.options);
            view.pending_resize = None;
            view.command(Command::Tab, window, cx);
            crate::menu::workspace_tests::submit_dialog(view, window, cx);
            assert!(view.activation_deadline.is_some());
            // A late font or scale change: the cell size moves, the grid not.
            view.options.cell_width_px += 1;
            view.resize_at(start);
            view.resize_at(start + Duration::from_secs(1));
            view.endpoints[0]
                .connection
                .handle
                .as_ref()
                .unwrap()
                .set_focus(&snapshot().boot_id, false)
                .unwrap();
        })
    });
    let mut methods = Vec::new();
    loop {
        match server.receive() {
            ClientMessage::ClientShellEndpointRequest { request, .. } => {
                let request: serde_json::Value = serde_json::from_str(&request).unwrap();
                methods.push(request["method"].as_str().unwrap_or_default().to_owned());
                server.respond(&request);
            }
            ClientMessage::ClientShellFocus { focused: false } => break,
            _ => {}
        }
    }
    assert!(
        methods.iter().any(|method| method == "tab.create"),
        "{methods:?}"
    );
    assert!(
        !methods.iter().any(|method| method == "tab.focus"),
        "the resize claimed the old tab: {methods:?}"
    );
}
