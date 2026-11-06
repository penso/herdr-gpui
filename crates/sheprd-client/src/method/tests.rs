use super::*;

#[test]
fn integration_methods_require_exact_advertisement() {
    for (method, name) in [
        (Method::IntegrationList, "integration.list"),
        (Method::IntegrationInstall, "integration.install"),
        (Method::IntegrationListV2, "integration.list.v2"),
        (Method::IntegrationInstallV2, "integration.install.v2"),
    ] {
        assert_eq!(method.as_str(), name);
        assert_eq!(method.to_string(), name);
        assert!(!method.advertised_in(&[]));
        assert!(!method.advertised_in(&[format!("{name}.extra")]));
        assert!(method.advertised_in(&["unknown.future".into(), name.into()]));
    }
    assert!(!Method::IntegrationInstall.advertised_in(&["integration.list".into()]));
}

#[test]
fn pane_rename_wire_name_and_advertisement() {
    assert_eq!(Method::PaneRename.as_str(), "pane.rename");
    assert_eq!(Method::PaneRename.to_string(), "pane.rename");
    assert!(Method::PaneRename.advertised_in(&["pane.rename".into()]));
    assert!(!Method::PaneRename.advertised_in(&["tab.rename".into()]));
}

#[test]
fn pane_clear_is_only_advertised_by_newer_daemons() {
    assert_eq!(Method::PaneClear.as_str(), "pane.clear");
    assert!(Method::PaneClear.advertised_in(&["pane.close".into(), "pane.clear".into()]));
    // Advertisement is an exact match: a method sharing the prefix is not clearing.
    assert!(!Method::PaneClear.advertised_in(&["pane.clear_agent_authority".into()]));
}

#[test]
fn dismiss_methods_match_the_daemon_spelling() {
    assert_eq!(
        Method::ProductAnnouncementDismiss.as_str(),
        "product_announcement.dismiss"
    );
    assert_eq!(
        Method::ReleaseNotesDismiss.as_str(),
        "release_notes.dismiss"
    );
    assert!(!Method::ReleaseNotesDismiss.advertised_in(&["product_announcement.dismiss".into()]));
}

#[test]
fn layout_key_action_wire_names() {
    assert_eq!(Method::PaneResize.as_str(), "pane.resize");
    assert_eq!(Method::PaneSwap.as_str(), "pane.swap");
}

#[test]
fn pane_input_set_wire_name() {
    assert_eq!(Method::PaneInputSet.as_str(), "pane.input.set");
    assert!(Method::PaneInputSet.advertised_in(&["pane.input.set".into()]));
    assert!(!Method::PaneInputSet.advertised_in(&["pane.input".into()]));
}

#[test]
fn link_methods_are_separate_advertisements() {
    assert_eq!(Method::PaneLinkResolve.as_str(), "pane.link.resolve");
    assert_eq!(Method::PaneLinkActivate.as_str(), "pane.link.activate");
    // A daemon may resolve hover regions without activating, or the reverse.
    assert!(Method::PaneLinkResolve.advertised_in(&["pane.link.resolve".into()]));
    assert!(!Method::PaneLinkActivate.advertised_in(&["pane.link.resolve".into()]));
}

#[test]
fn copy_search_wire_name_and_advertisement() {
    for (method, name) in [
        (Method::PaneCopyMotion, "pane.copy_motion"),
        (Method::PaneEditScrollback, "pane.edit_scrollback"),
        (Method::PaneSelectionRead, "pane.selection.read"),
    ] {
        assert_eq!(method.as_str(), name);
        assert!(method.advertised_in(&[name.into()]));
    }
    assert_eq!(Method::PaneCopySearch.as_str(), "pane.copy_search");
    assert!(Method::PaneCopySearch.advertised_in(&["pane.copy_search".into()]));
    assert!(!Method::PaneCopySearch.advertised_in(&["pane.copy_motion".into()]));
}

#[test]
fn workspace_move_is_not_the_block_move() {
    assert_eq!(Method::WorkspaceMove.as_str(), "workspace.move");
    assert!(Method::WorkspaceMove.advertised_in(&["workspace.move".into()]));
    // A daemon offering only the block move does not offer the single one.
    assert!(!Method::WorkspaceMove.advertised_in(&["workspace.move_block".into()]));
    assert!(!Method::WorkspaceMoveBlock.advertised_in(&["workspace.move".into()]));
}

#[test]
fn split_ratio_wire_name() {
    assert_eq!(
        Method::LayoutSetSplitRatio.as_str(),
        "layout.set_split_ratio"
    );
}

/// Herdr's `CLIENT_SHELL_METHODS` (src/server/client_commands.rs): the
/// only methods it answers on the client endpoint. Anything else is
/// rejected, so a variant outside this list is a dead code path.
const OFFERED: &[&str] = &[
    // keep-sorted start
    "client_shell.surface.set",
    "command.invoke",
    "integration.install",
    "integration.install.v2",
    "integration.list",
    "integration.list.v2",
    "layout.set_split_ratio",
    "pane.clear",
    "pane.close",
    "pane.copy_motion",
    "pane.copy_search",
    "pane.edit_scrollback",
    "pane.focus",
    "pane.focus_direction",
    "pane.input.set",
    "pane.link.activate",
    "pane.link.resolve",
    "pane.rename",
    "pane.resize",
    "pane.scroll",
    "pane.selection.read",
    "pane.split",
    "pane.swap",
    "pane.zoom",
    "product_announcement.dismiss",
    "release_notes.dismiss",
    "server.reload_config",
    "tab.close",
    "tab.create",
    "tab.focus",
    "tab.move",
    "tab.rename",
    "workspace.close",
    "workspace.create",
    "workspace.focus",
    "workspace.move",
    "workspace.move_block",
    "workspace.rename",
    "worktree.create",
    "worktree.list",
    "worktree.open",
    "worktree.remove",
    // keep-sorted end
];

#[test]
fn every_method_is_offered_to_endpoint_clients() {
    for method in Method::ALL {
        assert!(
            OFFERED.contains(&method.as_str()),
            "{method} is not offered to endpoint clients"
        );
    }
    // `workspace.get` is API-socket only; the snapshot carries what we need.
    assert!(!OFFERED.contains(&"workspace.get"));
}
