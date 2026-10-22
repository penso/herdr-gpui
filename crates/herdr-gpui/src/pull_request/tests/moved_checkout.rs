//! A main checkout whose shell left the repository Herdr keeps for it, as
//! after a Teleport opened it on one repository and its shell moved to
//! another, is looked up as the directory it is in, not the kept repository.

use super::*;
use crate::pull_request::workspace_input;
use herdr_client::protocol::ClientShellWorktree;

fn moved(linked: bool) -> herdr_client::protocol::ClientShellWorkspace {
    let mut workspace = crate::sidebar::layout_tests::snapshot(1)
        .workspaces
        .remove(0);
    let cwd = std::env::temp_dir().join("wgsextract-cli");
    workspace.new_workspace_cwd = cwd.to_str().unwrap().into();
    workspace.branch = Some("main".into());
    workspace.worktree = Some(ClientShellWorktree {
        key: std::env::temp_dir()
            .join("herdr-gpui/.git")
            .to_str()
            .unwrap()
            .into(),
        label: "herdr-gpui".into(),
        is_linked_worktree: linked,
    });
    workspace
}

#[test]
fn a_moved_main_checkout_is_looked_up_from_its_directory() {
    let workspace = moved(false);
    let input = workspace_input(&workspace, &Origin::Local).unwrap();
    assert_eq!(
        input,
        Input {
            checkout: Some(workspace.new_workspace_cwd.clone()),
            repo_key: None,
            branch: "main".into(),
        }
    );
    // A saved device's directory is not on this machine, so it has nothing
    // left to look up rather than the wrong repository.
    assert!(matches!(
        workspace_input(&workspace, &Origin::Ssh("device".into())),
        Err(Error::PrMetadata)
    ));
}

/// A linked checkout's own path is not in the snapshot, so its metadata is
/// kept wherever its shell is.
#[test]
fn a_linked_checkout_keeps_its_metadata() {
    let input = workspace_input(&moved(true), &Origin::Local).unwrap();
    assert!(input.repo_key.is_some_and(|key| key.ends_with(".git")));
    assert_eq!(input.checkout, None);
}
