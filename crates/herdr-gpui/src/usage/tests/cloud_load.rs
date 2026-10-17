use crate::usage::Host;
use herdr_client::{CloudProvider, ConnectTarget};

fn azure(id: &str) -> ConnectTarget {
    ConnectTarget::Cloud {
        provider: CloudProvider::Azure,
        account: "00000000-0000-0000-0000-000000000000".into(),
        id: id.into(),
        machine: "box".into(),
        session: "default".into(),
    }
}

#[test]
fn an_azure_machine_has_a_load_host_but_no_usage_host() {
    for id in ["herdr-vm", "sandbox.11111111-1111-1111-1111-111111111111"] {
        let target = azure(id);
        // Agent usage, ports, and checkpoints still skip it.
        assert_eq!(Host::of(&target), None);
        assert_eq!(
            Host::load_of(&target),
            Some(Host::Azure {
                account: "00000000-0000-0000-0000-000000000000".into(),
                id: id.into(),
            })
        );
    }
}

#[test]
fn other_hosts_load_where_they_are_used() {
    for target in [
        ConnectTarget::Local,
        ConnectTarget::Ssh {
            target: "build".into(),
            session: "default".into(),
        },
    ] {
        assert_eq!(Host::load_of(&target), Host::of(&target));
    }
}
