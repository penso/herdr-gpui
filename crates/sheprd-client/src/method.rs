//! The endpoint API methods this client invokes.
//!
//! The daemon's advertised method list stays an open `Vec<String>` on the wire,
//! because a peer may offer methods this client knows nothing about. What is
//! closed is the set this client can *send*, so that set is an enum: a method
//! name reaches the wire through `as_str` in exactly one place, and a typo is a
//! compile error instead of an `UnsupportedMethod` rejection at runtime.

/// Declares each method once, with its wire spelling, so the enum, `as_str`,
/// and the test list of every variant cannot drift apart.
macro_rules! methods {
    ($($variant:ident => $name:literal,)+) => {
        /// An endpoint API method. `as_str` is the wire spelling, and the only
        /// place a method name is written out.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Method {
            $($variant,)+
        }

        impl Method {
            /// Every variant, for tests that check the whole set.
            #[cfg(test)]
            pub(crate) const ALL: &[Self] = &[$(Self::$variant,)+];

            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)+
                }
            }
        }
    };
}

methods! {
    // keep-sorted start
    ClientShellSurfaceSet => "client_shell.surface.set",
    CommandInvoke => "command.invoke",
    IntegrationInstall => "integration.install",
    IntegrationInstallV2 => "integration.install.v2",
    IntegrationList => "integration.list",
    IntegrationListV2 => "integration.list.v2",
    LayoutSetSplitRatio => "layout.set_split_ratio",
    PaneClear => "pane.clear",
    PaneClose => "pane.close",
    PaneCopyMotion => "pane.copy_motion",
    PaneCopySearch => "pane.copy_search",
    PaneEditScrollback => "pane.edit_scrollback",
    PaneFocus => "pane.focus",
    PaneFocusDirection => "pane.focus_direction",
    PaneInputSet => "pane.input.set",
    PaneLinkActivate => "pane.link.activate",
    PaneLinkResolve => "pane.link.resolve",
    PaneRename => "pane.rename",
    PaneResize => "pane.resize",
    PaneScroll => "pane.scroll",
    PaneSelectionRead => "pane.selection.read",
    PaneSplit => "pane.split",
    PaneSwap => "pane.swap",
    PaneZoom => "pane.zoom",
    ProductAnnouncementDismiss => "product_announcement.dismiss",
    ReleaseNotesDismiss => "release_notes.dismiss",
    ServerReloadConfig => "server.reload_config",
    TabClose => "tab.close",
    TabCreate => "tab.create",
    TabFocus => "tab.focus",
    TabMove => "tab.move",
    TabRename => "tab.rename",
    WorkspaceClose => "workspace.close",
    WorkspaceCreate => "workspace.create",
    WorkspaceFocus => "workspace.focus",
    WorkspaceMove => "workspace.move",
    WorkspaceMoveBlock => "workspace.move_block",
    WorkspaceRename => "workspace.rename",
    WorktreeCreate => "worktree.create",
    WorktreeList => "worktree.list",
    WorktreeOpen => "worktree.open",
    WorktreeRemove => "worktree.remove",
    // keep-sorted end
}

impl Method {
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
mod tests;
