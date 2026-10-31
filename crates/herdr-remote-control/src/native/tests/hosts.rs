//! `GET /api/v2/hosts` and `POST /api/v2/hosts/{host}/pair` answers, as
//! the Swift tests decode them: every state a host can be in.

use super::fixtures::check;
use crate::control::{
    DeviceId,
    hosts::{Enrolled, HostId, HostLink, HostState},
};
use serde::Serialize;

#[derive(Serialize)]
struct Hosts {
    hosts: Vec<HostLink>,
}

#[test]
fn host_fixtures_match() {
    let link = |id: &str, name: &str, state| HostLink {
        host: HostId(id.into()),
        name: name.into(),
        state,
    };
    check(
        "hosts",
        &Hosts {
            hosts: vec![
                link(
                    "ssh:devbox",
                    "devbox",
                    HostState::Paired(Enrolled {
                        device: DeviceId("d-devbox".into()),
                        computer: "devbox".into(),
                        fingerprint: "AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89".into(),
                        origins: vec!["https://100.64.0.7:7443".into()],
                    }),
                ),
                link("ssh:gpu", "gpu box", HostState::Allowed),
                link("coder:ws1", "workspace", HostState::Offered),
                link("ssh:ci", "ci", HostState::Pending),
                link("ssh:lab", "lab", HostState::Asking),
                link(
                    "ssh:old",
                    "old box",
                    HostState::Failed {
                        reason: "No build for Linux riscv64 in this app".into(),
                    },
                ),
            ],
        },
    );
    check("host_state", &HostState::Pending);
}
