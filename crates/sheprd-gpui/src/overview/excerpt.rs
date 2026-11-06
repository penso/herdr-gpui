//! The last lines of a blocked agent's pane, so its row can say what the
//! agent wants without opening it. It reads only what this client already
//! holds for Go To's preview: the cells its host streams, or the frame
//! remembered when the tab was left. It never asks the daemon for anything.
//!
//! The daemon streams only the tab a client shows, so a blocked agent in a
//! tab this client has not shown since it blocked has no excerpt. A frame
//! from before the agent blocked is not one: it would show an old question
//! as the one waiting.

use crate::{notifications::safe_text, palette::LastSeen};
use sheprd_client::protocol::{FrameData, PaneSurfaceFrame, SurfaceRect};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

/// Lines a row shows.
const LINES: usize = 3;
/// Terminal text is untrusted; a row has one line for each of these.
const LINE_LIMIT: usize = 200;

/// One agent's excerpt, as the last tick found it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Excerpt {
    pub endpoint: String,
    pub pane: String,
    pub lines: Vec<String>,
}

/// The row `y` of `rect`, without what the cells leave blank at its end.
fn line(frame: &FrameData, rect: SurfaceRect, y: u16) -> Option<String> {
    let (width, y) = (usize::from(frame.width), usize::from(y));
    let start = y.checked_mul(width)?.checked_add(usize::from(rect.x))?;
    let columns = usize::from(rect.width).min(width.saturating_sub(usize::from(rect.x)));
    let cells = frame.cells.get(start..start.checked_add(columns)?)?;
    let text: String = cells
        .iter()
        // The cell after a wide character continues it.
        .filter(|cell| !cell.skip)
        .map(|cell| match cell.symbol.as_str() {
            "" => " ",
            symbol => symbol,
        })
        .collect();
    Some(safe_text(&text, LINE_LIMIT).trim().to_owned())
}

/// The last lines of `pane` in `surface` that say something, oldest first:
/// a rule or box edge drawn around a prompt says nothing.
fn lines(surface: &PaneSurfaceFrame, pane: &str) -> Option<Vec<String>> {
    let rect = surface
        .panes
        .iter()
        .find(|shown| shown.pane_id == pane)?
        .inner_rect;
    let bottom = rect.y.saturating_add(rect.height).min(surface.frame.height);
    let mut found: Vec<String> = (rect.y..bottom)
        .rev()
        .filter_map(|y| line(&surface.frame, rect, y))
        .filter(|text| text.chars().any(char::is_alphanumeric))
        .take(LINES)
        .collect();
    found.reverse();
    (!found.is_empty()).then_some(found)
}

/// What `pane`, blocked since `since`, shows now or showed when its tab was
/// last left. Nothing from another daemon boot is ever shown.
pub(super) fn of_pane(
    live: Option<&Arc<PaneSurfaceFrame>>,
    last_seen: &LastSeen,
    boot: &str,
    pane: &str,
    since: Option<SystemTime>,
) -> Option<Vec<String>> {
    let current = live
        .filter(|surface| surface.boot_id == boot)
        .and_then(|surface| lines(surface, pane));
    current.or_else(|| {
        let since = since?.duration_since(UNIX_EPOCH).ok()?.as_secs();
        last_seen
            .frames(boot)
            .filter(|(_, at)| *at >= since)
            .find_map(|(surface, _)| lines(surface, pane))
    })
}
