use super::*;

#[test]
fn ssh_workspace_paths_are_posix_even_when_the_client_is_windows() {
    let mut workspace = shell_workspace();
    workspace.branch = Some("main".into());
    workspace.worktree = Some(herdr_client::protocol::ClientShellWorktree {
        key: "/remote/project/.git".into(),
        label: "project".into(),
        is_linked_worktree: true,
    });
    let host = Origin::Ssh("work".into());
    let input = super::super::workspace_input(&workspace, &host).unwrap();
    assert_eq!(input.repo_key.as_deref(), Some("/remote/project/.git"));
    assert!(input.checkout.is_none());
    for key in ["relative", "C:\\project\\.git", "/remote\nproject/.git"] {
        workspace.worktree.as_mut().unwrap().key = key.into();
        assert!(matches!(
            super::super::workspace_input(&workspace, &host),
            Err(Error::PullRequest(super::super::Error::AbsolutePath))
        ));
    }
    workspace.worktree = None;
    workspace.new_workspace_cwd = "/local/decoy".into();
    assert!(matches!(
        super::super::workspace_input(&workspace, &host),
        Err(Error::PullRequest(super::super::Error::Metadata))
    ));
}
