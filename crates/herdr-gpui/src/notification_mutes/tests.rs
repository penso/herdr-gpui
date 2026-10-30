#![allow(clippy::unwrap_used)]
use super::*;
use herdr_client::protocol::ClientShellWorkspace;
use std::time::Duration;

/// Two workspaces: `w1` with tabs `w1:t1` and `w1:t2`, `w2` with `w2:t1`,
/// one pane in each tab.
fn snapshot(boot: &str, labels: [&str; 2]) -> ClientShellSnapshot {
    let places = [
        ("w1:t1", "w1", "one"),
        ("w1:t2", "w1", "two"),
        ("w2:t1", "w2", "1"),
    ];
    let workspaces = [("w1", labels[0]), ("w2", labels[1])].map(|(id, label)| {
        serde_json::json!({
            "workspace_id": id, "active_tab_id": format!("{id}:t1"), "new_workspace_cwd": "/tmp",
            "number": 1, "label": label, "custom_label": false, "branch": null,
            "worktree": null, "tokens": [], "focused": id == "w1", "agent_status": "idle"
        })
    });
    let tabs = places.map(|(tab, workspace, label)| {
        serde_json::json!({
            "tab_id": tab, "workspace_id": workspace, "number": 1, "label": label,
            "custom_label": false, "zoomed": false, "focused": false, "agent_status": "idle"
        })
    });
    let panes = places.map(|(tab, workspace, _)| {
        serde_json::json!({
            "pane_id": tab.replace(":t", ":p"), "workspace_id": workspace, "tab_id": tab,
            "label": null, "cwd": null, "foreground_cwd": null, "focused": false,
            "right_click_passthrough": false
        })
    });
    serde_json::from_value(serde_json::json!({
        "boot_id": boot, "revision": 1,
        "update_install_command": "", "latest_release_notes_available": false,
        "integration_updates_available": false, "worktree_directory": "",
        "tab_bar_right": [], "tab_bar_right_separator": "", "agent_order": [],
        "commands": [], "agents": [],
        "workspaces": workspaces, "tabs": tabs, "panes": panes,
    }))
    .unwrap()
}

/// `snapshot` with every workspace and tab named by the user.
fn named(mut snapshot: ClientShellSnapshot) -> ClientShellSnapshot {
    for workspace in &mut snapshot.workspaces {
        workspace.custom_label = true;
    }
    for tab in &mut snapshot.tabs {
        tab.custom_label = true;
    }
    snapshot
}

fn local() -> Daemon {
    Daemon::new("local", &ConnectTarget::Local)
}

/// When the tests mute, in seconds since 1970.
const MUTED_AT: u64 = 1_800_000_000;

fn at(seconds: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(seconds)
}

fn mute(mutes: &mut Mutes, shown: &ClientShellSnapshot, scope: Scope, id: &str) -> bool {
    mutes.set(&local(), shown, scope, id, true, at(MUTED_AT))
}

fn unmute(mutes: &mut Mutes, shown: &ClientShellSnapshot, scope: Scope, id: &str) -> bool {
    mutes.set(&local(), shown, scope, id, false, at(MUTED_AT))
}

#[test]
fn a_muted_workspace_silences_its_tabs_and_panes_and_no_other() {
    let shown = snapshot("b1", ["repo", "other"]);
    let mut mutes = Mutes::default();
    assert!(mute(&mut mutes, &shown, Scope::Workspace, "w1"));
    let silenced = mutes.silenced(&local(), &shown);
    assert!(silenced.workspace("w1"));
    assert!(silenced.tab("w1:t1") && silenced.tab("w1:t2"));
    assert!(
        !silenced.own_tab("w1:t1"),
        "silent through its workspace only"
    );
    assert!(silenced.pane("w1:p1") && silenced.pane("w1:p2"));
    assert!(!silenced.workspace("w2") && !silenced.tab("w2:t1") && !silenced.pane("w2:p1"));
    assert!(!silenced.is_empty() && silenced.any_workspace());
}

#[test]
fn a_tab_muted_alone_leaves_its_workspace_and_siblings() {
    let shown = snapshot("b1", ["repo", "other"]);
    let mut mutes = Mutes::default();
    assert!(mute(&mut mutes, &shown, Scope::Tab, "w1:t2"));
    let silenced = mutes.silenced(&local(), &shown);
    assert!(silenced.own_tab("w1:t2") && silenced.tab("w1:t2") && silenced.pane("w1:p2"));
    assert!(!silenced.workspace("w1") && !silenced.any_workspace());
    assert!(!silenced.tab("w1:t1") && !silenced.pane("w1:p1"));
}

#[test]
fn a_notice_is_silenced_by_any_place_it_names() {
    let shown = snapshot("b1", ["repo", "other"]);
    let mut mutes = Mutes::default();
    mute(&mut mutes, &shown, Scope::Tab, "w1:t2");
    let silenced = mutes.silenced(&local(), &shown);
    assert!(
        silenced.notice(None, None, Some("w1:p2")),
        "the pane's tab is muted"
    );
    assert!(silenced.notice(None, Some("w1:t2"), None));
    assert!(silenced.notice(Some("w2"), Some("w1:t2"), None));
    assert!(
        !silenced.notice(Some("w1"), None, None),
        "the workspace is not"
    );
    assert!(!silenced.notice(Some("w1"), Some("w1:t1"), Some("w1:p1")));
    // A pane the snapshot does not list follows the tab its notice names.
    assert!(silenced.notice(Some("w1"), Some("w1:t2"), Some("w1:p9")));
    assert!(!silenced.notice(None, None, Some("w1:p9")));
    assert!(!silenced.notice(None, None, None));
}

#[test]
fn only_real_changes_count_and_unmuting_restores_everything() {
    let shown = snapshot("b1", ["repo", "other"]);
    let mut mutes = Mutes::default();
    assert!(
        !unmute(&mut mutes, &shown, Scope::Workspace, "w1"),
        "nothing to unmute"
    );
    assert!(
        !mute(&mut mutes, &shown, Scope::Workspace, "w9"),
        "not shown"
    );
    assert_eq!(mutes.revision(), 0);
    assert!(mute(&mut mutes, &shown, Scope::Workspace, "w1"));
    assert!(
        !mute(&mut mutes, &shown, Scope::Workspace, "w1"),
        "already muted"
    );
    assert!(mute(&mut mutes, &shown, Scope::Tab, "w1:t1"));
    assert_eq!(mutes.revision(), 2);
    assert!(unmute(&mut mutes, &shown, Scope::Workspace, "w1"));
    let silenced = mutes.silenced(&local(), &shown);
    assert!(!silenced.workspace("w1") && !silenced.tab("w1:t2"));
    assert!(silenced.tab("w1:t1"), "a tab muted on its own stays muted");
    assert!(unmute(&mut mutes, &shown, Scope::Tab, "w1:t1"));
    assert!(mutes.silenced(&local(), &shown).is_empty());
    assert_eq!(mutes.revision(), 4);
}

#[test]
fn a_mute_follows_a_rename() {
    let mut mutes = Mutes::default();
    mute(
        &mut mutes,
        &snapshot("b1", ["repo", "other"]),
        Scope::Workspace,
        "w1",
    );
    let renamed = snapshot("b1", ["renamed", "other"]);
    let later = at(MUTED_AT + 1);
    assert!(mutes.silenced(&local(), &renamed).workspace("w1"));
    assert!(!mutes.current(&local(), &renamed, later));
    assert!(mutes.reconcile(&local(), &renamed, later));
    assert!(mutes.current(&local(), &renamed, later));
    assert!(
        !mutes.reconcile(&local(), &renamed, later),
        "already current"
    );
    assert_eq!(mutes.mutes[0].label, "renamed");
}

#[test]
fn a_mute_is_forgotten_once_its_target_stays_closed() {
    let shown = snapshot("b1", ["repo", "other"]);
    let mut mutes = Mutes::default();
    mute(&mut mutes, &shown, Scope::Workspace, "w1");
    mute(&mut mutes, &shown, Scope::Tab, "w2:t1");
    // Another window's snapshot from before the tab opened, or the moment
    // after it closed: the mute waits.
    let mut closed = shown.clone();
    closed.tabs.retain(|tab| tab.tab_id != "w2:t1");
    let soon = at(MUTED_AT + FORGET_AFTER_SECONDS - 1);
    assert!(mutes.current(&local(), &closed, soon));
    assert!(!mutes.reconcile(&local(), &closed, soon));
    assert!(mutes.silenced(&local(), &shown).own_tab("w2:t1"));
    let later = at(MUTED_AT + FORGET_AFTER_SECONDS);
    assert!(!mutes.current(&local(), &closed, later));
    assert!(mutes.reconcile(&local(), &closed, later));
    assert_eq!(mutes.mutes.len(), 1);
    // A tab that opens later under the same ID is a new tab.
    assert!(!mutes.silenced(&local(), &shown).own_tab("w2:t1"));
}

/// One window reconnected after a restart and muted a new tab while another
/// still shows the daemon process from before it.
#[test]
fn a_new_mute_outlasts_another_windows_snapshot_of_the_previous_process() {
    let mut before = snapshot("b1", ["repo", "other"]);
    before.tabs.retain(|tab| tab.tab_id != "w1:t2");
    let mut renamed = snapshot("b1", ["repo", "other"]);
    renamed.tabs[1].label = "older".into();
    let after = snapshot("b2", ["repo", "other"]);
    let mut mutes = Mutes::default();
    mute(&mut mutes, &after, Scope::Tab, "w1:t2");
    for stale in [&before, &renamed] {
        let soon = at(MUTED_AT + 1);
        assert!(mutes.current(&local(), stale, soon));
        assert!(!mutes.reconcile(&local(), stale, soon));
    }
    assert!(mutes.silenced(&local(), &after).own_tab("w1:t2"));
}

#[test]
fn a_restored_target_the_user_named_keeps_its_mute_in_a_new_daemon_process() {
    let shown = named(snapshot("b1", ["repo", "other"]));
    let mut mutes = Mutes::default();
    mute(&mut mutes, &shown, Scope::Workspace, "w1");
    mute(&mut mutes, &shown, Scope::Tab, "w2:t1");
    let restored = named(snapshot("b2", ["repo", "other"]));
    let later = at(MUTED_AT + 1);
    let silenced = mutes.silenced(&local(), &restored);
    assert!(silenced.workspace("w1") && silenced.own_tab("w2:t1"));
    assert!(!mutes.current(&local(), &restored, later));
    assert!(mutes.reconcile(&local(), &restored, later));
    assert!(mutes.current(&local(), &restored, later));
    assert!(mutes.mutes.iter().all(|mute| mute.boot == "b2"));
    // Once carried over, the mute follows renames in the new process too.
    let renamed = named(snapshot("b2", ["renamed", "other"]));
    assert!(mutes.silenced(&local(), &renamed).workspace("w1"));
}

/// Herdr names a workspace nobody named after its folder, and such a tab
/// after its position, so a daemon started afresh can show the same ID and
/// name on new work.
#[test]
fn a_new_daemon_process_does_not_inherit_a_mute_through_a_name_herdr_gave() {
    let shown = snapshot("b1", ["proj", "~"]);
    let mut mutes = Mutes::default();
    mute(&mut mutes, &shown, Scope::Workspace, "w1");
    mute(&mut mutes, &shown, Scope::Tab, "w2:t1");
    let fresh = snapshot("b2", ["proj", "~"]);
    assert!(mutes.silenced(&local(), &fresh).is_empty());
    let later = at(MUTED_AT + FORGET_AFTER_SECONDS);
    assert!(!mutes.current(&local(), &fresh, later));
    assert!(mutes.reconcile(&local(), &fresh, later));
    assert!(mutes.mutes.is_empty());
    // Named later in the new process, it is still new work.
    assert!(mutes.silenced(&local(), &named(fresh)).is_empty());
}

#[test]
fn a_fresh_daemon_reusing_an_id_under_another_label_does_not_inherit_its_mute() {
    let shown = snapshot("b1", ["repo", "other"]);
    let mut mutes = Mutes::default();
    mute(&mut mutes, &shown, Scope::Workspace, "w1");
    mute(&mut mutes, &shown, Scope::Tab, "w1:t2");
    let mut fresh = snapshot("b2", ["elsewhere", "other"]);
    fresh.tabs.retain(|tab| tab.tab_id != "w1:t2");
    assert!(mutes.silenced(&local(), &fresh).is_empty());
    // A new mute waits: another window may still show the old process.
    assert!(!mutes.reconcile(&local(), &fresh, at(MUTED_AT + 1)));
    assert_eq!(mutes.mutes.len(), 2);
    assert!(mutes.reconcile(&local(), &fresh, at(MUTED_AT + FORGET_AFTER_SECONDS)));
    assert!(mutes.mutes.is_empty());
    // Even when the new workspace later takes the old label.
    assert!(
        !mutes
            .silenced(&local(), &snapshot("b2", ["repo", "other"]))
            .workspace("w1")
    );
}

#[test]
fn mutes_belong_to_their_device_and_session() {
    let shown = snapshot("b1", ["repo", "other"]);
    let mut mutes = Mutes::default();
    mute(&mut mutes, &shown, Scope::Workspace, "w1");
    let ssh = |session: &str| {
        Daemon::new(
            "ssh:box",
            &ConnectTarget::Ssh {
                target: "box".into(),
                session: session.into(),
            },
        )
    };
    mutes.set(
        &ssh("default"),
        &shown,
        Scope::Workspace,
        "w2",
        true,
        at(MUTED_AT),
    );
    assert!(mutes.silenced(&local(), &shown).workspace("w1"));
    assert!(!mutes.silenced(&local(), &shown).workspace("w2"));
    assert!(mutes.silenced(&ssh("default"), &shown).workspace("w2"));
    assert!(mutes.silenced(&ssh("work"), &shown).is_empty());
    // Reconciling one session leaves the others' mutes alone.
    let mut empty = shown.clone();
    empty.workspaces.clear();
    empty.tabs.clear();
    let later = at(MUTED_AT + FORGET_AFTER_SECONDS);
    assert!(!mutes.reconcile(&ssh("work"), &empty, later));
    assert_eq!(mutes.mutes.len(), 2);
}

#[test]
fn a_local_daemon_is_its_socket_however_it_is_named() {
    let named = |target: ConnectTarget| Daemon::new("local", &target);
    let session = |name: &str, development| ConnectTarget::Session {
        name: name.into(),
        development,
    };
    let socket = ConnectTarget::Local.socket_path().unwrap();
    assert_eq!(
        named(ConnectTarget::Local),
        named(ConnectTarget::Socket(socket))
    );
    let work = session("work", false).socket_path().unwrap();
    assert_eq!(
        named(session("work", false)).session,
        work.to_string_lossy()
    );
    assert_eq!(
        named(session("work", false)),
        named(ConnectTarget::Socket(work))
    );
    assert_ne!(named(session("work", false)), named(session("work", true)));
    assert_ne!(
        named(session("work", false)),
        named(session("other", false))
    );
    let wsl = Daemon::new(
        "wsl:Ubuntu",
        &ConnectTarget::Wsl {
            distro: "Ubuntu".into(),
            session: "default".into(),
        },
    );
    assert_eq!(wsl.session, "default");
}

#[test]
fn saved_mutes_load_back_and_damaged_files_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gpui").join(FILE);
    let shown = snapshot("b1", ["repo", "other"]);
    let mut mutes = Mutes::at(Some(path.clone()));
    mute(&mut mutes, &shown, Scope::Workspace, "w1");
    mute(&mut mutes, &shown, Scope::Tab, "w2:t1");
    mutes.finish();
    let reopened = Mutes::at(Some(path.clone()));
    let silenced = reopened.silenced(&local(), &shown);
    assert!(silenced.workspace("w1") && silenced.own_tab("w2:t1"));
    assert_eq!(reopened.mutes[0].muted_at, MUTED_AT);

    let saved = |mute: &str| format!(r#"{{"mutes":[{{"endpoint":"local","session":"",{mute}}}]}}"#);
    let untimed = saved(r#""scope":"tab","id":"w1:t1","boot":"b","label":"one""#);
    assert_eq!(parse(untimed.as_bytes()).unwrap()[0].muted_at, 0);
    let empty_id = saved(r#""scope":"tab","id":"","boot":"b","label":"""#);
    assert!(matches!(parse(empty_id.as_bytes()), Err(Error::Invalid)));
    let control = saved(r#""scope":"tab","id":"w1:t1","boot":"b","label":"a\u0007""#);
    assert!(matches!(parse(control.as_bytes()), Err(Error::Invalid)));
    let unknown_scope = saved(r#""scope":"pane","id":"p","boot":"b","label":"""#);
    assert!(matches!(
        parse(unknown_scope.as_bytes()),
        Err(Error::Json(_))
    ));
    std::fs::write(&path, b"{not json").unwrap();
    assert!(Mutes::at(Some(path)).mutes.is_empty());
}

#[test]
fn many_spaces_mute_independently_and_the_oldest_changes_go_first_beyond_the_bound() {
    let mut mutes = Mutes::default();
    let mut shown = snapshot("b1", ["repo", "other"]);
    let workspace = shown.workspaces[0].clone();
    shown.workspaces = (0..2 * (MAX_MUTES + 3))
        .map(|index| ClientShellWorkspace {
            workspace_id: format!("w{index}"),
            ..workspace.clone()
        })
        .collect();
    // Every other workspace, oldest first.
    for index in (0..MAX_MUTES + 3).map(|index| 2 * index) {
        assert!(mute(
            &mut mutes,
            &shown,
            Scope::Workspace,
            &format!("w{index}")
        ));
    }
    let silenced = mutes.silenced(&local(), &shown);
    assert_eq!(mutes.mutes.len(), MAX_MUTES);
    assert!(!silenced.workspace("w0") && !silenced.workspace("w4"));
    assert!(silenced.workspace("w6") && silenced.workspace(&format!("w{}", 2 * (MAX_MUTES + 2))));
    assert!(!silenced.workspace("w7"), "never muted");
    assert_eq!(
        shown
            .workspaces
            .iter()
            .filter(|workspace| silenced.workspace(&workspace.workspace_id))
            .count(),
        MAX_MUTES
    );
}

#[test]
fn the_largest_collection_the_store_accepts_reads_back() {
    // Quotes double when escaped.
    let field = "\"".repeat(MAX_FIELD_BYTES);
    let mut shown = snapshot(&field, ["repo", "other"]);
    let workspace = shown.workspaces[0].clone();
    shown.workspaces = (0..MAX_MUTES)
        .map(|index| {
            let mut id = field.clone();
            id.replace_range(..8, &format!("{index:08}"));
            ClientShellWorkspace {
                workspace_id: id,
                // Longer than kept, in four-byte characters and control
                // characters, so the cut falls inside a character.
                label: "\u{1F507}\u{7}".repeat(MAX_FIELD_BYTES / 4 + 1) + "x",
                ..workspace.clone()
            }
        })
        .collect();
    let daemon = Daemon {
        endpoint: field.clone(),
        session: field.clone(),
    };
    let mut mutes = Mutes::default();
    for workspace in shown.workspaces.clone() {
        assert!(mutes.set(
            &daemon,
            &shown,
            Scope::Workspace,
            &workspace.workspace_id,
            true,
            at(MUTED_AT)
        ));
    }
    for mute in &mut mutes.mutes {
        assert!(mute.label.len() <= MAX_FIELD_BYTES && !mute.label.contains('\u{7}'));
        mute.muted_at = u64::MAX;
    }
    let bytes = serde_json::to_vec(&Saved {
        mutes: mutes.mutes.clone(),
    })
    .unwrap();
    assert!(
        bytes.len() as u64 <= MAX_FILE_BYTES,
        "{} bytes",
        bytes.len()
    );
    assert_eq!(parse(&bytes).unwrap().len(), MAX_MUTES);
    // The cut label still identifies its workspace.
    assert!(
        mutes
            .silenced(&daemon, &shown)
            .workspace(&shown.workspaces[0].workspace_id)
    );

    // An ID too long to keep, or one no file could hold within the bound,
    // is refused rather than saved unreadable.
    let revision = mutes.revision();
    for id in ["w".repeat(MAX_FIELD_BYTES + 1), "w\n1".to_owned()] {
        let mut odd = shown.clone();
        odd.workspaces[0].workspace_id = id.clone();
        assert!(!mutes.set(&daemon, &odd, Scope::Workspace, &id, true, at(MUTED_AT)));
    }
    assert_eq!(mutes.revision(), revision);
}

#[test]
fn labels_are_kept_without_control_characters_and_cut_at_a_character_boundary() {
    assert!(matches!(fit("short"), Cow::Borrowed("short")));
    let label = "a".repeat(MAX_FIELD_BYTES - 1) + "\u{1F507}";
    assert_eq!(fit(&label), "a".repeat(MAX_FIELD_BYTES - 1));
    assert_eq!(fit("tab\tname\n"), "tabname");

    let mut shown = named(snapshot("b1", ["repo\u{1b}[2J", "other"]));
    let mut mutes = Mutes::default();
    mute(&mut mutes, &shown, Scope::Workspace, "w1");
    assert_eq!(mutes.mutes[0].label, "repo[2J");
    shown.boot_id = "b2".into();
    assert!(mutes.silenced(&local(), &shown).workspace("w1"));
    assert!(!mutes.current(&local(), &shown, at(MUTED_AT)));
    assert!(mutes.reconcile(&local(), &shown, at(MUTED_AT)));
    assert_eq!(mutes.mutes[0].boot, "b2", "carried over, not forgotten");
}
