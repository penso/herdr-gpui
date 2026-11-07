use super::*;
use crate::{control::batch::BatchKind, notification_mutes::Mutes};

fn device(endpoint: &str, state: DeviceState) -> crate::control::batch::DeviceInfo {
    crate::control::batch::DeviceInfo {
        provider: "coder".into(),
        id: endpoint.into(),
        endpoint: endpoint.into(),
        label: endpoint.into(),
        state,
        step: None,
        error: None,
    }
}

#[gpui::test]
fn only_the_devices_a_batch_removed_lose_their_mutes(cx: &mut TestAppContext) {
    cx.update(|cx| {
        for id in ["gone", "forgotten", "failed"] {
            Mutes::update(cx, |mutes| mutes.set_host(id, true));
        }
        let batch = BatchInfo {
            id: 1,
            kind: BatchKind::Destroy,
            phase: Phase::Done,
            devices: vec![
                device("gone", DeviceState::Destroyed),
                device("forgotten", DeviceState::Forgotten),
                device("failed", DeviceState::Failed),
            ],
        };
        forget_mutes(&batch, cx);
        let mutes = Mutes::of(cx).unwrap();
        assert!(!mutes.host("gone") && !mutes.host("forgotten"));
        assert!(mutes.host("failed"));
    });
}
