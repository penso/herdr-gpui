use herdr_client::protocol::{ClientShellPane, ClientShellSnapshot, PaneSurfaceFrame};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ViewMode {
    #[default]
    Terminal,
    Agent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TabTarget {
    pub endpoint_id: String,
    pub boot_id: String,
    pub tab_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PaneTarget {
    pub endpoint_id: String,
    pub boot_id: String,
    pub tab_id: String,
    pub pane_id: String,
}

#[derive(Default)]
pub(crate) struct AgentModes {
    endpoints: HashMap<String, EndpointModes>,
}

#[derive(Default)]
struct EndpointModes {
    boot_id: Option<String>,
    agent_tabs: HashSet<String>,
}

impl AgentModes {
    pub(crate) fn reconcile(&mut self, endpoint_id: &str, snapshot: &ClientShellSnapshot) {
        let modes = self.endpoints.entry(endpoint_id.to_owned()).or_default();
        if modes.boot_id.as_deref() != Some(snapshot.boot_id.as_str()) {
            modes.agent_tabs.clear();
            modes.boot_id = Some(snapshot.boot_id.clone());
        }
        modes
            .agent_tabs
            .retain(|id| snapshot.tabs.iter().any(|tab| tab.tab_id == *id));
    }

    pub(crate) fn retain_endpoints(&mut self, mut retain: impl FnMut(&str) -> bool) {
        self.endpoints.retain(|id, _| retain(id));
    }

    pub(crate) fn mode(&self, endpoint_id: &str, boot_id: &str, tab_id: &str) -> ViewMode {
        if self.endpoints.get(endpoint_id).is_some_and(|modes| {
            modes.boot_id.as_deref() == Some(boot_id) && modes.agent_tabs.contains(tab_id)
        }) {
            ViewMode::Agent
        } else {
            ViewMode::Terminal
        }
    }

    /// Returns whether the target was accepted, even if its mode was unchanged.
    pub(crate) fn set_mode(
        &mut self,
        target: &TabTarget,
        mode: ViewMode,
        snapshot: &ClientShellSnapshot,
    ) -> bool {
        self.reconcile(&target.endpoint_id, snapshot);
        if target.boot_id != snapshot.boot_id
            || !snapshot.tabs.iter().any(|tab| tab.tab_id == target.tab_id)
        {
            return false;
        }
        let Some(modes) = self.endpoints.get_mut(&target.endpoint_id) else {
            return false;
        };
        match mode {
            ViewMode::Terminal => {
                modes.agent_tabs.remove(&target.tab_id);
            }
            ViewMode::Agent => {
                modes.agent_tabs.insert(target.tab_id.clone());
            }
        }
        true
    }

    pub(crate) fn focused_target(
        &self,
        endpoint_id: &str,
        snapshot: &ClientShellSnapshot,
    ) -> Option<PaneTarget> {
        let pane = focused_pane(snapshot)?;
        (self.mode(endpoint_id, &snapshot.boot_id, &pane.tab_id) == ViewMode::Agent).then(|| {
            PaneTarget {
                endpoint_id: endpoint_id.to_owned(),
                boot_id: snapshot.boot_id.clone(),
                tab_id: pane.tab_id.clone(),
                pane_id: pane.pane_id.clone(),
            }
        })
    }
}

fn focused_pane(snapshot: &ClientShellSnapshot) -> Option<&ClientShellPane> {
    let workspace_id = snapshot.focused_workspace_id.as_deref()?;
    let tab_id = snapshot.focused_tab_id.as_deref()?;
    let pane_id = snapshot.focused_pane_id.as_deref()?;
    snapshot.workspaces.iter().find(|workspace| {
        workspace.workspace_id == workspace_id && workspace.active_tab_id == tab_id
    })?;
    snapshot
        .tabs
        .iter()
        .find(|tab| tab.tab_id == tab_id && tab.workspace_id == workspace_id)?;
    snapshot.panes.iter().find(|pane| {
        pane.pane_id == pane_id && pane.tab_id == tab_id && pane.workspace_id == workspace_id
    })
}

/// Submission additionally requires a coherent rendered surface with no popup.
pub(crate) fn valid_target(
    target: &PaneTarget,
    endpoint_id: &str,
    snapshot: &ClientShellSnapshot,
    surface: &PaneSurfaceFrame,
) -> bool {
    target.endpoint_id == endpoint_id
        && target.boot_id == snapshot.boot_id
        && surface.boot_id == snapshot.boot_id
        && surface.projection_revision == snapshot.revision
        && surface.popup.is_none()
        && focused_pane(snapshot)
            .is_some_and(|pane| pane.tab_id == target.tab_id && pane.pane_id == target.pane_id)
        && surface
            .panes
            .iter()
            .any(|pane| pane.pane_id == target.pane_id)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use herdr_client::protocol::{
        ClientShellPopupSurface, FrameData, PaneSurfacePane, SurfaceRect,
    };

    fn snapshot() -> ClientShellSnapshot {
        serde_json::from_str(include_str!(
            "../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
        ))
        .unwrap()
    }

    fn tab_target(snapshot: &ClientShellSnapshot) -> TabTarget {
        TabTarget {
            endpoint_id: "local".into(),
            boot_id: snapshot.boot_id.clone(),
            tab_id: snapshot.focused_tab_id.clone().unwrap(),
        }
    }

    fn agent_target(snapshot: &ClientShellSnapshot) -> PaneTarget {
        let mut modes = AgentModes::default();
        assert!(modes.set_mode(&tab_target(snapshot), ViewMode::Agent, snapshot));
        modes.focused_target("local", snapshot).unwrap()
    }

    fn surface(snapshot: &ClientShellSnapshot) -> PaneSurfaceFrame {
        let rect = SurfaceRect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
        PaneSurfaceFrame {
            boot_id: snapshot.boot_id.clone(),
            projection_revision: snapshot.revision,
            surface_revision: 1,
            frame: FrameData {
                cells: vec![],
                width: 0,
                height: 0,
                cursor: None,
                hyperlinks: vec![],
                graphics: vec![],
            },
            panes: vec![PaneSurfacePane {
                pane_id: snapshot.focused_pane_id.clone().unwrap(),
                content_revision: 1,
                rect,
                inner_rect: rect,
                scrollbar_rect: None,
                scroll: None,
                focused: true,
                mouse_reporting: false,
                sgr_pixel_mouse: false,
                alternate_screen_active: false,
                pixel_width: 0,
                pixel_height: 0,
            }],
            splits: vec![],
            popup: None,
            graphics: Default::default(),
        }
    }

    #[test]
    fn defaults_and_selection_are_independent_of_agent_detection() {
        let mut snapshot = snapshot();
        let mut modes = AgentModes::default();
        let target = tab_target(&snapshot);
        assert_eq!(ViewMode::default(), ViewMode::Terminal);
        modes.reconcile("local", &snapshot);
        assert_eq!(
            modes.mode("local", &target.boot_id, &target.tab_id),
            ViewMode::Terminal
        );
        assert_eq!(modes.focused_target("local", &snapshot), None);
        snapshot.agents.clear();
        assert!(modes.set_mode(&target, ViewMode::Agent, &snapshot));
        assert!(modes.set_mode(&target, ViewMode::Agent, &snapshot));
        assert!(modes.focused_target("local", &snapshot).is_some());
        assert_eq!(
            modes.mode("local", "other-boot", &target.tab_id),
            ViewMode::Terminal
        );
        assert_eq!(
            modes.mode("local", &target.boot_id, "missing"),
            ViewMode::Terminal
        );
        assert!(modes.set_mode(&target, ViewMode::Terminal, &snapshot));
        assert_eq!(modes.focused_target("local", &snapshot), None);
    }

    #[test]
    fn inactive_tab_selection_does_not_change_daemon_focus() {
        let mut snapshot = snapshot();
        let mut tab = snapshot.tabs[0].clone();
        tab.tab_id = "inactive-tab".into();
        tab.focused = false;
        let target = TabTarget {
            endpoint_id: "local".into(),
            boot_id: snapshot.boot_id.clone(),
            tab_id: tab.tab_id.clone(),
        };
        snapshot.tabs.push(tab);
        let before = snapshot.clone();
        let mut modes = AgentModes::default();
        assert!(modes.set_mode(&target, ViewMode::Agent, &snapshot));
        assert_eq!(
            modes.mode("local", &target.boot_id, &target.tab_id),
            ViewMode::Agent
        );
        assert_eq!(modes.focused_target("local", &snapshot), None);
        assert_eq!(snapshot, before);
    }

    #[test]
    fn reconcile_preserves_same_boot_reconnect_but_prunes_deleted_tabs() {
        let mut snapshot = snapshot();
        let mut modes = AgentModes::default();
        let target = tab_target(&snapshot);
        assert!(modes.set_mode(&target, ViewMode::Agent, &snapshot));
        snapshot.revision += 1;
        modes.reconcile("local", &snapshot.clone());
        assert_eq!(
            modes.mode("local", &target.boot_id, &target.tab_id),
            ViewMode::Agent
        );
        let tabs = snapshot.tabs.clone();
        snapshot.tabs.clear();
        modes.reconcile("local", &snapshot);
        assert_eq!(
            modes.mode("local", &target.boot_id, &target.tab_id),
            ViewMode::Terminal
        );
        assert!(!modes.set_mode(&target, ViewMode::Agent, &snapshot));
        snapshot.tabs = tabs;
        modes.reconcile("local", &snapshot);
        assert_eq!(
            modes.mode("local", &target.boot_id, &target.tab_id),
            ViewMode::Terminal
        );
    }

    #[test]
    fn changed_boot_clears_modes_even_when_selection_target_is_stale() {
        let mut snapshot = snapshot();
        let mut modes = AgentModes::default();
        let target = tab_target(&snapshot);
        assert!(modes.set_mode(&target, ViewMode::Agent, &snapshot));
        snapshot.boot_id = "new-boot".into();
        assert!(!modes.set_mode(&target, ViewMode::Agent, &snapshot));
        assert_eq!(
            modes.mode("local", &target.boot_id, &target.tab_id),
            ViewMode::Terminal
        );
        assert_eq!(
            modes.mode("local", &snapshot.boot_id, &target.tab_id),
            ViewMode::Terminal
        );
        assert_eq!(modes.focused_target("local", &snapshot), None);
    }

    #[test]
    fn inconsistent_focus_and_membership_reject_targets() {
        let snapshot = snapshot();
        let frame = surface(&snapshot);
        let target = agent_target(&snapshot);
        let mut modes = AgentModes::default();
        assert!(modes.set_mode(&tab_target(&snapshot), ViewMode::Agent, &snapshot));
        let mutations: &[fn(&mut ClientShellSnapshot)] = &[
            |s| s.focused_workspace_id = None,
            |s| s.focused_tab_id = None,
            |s| s.focused_pane_id = None,
            |s| s.focused_workspace_id = Some("other".into()),
            |s| s.focused_tab_id = Some("other".into()),
            |s| s.focused_pane_id = Some("other".into()),
            |s| s.workspaces.clear(),
            |s| s.tabs.clear(),
            |s| s.panes.clear(),
            |s| s.workspaces[0].active_tab_id = "other".into(),
            |s| s.tabs[0].workspace_id = "other".into(),
            |s| s.panes[0].workspace_id = "other".into(),
            |s| s.panes[0].tab_id = "other".into(),
        ];
        for mutate in mutations {
            let mut changed = snapshot.clone();
            mutate(&mut changed);
            assert_eq!(modes.focused_target("local", &changed), None);
            assert!(!valid_target(&target, "local", &changed, &frame));
        }
    }

    #[test]
    fn submission_requires_current_target_and_coherent_rendered_pane() {
        let snapshot = snapshot();
        let frame = surface(&snapshot);
        let target = agent_target(&snapshot);
        assert!(valid_target(&target, "local", &snapshot, &frame));
        for changed in [
            PaneTarget {
                boot_id: "other".into(),
                ..target.clone()
            },
            PaneTarget {
                tab_id: "other".into(),
                ..target.clone()
            },
            PaneTarget {
                pane_id: "other".into(),
                ..target.clone()
            },
        ] {
            assert!(!valid_target(&changed, "local", &snapshot, &frame));
        }
        let mutations: &[fn(&mut PaneSurfaceFrame)] = &[
            |s| s.boot_id = "other".into(),
            |s| s.projection_revision -= 1,
            |s| s.projection_revision += 1,
            |s| s.panes.clear(),
            |s| s.panes[0].pane_id = "other".into(),
        ];
        for mutate in mutations {
            let mut changed = frame.clone();
            mutate(&mut changed);
            assert!(!valid_target(&target, "local", &snapshot, &changed));
        }
    }

    #[test]
    fn changing_focus_to_another_member_pane_invalidates_captured_target() {
        let mut snapshot = snapshot();
        let captured = agent_target(&snapshot);
        let mut pane = snapshot.panes[0].clone();
        pane.pane_id = "second-pane".into();
        snapshot.panes[0].focused = false;
        snapshot.focused_pane_id = Some(pane.pane_id.clone());
        snapshot.panes.push(pane);
        snapshot.revision += 1;
        let frame = surface(&snapshot);
        assert!(!valid_target(&captured, "local", &snapshot, &frame));
        assert!(valid_target(
            &agent_target(&snapshot),
            "local",
            &snapshot,
            &frame
        ));
    }

    #[test]
    fn popups_prevent_submission_without_discarding_selection() {
        let snapshot = snapshot();
        let mut frame = surface(&snapshot);
        let target = agent_target(&snapshot);
        frame.popup = Some(Box::new(ClientShellPopupSurface {
            terminal_id: "popup".into(),
            title: String::new(),
            width: None,
            height: None,
            frame: frame.frame.clone(),
            mouse_reporting: false,
            sgr_pixel_mouse: false,
            pixel_width: 0,
            pixel_height: 0,
        }));
        assert!(!valid_target(&target, "local", &snapshot, &frame));
        assert_eq!(agent_target(&snapshot), target);
        frame.popup = None;
        assert!(valid_target(&target, "local", &snapshot, &frame));
    }

    #[test]
    fn identical_host_identities_remain_isolated_and_prune_only_their_endpoint() {
        let snapshot = snapshot();
        let local = tab_target(&snapshot);
        let remote = TabTarget {
            endpoint_id: "remote".into(),
            ..local.clone()
        };
        let mut modes = AgentModes::default();
        modes.set_mode(&local, ViewMode::Agent, &snapshot);
        assert_eq!(modes.focused_target("remote", &snapshot), None);
        modes.set_mode(&remote, ViewMode::Agent, &snapshot);
        let local_pane = modes.focused_target("local", &snapshot).unwrap();
        let remote_pane = modes.focused_target("remote", &snapshot).unwrap();
        assert_ne!(local_pane, remote_pane);
        assert!(!valid_target(
            &local_pane,
            "remote",
            &snapshot,
            &surface(&snapshot)
        ));
        let mut drafts = HashMap::new();
        drafts.insert(local_pane.clone(), "local draft");
        drafts.insert(remote_pane.clone(), "remote draft");
        assert_eq!(drafts.len(), 2);
        let mut changed = snapshot.clone();
        changed.tabs.clear();
        modes.reconcile("remote", &changed);
        assert_eq!(modes.focused_target("local", &snapshot), Some(local_pane));
        assert_eq!(modes.focused_target("remote", &snapshot), None);
        modes.set_mode(&remote, ViewMode::Agent, &snapshot);
        changed.boot_id = "new-boot".into();
        modes.reconcile("remote", &changed);
        assert_eq!(
            modes.mode("local", &local.boot_id, &local.tab_id),
            ViewMode::Agent
        );
        modes.retain_endpoints(|id| id == "remote");
        assert_eq!(modes.focused_target("local", &snapshot), None);
    }
}
