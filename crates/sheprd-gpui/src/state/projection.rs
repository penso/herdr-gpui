//! Keep a coherent displayed pair while newer metadata waits for its surface.
use super::{ConnectionStatus, LiveState, coherent};
use sheprd_client::{
    SurfaceImages,
    protocol::{ClientShellSnapshot, PaneSurfaceFrame},
};
use std::sync::Arc;

fn same_input_projection(old: &ClientShellSnapshot, new: &ClientShellSnapshot) -> bool {
    // New protocol fields must make an explicit choice about input invalidation.
    let ClientShellSnapshot {
        boot_id,
        revision: _,
        config_diagnostic: _,
        product_announcement: _,
        update_available: _,
        update_install_command: _,
        server_keybindings_toml,
        latest_release_notes_available: _,
        integration_updates_available: _,
        worktree_directory: _,
        release_notes: _,
        focused_workspace_id,
        focused_tab_id,
        focused_pane_id,
        tab_bar_right: _,
        tab_bar_right_separator: _,
        agent_view_label: _,
        agent_order: _,
        workspaces: _,
        tabs: _,
        panes: _,
        agents: _,
        commands,
    } = new;
    if &old.boot_id != boot_id
        || &old.focused_workspace_id != focused_workspace_id
        || &old.focused_tab_id != focused_tab_id
        || &old.focused_pane_id != focused_pane_id
        || &old.server_keybindings_toml != server_keybindings_toml
        || &old.commands != commands
    {
        return false;
    }
    let zoomed = |snapshot: &ClientShellSnapshot| {
        snapshot
            .tabs
            .iter()
            .find(|tab| Some(&tab.tab_id) == snapshot.focused_tab_id.as_ref())
            .map(|tab| tab.zoomed)
    };
    zoomed(old) == zoomed(new) && input_panes(old).eq(input_panes(new))
}

fn input_panes(
    snapshot: &ClientShellSnapshot,
) -> impl Iterator<Item = (&str, &str, &str, bool, bool)> {
    snapshot
        .panes
        .iter()
        .filter(move |pane| Some(&pane.tab_id) == snapshot.focused_tab_id.as_ref())
        .map(|pane| {
            (
                pane.pane_id.as_str(),
                pane.workspace_id.as_str(),
                pane.tab_id.as_str(),
                pane.focused,
                pane.right_click_passthrough,
            )
        })
}

impl LiveState {
    /// The newest received metadata, which may be ahead of the displayed pair.
    /// Notices, sounds, checkpoints, and notice clicks read it; terminal
    /// rendering, hit testing, and input stay on `snapshot`.
    pub(crate) fn latest_snapshot(&self) -> Option<&Arc<ClientShellSnapshot>> {
        self.pending_snapshot.as_ref().or(self.snapshot.as_ref())
    }

    pub(crate) fn invalidate_surface(&mut self) {
        self.surface = None;
        if let Some(snapshot) = self.pending_snapshot.take() {
            self.commit_snapshot(snapshot);
        }
        self.dirty = true;
    }

    pub(super) fn receive_snapshot(&mut self, snapshot: Arc<ClientShellSnapshot>) {
        if self.latest_snapshot().is_some_and(|latest| {
            latest.boot_id == snapshot.boot_id && latest.revision > snapshot.revision
        }) {
            return;
        }
        if self.surface_ready()
            && self
                .snapshot
                .as_ref()
                .is_some_and(|old| same_input_projection(old, &snapshot))
            && self
                .surface
                .as_ref()
                .is_some_and(|surface| surface.projection_revision != snapshot.revision)
        {
            self.pending_snapshot = Some(snapshot);
            return;
        }
        self.pending_snapshot = None;
        self.commit_snapshot(snapshot);
    }

    pub(super) fn receive_surface(&mut self, surface: Arc<PaneSurfaceFrame>) {
        if let Some(pending) = &self.pending_snapshot {
            if !coherent(pending, &surface) {
                return;
            }
            let snapshot = self.pending_snapshot.take();
            if let Some(snapshot) = snapshot {
                self.commit_snapshot(snapshot);
            }
        }
        if self
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| coherent(snapshot, &surface))
        {
            if let Some(images) = self.pending_surface_images.take() {
                self.surface_images = images;
            }
            self.surface = Some(surface);
        }
    }

    pub(super) fn receive_surface_images(&mut self, images: Arc<SurfaceImages>) {
        if self.surface.is_some() {
            // Session emits pixels before their surface. A UI poll between
            // those events must keep the old picture's own asset catalog.
            self.pending_surface_images = Some(images);
        } else {
            self.surface_images = images;
            self.pending_surface_images = None;
        }
    }

    fn commit_snapshot(&mut self, snapshot: Arc<ClientShellSnapshot>) {
        let boot_changed = self
            .snapshot
            .as_ref()
            .is_some_and(|old| old.boot_id != snapshot.boot_id);
        if boot_changed {
            self.notifications.clear();
            self.cancel_sounds();
            self.sound_cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
            self.bells = 0;
            self.window_title = None;
            self.surface_images = Default::default();
            self.pending_surface_images = None;
        }
        if let Some(activation) = &mut self.activation
            && activation.boot != snapshot.boot_id
        {
            activation.failed = true;
        }
        self.missing_installation = false;
        if self
            .surface
            .as_ref()
            .is_some_and(|surface| !coherent(&snapshot, surface))
            || self
                .snapshot
                .as_ref()
                .is_some_and(|old| !same_input_projection(old, &snapshot))
        {
            self.surface = None;
        }
        if self
            .announcement_dismissal
            .as_ref()
            .is_some_and(|dismissal| !dismissal.covers(&snapshot))
        {
            self.announcement_dismissal = None;
        }
        self.status = ConnectionStatus::Connected;
        self.previous_pane = match &self.snapshot {
            Some(old) if old.boot_id == snapshot.boot_id => old
                .focused_pane_id
                .clone()
                .filter(|old| Some(old) != snapshot.focused_pane_id.as_ref())
                .or_else(|| self.previous_pane.take()),
            _ => None,
        };
        let previous = self
            .snapshot
            .as_deref()
            .filter(|old| old.boot_id == snapshot.boot_id);
        self.agent_recency.observe(previous, &snapshot);
        self.snapshot = Some(snapshot);
    }
}
