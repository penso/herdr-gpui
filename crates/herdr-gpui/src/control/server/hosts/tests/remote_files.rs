use super::super::remote_files::{LIST_BUDGET, list};
use super::*;
use crate::{
    control::protocol::hosts::{CopyState, RemoteCopy},
    remote_files::{Listed, Policy, SyncState},
};
use std::path::Path;

fn listed<'a>(target: &'a str, remote_path: &'a str, state: SyncState) -> Listed<'a> {
    Listed {
        target,
        remote_path,
        local: Path::new("/cache/1/notes.md"),
        state,
        policy: Policy::Ask,
    }
}

fn copies(response: Response) -> (Vec<RemoteCopy>, usize) {
    match response {
        Response::Host {
            reply: Reply::RemoteFiles { copies, omitted },
        } => (copies, omitted),
        other => panic!("not a remote files reply: {other:?}"),
    }
}

#[test]
fn each_copy_state_is_listed_as_its_own_state() {
    let h = [7; 32];
    let cases = [
        (SyncState::Synced, CopyState::Synced),
        (SyncState::Edited { sha256: h }, CopyState::Edited),
        (SyncState::Asking { sha256: h }, CopyState::Asking),
        (SyncState::Uploading { sha256: h }, CopyState::Uploading),
        (SyncState::Conflict { sha256: h }, CopyState::Conflict),
        (SyncState::Declined { sha256: h }, CopyState::Declined),
        (SyncState::Refetching { sha256: h }, CopyState::Refetching),
    ];
    for (state, expected) in cases {
        let (listed, omitted) = copies(list([listed("me@devbox", "/srv/a", state)], LIST_BUDGET));
        assert_eq!(omitted, 0);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].state, expected, "{state:?}");
    }
}

#[test]
fn a_copy_lists_its_host_paths_and_upload_policy() {
    let always = Listed {
        policy: Policy::Always,
        ..listed("me@devbox", "/srv/notes.md", SyncState::Synced)
    };
    let ask = listed("me@devbox", "/srv/a.md", SyncState::Synced);
    let (listed, _) = copies(list([always, ask], LIST_BUDGET));
    // By host and path, whatever order the window keeps them in.
    assert_eq!(
        listed,
        vec![
            RemoteCopy {
                host: "me@devbox".into(),
                path: "/srv/a.md".into(),
                local: Path::new("/cache/1/notes.md")
                    .to_string_lossy()
                    .into_owned(),
                state: CopyState::Synced,
                auto_upload: false,
            },
            RemoteCopy {
                host: "me@devbox".into(),
                path: "/srv/notes.md".into(),
                local: Path::new("/cache/1/notes.md")
                    .to_string_lossy()
                    .into_owned(),
                state: CopyState::Synced,
                auto_upload: true,
            },
        ]
    );
}

#[test]
fn copies_past_the_answer_s_bound_are_counted_not_sent() {
    let paths: Vec<String> = (0..10).map(|n| format!("/srv/{n}.md")).collect();
    let all = || {
        paths
            .iter()
            .map(|path| listed("me@devbox", path, SyncState::Synced))
    };
    let one = serde_json::to_vec(&RemoteCopy::from(listed(
        "me@devbox",
        &paths[0],
        SyncState::Synced,
    )))
    .unwrap()
    .len()
        + 1;
    let (sent, omitted) = copies(list(all(), one * 3));
    assert_eq!((sent.len(), omitted), (3, paths.len() - 3));
    assert_eq!(sent[0].path, "/srv/0.md");
    let (sent, omitted) = copies(list(all(), LIST_BUDGET));
    assert_eq!((sent.len(), omitted), (paths.len(), 0));
    let (sent, omitted) = copies(list(all(), 0));
    assert_eq!((sent.len(), omitted), (0, paths.len()));
}

#[gpui::test]
fn a_window_without_copies_lists_none(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        let (listed, omitted) = copies(list(view.remote_files.list(), LIST_BUDGET));
        assert_eq!((listed.len(), omitted), (0, 0));
    });
}
