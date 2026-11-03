//! Go To's preview of the selected row, as in Telescope and snacks.picker.
//!
//! It shows only what this client already holds: a pane's cells when its
//! tab is the one its host is streaming, painted read-only from that
//! prepared surface; else the tab's last frame from before the host moved
//! on, labelled with its age; or else a few facts from the host's snapshot.
//! It never asks the daemon for anything, so there is no fetch to bound or
//! cancel.

use super::{go_to::status_badge, last_seen::LastSeen, projects};
use crate::{
    NavigationTarget,
    config::Theme,
    i18n::fl,
    terminal_painter::{Ground, Layer, Part, Span, TerminalPainter},
};
use gpui::*;
use herdr_client::protocol::{ClientShellSnapshot, PaneSurfaceFrame, SurfaceRect};
use std::{cell::RefCell, rc::Rc, sync::Arc};

/// Facts listed for a row with no cells on hand; the rest are left out.
const MAX_FACTS: usize = 10;
/// The smallest font a screen shrinks to before it is cropped instead.
const MIN_FONT_SIZE: f32 = 5.;

/// What the preview shows for the selected Go To row.
pub(super) enum Preview {
    /// Cells of `rect` within a surface this client already holds.
    Screen {
        surface: Arc<PaneSurfaceFrame>,
        rect: SurfaceRect,
        /// Unix seconds when a remembered frame was left; none when live.
        seen_at: Option<u64>,
    },
    /// The row is not on any surface this client holds.
    Facts {
        title: SharedString,
        facts: Vec<(SharedString, SharedString)>,
    },
}

/// The preview of `target`, listed from `boot`, given what its host's
/// connection holds and remembers. Nothing from another daemon boot is
/// ever shown.
pub(super) fn preview(
    snapshot: Option<&ClientShellSnapshot>,
    surface: Option<&Arc<PaneSurfaceFrame>>,
    last_seen: &LastSeen,
    boot: &str,
    target: NavigationTarget<&str>,
) -> Option<Preview> {
    let snapshot = snapshot.filter(|snapshot| snapshot.boot_id == boot)?;
    let live = surface
        .filter(|surface| surface.boot_id == boot)
        .map(|surface| (surface, None));
    // Live cells first, then the newest remembered frame that shows it.
    live.into_iter()
        .chain(
            last_seen
                .frames(boot)
                .map(|(surface, at)| (surface, Some(at))),
        )
        .find_map(|(surface, seen_at)| {
            screen(snapshot, surface, target.clone()).map(|rect| Preview::Screen {
                surface: surface.clone(),
                rect,
                seen_at,
            })
        })
        .or_else(|| facts(snapshot, target))
}

/// Where `target` lies within `surface`, if the surface shows it.
fn screen(
    snapshot: &ClientShellSnapshot,
    surface: &Arc<PaneSurfaceFrame>,
    target: NavigationTarget<&str>,
) -> Option<SurfaceRect> {
    let whole = SurfaceRect {
        x: 0,
        y: 0,
        width: surface.frame.width,
        height: surface.frame.height,
    };
    // A surface streams one tab; any of its panes says which.
    let shown = surface
        .panes
        .first()
        .and_then(|shown| snapshot.panes.iter().find(|p| p.pane_id == shown.pane_id));
    match target {
        // A remembered frame can outlive a pane closed since.
        NavigationTarget::Pane(id) if !snapshot.panes.iter().any(|p| p.pane_id == id) => None,
        NavigationTarget::Pane(id) => surface
            .panes
            .iter()
            .find(|pane| pane.pane_id == id)
            .map(|pane| pane.inner_rect),
        NavigationTarget::Tab(id) => shown.filter(|pane| pane.tab_id == id).map(|_| whole),
        NavigationTarget::Workspace(id) => {
            shown.filter(|pane| pane.workspace_id == id).map(|_| whole)
        }
    }
}

fn facts(snapshot: &ClientShellSnapshot, target: NavigationTarget<&str>) -> Option<Preview> {
    let workspace = |id: &str| snapshot.workspaces.iter().find(|w| w.workspace_id == id);
    let tab = |id: &str| snapshot.tabs.iter().find(|t| t.tab_id == id);
    let agent = |pane: &str| snapshot.agents.iter().find(|a| a.pane_id == pane);
    // An agent by its name and status words, a terminal by its directory.
    let pane_fact = |pane: &herdr_client::protocol::ClientShellPane| {
        let name = pane
            .label
            .as_deref()
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .map_or_else(|| fl!("palette-go-to-terminal"), str::to_owned);
        match agent(&pane.pane_id) {
            Some(agent) => (
                crate::sidebar::agent_name(agent).to_owned(),
                crate::sidebar::state_label(agent, &status_badge(agent.agent_status)).into_owned(),
            ),
            None => (
                name,
                pane.foreground_cwd
                    .as_deref()
                    .or(pane.cwd.as_deref())
                    .unwrap_or_default()
                    .to_owned(),
            ),
        }
    };
    let (title, mut facts): (String, Vec<(String, String)>) = match target {
        NavigationTarget::Pane(id) => {
            let pane = snapshot.panes.iter().find(|p| p.pane_id == id)?;
            let (name, state) = pane_fact(pane);
            let mut facts = Vec::new();
            if agent(id).is_some() {
                facts.push((fl!("palette-go-to-fact-status"), state));
            }
            if let Some(cwd) = pane.foreground_cwd.as_deref().or(pane.cwd.as_deref()) {
                facts.push((fl!("palette-go-to-fact-directory"), cwd.into()));
            }
            if let Some(workspace) = workspace(&pane.workspace_id) {
                facts.push((fl!("palette-go-to-fact-workspace"), workspace.label.clone()));
            }
            if let Some(tab) = tab(&pane.tab_id) {
                facts.push((fl!("palette-go-to-fact-tab"), tab.label.clone()));
            }
            (name, facts)
        }
        NavigationTarget::Tab(id) => {
            let tab = tab(id)?;
            let mut facts = Vec::new();
            if let Some(workspace) = workspace(&tab.workspace_id) {
                facts.push((fl!("palette-go-to-fact-workspace"), workspace.label.clone()));
            }
            facts.extend(
                snapshot
                    .panes
                    .iter()
                    .filter(|pane| pane.tab_id == id)
                    .map(pane_fact),
            );
            (tab.label.clone(), facts)
        }
        NavigationTarget::Workspace(id) => {
            let workspace = workspace(id)?;
            let mut facts = Vec::new();
            if let Some(branch) = &workspace.branch {
                facts.push((fl!("palette-go-to-fact-branch"), branch.clone()));
            }
            if let Some(root) = projects::launch_root(snapshot, id) {
                facts.push((fl!("palette-go-to-fact-path"), root.into()));
            }
            let tabs = snapshot
                .tabs
                .iter()
                .filter(|t| t.workspace_id == id)
                .count();
            facts.push((fl!("palette-go-to-fact-tabs"), tabs.to_string()));
            facts.extend(
                snapshot
                    .panes
                    .iter()
                    .filter(|pane| pane.workspace_id == id && agent(&pane.pane_id).is_some())
                    .map(pane_fact),
            );
            (workspace.label.clone(), facts)
        }
    };
    facts.truncate(MAX_FACTS);
    Some(Preview::Facts {
        title: title.into(),
        facts: facts
            .into_iter()
            .map(|(name, value)| (name.into(), value.into()))
            .collect(),
    })
}

/// How the terminal is drawn, scaled from the configured terminal font.
#[derive(Clone)]
pub(super) struct Look {
    pub font: Font,
    /// The configured terminal font size: a preview never draws larger.
    pub size: f32,
    /// Line height and cell width as multiples of the font size.
    pub line_ratio: f32,
    pub cell_ratio: f32,
    pub theme: Theme,
}

/// The rows of `rect` that lie within a `width` by `height` frame.
fn spans(rect: SurfaceRect, width: u16, height: u16) -> Vec<Span> {
    let columns = rect.x.min(width)..rect.x.saturating_add(rect.width).min(width);
    (rect.y.min(height)..rect.y.saturating_add(rect.height).min(height))
        .map(|row| Span {
            row,
            columns: columns.clone(),
        })
        .collect()
}

/// The font size that fits `columns` by `rows` cells in `bounds`, in half
/// points so a resize does not reshape glyphs at every pixel.
fn fitted_size(look: &Look, bounds: Size<Pixels>, columns: u16, rows: u16) -> f32 {
    let wide = f32::from(bounds.width) / (f32::from(columns.max(1)) * look.cell_ratio);
    let tall = f32::from(bounds.height) / (f32::from(rows.max(1)) * look.line_ratio);
    ((wide.min(tall).min(look.size) * 2.).floor() / 2.).max(MIN_FONT_SIZE)
}

/// Paints `rect` of `surface`, shrunk to fit. A screen too large even at the
/// smallest size keeps its bottom-left corner, where the latest output is.
/// `painter` is the preview's own, so the terminal's glyph cache is never
/// reshaped at the preview's size.
pub(super) fn paint_screen(
    surface: Arc<PaneSurfaceFrame>,
    rect: SurfaceRect,
    painter: Rc<RefCell<TerminalPainter>>,
    look: Look,
) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, cx| {
            let frame = &surface.frame;
            let area = spans(rect, frame.width, frame.height);
            let size = fitted_size(&look, bounds.size, rect.width, rect.height);
            let cell_height = size * look.line_ratio;
            let mut painter = painter.borrow_mut();
            painter.set_appearance(size, cell_height, look.theme.clone());
            let cell_width = painter.cell_width(&look.font, window, cx);
            let height = f32::from(rect.height) * cell_height;
            let rise = (height - f32::from(bounds.size.height)).max(0.);
            let origin = bounds.origin
                - point(
                    px(f32::from(rect.x) * cell_width),
                    px(f32::from(rect.y) * cell_height + rise),
                );
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                for layer in Layer::ALL {
                    painter.paint_frame(
                        frame,
                        origin,
                        None,
                        Ground::Opaque,
                        cell_width,
                        &look.font,
                        &[],
                        &[],
                        Some(Part { area: &area, layer }),
                        None,
                        window,
                        cx,
                    );
                }
            });
        },
    )
    .size_full()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
