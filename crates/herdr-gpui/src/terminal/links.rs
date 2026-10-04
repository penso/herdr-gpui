//! Resolve user-selected web links within the painted pane or popup only.
use super::{HIDDEN, InputTarget, popup_origin, wheel_target};
use herdr_client::protocol::{FrameData, PaneSurfaceFrame};
use std::ops::Range;

pub(super) const MAX_ROW_BYTES: usize = 32768;

fn web_url(value: &str) -> Option<String> {
    crate::browser::WebUrl::try_from(value)
        .ok()
        .map(String::from)
}

pub(crate) fn link_at(
    surface: &PaneSurfaceFrame,
    x: f32,
    y: f32,
    cell_width: f32,
    cell_height: f32,
) -> Option<String> {
    // Share popup isolation, pane bounds, and invalid-geometry handling with input.
    let target = wheel_target(surface, x, y, cell_width, cell_height)?;
    let (frame, x, y, start, end) = match target.target {
        InputTarget::Popup(_) => {
            let popup = surface.popup.as_ref()?;
            let origin = popup_origin(&surface.frame, &popup.frame, cell_width, cell_height);
            (
                &popup.frame,
                x - f32::from(origin.x),
                y - f32::from(origin.y),
                0,
                popup.frame.width,
            )
        }
        InputTarget::Pane(id) => {
            let pane = surface.panes.iter().find(|pane| pane.pane_id == id)?;
            (
                &surface.frame,
                x,
                y,
                pane.inner_rect.x,
                pane.inner_rect.x.saturating_add(pane.inner_rect.width),
            )
        }
    };
    frame_link(
        frame,
        (x / cell_width).floor() as u16,
        (y / cell_height).floor() as u16,
        start,
        end,
    )
}

fn frame_link(frame: &FrameData, column: u16, row: u16, start: u16, end: u16) -> Option<String> {
    if column < start || column >= end || end > frame.width || row >= frame.height {
        return None;
    }
    let offset = usize::from(row) * usize::from(frame.width);
    let cells = frame
        .cells
        .get(offset + usize::from(start)..offset + usize::from(end))?;
    let selected = usize::from(column - start);
    let mut source = selected;
    while cells.get(source)?.skip && source > 0 {
        source -= 1;
    }
    let cell = cells.get(source)?;
    if cell.modifier & HIDDEN != 0 {
        return None;
    }
    // An explicit link is authoritative, even if its destination is disallowed.
    if let Some(index) = cells[selected].hyperlink.or(cell.hyperlink) {
        return web_url(frame.hyperlinks.get(index as usize)?);
    }
    let mut text = String::new();
    let mut hit = 0;
    for (index, cell) in cells.iter().enumerate() {
        if index == source {
            hit = text.len();
        }
        if cell.skip {
            continue;
        }
        let symbol = if cell.modifier & HIDDEN != 0 || cell.symbol.is_empty() {
            " "
        } else {
            &cell.symbol
        };
        if text.len() + symbol.len() > MAX_ROW_BYTES {
            return None;
        }
        text.push_str(symbol);
    }
    // Plain URLs are row-local: the surface doesn't distinguish soft wraps from
    // separate lines, so joining rows could silently change the destination.
    // A daemon offering `pane.link.resolve` reads wraps from its own terminal
    // state instead; see `crate::links`.
    match plain_url(&text, hit)? {
        (_, true) => None,
        (range, false) => web_url(&text[range]),
    }
}

/// The byte range of the plain web URL in one row's `text` that covers the
/// byte at `hit`, trimmed of the prose punctuation around it, and whether the
/// URL runs to the end of the row, where it may continue off-screen or on the
/// next row.
pub(super) fn plain_url(text: &str, hit: usize) -> Option<(Range<usize>, bool)> {
    for (start, _) in text.match_indices("http") {
        if start > hit {
            break;
        }
        let tail = &text[start..];
        if !tail.starts_with("http://") && !tail.starts_with("https://") {
            continue;
        }
        let end = tail
            .find(|c: char| {
                c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | '"' | '\'' | '`')
            })
            .unwrap_or(tail.len());
        // Check the original token: punctuation at the edge may be part of a
        // destination continuing off-screen or on the next row.
        let open = end == tail.len();
        let mut candidate = tail[..end].trim_end_matches(['.', ',', ';', ':', '!', '?']);
        for (open, close) in [('(', ')'), ('[', ']'), ('{', '}')] {
            let excess = candidate
                .matches(close)
                .count()
                .saturating_sub(candidate.matches(open).count());
            for _ in 0..excess {
                let Some(trimmed) = candidate.strip_suffix(close) else {
                    break;
                };
                candidate = trimmed;
            }
        }
        if hit < start + candidate.len() {
            return Some((start..start + candidate.len(), open));
        }
        if open {
            return None;
        }
    }
    None
}

#[cfg(test)]
mod tests;
