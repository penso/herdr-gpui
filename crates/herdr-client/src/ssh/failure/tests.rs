#![allow(clippy::unwrap_used)]
use super::*;
use std::{
    net::IpAddr,
    process::{Command, Stdio},
};

#[test]
fn ssh_failures_are_classified_from_exit_code_and_stderr() {
    let cases: [(Option<i32>, &str, SshFailure); 12] = [
        (
            Some(255),
            "Host key verification failed.\r\n",
            SshFailure::HostKey,
        ),
        (
            Some(255),
            "@@@ WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED! @@@\n",
            SshFailure::HostKey,
        ),
        (
            Some(255),
            "me@host: Permission denied (publickey).\n",
            SshFailure::Auth,
        ),
        (
            Some(255),
            "Received disconnect: Too many authentication failures\n",
            SshFailure::Auth,
        ),
        (
            Some(255),
            "ssh: Could not resolve hostname nope: nodename nor servname provided\n",
            SshFailure::Unreachable,
        ),
        (
            Some(255),
            "ssh: connect to host h port 22: Operation timed out\n",
            SshFailure::Unreachable,
        ),
        (
            Some(255),
            "ssh: connect to host 10.0.0.19 port 22: No route to host\n",
            if cfg!(target_os = "macos") {
                SshFailure::LocalNetworkDenied
            } else {
                SshFailure::NoRoute
            },
        ),
        // Only local network addresses are gated by Local Network privacy.
        (
            Some(255),
            "ssh: connect to host 100.64.1.2 port 22: No route to host\n",
            SshFailure::NoRoute,
        ),
        (Some(255), "something new\n", SshFailure::Other),
        (Some(127), "", SshFailure::HerdrMissing),
        // The remote's own failures are never read as ssh's.
        (Some(1), "Permission denied\n", SshFailure::Other),
        (None, "Host key verification failed.\n", SshFailure::Other),
    ];
    for (code, stderr, expected) in cases {
        assert_eq!(
            SshFailure::classify(code, stderr.as_bytes()),
            expected,
            "{stderr}"
        );
    }
}

#[test]
fn only_local_network_addresses_count_as_local() {
    let local = |stderr: &str| {
        refused_host(stderr)
            .and_then(|host| host.parse().ok())
            .is_some_and(is_local)
    };
    assert!(local(
        "ssh: connect to host 192.168.1.4 port 22: No route to host"
    ));
    assert!(local(
        "ssh: connect to host 169.254.3.1 port 2222: No route to host"
    ));
    assert!(local(
        "ssh: connect to host fe80::1 port 22: No route to host"
    ));
    assert!(local(
        "ssh: connect to host fd12::7 port 22: No route to host"
    ));
    assert!(!local(
        "ssh: connect to host 8.8.8.8 port 22: No route to host"
    ));
    assert!(!local(
        "ssh: connect to host 100.101.102.103 port 22: No route to host"
    ));
    // Tailscale's IPv6 range is unique-local, but a tunnel rather than the LAN.
    assert!(!local(
        "ssh: connect to host fd7a:115c:a1e0::1 port 22: No route to host"
    ));
    assert!(local(
        "ssh: connect to host fd7a:115c:a1e1::1 port 22: No route to host"
    ));
    assert!(refused_host("ssh: Could not resolve hostname box: nodename nor servname").is_none());
}

/// `ssh` names the host as it was given, so a hostname is resolved before
/// deciding whether macOS gated it; an address is never looked up.
#[test]
fn a_hostname_is_resolved_before_its_route_is_classified() {
    let denied = "ssh: connect to host box.local port 22: No route to host\n";
    let classify = |stderr: &str, addresses: Vec<IpAddr>| {
        SshFailure::classify_with(Some(255), stderr.as_bytes(), |host| {
            assert_eq!(host, "box.local");
            addresses
        })
    };
    let lan = vec!["10.0.0.19".parse().unwrap()];
    assert_eq!(
        classify(denied, lan),
        if cfg!(target_os = "macos") {
            SshFailure::LocalNetworkDenied
        } else {
            SshFailure::NoRoute
        }
    );
    assert_eq!(
        classify(denied, vec!["100.64.1.2".parse().unwrap()]),
        SshFailure::NoRoute
    );
    // An unresolved name, as after the lookup times out, is a plain no route.
    assert_eq!(classify(denied, Vec::new()), SshFailure::NoRoute);
    assert_eq!(
        SshFailure::classify_with(
            Some(255),
            b"ssh: connect to host 8.8.8.8 port 22: No route to host\n",
            |_| unreachable!("an address needs no lookup"),
        ),
        SshFailure::NoRoute
    );
}

#[test]
fn only_failures_the_user_must_fix_wait_for_them() {
    assert!(SshFailure::HostKey.needs_user());
    assert!(SshFailure::Auth.needs_user());
    assert!(SshFailure::HerdrMissing.needs_user());
    assert!(!SshFailure::Unreachable.needs_user());
    assert!(!SshFailure::NoRoute.needs_user());
    // Granting the permission must be picked up by the next prompt redial.
    assert!(!SshFailure::LocalNetworkDenied.needs_user());
    assert!(!SshFailure::Other.needs_user());
}

#[test]
fn the_stderr_tail_is_bounded_to_the_latest_output() {
    let mut input = vec![b'x'; STDERR_TAIL * 3];
    input.extend_from_slice(b"Permission denied");
    let kept = tail(input.as_slice());
    assert_eq!(kept.len(), STDERR_TAIL);
    assert!(kept.ends_with(b"Permission denied"));
}

#[test]
fn a_closed_child_is_diagnosed_from_its_real_exit_and_stderr() {
    let mut child = Command::new("/bin/sh")
        .args([
            "-c",
            "printf 'me@host: Permission denied (publickey).\\n' >&2; exit 255",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stderr = drain(child.stderr.take().unwrap()).unwrap();
    let stop = AtomicBool::new(false);
    assert_eq!(diagnose(&mut child, &stderr, &stop), SshFailure::Auth);
}

#[test]
fn a_child_that_will_not_exit_is_not_waited_on_past_cancellation() {
    let mut child = Command::new("/bin/sh")
        .args(["-c", "exec sleep 30"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stderr = drain(child.stderr.take().unwrap()).unwrap();
    let stop = AtomicBool::new(true);
    let started = Instant::now();
    assert_eq!(diagnose(&mut child, &stderr, &stop), SshFailure::Other);
    assert!(started.elapsed() < DIAGNOSE_TIMEOUT);
    child.kill().unwrap();
    child.wait().unwrap();
}

#[test]
fn a_refused_bridge_reports_its_class_through_the_error() {
    let error = crate::Error::SshRefused(SshFailure::Auth);
    assert_eq!(error.kind(), ErrorKind::PermissionDenied);
    assert!(
        error
            .to_string()
            .starts_with("SSH bridge closed: authentication failed")
    );
    let source = std::error::Error::source(&error).unwrap();
    assert_eq!(source.downcast_ref::<SshFailure>(), Some(&SshFailure::Auth));
}
