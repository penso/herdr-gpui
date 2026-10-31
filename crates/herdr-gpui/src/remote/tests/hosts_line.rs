use super::*;
use herdr_remote_control::control::{
    DeviceId,
    hosts::{Enrolled, HostId, HostLink, HostState},
};

fn link(name: &str, state: HostState) -> HostLink {
    HostLink {
        host: HostId(format!("ssh:{name}")),
        name: name.into(),
        state,
    }
}

#[test]
fn a_phones_machines_read_by_where_they_stand() {
    let mut device = Device {
        id: DeviceId("d1".into()),
        name: "iPhone".into(),
        created_ms: 0,
        last_seen_ms: None,
        push: false,
        connected: false,
        via: None,
        hosts: Vec::new(),
    };
    assert_eq!(hosts_line(&device), None);
    device.hosts = vec![
        link(
            "devbox",
            HostState::Paired(Enrolled {
                device: DeviceId("h1".into()),
                computer: "devbox".into(),
                fingerprint: "AB".into(),
                origins: Vec::new(),
            }),
        ),
        link("gpu", HostState::Allowed),
        link("lab", HostState::Asking),
        link("ci", HostState::Pending),
        link("old", HostState::Failed { reason: "x".into() }),
        link("other", HostState::Offered),
    ];
    assert_eq!(
        hosts_line(&device).as_deref(),
        Some("Also on devbox \u{b7} pairing lab, ci \u{b7} failed on old")
    );
}
