#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::{
    Daemon, ListeningPorts, Reading,
    scan::{Bind, Link, MAX_PORTS, Origin, Port, Ports, parse},
    tunnel::{self, Key},
};
use crate::{Error, usage::Host};
use herdr_client::ConnectTarget;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

#[cfg(unix)]
mod repeated_scans;
mod ssh_links;
mod status_bar;
mod wsl_hosts;

/// The ports one workspace was found with, whichever daemon it named.
fn of<'a>(ports: &'a Ports, workspace: &str) -> &'a [Port] {
    ports
        .iter()
        .find(|(owner, _)| owner.workspace == workspace)
        .map(|(_, ports)| ports.as_slice())
        .unwrap()
}

fn remote(session: &str) -> Daemon {
    Daemon::of(&ConnectTarget::Ssh {
        target: "devbox".into(),
        session: session.into(),
    })
    .unwrap()
}

fn port(number: u16, bind: Bind, process: &str) -> Port {
    Port {
        number,
        bind,
        process: process.into(),
    }
}

#[test]
fn listeners_belong_to_the_workspace_their_process_names() {
    // lsof's addresses, as the macOS branch prints them.
    let ports = parse(
        "L 10 *:5173 node\n\
         L 10 [::1]:5173 node\n\
         L 11 127.0.0.1:3000 Google Chrome He\n\
         L 12 *:7000 ControlCenter\n\
         L 13 192.168.1.5:8080 python3\n\
         E 10 w7V\n\
         E 11 w7V\n\
         E 13 wE\n",
    )
    .unwrap();
    assert_eq!(
        of(&ports, "w7V"),
        [
            port(3000, Bind::Loopback, "Google Chrome He"),
            // Both sockets of one server are one port, the wider kept.
            port(5173, Bind::Any, "node"),
        ]
    );
    assert_eq!(
        of(&ports, "wE"),
        [port(
            8080,
            Bind::Address(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 5))),
            "python3"
        )]
    );
    // A process started outside Herdr belongs to no workspace.
    assert_eq!(ports.len(), 2);
}

#[test]
fn ss_addresses_parse_like_lsof_ones() {
    let ports = parse(
        "L 1 0.0.0.0:3000 node\n\
         L 1 [::]:3000 node\n\
         L 2 127.0.0.53%lo:53 resolved\n\
         L 3 [fe80::1%eth0]:9000 api\n\
         L 4 [::1]:4000 vite\n\
         E 1 w1\nE 2 w1\nE 3 w1\nE 4 w1\n",
    )
    .unwrap();
    let binds: Vec<(u16, Bind)> = of(&ports, "w1")
        .iter()
        .map(|port| (port.number, port.bind))
        .collect();
    assert_eq!(
        binds,
        [
            (53, Bind::Loopback),
            (3000, Bind::Any),
            (4000, Bind::Loopback),
            (
                9000,
                Bind::Address(IpAddr::V6(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1)))
            ),
        ]
    );
}

#[test]
fn malformed_and_untrusted_lines_are_dropped() {
    let long = "w".repeat(65);
    let text = format!(
        "garbage\n\
         L x *:1 a\n\
         L 1 *:0 zero\n\
         L 1 *:70000 big\n\
         L 1 nohost dev\n\
         L 1 example.com:80 named\n\
         L 1 *:81 \x1b[31mred\x07\n\
         L 2 *:82 eq\n\
         L 3 *:83 long\n\
         E 1 w1\n\
         E 2 a=b\n\
         E 3 {long}\n\
         E y w1\n"
    );
    let ports = parse(&text).unwrap();
    assert_eq!(ports.len(), 1);
    // Control characters are stripped from the name; the rest is display text.
    assert_eq!(of(&ports, "w1"), [port(81, Bind::Any, "[31mred")]);
}

#[test]
fn a_host_without_tools_says_so() {
    assert!(matches!(parse("N\n"), Err(Error::ListeningPortsTool)));
    // An idle host answers with nothing at all, which is no ports, not an error.
    assert_eq!(parse("\n").unwrap(), Ports::new());
}

#[test]
fn each_workspace_keeps_its_lowest_ports_only() {
    let mut text = String::new();
    for number in (1..=MAX_PORTS as u16 + 8).rev() {
        text.push_str(&format!("L 1 *:{} srv\n", 1000 + number));
    }
    text.push_str("E 1 w1\n");
    let ports = parse(&text).unwrap();
    let numbers: Vec<u16> = of(&ports, "w1").iter().map(|port| port.number).collect();
    assert_eq!(numbers.len(), MAX_PORTS);
    assert_eq!(numbers.first(), Some(&1001));
    assert!(numbers.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn local_ports_open_where_they_listen() {
    let local = Origin::Local;
    let url = |port: &Port| match port.link(&local) {
        Some(Link::Page(url)) => Some(url.as_str().to_owned()),
        _ => None,
    };
    let any = port(3000, Bind::Any, "node");
    let loopback = port(5173, Bind::Loopback, "vite");
    let lan = port(
        8080,
        Bind::Address(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2))),
        "py",
    );
    let v6 = port(
        9000,
        Bind::Address(IpAddr::V6(Ipv6Addr::new(0xfd00, 0, 0, 0, 0, 0, 0, 2))),
        "api",
    );
    assert_eq!(url(&any).as_deref(), Some("http://localhost:3000/"));
    assert_eq!(url(&loopback).as_deref(), Some("http://localhost:5173/"));
    assert_eq!(url(&lan).as_deref(), Some("http://10.0.0.2:8080/"));
    assert_eq!(url(&v6).as_deref(), Some("http://[fd00::2]:9000/"));
    // A WSL distribution's ports are this machine's too.
    assert_eq!(Origin::new(&Host::Wsl("Ubuntu".into())), Origin::Local);
    assert_eq!(any.address(), "*:3000");
    assert_eq!(loopback.address(), "localhost:5173");
}

#[test]
fn only_a_different_scan_changes_what_is_shown() {
    let mut reading = Reading::default();
    let found = parse("L 1 *:3000 node\nE 1 w1\n").unwrap();
    assert!(reading.apply(Ok(found.clone())));
    assert!(!reading.apply(Ok(found.clone())));
    // A failed scan keeps the last ports and is not a change.
    assert!(!reading.apply(Err(Error::ListeningPorts(Box::new(
        Error::UsageUnreachable
    )))));
    assert_eq!(reading.ports, found);
    let error = reading.error.clone().unwrap();
    assert!(
        error.starts_with("Could not read listening ports"),
        "{error}"
    );
    assert!(error.contains("SSH"), "the cause is kept: {error}");
    assert!(reading.apply(Ok(Ports::new())));
    assert_eq!(reading.error, None);
}

#[test]
fn seeded_hosts_are_looked_up_by_workspace_and_forgotten_when_dropped() {
    let mut ports = ListeningPorts::default();
    let daemon = remote("default");
    let host = daemon.host().clone();
    ports.seed(host.clone(), parse("L 1 *:3000 node\nE 1 w1\n").unwrap());
    assert_eq!(
        ports
            .get(&daemon, "w1")
            .map_or(0, |listed| listed.ports.len()),
        1
    );
    assert!(ports.get(&daemon, "w2").is_none());
    assert!(
        ports
            .get(&Daemon::of(&ConnectTarget::Local).unwrap(), "w1")
            .is_none()
    );
    // A host still wanted keeps its reading; no worker is started for it.
    assert!(!ports.poll([host.clone(), host.clone()]));
    assert_eq!(
        ports
            .get(&daemon, "w1")
            .map_or(0, |listed| listed.ports.len()),
        1
    );
    assert!(
        ports.poll(std::iter::empty()),
        "dropping shown ports is a change"
    );
    assert!(ports.get(&daemon, "w1").is_none());
    assert!(!ports.poll(std::iter::empty()));
}

/// The real step against this machine's own tools: a socket this test holds
/// is listed with this process's id.
#[cfg(unix)]
#[test]
fn this_machine_lists_a_socket_this_process_holds() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let number = listener.local_addr().unwrap().port();
    let mut shell = super::local_shell().unwrap();
    let output = shell
        .run(super::scan::COMMAND, super::STEP_TIMEOUT)
        .unwrap();
    if output.stdout.lines().any(|line| line == "N") {
        // Neither ss nor lsof here; parse must say so rather than show nothing.
        assert!(matches!(
            parse(&output.stdout),
            Err(Error::ListeningPortsTool)
        ));
        return;
    }
    let expected = format!("L {} 127.0.0.1:{number} ", std::process::id());
    assert!(
        output
            .stdout
            .lines()
            .any(|line| line.starts_with(&expected)),
        "{expected:?} missing from {:?}",
        output.stdout
    );
    parse(&output.stdout).unwrap();
}

#[test]
fn listeners_name_the_daemon_whose_pane_started_them() {
    let ports = parse(
        "L 1 *:3000 node\n\
         L 2 *:4000 vite\n\
         L 3 *:5000 api\n\
         E 1 w1 /home/me/.config/herdr/sessions/my work/herdr.sock\n\
         E 2 w2 \n\
         E 3 w3 /bad\u{7}/herdr.sock\n",
    )
    .unwrap();
    let socket = |workspace: &str| {
        ports
            .keys()
            .find(|owner| owner.workspace == workspace)
            .map(|owner| owner.socket.as_str())
    };
    // Paths may hold spaces; the socket is the rest of the line.
    assert_eq!(
        socket("w1"),
        Some("/home/me/.config/herdr/sessions/my work/herdr.sock")
    );
    assert_eq!(socket("w2"), Some(""));
    assert_eq!(socket("w3"), Some(""), "control characters are not a path");
}

#[test]
fn two_sessions_on_one_host_keep_their_own_ports() {
    let mut ports = ListeningPorts::default();
    let found = parse(
        "L 1 *:3000 web\n\
         L 2 *:4000 api\n\
         L 3 *:5000 old\n\
         E 1 w1 /home/me/.config/herdr/herdr.sock\n\
         E 2 w1 /home/me/.config/herdr/sessions/work/herdr.sock\n\
         E 3 w9\n",
    )
    .unwrap();
    ports.seed(Host::Ssh("devbox".into()), found);
    let numbers = |daemon: &Daemon, workspace: &str| -> Vec<u16> {
        ports
            .get(daemon, workspace)
            .map(|listed| listed.ports.iter().map(|port| port.number).collect())
            .unwrap_or_default()
    };
    // One workspace id in two sessions: each session sees only its own.
    assert_eq!(numbers(&remote("default"), "w1"), [3000]);
    assert_eq!(numbers(&remote("work"), "w1"), [4000]);
    assert!(numbers(&remote("other"), "w1").is_empty());
    // A pane that did not name its daemon is shown to any of them.
    assert_eq!(numbers(&remote("work"), "w9"), [5000]);
}

#[cfg(unix)]
#[test]
fn a_local_daemon_owns_the_api_socket_beside_its_client_socket() {
    let daemon = Daemon::of(&ConnectTarget::Socket(
        "/cfg/herdr/sessions/work/herdr-client.sock".into(),
    ))
    .unwrap();
    assert!(daemon.owns("/cfg/herdr/sessions/work/herdr.sock"));
    assert!(!daemon.owns("/cfg/herdr/herdr.sock"));
    assert!(!daemon.owns("/cfg/herdr-dev/sessions/work/herdr.sock"));
    assert!(daemon.owns(""));
    let custom = Daemon::of(&ConnectTarget::Socket("/run/x/dev-client.sock".into())).unwrap();
    assert!(custom.owns("/run/x/dev.sock"));
    assert!(!custom.owns("/run/x/herdr.sock"));
}

#[test]
fn a_tunnel_keeps_its_local_port_while_it_is_free() {
    let free = |port: u16| Ok(if port == 0 { 50000 } else { port });
    assert_eq!(tunnel::pick(Some(41000), free).unwrap(), 41000);
    assert_eq!(tunnel::pick(None, free).unwrap(), 50000);
    let taken = |port: u16| {
        if port == 41000 {
            Err(std::io::ErrorKind::AddrInUse.into())
        } else {
            Ok(50001)
        }
    };
    assert_eq!(tunnel::pick(Some(41000), taken).unwrap(), 50001);
    // The real binder never offers a port someone holds.
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let busy = held.local_addr().unwrap().port();
    let other = tunnel::free_port(Some(busy)).unwrap();
    assert!(other != busy && other != 0);
}

#[test]
fn a_bad_target_never_starts_ssh() {
    let key = Key {
        target: "-oProxyCommand=bad".into(),
        address: None,
        port: 3000,
    };
    assert!(matches!(
        tunnel::open(&key, None, &tunnel::Attempt::default()),
        Err(Error::Client(_))
    ));
}

/// A child that runs until killed, standing in for `ssh -N`.
#[cfg(unix)]
fn idle_child() -> std::process::Child {
    std::process::Command::new("/bin/sh")
        .args(["-c", "exec sleep 600"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap()
}

/// Whether `pid` has exited and been reaped, waiting a bounded while for the
/// reaper thread.
#[cfg(unix)]
fn gone(pid: u32) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        let alive = std::process::Command::new("/bin/kill")
            .args(["-0", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        if !alive {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    false
}

#[cfg(unix)]
#[test]
fn tunnels_close_with_their_port_and_never_open_twice() {
    let key = Key {
        target: "devbox".into(),
        address: None,
        port: 5173,
    };
    let mut tunnels = tunnel::Tunnels::default();
    assert_eq!(
        tunnels.begin(&key),
        Some(Some(5173)),
        "the server's own number first"
    );
    assert_eq!(tunnels.begin(&key), None, "one attempt at a time");
    let child = idle_child();
    let pid = child.id();
    assert_eq!(
        tunnels.finish(key.clone(), tunnel::Tunnel::around(child, 41000)),
        Some(41000)
    );
    assert_eq!(tunnels.local(&key), Some(41000));
    // Reopening after a close prefers the same local port.
    assert_eq!(tunnels.begin(&key), Some(Some(41000)));
    tunnels.fail(&key);
    // The port stops listening: its tunnel's child is killed and reaped.
    tunnels.retain(|_| false);
    assert_eq!(tunnels.len(), 0);
    assert!(gone(pid), "the tunnel's child outlived it");
}

#[cfg(unix)]
#[test]
fn a_tunnel_can_be_closed_by_hand_and_reopens_on_its_old_port() {
    let (mine, other) = (
        Key {
            target: "devbox".into(),
            address: None,
            port: 5173,
        },
        Key {
            target: "devbox".into(),
            address: None,
            port: 5174,
        },
    );
    let mut tunnels = tunnel::Tunnels::default();
    let (child, kept) = (idle_child(), idle_child());
    let (pid, kept_pid) = (child.id(), kept.id());
    assert!(tunnels.begin(&mine).is_some());
    tunnels.finish(mine.clone(), tunnel::Tunnel::around(child, 41005));
    assert!(tunnels.begin(&other).is_some());
    tunnels.finish(other.clone(), tunnel::Tunnel::around(kept, 41006));
    assert!(tunnels.is_open(&mine));

    assert!(tunnels.close(&mine));

    assert!(!tunnels.is_open(&mine));
    assert!(gone(pid), "the closed tunnel's child outlived it");
    assert!(tunnels.is_open(&other), "other tunnels stay open");
    assert!(!gone(kept_pid));
    assert!(!tunnels.close(&mine), "nothing left to close");
    assert_eq!(tunnels.begin(&mine), Some(Some(41005)));
}

#[cfg(unix)]
#[test]
fn closing_a_tunnel_still_opening_drops_its_result() {
    let key = Key {
        target: "devbox".into(),
        address: None,
        port: 8081,
    };
    let mut tunnels = tunnel::Tunnels::default();
    assert!(tunnels.begin(&key).is_some());
    assert!(tunnels.close(&key));
    let child = idle_child();
    let pid = child.id();
    assert_eq!(
        tunnels.finish(key.clone(), tunnel::Tunnel::around(child, 41007)),
        None
    );
    assert!(!tunnels.is_open(&key));
    assert!(gone(pid));
}

#[cfg(unix)]
#[test]
fn closing_all_kills_a_tunnel_that_is_still_connecting() {
    let key = Key {
        target: "devbox".into(),
        address: None,
        port: 3002,
    };
    let mut tunnels = tunnel::Tunnels::default();
    assert!(tunnels.begin(&key).is_some());
    // The background task's child, owned by the attempt while connecting.
    let attempt = tunnels.attempt(&key).unwrap();
    attempt.adopt(idle_child());

    tunnels.close_all();

    assert!(killed(&attempt), "the connecting child outlived close_all");
    assert!(
        !tunnels.fail(&key),
        "nobody waits for the cancelled attempt"
    );
}

#[cfg(unix)]
#[test]
fn closing_a_connecting_port_by_hand_or_when_it_unlists_kills_its_child() {
    for by_hand in [true, false] {
        let key = Key {
            target: "devbox".into(),
            address: None,
            port: 3003,
        };
        let mut tunnels = tunnel::Tunnels::default();
        assert!(tunnels.begin(&key).is_some());
        let attempt = tunnels.attempt(&key).unwrap();
        attempt.adopt(idle_child());

        if by_hand {
            assert!(tunnels.close(&key));
        } else {
            assert!(!tunnels.retain(|_| false), "no open tunnel went away");
        }

        assert!(killed(&attempt), "by_hand={by_hand}");
    }
}

#[cfg(unix)]
#[test]
fn an_attempt_cancelled_before_its_child_exists_kills_it_on_arrival() {
    let attempt = tunnel::Attempt::default();
    attempt.cancel();
    attempt.adopt(idle_child());
    assert!(killed(&attempt));
}

#[cfg(unix)]
#[test]
fn quitting_kills_a_finished_tunnel_still_on_its_way_to_the_window() {
    let key = Key {
        target: "devbox".into(),
        address: None,
        port: 3004,
    };
    let mut tunnels = tunnel::Tunnels::default();
    assert!(tunnels.begin(&key).is_some());
    let attempt = tunnels.attempt(&key).unwrap();
    attempt.adopt(idle_child());
    // SSH reported ready; the result has not reached `finish` yet.
    let in_flight = tunnel::Tunnel::new(&attempt, 41011);

    tunnels.close_all();

    assert!(
        killed(&attempt),
        "the finished tunnel's child outlived quit"
    );
    assert_eq!(tunnels.finish(key, in_flight), None);
}

#[cfg(unix)]
#[test]
fn retain_reports_when_a_dead_tunnel_is_pruned() {
    let key = Key {
        target: "devbox".into(),
        address: None,
        port: 9001,
    };
    let mut tunnels = tunnel::Tunnels::default();
    let mut child = idle_child();
    child.kill().unwrap();
    child.wait().unwrap();
    tunnels.insert(key.clone(), tunnel::Tunnel::around(child, 41008));
    assert!(tunnels.retain(|_| true), "the redraw must be requested");
    assert!(!tunnels.is_open(&key));
    assert!(!tunnels.retain(|_| true), "nothing more changed");
}

/// Whether the child `attempt` holds was killed, waiting a bounded while.
/// The attempt only kills it; reaping is its owner's job, so this reaps it
/// here, which works on every Unix rather than reading `/proc`.
#[cfg(unix)]
fn killed(attempt: &tunnel::Attempt) -> bool {
    use std::os::unix::process::ExitStatusExt;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        if let Some(status) = attempt.try_wait().unwrap() {
            // `Child::kill` sends SIGKILL on every Unix.
            return status.signal() == Some(9);
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    false
}

#[cfg(unix)]
#[test]
fn closing_all_tunnels_kills_every_child_and_drops_late_results() {
    let (open, opening) = (
        Key {
            target: "devbox".into(),
            address: None,
            port: 3000,
        },
        Key {
            target: "devbox".into(),
            address: None,
            port: 3001,
        },
    );
    let mut tunnels = tunnel::Tunnels::default();
    let child = idle_child();
    let pid = child.id();
    tunnels.insert(open.clone(), tunnel::Tunnel::around(child, 41003));
    assert!(tunnels.begin(&opening).is_some());

    tunnels.close_all();

    assert_eq!(tunnels.len(), 0);
    assert!(gone(pid), "the tunnel's child outlived close_all");
    // The attempt still in flight finds nobody waiting for it.
    let late = idle_child();
    let late_pid = late.id();
    assert_eq!(
        tunnels.finish(opening, tunnel::Tunnel::around(late, 41004)),
        None
    );
    assert!(gone(late_pid));
    assert_eq!(
        tunnels.begin(&open),
        Some(Some(3000)),
        "no stale local port kept"
    );
}

#[cfg(unix)]
#[test]
fn a_tunnel_finished_after_its_port_closed_is_dropped() {
    let key = Key {
        target: "devbox".into(),
        address: None,
        port: 8080,
    };
    let mut tunnels = tunnel::Tunnels::default();
    assert!(tunnels.begin(&key).is_some());
    tunnels.retain(|_| false);
    let child = idle_child();
    let pid = child.id();
    assert_eq!(
        tunnels.finish(key.clone(), tunnel::Tunnel::around(child, 41001)),
        None
    );
    assert_eq!(tunnels.local(&key), None);
    assert!(gone(pid));
}

#[cfg(unix)]
#[test]
fn a_dead_tunnel_is_forgotten() {
    let key = Key {
        target: "devbox".into(),
        address: None,
        port: 9000,
    };
    let mut tunnels = tunnel::Tunnels::default();
    let mut child = idle_child();
    child.kill().unwrap();
    child.wait().unwrap();
    tunnels.insert(key.clone(), tunnel::Tunnel::around(child, 41002));
    assert_eq!(tunnels.local(&key), None);
    assert_eq!(tunnels.len(), 0);
}

/// A child printing `script`'s output on a pipe, standing in for `ssh -N`.
#[cfg(unix)]
fn scripted(script: &str) -> (std::process::Child, std::process::ChildStdout) {
    let mut child = std::process::Command::new("/bin/sh")
        .args(["-c", script])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    (child, stdout)
}

#[cfg(unix)]
#[test]
fn another_process_on_the_port_is_never_taken_for_the_tunnel() {
    // Something else already listens on the port the tunnel was given; a
    // probe would connect to it. SSH has not said it holds the port, so the
    // tunnel is not ready, however long the port answers.
    let squatter = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let _port = squatter.local_addr().unwrap().port();
    let (mut child, stdout) = scripted("exec sleep 600");
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(300);
    let result = tunnel::ready(|| child.try_wait(), stdout, deadline);
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(
        matches!(result, Err(Error::Tunnel(tunnel::Error::Timeout))),
        "{result:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_tunnel_is_ready_once_ssh_prints_its_line() {
    let (mut child, stdout) =
        scripted("echo 'shell noise'; echo herdr-forward-ready; exec sleep 600");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let result = tunnel::ready(|| child.try_wait(), stdout, deadline);
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(result.is_ok(), "{result:?}");
}

#[cfg(unix)]
#[test]
fn ssh_ending_before_its_forward_is_bound_reports_why() {
    // ExitOnForwardFailure: a taken port ends SSH before LocalCommand runs.
    let (mut child, stdout) = scripted("echo 'bind failed' >&2; exit 255");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    match tunnel::ready(|| child.try_wait(), stdout, deadline) {
        Err(Error::Tunnel(tunnel::Error::Exited(status))) => assert_eq!(status.code(), Some(255)),
        other => panic!("expected the exit status, got {other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn a_local_port_is_never_handed_back_to_a_previous_remote_port() {
    let first = Key {
        target: "devbox".into(),
        address: None,
        port: 3000,
    };
    let second = Key {
        target: "devbox".into(),
        address: None,
        port: 4000,
    };
    let mut tunnels = tunnel::Tunnels::default();
    assert!(tunnels.begin(&first).is_some());
    let (exiting, _) = scripted("exit 0");
    assert_eq!(
        tunnels.finish(first.clone(), tunnel::Tunnel::around(exiting, 41000)),
        Some(41000)
    );
    // The first tunnel dies; its local port is reused by the second.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while tunnels.local(&first).is_some() {
        assert!(
            std::time::Instant::now() < deadline,
            "the child never exited"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(tunnels.begin(&second).is_some());
    assert_eq!(
        tunnels.finish(second.clone(), tunnel::Tunnel::around(idle_child(), 41000)),
        Some(41000)
    );
    // Reopening the first must not prefer the port now showing the second.
    assert_eq!(tunnels.begin(&first), Some(Some(3000)));
    tunnels.retain(|_| false);
}
