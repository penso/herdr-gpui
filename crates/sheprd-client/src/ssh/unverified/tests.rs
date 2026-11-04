use super::*;

// The list is process-wide, so each test uses targets no other test names.

#[test]
fn a_target_is_verified_unless_the_user_skipped_it() {
    assert_eq!(HostKeys::of("verified.test"), HostKeys::Verify);
    set_host_keys("skipped.test", HostKeys::Skip);
    assert_eq!(HostKeys::of("skipped.test"), HostKeys::Skip);
    set_host_keys("skipped.test", HostKeys::Verify);
    assert_eq!(HostKeys::of("skipped.test"), HostKeys::Verify);
}

#[test]
fn skipping_overrides_the_strict_policy_and_reads_no_known_hosts() {
    assert!(HostKeys::Verify.options().is_empty());
    let options = HostKeys::Skip.options();
    assert!(options.contains(&"StrictHostKeyChecking=no"));
    assert!(options.contains(&"UserKnownHostsFile=/dev/null"));
    assert!(options.contains(&"GlobalKnownHostsFile=/dev/null"));
}

#[test]
fn commands_for_a_skipped_target_put_the_override_before_the_policy() {
    set_host_keys("command.test", HostKeys::Skip);
    let command = crate::ssh::command("command.test", "true");
    let args: Vec<_> = command.get_args().filter_map(|arg| arg.to_str()).collect();
    let skip = args
        .iter()
        .position(|arg| *arg == "StrictHostKeyChecking=no");
    let strict = args
        .iter()
        .position(|arg| *arg == "StrictHostKeyChecking=yes");
    assert!(skip.is_some_and(|skip| strict.is_some_and(|strict| skip < strict)));
    let verified = crate::ssh::command("other-command.test", "true");
    assert!(
        !verified
            .get_args()
            .any(|arg| arg == "StrictHostKeyChecking=no")
    );
    set_host_keys("command.test", HostKeys::Verify);
}

#[test]
fn a_replacement_list_is_bounded() {
    let many = (0..MAX_UNVERIFIED_HOSTS + 10).map(|index| format!("host-{index}.test"));
    assert_eq!(bounded(many).len(), MAX_UNVERIFIED_HOSTS);
    assert_eq!(
        bounded(["a.test", "a.test", "b.test"]),
        BTreeSet::from(["a.test".to_owned(), "b.test".to_owned()])
    );
}
