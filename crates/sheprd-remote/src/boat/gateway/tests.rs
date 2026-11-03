#![allow(clippy::unwrap_used)]
use super::*;
use std::fs;

mod paths;
#[cfg(unix)]
mod teleport;

fn access() -> SshAccess {
    SshAccess {
        machine_ip: Some("203.0.113.10".into()),
        ssh_endpoint: None,
        ssh_user: "user".into(),
        host_key: "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAfixture".into(),
    }
}

#[test]
fn builds_a_strict_pinned_command_for_direct_and_forwarded_hosts() {
    let dir = tempfile::tempdir().unwrap();
    let private = dir.path().join("key");
    fs::write(&private, "fixture").unwrap();
    let gateway = Gateway::new(&access(), &private).unwrap();
    let args: Vec<_> = gateway
        .command
        .get_args()
        .map(|arg| arg.to_string_lossy())
        .collect();
    assert!(args.iter().any(|arg| arg == "StrictHostKeyChecking=yes"));
    assert!(args.iter().any(|arg| arg == "203.0.113.10"));
    assert!(args.iter().any(|arg| arg == "22"));

    let mut forwarded = access();
    forwarded.ssh_endpoint = Some("198.51.100.7:22001".into());
    let gateway = Gateway::new(&forwarded, &private).unwrap();
    let args: Vec<_> = gateway
        .command
        .get_args()
        .map(|arg| arg.to_string_lossy())
        .collect();
    assert!(args.iter().any(|arg| arg == "198.51.100.7"));
    assert!(args.iter().any(|arg| arg == "22001"));
}

#[test]
fn rejects_untrusted_ssh_fields() {
    let dir = tempfile::tempdir().unwrap();
    let private = dir.path().join("key");
    fs::write(&private, "fixture").unwrap();
    for bad in [
        SshAccess {
            ssh_user: "-oProxyCommand=x".into(),
            ..access()
        },
        SshAccess {
            host_key: "ssh-ed25519 value\nmalicious".into(),
            ..access()
        },
        SshAccess {
            machine_ip: Some("host name".into()),
            ..access()
        },
        SshAccess {
            ssh_endpoint: Some("203.0.113.2:0".into()),
            ..access()
        },
    ] {
        assert!(Gateway::new(&bad, &private).is_err());
    }
}
