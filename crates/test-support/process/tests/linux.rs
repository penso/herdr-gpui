use super::*;

#[test]
fn reaped_before_open_or_during_read_is_dead() {
    for errno in [libc::ENOENT, libc::ESRCH] {
        assert!(linux_stat_dead(Err(io::Error::from_raw_os_error(errno))).unwrap());
    }
}

#[test]
fn other_read_failures_are_not_mistaken_for_an_exit() {
    for errno in [libc::EACCES, libc::EIO] {
        let error = linux_stat_dead(Err(io::Error::from_raw_os_error(errno))).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(errno));
    }
}

#[test]
fn stat_distinguishes_live_and_zombie_and_rejects_missing_state() {
    for state in ["R", "S", "T", "Z"] {
        // Linux process names can contain spaces and closing parentheses.
        let stat = format!("42 (child ) name) {state} 1 2 3");
        assert_eq!(linux_stat_dead(Ok(stat)).unwrap(), state == "Z");
    }
    for stat in ["", "42 (child)"] {
        let error = linux_stat_dead(Ok(stat.to_owned())).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}
