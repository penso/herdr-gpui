use super::text;
use crate::control::protocol::hosts::{LocalPortInfo, LocalPorts};

#[test]
fn every_local_port_reads_as_one_line() {
    let ports = LocalPorts {
        listening: vec![
            LocalPortInfo {
                port: 3000,
                bind: "localhost:3000".into(),
                process: "node".into(),
                pid: 4242,
                name: "vite".into(),
                command: "node /home/dev/app/node_modules/.bin/vite".into(),
                project: Some("app".into()),
                cpu_percent: Some(2.5),
                memory_bytes: Some(80 << 20),
                system: false,
                directory: Some("~/app".into()),
                uptime_seconds: Some(3_900),
                paused: true,
                workspace: Some("web".into()),
                url: Some("http://localhost:3000/".into()),
            },
            LocalPortInfo {
                port: 8080,
                bind: "*:8080".into(),
                process: "python3".into(),
                pid: 4343,
                name: String::new(),
                command: String::new(),
                project: None,
                cpu_percent: None,
                memory_bytes: None,
                system: true,
                directory: None,
                uptime_seconds: None,
                paused: false,
                workspace: None,
                url: None,
            },
        ],
        note: None,
    };
    assert_eq!(
        text::local_ports(&ports),
        "localhost:3000           vite (node, pid 4242) paused cpu 2.5% mem 80 MB up 1h 5m  ~/app  workspace web  http://localhost:3000/\n\
         *:8080                   python3 (pid 4343) system"
    );
    let empty = LocalPorts {
        listening: Vec::new(),
        note: Some("The latest scan failed".into()),
    };
    assert_eq!(
        text::local_ports(&empty),
        "Nothing on this computer listens on a TCP port\nThe latest scan failed"
    );
}

#[test]
fn ports_help_names_the_local_verbs() {
    let help = crate::control::client::help::usage(Some("ports"));
    for verb in ["ports all", "ports stop|pause PORT", "ports resume PORT"] {
        assert!(help.contains(verb), "{verb}: {help}");
    }
}
