use super::*;

/// What a script's `ssh` reports when it ends with `status` after printing
/// `stderr`, run as a fake `ssh` would end.
fn failure(status: i32, stderr: &str) -> Error {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "printf '%s' \"$1\" >&2; exit \"$2\"", "sh", stderr, &status.to_string()]);
    run_script_command(command, io::empty(), io::sink(), LIMITS, &AtomicBool::new(false)).unwrap_err()
}

#[test]
fn a_connection_sshd_dropped_before_key_exchange_is_told_apart() {
    // sshd past `MaxStartups` resets the connection, or closes it.
    let reset = "kex_exchange_identification: read: Connection reset by peer\nConnection reset by 100.64.0.1 port 22";
    assert!(failure(255, reset).dropped_before_key_exchange());
    assert!(failure(255, "kex_exchange_identification: Connection closed by remote host").dropped_before_key_exchange());
    // OpenSSH before 8.0 named the same step differently.
    assert!(failure(255, "ssh_exchange_identification: read: Connection reset by peer").dropped_before_key_exchange());
}

#[test]
fn other_failures_are_not_dropped_connections() {
    // Refused credentials, a host that is down, and the script's own failure
    // after it ran, even one that prints the same words, are not retried.
    assert!(!failure(255, "user@host: Permission denied (publickey).").dropped_before_key_exchange());
    assert!(!failure(255, "ssh: connect to host box port 22: Connection refused").dropped_before_key_exchange());
    assert!(!failure(255, "client_loop: send disconnect: Connection reset by peer").dropped_before_key_exchange());
    assert!(!failure(1, "kex_exchange_identification: read: Connection reset by peer").dropped_before_key_exchange());
    assert!(!Error::ScriptCancelled.dropped_before_key_exchange());
}

#[test]
fn a_host_ssh_could_not_reach_is_told_apart_from_a_script_that_failed_on_it() {
    // `ssh` exits 255 for its own failures, whatever it printed.
    assert!(failure(255, "kex_exchange_identification: read: Connection reset by peer").host_unreached());
    assert!(failure(255, "ssh: connect to host box port 22: Connection refused").host_unreached());
    assert!(failure(255, "user@host: Permission denied (publickey).").host_unreached());
    assert!(Error::ScriptTimeout.host_unreached());
    assert!(Error::ScriptConnect(io::Error::from(ErrorKind::ConnectionRefused)).host_unreached());
    // The host answered: the script ran and failed, or was stopped here.
    assert!(!failure(128, "fatal: not a git repository").host_unreached());
    assert!(!failure(1, "kex_exchange_identification: read: Connection reset by peer").host_unreached());
    assert!(!Error::ScriptCancelled.host_unreached());
    assert!(!Error::ScriptOutputLimit.host_unreached());
}
