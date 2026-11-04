use super::*;
use sheprd_client::protocol::ClientShellWorktree;

#[test]
fn workspace_choices_offer_new_workspace_then_other_workspaces() {
    let snapshot = snapshot();
    assert_eq!(
        workspace_choices(&snapshot, "w1"),
        [
            Choice {
                destination: Destination::NewWorkspace,
                workspace: None,
                place: "New Workspace".into(),
            },
            Choice {
                destination: Destination::NewTab {
                    workspace_id: "w2".into()
                },
                workspace: None,
                place: "other".into(),
            },
        ]
    );
}

#[test]
fn workspace_choices_preserve_worktree_grouping_from_sidebar() {
    let mut snapshot = snapshot();
    let mut w1 = snapshot.workspaces[0].clone();
    w1.workspace_id = "w1".into();
    w1.label = "repo-main".into();
    w1.worktree = Some(ClientShellWorktree {
        key: "repo-a".into(),
        label: "repo-a".into(),
        is_linked_worktree: false,
    });
    let mut w2 = w1.clone();
    w2.workspace_id = "w2".into();
    w2.label = "unrelated".into();
    w2.worktree = None;
    let mut w3 = w1.clone();
    w3.workspace_id = "w3".into();
    w3.label = "repo-feat".into();
    w3.worktree = Some(ClientShellWorktree {
        key: "repo-a".into(),
        label: "repo-a".into(),
        is_linked_worktree: true,
    });
    let mut w4 = w2.clone();
    w4.workspace_id = "w4".into();
    w4.label = "another".into();
    snapshot.workspaces = vec![w1, w2, w3, w4];
    let choices = workspace_choices(&snapshot, "w2");
    let places: Vec<&str> = choices.iter().map(|c| c.place.as_str()).collect();
    assert_eq!(
        places,
        ["New Workspace", "repo-main", "repo-feat", "another"]
    );
}

#[test]
fn tab_request_validates_every_pane_and_destination() {
    assert!(matches!(
        TabRequest::new(Vec::new(), Destination::NewWorkspace, None),
        Err(Error::InvalidId(rejected)) if rejected.is_empty()
    ));
    for id in ["", "--new-workspace", "w1\np2"] {
        assert!(matches!(
            TabRequest::new(vec!["w1:p1".into(), id.into()], Destination::NewWorkspace, None),
            Err(Error::InvalidId(rejected)) if rejected == id
        ));
    }
    assert!(matches!(
        TabRequest::new(
            vec!["w1:p1".into()],
            Destination::NewWorkspace,
            Some("bad\nname".into()),
        ),
        Err(Error::InvalidId(rejected)) if rejected == "bad\nname"
    ));
    assert!(
        TabRequest::new(
            vec!["w1:p1".into(), "w1:p2".into()],
            Destination::NewTab {
                workspace_id: "w2".into()
            },
            Some("my-tab".into()),
        )
        .is_ok()
    );
}

#[test]
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn tab_request_moves_all_panes_into_the_created_tab() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    let bin = home.path().join(".local/bin");
    std::fs::create_dir_all(&bin).unwrap();
    let fake = bin.join("herdr");
    std::fs::write(
        &fake,
        r#"#!/bin/sh
printf '%s\n' "$*" >> "$HOME/calls"
if [ "$3" = "tab" ] && [ "$4" = "rename" ]; then
  printf '%s\n' '{"result":{"type":"tab_rename"}}'
  exit 0
fi
case "$5" in
  w1:p1)
    printf '%s\n' '{"result":{"type":"pane_move","move_result":{"changed":true,"created_tab":{"tab_id":"w2:t9"},"pane":{"tab_id":"w2:t9"}}}}'
    ;;
  w1:p2|w1:p3)
    printf '%s\n' '{"result":{"type":"pane_move","move_result":{"changed":true,"pane":{"tab_id":"w2:t9"}}}}'
    ;;
  *)
    printf '%s\n' '{"id":"cli:pane:move","error":{"code":"pane_not_found","message":"pane not found"}}' >&2
    exit 1
    ;;
esac
"#,
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut host = host_for(&ConnectTarget::Session {
        name: "work".into(),
        development: false,
    })
    .unwrap();
    host.env = vec![
        ("HOME".into(), home.path().to_string_lossy().into_owned()),
        ("PATH".into(), "/usr/bin:/bin".into()),
    ];
    let cancelled = AtomicBool::new(false);
    let request = TabRequest::new(
        vec!["w1:p1".into(), "w1:p2".into(), "w1:p3".into()],
        Destination::NewTab {
            workspace_id: "w2".into(),
        },
        Some("tab-name".into()),
    )
    .unwrap();
    assert!(request.run(&host, &cancelled).unwrap().warning.is_none());
    assert_eq!(
        std::fs::read_to_string(home.path().join("calls")).unwrap(),
        "--session work pane move w1:p1 --new-tab --workspace w2 --focus\n\
         --session work tab rename w2:t9 tab-name\n\
         --session work pane move w1:p2 --tab w2:t9 --split right --focus\n\
         --session work pane move w1:p3 --tab w2:t9 --split right --focus\n"
    );
}

#[test]
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn tab_request_continues_on_rename_failure_and_reports_partial_move_on_later_error() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    let bin = home.path().join(".local/bin");
    std::fs::create_dir_all(&bin).unwrap();
    let fake = bin.join("herdr");
    std::fs::write(
        &fake,
        r#"#!/bin/sh
printf '%s\n' "$*" >> "$HOME/calls"
if [ "$3" = "tab" ] && [ "$4" = "rename" ]; then
  exit 1
fi
case "$5" in
  w1:p1)
    printf '%s\n' '{"result":{"type":"pane_move","move_result":{"changed":true,"created_tab":{"tab_id":"w2:t9"},"pane":{"tab_id":"w2:t9"}}}}'
    ;;
  w1:p2)
    printf '%s\n' '{"id":"cli:pane:move","error":{"code":"pane_not_found","message":"pane not found"}}' >&2
    exit 1
    ;;
esac
"#,
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut host = host_for(&ConnectTarget::Session {
        name: "work".into(),
        development: false,
    })
    .unwrap();
    host.env = vec![
        ("HOME".into(), home.path().to_string_lossy().into_owned()),
        ("PATH".into(), "/usr/bin:/bin".into()),
    ];
    let cancelled = AtomicBool::new(false);
    let request = TabRequest::new(
        vec!["w1:p1".into(), "w1:p2".into()],
        Destination::NewTab {
            workspace_id: "w2".into(),
        },
        Some("tab-name".into()),
    )
    .unwrap();
    let err = request.run(&host, &cancelled).unwrap_err();
    match err {
        Error::PartialTabMove {
            pane_id,
            created_tab_id,
            source: _,
        } => {
            assert_eq!(pane_id, "w1:p2");
            assert_eq!(created_tab_id, "w2:t9");
        }
        other => panic!("expected PartialTabMove, got {other:?}"),
    }
}

#[test]
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn tab_request_finishes_the_move_and_warns_when_the_rename_fails() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    let bin = home.path().join(".local/bin");
    std::fs::create_dir_all(&bin).unwrap();
    let fake = bin.join("herdr");
    std::fs::write(
        &fake,
        r#"#!/bin/sh
printf '%s\n' "$*" >> "$HOME/calls"
if [ "$3" = "tab" ] && [ "$4" = "rename" ]; then
  printf '%s\n' '{"id":"cli:tab:rename","error":{"code":"tab_not_found","message":"tab not found"}}' >&2
  exit 1
fi
case "$5" in
  w1:p1)
    printf '%s\n' '{"result":{"type":"pane_move","move_result":{"changed":true,"created_tab":{"tab_id":"w2:t9"},"pane":{"tab_id":"w2:t9"}}}}'
    ;;
  w1:p2)
    printf '%s\n' '{"result":{"type":"pane_move","move_result":{"changed":true,"pane":{"tab_id":"w2:t9"}}}}'
    ;;
esac
"#,
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut host = host_for(&ConnectTarget::Session {
        name: "work".into(),
        development: false,
    })
    .unwrap();
    host.env = vec![
        ("HOME".into(), home.path().to_string_lossy().into_owned()),
        ("PATH".into(), "/usr/bin:/bin".into()),
    ];
    let cancelled = AtomicBool::new(false);
    let request = TabRequest::new(
        vec!["w1:p1".into(), "w1:p2".into()],
        Destination::NewTab {
            workspace_id: "w2".into(),
        },
        Some("tab-name".into()),
    )
    .unwrap();
    let moved = request.run(&host, &cancelled).unwrap();
    // The remaining pane still moved after the rename failed.
    assert!(
        std::fs::read_to_string(home.path().join("calls"))
            .unwrap()
            .ends_with("--session work pane move w1:p2 --tab w2:t9 --split right --focus\n")
    );
    let warning = moved.warning.unwrap();
    assert!(warning.to_string().contains("tab not found"));
    match warning {
        Error::Rename { tab_id, source } => {
            assert_eq!(tab_id, "w2:t9");
            assert!(matches!(*source, Error::Refused { ref code, .. } if code == "tab_not_found"));
        }
        other => panic!("expected Rename, got {other:?}"),
    }
}
