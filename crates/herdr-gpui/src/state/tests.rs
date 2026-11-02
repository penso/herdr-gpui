use super::*;
use herdr_client::protocol::FrameData;

mod agent_recency;
mod agent_status;
mod announcements;
mod atomic_projection;
mod checkpoint_turns;
mod cleanup_slot;
mod connection_status;
mod daemon_messages;
mod daemon_version;
mod focus_acks;
mod landing_slot;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod local_peer_warning;
mod requests;
mod surfaces;
mod version_mismatch;
mod workspace_move;

fn snapshot() -> Arc<ClientShellSnapshot> {
    Arc::new(
        serde_json::from_str(include_str!(
            "../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
        ))
        .unwrap(),
    )
}

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
