//! The selection a plugin action is invoked with. Herdr resolves the text
//! itself from the coordinates `command.invoke` carries, so a plugin reads
//! exactly what was highlighted on the host that owns the pane, never the
//! local clipboard. As Herdr's own client does, only a highlight in the
//! focused pane counts, fenced by the content revision of the frame on screen
//! when the action runs: the daemon refuses the invocation with
//! `stale_content` rather than read cells that changed after that frame.

use super::HerdrWindow;
use herdr_client::scrollback::{SelectionReadParams, TextRange};

impl HerdrWindow {
    /// The highlighted cells of the focused pane, as `command.invoke` takes
    /// them: a released pointer selection or a copy-mode mark. `None` while
    /// the surface is not ready or a popup covers the panes, since a popup
    /// selection never belongs to the pane underneath, and for a selection
    /// in any other pane.
    pub(crate) fn plugin_selection(&self) -> Option<SelectionReadParams> {
        let pane = self.focused_surface_pane()?;
        let surface = self.live.surface.as_deref()?;
        let boot = &self.live.snapshot.as_deref()?.boot_id;
        let range = match &self.selection {
            Some(selection) if !selection.dragging() => selection
                .content_range(surface, self.cell_width, self.config.terminal.line_height())
                .filter(|(pane_id, _)| *pane_id == pane.pane_id)
                .map(|(_, range)| range),
            Some(_) => None,
            None => self.copy_mode_marked(boot, pane),
        }?;
        let TextRange { start, end } = range;
        Some(SelectionReadParams {
            pane_id: pane.pane_id.clone(),
            anchor: start,
            cursor: end,
            content_revision: Some(pane.content_revision),
        })
    }

    /// Fences a selection captured earlier, as the palette's is when it
    /// opens, by the frame on screen now, so output while the palette was
    /// open does not refuse the action. A pane no longer focused or covered
    /// by a popup keeps its old revision, which the daemon refuses.
    pub(crate) fn fence_plugin_selection(&self, selection: &mut SelectionReadParams) {
        if let Some(pane) = self
            .focused_surface_pane()
            .filter(|pane| pane.pane_id == selection.pane_id)
        {
            selection.content_revision = Some(pane.content_revision);
        }
    }
}
