use super::*;

const LOCAL: Checkout<&str> = Checkout::Local("/home/user/src/app");

fn ssh(git_dir: &str) -> Checkout<&str> {
    Checkout::Ssh {
        target: "devbox",
        git_dir,
    }
}

/// An `Avatars` whose local and SSH queues the test holds apart, with room
/// for `remote` SSH requests.
fn split(
    remote: usize,
) -> (
    Avatars,
    mpsc::Receiver<Checkout<String>>,
    mpsc::Receiver<Checkout<String>>,
) {
    let (local_requests, local) = mpsc::sync_channel(MAX_REQUESTS);
    let (remote_requests, ssh) = mpsc::sync_channel(remote);
    let (_outgoing, results) = mpsc::channel();
    let avatars = Avatars {
        local_requests,
        remote_requests,
        results,
        local: HashMap::new(),
        remote: HashMap::new(),
        links: HashMap::new(),
        offline: links::Offline::default(),
    };
    (avatars, local, ssh)
}

#[test]
fn local_and_ssh_lookups_use_separate_queues() {
    let (mut avatars, local, remote) = split(MAX_REQUESTS);
    let now = Instant::now();
    avatars.request(ssh("/a/.git"), now);
    avatars.request(LOCAL, now);
    assert_eq!(local.try_iter().collect::<Vec<_>>(), [LOCAL.to_owned()]);
    assert_eq!(
        remote.try_iter().collect::<Vec<_>>(),
        [ssh("/a/.git").to_owned()]
    );
}

#[test]
fn a_backed_up_ssh_queue_does_not_refuse_local_lookups() {
    let (mut avatars, local, remote) = split(1);
    let now = Instant::now();
    avatars.request(ssh("/a/.git"), now);
    // The SSH queue is full, so this one is refused and left to a later update.
    avatars.request(ssh("/b/.git"), now);
    avatars.request(LOCAL, now);
    assert_eq!(local.try_iter().collect::<Vec<_>>(), [LOCAL.to_owned()]);
    assert_eq!(
        remote.try_iter().collect::<Vec<_>>(),
        [ssh("/a/.git").to_owned()]
    );

    // The refused checkout was not recorded as pending, so it is asked again.
    avatars.request(ssh("/b/.git"), now);
    assert_eq!(
        remote.try_iter().collect::<Vec<_>>(),
        [ssh("/b/.git").to_owned()]
    );
}
