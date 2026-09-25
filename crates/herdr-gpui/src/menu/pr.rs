use super::Page;
use crate::{
    HerdrWindow,
    pull_request::{Input, PullRequest, State, repository_input},
};
use gpui_kit::{
    component::{ActiveTheme as _, h_flex, tag::Tag, v_flex},
    prelude::*,
    *,
};
use herdr_client::protocol::*;
use std::sync::Arc;

impl HerdrWindow {
    pub(super) fn refresh_workspace_pr(&mut self) {
        self.menu.pr.clear();
        if !self.menu.github.connected() {
            return;
        }
        let result = (|| {
            let target = self
                .menu
                .target
                .as_ref()
                .ok_or(crate::Error::StaleWorkspace)?;
            if target.worktree.is_none() || target.branch.as_deref().is_none_or(str::is_empty) {
                return Err(crate::Error::PrMetadata);
            }
            if self.selected_endpoint != 0 || !self.live.local_daemon_peer {
                return Err(crate::Error::PrUntrustedEndpoint);
            }
            if !self.workspace_pr_target_current() {
                return Err(crate::Error::StaleWorkspace);
            }
            repository_input(target.worktree.as_ref(), target.branch.as_deref())
        })();
        match result {
            Ok(input) => {
                self.sync_pr_scope();
                self.menu.pr_connection = Some(Arc::downgrade(
                    &self.endpoints[self.selected_endpoint].connection.inbox,
                ));
                self.menu
                    .pr_cache
                    .present(&input, &mut self.menu.pr, std::time::Instant::now());
            }
            Err(error) => self.menu.pr.message = Some(error.to_string()),
        }
    }

    pub(super) fn sync_pr_scope(&mut self) {
        let endpoint = &self.endpoints[self.selected_endpoint];
        let same_connection = self
            .menu
            .pr_cache_connection
            .as_ref()
            .and_then(std::sync::Weak::upgrade)
            .is_some_and(|old| Arc::ptr_eq(&old, &endpoint.connection.inbox));
        if !same_connection {
            self.menu.pr_cache.clear();
            self.menu.pr_cache_connection = Some(Arc::downgrade(&endpoint.connection.inbox));
        }
        if let (Some(snapshot), Some(profile)) = (&self.live.snapshot, &self.menu.github.profile) {
            self.menu.pr_cache.scope(
                (
                    self.selection_epoch,
                    endpoint.generation,
                    snapshot.boot_id.clone(),
                ),
                profile.token.clone(),
            );
        }
    }

    fn workspace_pr_target_current(&self) -> bool {
        self.menu_target_current()
            && self.live.status.is_connected()
            && self.menu.pr_connection.as_ref().is_none_or(|old| {
                old.upgrade().is_some_and(|old| {
                    Arc::ptr_eq(
                        &old,
                        &self.endpoints[self.selected_endpoint].connection.inbox,
                    )
                })
            })
            && self.menu.target.as_ref().is_some_and(|target| {
                self.live.snapshot.as_ref().is_some_and(|snapshot| {
                    snapshot.boot_id == target.boot_id
                        && snapshot.workspaces.iter().any(|workspace| {
                            workspace.workspace_id == target.id
                                && workspace.worktree == target.worktree
                                && workspace.branch == target.branch
                        })
                })
            })
    }

    pub(crate) fn update_workspace_pr(&mut self) -> bool {
        let mut changed = false;
        if !self.menu.github.connected() {
            self.menu.pr_cache.clear();
            self.menu.pr.clear();
            return changed;
        }
        let eligible = self.selected_endpoint == 0
            && self.live.local_daemon_peer
            && self.live.status.is_connected()
            && self.live.snapshot.is_some();
        if eligible {
            self.sync_pr_scope();
            let now = std::time::Instant::now();
            if let Some(snapshot) = &self.live.snapshot {
                if self
                    .menu
                    .pr_snapshot
                    .as_ref()
                    .and_then(std::sync::Weak::upgrade)
                    .is_none_or(|old| !Arc::ptr_eq(&old, snapshot))
                {
                    self.menu.pr_snapshot = Some(Arc::downgrade(snapshot));
                    self.menu.pr_cache.retain(|input| {
                        snapshot.workspaces.iter().any(|workspace| {
                            workspace
                                .worktree
                                .as_ref()
                                .is_some_and(|tree| tree.key == input.repo_key)
                                && workspace.branch.as_ref() == Some(&input.branch)
                        })
                    });
                }
                if self.menu.pr_cache.scan_due(now) {
                    let priority = self
                        .menu
                        .target
                        .as_ref()
                        .filter(|_| self.menu.page == Some(Page::Workspace))
                        .map(|target| target.id.as_str());
                    let inputs = workspace_pr_inputs(snapshot, priority, self.menu.pr_cache.cursor);
                    self.menu.pr_cache.schedule(inputs, now);
                }
            }
            changed |= self.menu.pr_cache.poll(now);
        } else {
            self.menu.pr_cache.clear();
        }
        if self.menu.page == Some(Page::Workspace) && !self.workspace_pr_target_current() {
            if self.menu.pr.message.as_deref()
                != Some("Workspace changed or disconnected. Reopen the menu.")
            {
                self.menu.pr.clear();
                self.menu.pr.message =
                    Some("Workspace changed or disconnected. Reopen the menu.".into());
                changed = true;
            }
        } else if changed && self.menu.page == Some(Page::Workspace) {
            self.refresh_workspace_pr();
        }
        changed
    }

    pub(super) fn open_workspace_pr(&self, cx: &mut Context<Self>) {
        if self.menu.github.connected()
            && self.workspace_pr_target_current()
            && let Some(pr) = &self.menu.pr.value
        {
            cx.open_url(&pr.url);
        }
    }

    /// The workspace menu's pull request section, read live from the lookup so
    /// it follows the answer while the menu is open.
    pub(super) fn render_workspace_pr(&self, cx: &App) -> AnyElement {
        let pr = &self.menu.pr;
        let muted = cx.theme().muted_foreground;
        let mut section = v_flex()
            .debug_selector(|| "workspace-pr".into())
            .w_full()
            .min_w_0()
            .gap_1()
            .py_1();
        if let Some(value) = &pr.value {
            section = section
                .child(
                    div()
                        .debug_selector(|| "workspace-pr-title".into())
                        .truncate()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(format!("#{} {}", value.number, value.title)),
                )
                .child(div().truncate().text_sm().text_color(muted).child(format!(
                    "{} -> {}",
                    value.head_ref_name, value.base_ref_name
                )))
                .child(
                    h_flex()
                        .gap_2()
                        .child(Tag::secondary().child(value.lifecycle()))
                        .child(div().min_w_0().text_color(muted).child(value.review())),
                )
                .child(div().text_color(muted).child(value.merge_status()))
                .child(div().child(value.checks_summary.clone()))
                .child(
                    h_flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            div()
                                .text_color(cx.theme().green)
                                .child(format!("+{}", value.additions)),
                        )
                        .child(
                            div()
                                .text_color(cx.theme().red)
                                .child(format!("-{}", value.deletions)),
                        )
                        .child(div().text_color(muted).child(format!(
                            "{} {}",
                            value.changed_files,
                            if value.changed_files == 1 {
                                "file"
                            } else {
                                "files"
                            }
                        ))),
                );
        }
        let note = if pr.loading {
            Some("Checking GitHub...".to_owned())
        } else if let Some(message) = &pr.message {
            Some(format!(
                "{}{message}",
                if pr.value.is_some() { "Stale: " } else { "" }
            ))
        } else if pr.value.is_none() {
            Some("No PR found for this origin and branch.".to_owned())
        } else {
            None
        };
        section
            .children(note.map(|note| div().text_color(muted).child(note)))
            .into_any_element()
    }

    #[cfg(any(test, all(feature = "integration-test", target_os = "macos")))]
    pub(crate) fn workspace_pr_fixture(&mut self, value: PullRequest) -> crate::Result<()> {
        let target = self
            .menu
            .target
            .as_ref()
            .ok_or(crate::Error::StaleWorkspace)?;
        let input = repository_input(target.worktree.as_ref(), target.branch.as_deref())?;
        self.menu.github = crate::github::Auth::connected_fixture();
        self.live.local_daemon_peer = true;
        self.sync_pr_scope();
        self.menu
            .pr_cache
            .seed(input, value, std::time::Instant::now());
        self.refresh_workspace_pr();
        Ok(())
    }

    #[cfg(any(test, feature = "integration-test"))]
    pub(crate) fn github_fixture(
        &mut self,
        waiting: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_github(false, window, cx);
        self.menu.github = crate::github::Auth::fixture(waiting);
        cx.notify();
    }
}

/// A pull request's lifecycle in the kit's semantic colors.
pub(super) fn pr_color(pr: &PullRequest, cx: &App) -> Hsla {
    let theme = cx.theme();
    match pr.state {
        State::Merged => theme.magenta,
        State::Closed => theme.danger,
        State::Unknown => theme.muted_foreground,
        State::Open if pr.is_draft => theme.muted_foreground,
        State::Open => theme.success,
    }
}

#[cfg(test)]
fn checkout_input(
    response: &serde_json::Value,
    id: &str,
    worktree: Option<&ClientShellWorktree>,
    branch: Option<&str>,
) -> crate::Result<Input> {
    let result = &response["result"];
    let workspace = &result["workspace"];
    let tree = &workspace["worktree"];
    let mut input = repository_input(worktree, branch)?;
    if response.get("error").is_some()
        || result["type"] != "workspace_info"
        || workspace["workspace_id"] != id
        || tree["repo_key"] != input.repo_key
    {
        return Err(crate::Error::WorkspaceCheckoutChanged);
    }
    let checkout = tree["checkout_path"]
        .as_str()
        .filter(|s| std::path::Path::new(s).is_absolute())
        .ok_or(crate::Error::PrAbsolutePath)?;
    input.checkout = Some(checkout.into());
    Ok(input)
}

fn workspace_pr_inputs<'a>(
    snapshot: &'a ClientShellSnapshot,
    open: Option<&'a str>,
    cursor: usize,
) -> impl Iterator<Item = Input> + 'a {
    let count = snapshot.workspaces.len();
    let start = cursor % count.max(1);
    let priority = open.or(snapshot.focused_workspace_id.as_deref());
    // Alternate priority and round-robin admission so focus changes cannot
    // starve the rest of the live workspace list. Cache scheduling deduplicates.
    snapshot
        .workspaces
        .iter()
        .filter(move |workspace| {
            cursor.is_multiple_of(2) && Some(workspace.workspace_id.as_str()) == priority
        })
        .chain(snapshot.workspaces.iter().cycle().skip(start).take(count))
        .filter_map(|workspace| {
            repository_input(workspace.worktree.as_ref(), workspace.branch.as_deref()).ok()
        })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::{checkout_input, repository_input};
    use herdr_client::protocol::ClientShellWorktree;

    #[test]
    fn metadata_priority_alternates_with_round_robin_and_skips_ineligible_workspaces() {
        let mut snapshot = crate::sidebar::layout_tests::snapshot(6);
        snapshot.focused_workspace_id = Some("w5".into());
        let branches = |snapshot: &super::ClientShellSnapshot, open, cursor| {
            super::workspace_pr_inputs(snapshot, open, cursor)
                .map(|input| input.branch)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            branches(&snapshot, None, 0)[0],
            "worktree/sidebar-child-with-a-long-readable-branch-name"
        );
        assert_eq!(
            branches(&snapshot, Some("w4"), 0)[0],
            "worktree/sidebar-child"
        );
        assert_eq!(
            branches(&snapshot, Some("w4"), 1)[0],
            "develop",
            "every other admission serves the rest"
        );
        assert_eq!(
            branches(&snapshot, None, 5)[0],
            "worktree/sidebar-child-with-a-long-readable-branch-name"
        );
        snapshot.workspaces[3].worktree.as_mut().unwrap().key = "relative".into();
        snapshot.workspaces[4].branch = None;
        assert_eq!(branches(&snapshot, None, 1).len(), 1);
        snapshot.workspaces.clear();
        assert!(branches(&snapshot, None, 0).is_empty());
    }

    #[gpui_kit::test]
    fn cached_menu_open_is_immediate_and_does_not_touch_deletion_response(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let (view, cx) =
            crate::test_support::add_window_view(cx, crate::sidebar::layout_tests::fixture_window);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.live.status = crate::state::ConnectionStatus::Connected;
                view.open_workspace_menu("w3", Default::default(), window, cx);
                view.workspace_pr_fixture(crate::pull_request::fixture().unwrap())
                    .unwrap();
                view.dismiss_menu(window, cx);
                let pending = Some(("delete-request".into(), None));
                view.live.dialog_response = pending.clone();
                view.endpoints[0]
                    .connection
                    .inbox
                    .lock()
                    .unwrap()
                    .dialog_response = pending.clone();
                view.open_workspace_menu("w3", Default::default(), window, cx);
                assert_eq!(view.menu.pr.value.as_ref().unwrap().number, 8);
                assert!(!view.menu.pr.loading);
                assert!(
                    matches!(&view.live.dialog_response, Some((id, None)) if id == "delete-request")
                );
                assert!(matches!(
                    &view.endpoints[0]
                        .connection
                        .inbox
                        .lock()
                        .unwrap()
                        .dialog_response,
                    Some((id, None)) if id == "delete-request"
                ));
                // Switching repository/branch never reuses this cached PR.
                view.dismiss_menu(window, cx);
                view.open_workspace_menu("w4", Default::default(), window, cx);
                assert!(view.menu.pr.value.is_none());
                assert!(view.menu.pr.loading);
            })
        });
    }

    #[cfg(all(feature = "integration-test", target_os = "macos"))]
    #[test]
    #[allow(clippy::expect_used)]
    #[ignore = "requires explicit HERDR_TEST_PR_SOCKET/REPO_KEY/BRANCH; HERDR_TEST_PR_GITHUB=1 additionally uses existing sign-in"]
    fn live_local_pr_lookup() {
        use herdr_client::{
            ClientEvent, ConnectOptions, ConnectTarget, Method, connect_with_connector,
        };
        use std::{
            env,
            path::PathBuf,
            time::{Duration, Instant},
        };

        let socket = env::var_os("HERDR_TEST_PR_SOCKET").expect("explicit PR socket required");
        let repo_key =
            env::var("HERDR_TEST_PR_REPO_KEY").expect("explicit repository key required");
        let branch = env::var("HERDR_TEST_PR_BRANCH").expect("explicit branch required");
        let client = connect_with_connector(
            ConnectTarget::Socket(PathBuf::from(socket)),
            ConnectOptions::default(),
            false,
            |target, stop| {
                let (stream, local) =
                    crate::daemon::connect(target, stop, || panic!("must not start daemon"))?;
                if !local {
                    return Err(std::io::Error::other("local endpoint validation failed"));
                }
                eprintln!("Live local endpoint validation passed.");
                Ok(stream)
            },
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut supports_workspace_get = false;
        let snapshot = loop {
            let event = client
                .events
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("snapshot deadline");
            match event {
                ClientEvent::Connected(welcome) => {
                    supports_workspace_get = welcome
                        .methods
                        .iter()
                        .any(|method| method == Method::WorkspaceGet.as_str())
                }
                ClientEvent::Snapshot(snapshot) => break snapshot,
                ClientEvent::Disconnected { reason } => panic!("local connection failed: {reason}"),
                _ => {}
            }
        };
        let workspace = snapshot
            .workspaces
            .iter()
            .find(|workspace| {
                workspace.branch.as_deref() == Some(&branch)
                    && workspace
                        .worktree
                        .as_ref()
                        .is_some_and(|tree| tree.key == repo_key)
            })
            .expect("requested repository/branch not present in daemon snapshot");
        let input = if supports_workspace_get {
            let id = client
                .handle
                .request(
                    &snapshot.boot_id,
                    Method::WorkspaceGet,
                    serde_json::json!({"workspace_id":workspace.workspace_id}),
                )
                .unwrap();
            let response = loop {
                let event = client
                    .events
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .expect("workspace response deadline");
                match event {
                    ClientEvent::Response {
                        request_id,
                        response,
                    } if request_id == id => break response,
                    ClientEvent::Disconnected { reason } => {
                        panic!("read-only workspace request failed: {reason}");
                    }
                    ClientEvent::CommandRejected { reason, .. } => {
                        panic!("read-only workspace request failed: {reason}");
                    }
                    _ => {}
                }
            };
            checkout_input(
                &response,
                &workspace.workspace_id,
                workspace.worktree.as_ref(),
                workspace.branch.as_deref(),
            )
            .unwrap()
        } else {
            eprintln!("Using validated Git worktree registry for older daemon.");
            repository_input(workspace.worktree.as_ref(), workspace.branch.as_deref()).unwrap()
        };
        client.handle.disconnect();
        crate::pull_request::local_repository(
            &input,
            Instant::now() + Duration::from_secs(15),
            &|| false,
        )
        .unwrap();
        eprintln!("Live checkout common-directory, branch, and GitHub origin validation passed.");
        if env::var_os("HERDR_TEST_PR_GITHUB").as_deref() != Some(std::ffi::OsStr::new("1")) {
            eprintln!("Authenticated GitHub lookup not requested (set HERDR_TEST_PR_GITHUB=1).");
            return;
        }
        let mut auth = crate::github::Auth::default();
        auth.initialize(&crate::config::Config::default());
        let deadline = Instant::now() + Duration::from_secs(45);
        while auth.loading_profile() && Instant::now() < deadline {
            auth.poll();
            std::thread::sleep(Duration::from_millis(10));
        }
        let profile = auth
            .profile
            .as_ref()
            .expect("existing GitHub sign-in unavailable");
        let mut lookup = crate::pull_request::Lookup::default();
        lookup.request(input, profile.token.clone());
        let deadline = Instant::now() + Duration::from_secs(20);
        while lookup.loading && Instant::now() < deadline {
            lookup.poll();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!lookup.loading, "PR worker deadline");
        assert!(
            lookup.message.is_none(),
            "PR lookup failed: {:?}",
            lookup.message
        );
        assert!(lookup.checked.is_some());
        eprintln!(
            "Live local endpoint, checkout identity/branch, and authenticated PR lookup passed (PR present: {}).",
            lookup.value.is_some()
        );
    }

    #[test]
    fn checkout_response_requires_authoritative_identity_and_absolute_path() {
        let root = std::env::temp_dir();
        let repo_key = root.join("repo/.git").to_str().unwrap().to_owned();
        let checkout = root.join("worktree").to_str().unwrap().to_owned();
        let tree = ClientShellWorktree {
            key: repo_key.clone(),
            label: "repo".into(),
            is_linked_worktree: true,
        };
        let response = serde_json::json!({"result":{"type":"workspace_info", "workspace":{
            "workspace_id":"w", "worktree":{"repo_key":repo_key, "checkout_path":checkout}
        }}});
        let input = checkout_input(&response, "w", Some(&tree), Some("feature")).unwrap();
        assert_eq!(input.checkout.as_deref(), Some(checkout.as_str()));
        assert_eq!(input.repo_key, repo_key);
        assert_eq!(input.branch, "feature");
        let fallback = repository_input(Some(&tree), Some("feature")).unwrap();
        assert!(fallback.checkout.is_none());
        assert_eq!(fallback.repo_key, input.repo_key);
        assert_eq!(fallback.branch, input.branch);
        assert!(checkout_input(&response, "wrong", Some(&tree), Some("feature")).is_err());
        for branch in [None, Some(""), Some("bad\nbranch")] {
            assert!(checkout_input(&response, "w", Some(&tree), branch).is_err());
        }
        let mut bad = response.clone();
        bad["result"]["workspace"]["worktree"]["checkout_path"] = "relative".into();
        assert!(checkout_input(&bad, "w", Some(&tree), Some("feature")).is_err());
        let mut bad = response.clone();
        bad["result"]["workspace"]["worktree"]["repo_key"] = "/other/.git".into();
        assert!(checkout_input(&bad, "w", Some(&tree), Some("feature")).is_err());
        let mut bad = response;
        bad["error"] = serde_json::json!({"code":"unsupported"});
        assert!(checkout_input(&bad, "w", Some(&tree), Some("feature")).is_err());
    }
}
