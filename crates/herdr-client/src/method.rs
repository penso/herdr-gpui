//! The endpoint API methods this client invokes.
//!
//! The daemon's advertised method list stays an open `Vec<String>` on the wire,
//! because a peer may offer methods this client knows nothing about. What is
//! closed is the set this client can *send*, so that set is an enum: a method
//! name reaches the wire through `as_str` in exactly one place, and a typo is a
//! compile error instead of an `UnsupportedMethod` rejection at runtime.

/// An endpoint API method. `as_str` is the wire spelling, and the only place a
/// method name is written out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    ClientShellSurfaceSet,
    CommandInvoke,
    LayoutSetSplitRatio,
    PaneClear,
    PaneClose,
    PaneFocus,
    PaneFocusDirection,
    PaneRename,
    PaneScroll,
    PaneSplit,
    PaneZoom,
    ServerReloadConfig,
    TabClose,
    TabCreate,
    TabFocus,
    TabRename,
    WorkspaceClose,
    WorkspaceCreate,
    WorkspaceFocus,
    WorkspaceGet,
    WorkspaceMoveBlock,
    WorkspaceRename,
    WorktreeCreate,
    WorktreeList,
    WorktreeOpen,
    WorktreeRemove,
}

impl Method {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ClientShellSurfaceSet => "client_shell.surface.set",
            Self::CommandInvoke => "command.invoke",
            Self::LayoutSetSplitRatio => "layout.set_split_ratio",
            Self::PaneClear => "pane.clear",
            Self::PaneClose => "pane.close",
            Self::PaneFocus => "pane.focus",
            Self::PaneFocusDirection => "pane.focus_direction",
            Self::PaneRename => "pane.rename",
            Self::PaneScroll => "pane.scroll",
            Self::PaneSplit => "pane.split",
            Self::PaneZoom => "pane.zoom",
            Self::ServerReloadConfig => "server.reload_config",
            Self::TabClose => "tab.close",
            Self::TabCreate => "tab.create",
            Self::TabFocus => "tab.focus",
            Self::TabRename => "tab.rename",
            Self::WorkspaceClose => "workspace.close",
            Self::WorkspaceCreate => "workspace.create",
            Self::WorkspaceFocus => "workspace.focus",
            Self::WorkspaceGet => "workspace.get",
            Self::WorkspaceMoveBlock => "workspace.move_block",
            Self::WorkspaceRename => "workspace.rename",
            Self::WorktreeCreate => "worktree.create",
            Self::WorktreeList => "worktree.list",
            Self::WorktreeOpen => "worktree.open",
            Self::WorktreeRemove => "worktree.remove",
        }
    }

    /// Whether a peer's advertised method list contains this method. The list
    /// is remote data, so it is compared as text rather than parsed.
    pub fn advertised_in(self, methods: &[String]) -> bool {
        methods.iter().any(|method| method == self.as_str())
    }
}

impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn split_ratio_wire_name() {
        assert_eq!(
            Method::LayoutSetSplitRatio.as_str(),
            "layout.set_split_ratio"
        );
    }
}
