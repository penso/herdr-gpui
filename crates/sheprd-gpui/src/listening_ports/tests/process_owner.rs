use super::*;

#[test]
fn a_listener_belongs_to_its_own_process_not_to_its_port_number() {
    let mut ports = ListeningPorts::default();
    let daemon = remote("default");
    // Two processes on one number; only pid 1 came from a pane.
    let scan = parse("L 1 127.0.0.1:3000 node\nL 2 10.0.0.2:3000 nginx\nE 1 w1\n").unwrap();
    ports.seed(daemon.host().clone(), scan);
    assert_eq!(ports.workspace_of(&daemon, 1), Some("w1"));
    assert_eq!(ports.workspace_of(&daemon, 2), None);
}

#[test]
fn processes_sharing_a_socket_each_name_their_workspace() {
    let mut ports = ListeningPorts::default();
    let daemon = remote("default");
    let scan = parse("L 1 *:3000 gunicorn\nL 2 *:3000 gunicorn\nE 1 w1\nE 2 w1\n").unwrap();
    ports.seed(daemon.host().clone(), scan);
    assert_eq!(ports.workspace_of(&daemon, 1), Some("w1"));
    assert_eq!(ports.workspace_of(&daemon, 2), Some("w1"));
}

#[test]
fn a_process_of_another_daemon_has_no_workspace_here() {
    let mut ports = ListeningPorts::default();
    let daemon = remote("default");
    let scan = parse("L 1 *:3000 node\nE 1 w1 /run/other.sock\n").unwrap();
    ports.seed(daemon.host().clone(), scan);
    assert_eq!(ports.workspace_of(&remote("other"), 1), None);
}

#[test]
fn only_listening_processes_are_kept() {
    let scan = parse("L 1 *:3000 node\nE 1 w1\nE 2 w1\n").unwrap();
    assert_eq!(scan.pids.keys().copied().collect::<Vec<_>>(), [1]);
}

#[test]
fn a_process_taking_over_a_port_is_a_change() {
    let mut reading = Reading::default();
    assert!(reading.apply(parse("L 1 *:3000 node\nE 1 w1\n")));
    assert!(reading.apply(parse("L 2 *:3000 node\nE 2 w1\n")));
}
