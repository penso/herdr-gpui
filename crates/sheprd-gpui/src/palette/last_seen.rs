//! The last frame each host streamed for a tab before it moved to another,
//! so Go To can preview a pane that is no longer on screen, labelled with
//! when it was seen. The daemon streams only the tab a client shows, and
//! this client never asks it for more.
//!
//! Frames are the connection's own shared `Arc`s, kept as they arrived:
//! remembering one copies nothing, but it keeps alive a frame that would
//! otherwise be freed, about a megabyte for a full window, and more with a
//! popup or images. Everything a frame holds is counted. So memory, not
//! tab count, bounds them: [`HOST_BYTES`] per host and [`TOTAL_BYTES`]
//! across hosts, the longest unseen going first. Each frame is stamped with
//! its daemon boot and never shown for another.

use sheprd_client::protocol::{
    CellData, ClientShellPopupSurface, ClientShellSnapshot, FrameData, PaneSurfaceFrame,
    PaneSurfacePane, PaneSurfaceSplit, SurfaceGraphicsAsset, SurfaceGraphicsAssetKey,
    SurfaceGraphicsPlacement, SurfaceGraphicsSource, SurfaceGraphicsTarget,
};
use std::{collections::VecDeque, sync::Arc};

/// Bytes of frames one host keeps alive.
pub(super) const HOST_BYTES: usize = 4 << 20;
/// Bytes of frames all hosts together keep alive.
pub(super) const TOTAL_BYTES: usize = 8 << 20;
/// Frames per host whatever their size, so empty ones stay bounded too.
const MAX_TABS: usize = 64;
/// The least a non-empty string's heap allocation takes.
const MIN_ALLOCATION: usize = 16;

struct Seen {
    tab: String,
    surface: Arc<PaneSurfaceFrame>,
    /// Unix seconds when the tab was left.
    at: u64,
    bytes: usize,
}

/// One host's remembered frames, oldest first.
#[derive(Default)]
pub(crate) struct LastSeen {
    frames: VecDeque<Seen>,
    bytes: usize,
}

/// The heap behind a string of `capacity`: any allocation takes at least an
/// allocator block.
fn text(capacity: usize) -> usize {
    match capacity {
        0 => 0,
        capacity => capacity.max(MIN_ALLOCATION),
    }
}

/// The heap a grid holds: cells, their symbols, links, and graphics bytes.
fn grid(frame: &FrameData) -> usize {
    let symbols: usize = frame
        .cells
        .iter()
        .map(|cell| text(cell.symbol.capacity()))
        .sum();
    let links: usize = frame
        .hyperlinks
        .iter()
        .map(|link| text(link.capacity()))
        .sum();
    frame.cells.capacity() * size_of::<CellData>()
        + symbols
        + frame.hyperlinks.capacity() * size_of::<String>()
        + links
        + frame.graphics.capacity()
}

/// The strings an image key holds. Matched without a catch-all, so a new
/// source or target cannot go uncounted.
fn key(key: &SurfaceGraphicsAssetKey) -> usize {
    match &key.source {
        SurfaceGraphicsSource::Terminal { target, .. } => match target {
            SurfaceGraphicsTarget::Pane { pane_id } => text(pane_id.capacity()),
            SurfaceGraphicsTarget::Popup { terminal_id } => text(terminal_id.capacity()),
        },
        SurfaceGraphicsSource::PaneLayer { pane_id, layer_id } => {
            text(pane_id.capacity()) + text(layer_id.capacity())
        }
    }
}

/// The heap a whole frame keeps alive, which is what remembering it costs:
/// the frame itself, its grid, any popup over it, its panes and splits, and
/// image data.
/// Counted once, when its tab is left.
fn bytes(surface: &PaneSurfaceFrame) -> usize {
    let panes: usize = surface
        .panes
        .iter()
        .map(|pane| text(pane.pane_id.capacity()))
        .sum();
    let splits: usize = surface
        .splits
        .iter()
        .map(|split| split.path.capacity())
        .sum();
    let popup = surface.popup.as_deref().map_or(0, |popup| {
        size_of::<ClientShellPopupSurface>()
            + grid(&popup.frame)
            + text(popup.terminal_id.capacity())
            + text(popup.title.capacity())
    });
    let scene = &surface.graphics;
    let assets: usize = scene
        .assets
        .iter()
        .map(|asset| asset.data.capacity() + key(&asset.key))
        .sum();
    let placements: usize = scene.placements.iter().map(|p| key(&p.asset)).sum();
    let retained: usize = scene.retained_assets.iter().map(key).sum();
    // The shared allocation itself: two reference counts and the frame.
    2 * size_of::<usize>()
        + size_of::<PaneSurfaceFrame>()
        + text(surface.boot_id.capacity())
        + grid(&surface.frame)
        + surface.panes.capacity() * size_of::<PaneSurfacePane>()
        + panes
        + surface.splits.capacity() * size_of::<PaneSurfaceSplit>()
        + splits
        + popup
        + scene.assets.capacity() * size_of::<SurfaceGraphicsAsset>()
        + assets
        + scene.placements.capacity() * size_of::<SurfaceGraphicsPlacement>()
        + placements
        + scene.retained_assets.capacity() * size_of::<SurfaceGraphicsAssetKey>()
        + retained
}

/// The tab `surface` streams, by `snapshot`, when both are of one boot.
fn tab_of<'a>(
    snapshot: Option<&'a ClientShellSnapshot>,
    surface: Option<&Arc<PaneSurfaceFrame>>,
) -> Option<&'a str> {
    let (snapshot, surface) = snapshot.zip(surface)?;
    if snapshot.boot_id != surface.boot_id {
        return None;
    }
    // A surface streams one tab; any of its panes says which.
    let shown = surface.panes.first()?;
    snapshot
        .panes
        .iter()
        .find(|pane| pane.pane_id == shown.pane_id)
        .map(|pane| pane.tab_id.as_str())
}

impl LastSeen {
    /// Called as a host's state moves from `before` to `after`: a tab no
    /// longer streamed is remembered as last seen, and the tab now streamed
    /// is forgotten, since its live cells take over.
    pub(crate) fn observe(
        &mut self,
        before: (Option<&ClientShellSnapshot>, Option<&Arc<PaneSurfaceFrame>>),
        after: (Option<&ClientShellSnapshot>, Option<&Arc<PaneSurfaceFrame>>),
        now: u64,
    ) {
        let left = tab_of(before.0, before.1);
        let shown = tab_of(after.0, after.1);
        if let Some(boot) = after.0.map(|snapshot| snapshot.boot_id.as_str()) {
            self.forget(|seen| seen.surface.boot_id != boot);
        }
        if let (Some(tab), Some(surface)) = (left, before.1)
            && shown != Some(tab)
            // A tab already gone is never chosen, so it is not worth a slot.
            && after.0.is_some_and(|snapshot| snapshot.tabs.iter().any(|t| t.tab_id == tab))
        {
            self.forget(|seen| seen.tab == tab);
            let bytes = bytes(surface);
            // A frame larger than the whole budget is not worth evicting for.
            if bytes <= HOST_BYTES {
                self.frames.push_back(Seen {
                    tab: tab.to_owned(),
                    surface: surface.clone(),
                    at: now,
                    bytes,
                });
                self.bytes += bytes;
            }
            while self.bytes > HOST_BYTES || self.frames.len() > MAX_TABS {
                self.forget_oldest();
            }
        }
        if let Some(tab) = shown {
            self.forget(|seen| seen.tab == tab);
        }
    }

    fn forget(&mut self, mut gone: impl FnMut(&Seen) -> bool) {
        let mut freed = 0;
        self.frames.retain(|seen| {
            let keep = !gone(seen);
            if !keep {
                freed += seen.bytes;
            }
            keep
        });
        self.bytes -= freed;
    }

    fn forget_oldest(&mut self) {
        if let Some(seen) = self.frames.pop_front() {
            self.bytes -= seen.bytes;
        }
    }

    /// Keeps every host's frames within [`TOTAL_BYTES`] together, forgetting
    /// the longest unseen of any host first.
    pub(crate) fn trim<'a>(hosts: impl IntoIterator<Item = &'a mut Self>) {
        let mut hosts: Vec<&mut Self> = hosts.into_iter().collect();
        let mut total: usize = hosts.iter().map(|host| host.bytes).sum();
        while total > TOTAL_BYTES {
            let Some(oldest) = hosts
                .iter_mut()
                .filter_map(|host| Some((host.frames.front()?.at, host)))
                .min_by_key(|(at, _)| *at)
                .map(|(_, host)| host)
            else {
                return;
            };
            let before = oldest.bytes;
            oldest.forget_oldest();
            total -= before - oldest.bytes;
        }
    }

    /// Remembered frames of `boot` with when each was left, newest first.
    pub(crate) fn frames<'a>(
        &'a self,
        boot: &'a str,
    ) -> impl Iterator<Item = (&'a Arc<PaneSurfaceFrame>, u64)> {
        self.frames
            .iter()
            .rev()
            .filter(move |seen| seen.surface.boot_id == boot)
            .map(|seen| (&seen.surface, seen.at))
    }
}
