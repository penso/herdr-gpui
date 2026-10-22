use super::*;
use herdr_client::protocol::ClientShellWorktree;
use std::path::PathBuf;

fn workspace(worktree: Option<&str>) -> ClientShellWorkspace {
    let mut workspace = crate::sidebar::layout_tests::snapshot(1)
        .workspaces
        .remove(0);
    workspace.new_workspace_cwd = "/home/user/src/app".into();
    workspace.worktree = worktree.map(|key| ClientShellWorktree {
        key: key.into(),
        label: "app".into(),
        is_linked_worktree: false,
    });
    workspace
}

fn ssh(target: &str) -> ConnectTarget {
    ConnectTarget::Ssh {
        target: target.into(),
        session: "default".into(),
    }
}

#[test]
fn ssh_devices_read_the_reported_git_directory_on_the_host() {
    let workspace = workspace(Some("/home/user/src/app/.git"));
    let target = ssh("devbox");
    // An SSH device is read there, wherever it sits in the device list.
    for local in [false, true] {
        assert_eq!(
            Checkout::of(local, &target, &workspace),
            Some(Checkout::Ssh {
                target: "devbox",
                git_dir: "/home/user/src/app/.git",
            })
        );
    }
}

#[test]
fn ssh_workspaces_without_worktree_metadata_get_no_checkout() {
    // The directory is the host's, so it is never resolved by local Git.
    assert_eq!(Checkout::of(false, &ssh("devbox"), &workspace(None)), None);
}

#[test]
fn only_the_local_daemon_resolves_directories_with_local_git() {
    let workspace = workspace(Some("/home/user/src/app/.git"));
    assert_eq!(
        Checkout::of(true, &ConnectTarget::Local, &workspace),
        Some(Checkout::Local("/home/user/src/app"))
    );
    // A socket may be forwarded from an unknown machine.
    let socket = ConnectTarget::Socket(PathBuf::from("/tmp/herdr.sock"));
    assert_eq!(Checkout::of(false, &socket, &workspace), None);
    assert_eq!(Checkout::of(false, &ConnectTarget::Local, &workspace), None);
}

#[test]
fn remote_avatars_are_kept_per_host_and_apart_from_local_paths() {
    let (mut avatars, incoming, outgoing) = detached();
    let now = Instant::now();
    let git_dir = "/home/user/src/app/.git";
    let devbox = Checkout::Ssh {
        target: "devbox",
        git_dir,
    };
    let other = Checkout::Ssh {
        target: "other",
        git_dir,
    };
    for checkout in [devbox, other, Checkout::Local(git_dir), devbox, other] {
        avatars.request(checkout, now);
    }
    assert_eq!(
        incoming.try_iter().collect::<Vec<_>>(),
        [
            devbox.to_owned(),
            other.to_owned(),
            Checkout::Local(git_dir.into())
        ]
    );
    let image = Arc::new(Image::empty());
    assert!(
        outgoing
            .send((devbox.to_owned(), Ok(Some(image.clone()))))
            .is_ok()
    );
    assert!(outgoing.send((other.to_owned(), Ok(None))).is_ok());
    assert!(avatars.poll(now));
    assert!(
        avatars
            .image(devbox)
            .is_some_and(|cached| Arc::ptr_eq(&cached, &image))
    );
    assert!(avatars.image(other).is_none());
    assert!(avatars.image(Checkout::Local(git_dir)).is_none());
}

#[test]
fn requests_stop_at_the_bound_across_hosts() {
    let (mut avatars, incoming, _outgoing) = detached();
    let now = Instant::now();
    let dirs = (0..=MAX_REQUESTS)
        .map(|index| format!("/repo/{index}/.git"))
        .collect::<Vec<_>>();
    for (index, git_dir) in dirs.iter().enumerate() {
        let target = if index % 2 == 0 { "devbox" } else { "other" };
        avatars.request(Checkout::Ssh { target, git_dir }, now);
    }
    assert_eq!(incoming.try_iter().count(), MAX_REQUESTS);
}

#[test]
#[ignore = "requires HERDR_AVATAR_TEST_SSH, HERDR_AVATAR_TEST_GIT_DIR, and noninteractive SSH"]
fn reads_the_repository_owner_over_ssh() {
    let target = std::env::var("HERDR_AVATAR_TEST_SSH").expect("set a saved SSH target");
    let git_dir = std::env::var("HERDR_AVATAR_TEST_GIT_DIR").expect("set the host's Git dir");
    let owner = repo_owner(
        Checkout::Ssh {
            target: &target,
            git_dir: &git_dir,
        },
        &Default::default(),
    )
    .expect("host read over SSH");
    assert!(owner.is_some(), "no GitHub owner read over SSH");
    eprintln!("Owner read over SSH: {owner:?}");
}
