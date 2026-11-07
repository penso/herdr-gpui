use super::*;

#[test]
fn the_single_workspace_move_follows_each_welcome() {
    let welcome = |methods: &[&str]| {
        ClientEvent::Connected(sheprd_client::protocol::endpoint::EndpointServerWelcome {
            generation: 1,
            server_version: "test".into(),
            snapshot_codec: String::new(),
            surface_codec: String::new(),
            input_codec: String::new(),
            blob_codec: String::new(),
            methods: methods.iter().map(|method| (*method).into()).collect(),
            capabilities: Vec::new(),
            error: None,
        })
    };
    let mut state = LiveState::default();
    state.apply(welcome(&["workspace.move_block", "workspace.move"]));
    assert!(state.supports_workspace_move);
    // A reconnect to a daemon without it forgets the previous answer.
    state.apply(welcome(&["workspace.move_block"]));
    assert!(!state.supports_workspace_move);
}
