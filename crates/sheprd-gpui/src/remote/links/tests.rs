use super::*;

fn ssh(target: &str) -> ConnectTarget {
    ConnectTarget::Ssh {
        target: target.into(),
        session: "default".into(),
    }
}

#[test]
fn every_enabled_machine_is_a_host_once() {
    let (devbox, devbox_dev, gpu, off) = (
        ssh("devbox"),
        ConnectTarget::Ssh {
            target: "devbox".into(),
            session: "dev".into(),
        },
        ssh("gpu"),
        ssh("off"),
    );
    let socket = ConnectTarget::Socket("/tmp/herdr.sock".into());
    let local = ConnectTarget::Local;
    let known = known([
        ("local", "This Mac", &local, true),
        ("ssh:1", "devbox", &devbox, true),
        ("ssh:2", "devbox (dev)", &devbox_dev, true),
        ("ssh:3", "gpu\u{7}box", &gpu, true),
        ("ssh:4", "off", &off, false),
        ("socket", "custom", &socket, true),
    ]);
    assert_eq!(
        known
            .iter()
            .map(|known| (known.host.id.0.as_str(), known.host.name.as_str()))
            .collect::<Vec<_>>(),
        vec![("ssh:1", "devbox"), ("ssh:3", "gpubox")]
    );
    assert_eq!(known[0].target, devbox);
}

#[test]
fn a_hosts_answer_is_read_strictly() {
    let answer = br#"{"device":"d1","computer":"devbox","fingerprint":"AB:CD","origins":["https://100.64.0.7:7443"]}"#;
    let enrolled = parse_enrolled(answer).unwrap();
    assert_eq!(enrolled.device.0, "d1");
    assert_eq!(enrolled.origins, ["https://100.64.0.7:7443"]);
    assert!(matches!(parse_enrolled(b"not json"), Err(Error::Answer(_))));
    let empty = br#"{"device":"","computer":"devbox","fingerprint":"AB","origins":[]}"#;
    assert!(matches!(parse_enrolled(empty), Err(Error::Answer(_))));
}

#[test]
fn a_failure_reads_with_its_causes() {
    let error = Error::Build(host_build::Error::Binary {
        program: PROGRAM,
        path: "/nowhere".into(),
        source: std::io::Error::new(std::io::ErrorKind::NotFound, "gone"),
    });
    let reason = reason(&error);
    assert!(reason.starts_with("Could not read the herdr-remote-control build"));
    assert!(reason.ends_with(": gone"));
}

#[test]
fn a_host_job_without_a_host_fails_without_running_anything() {
    let job = Job {
        pairing: HostPairing {
            device: DeviceId("d1".into()),
            host: hosts::HostId("ssh:gone".into()),
            name: "iPhone".into(),
            public_key: "KEY".into(),
            confirm: false,
            replace: None,
        },
        target: None,
        via: "MacBook Pro".into(),
        apns: None,
    };
    assert!(matches!(
        pair(&job, &AtomicBool::new(false)),
        HostOutcome::Failed { reason } if reason == Error::Gone.to_string()
    ));
}
