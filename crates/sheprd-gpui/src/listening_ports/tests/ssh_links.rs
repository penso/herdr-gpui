//! A remote port opens through an SSH tunnel on this machine's loopback,
//! on its own number when that is free.

use super::*;

fn key(port: u16, address: Option<IpAddr>) -> Key {
    Key {
        target: "me@devbox".into(),
        address,
        port,
    }
}

#[test]
fn every_remote_port_opens_through_a_tunnel() {
    let remote = Origin::new(&Host::Ssh("me@devbox".into()));
    let lan = IpAddr::V4(Ipv4Addr::new(100, 64, 0, 8));
    let v6 = IpAddr::V6(Ipv6Addr::new(0xfd00, 0, 0, 0, 0, 0, 0, 2));
    // Wildcard and loopback servers are dialled at the remote's localhost,
    // never at the address SSH reaches the host by.
    for bind in [Bind::Loopback, Bind::Any] {
        assert_eq!(
            port(5173, bind, "vite").link(&remote),
            Some(Link::Tunnel(key(5173, None)))
        );
    }
    // A server bound to one address is dialled there, through SSH.
    for address in [lan, v6] {
        assert_eq!(
            port(8080, Bind::Address(address), "py").link(&remote),
            Some(Link::Tunnel(key(8080, Some(address))))
        );
    }
    assert_eq!(Link::Tunnel(key(5173, None)).label(), ":5173");
}

#[test]
fn a_first_tunnel_prefers_the_servers_own_port() {
    let mut tunnels = tunnel::Tunnels::default();
    assert_eq!(tunnels.begin(&key(5173, None)), Some(Some(5173)));
    // A privileged port moves up as the host menu's Forward port moves it.
    assert_eq!(tunnels.begin(&key(80, None)), Some(Some(10080)));
}

#[cfg(unix)]
#[test]
fn a_port_another_remote_port_last_used_is_not_preferred() {
    let mut tunnels = tunnel::Tunnels::default();
    let first = key(3000, None);
    let second = key(41000, None);
    assert!(tunnels.begin(&first).is_some());
    // 3000 was taken here, so the first remote port got 41000.
    assert_eq!(
        tunnels.finish(first.clone(), tunnel::Tunnel::around(idle_child(), 41000)),
        Some(41000)
    );
    assert!(tunnels.close(&first));
    // A page may still show the first server at 41000.
    assert_eq!(tunnels.begin(&second), Some(None));
    assert!(tunnels.fail(&second));
    // The first keeps its remembered port.
    assert_eq!(tunnels.begin(&first), Some(Some(41000)));
    tunnels.close_all();
}

#[test]
fn a_tunnel_lasts_only_while_its_port_listens_at_the_address_it_dials() {
    let mut ports = ListeningPorts::default();
    let host = Host::Ssh("me@devbox".into());
    ports.seed(host.clone(), parse("L 1 *:5173 vite\nE 1 w1\n").unwrap());
    assert!(ports.tunnels_to(&key(5173, None)));
    assert!(!ports.tunnels_to(&key(5174, None)));
    let lan = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2));
    assert!(!ports.tunnels_to(&key(5173, Some(lan))));
    // The server restarts bound to one address: the old tunnel's key is gone.
    ports.seed(host, parse("L 1 10.0.0.2:5173 vite\nE 1 w1\n").unwrap());
    assert!(!ports.tunnels_to(&key(5173, None)));
    assert!(ports.tunnels_to(&key(5173, Some(lan))));
    // Another host's ports never keep this one's tunnel.
    let other = Key {
        target: "devbox2".into(),
        ..key(5173, Some(lan))
    };
    assert!(!ports.tunnels_to(&other));
}
