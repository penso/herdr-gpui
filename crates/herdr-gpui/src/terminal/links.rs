//! Resolve user-selected links within the painted pane or popup only: web
//! addresses, the local file paths a pane prints or names, which open only
//! on the machine that runs the pane, and the other addresses with a defined
//! scheme. Reading a row decides nothing about the disk: whether a path is
//! there is asked once the link modifier is held over it, and every link
//! opens only on a link-modifier click.
use super::{HIDDEN, InputTarget, popup_origin, selection::shown, wheel_target};
use herdr_client::protocol::{CellData, FrameData, PaneSurfaceFrame};
use std::ops::Range;

mod external;
mod path;
pub(crate) use path::{escape_into, escaped_len};

pub(crate) use external::ExternalUrl;

pub(super) const MAX_ROW_BYTES: usize = 32768;

fn web_url(value: &str) -> Option<String> {
    crate::browser::WebUrl::try_from(value)
        .ok()
        .map(String::from)
}

/// The longest hyperlink target read as a path, Linux's `PATH_MAX`.
const MAX_PATH_BYTES: usize = 4096;

/// What an explicit hyperlink points at when it is not a web address: the
/// file a `file://` address or a target without a scheme names, or an
/// address with another scheme. Nothing here asks whether the file exists.
fn explicit_target(destination: &str) -> RowTarget {
    let named = |path: String| RowTarget::Path {
        path,
        line: None,
        explicit: true,
    };
    let url = match url::Url::parse(destination) {
        Ok(url) => url,
        // No scheme: a path as a program would print it, with `~/` and
        // relative paths left for the click to resolve.
        Err(url::ParseError::RelativeUrlWithoutBase)
            if !destination.is_empty()
                && destination.len() <= MAX_PATH_BYTES
                && !destination.chars().any(char::is_control) =>
        {
            return named(destination.to_owned());
        }
        Err(_) => return RowTarget::Malformed,
    };
    if url.scheme() != "file" {
        return ExternalUrl::try_from(destination)
            .map_or(RowTarget::Malformed, RowTarget::External);
    }
    // A file on another host is not this machine's to open, so only an empty
    // host or `localhost` is local: Windows would otherwise read any other
    // host as a network share.
    if let Some(host) = url
        .host_str()
        .filter(|host| !host.is_empty() && *host != "localhost")
    {
        return RowTarget::OtherHost(host.to_owned());
    }
    url.to_file_path()
        .ok()
        .and_then(|path| path.into_os_string().into_string().ok())
        .map_or(RowTarget::Malformed, named)
}

/// A document link in rich text, using the terminal's existing path rules.
/// Classification performs no file access; opening still requires a click.
pub(crate) fn file_target(destination: &str) -> Option<RowTarget> {
    if destination.starts_with(['#', '?'])
        || destination.starts_with("//")
        || destination.starts_with("\\\\")
        || destination.len() > MAX_PATH_BYTES
        || destination.chars().any(char::is_control)
    {
        return None;
    }
    #[cfg(windows)]
    {
        // A drive letter is a path prefix here, not a URL scheme.
        if std::path::Path::new(destination).is_absolute() {
            let (path, line) = path::without_location(destination);
            return Some(RowTarget::Path {
                path: path.to_owned(),
                line,
                explicit: true,
            });
        }
    }
    let RowTarget::Path { path, .. } = explicit_target(destination) else {
        return None;
    };
    let (path, line) = path::without_location(&path);
    Some(RowTarget::Path {
        path: path.to_owned(),
        line,
        explicit: true,
    })
}

/// What an address read from plain text points at. Only the defined schemes
/// are links there: the web's, `file://` on this host, and the ones that
/// open without asking. A printed path is one only if its file exists.
fn plain_address(text: &str) -> Option<RowTarget> {
    if let Some(url) = web_url(text) {
        return Some(RowTarget::Web(url));
    }
    match explicit_target(text) {
        RowTarget::External(url) if url.trusted() => Some(RowTarget::External(url)),
        RowTarget::Path { path, .. } if text.starts_with("file://") => Some(RowTarget::Path {
            path,
            line: None,
            explicit: false,
        }),
        _ => None,
    }
}

/// What a row-local link points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RowTarget {
    /// A web address that passed `WebUrl` validation.
    Web(String),
    /// A local path as the pane printed it, with `~/` and relative paths left
    /// for the click to resolve, or the path an explicit hyperlink names, and
    /// the line a `:line` suffix named. An `explicit` one is a link whether
    /// or not its file exists; printed text is one only when it does.
    Path {
        path: String,
        line: Option<u32>,
        explicit: bool,
    },
    /// An explicit hyperlink to a file on another host, by that host's name.
    /// It is never opened here; a click says so.
    OtherHost(String),
    /// An explicit hyperlink whose target is no address or path this client
    /// reads. It is still marked as a hyperlink; a click says what is wrong.
    Malformed,
    /// The address of an explicit hyperlink with any other scheme, such as
    /// `mailto:`. Plain text is never read as one.
    External(ExternalUrl),
}

/// A link read from one row, and the frame columns it covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RowLink {
    pub(crate) target: RowTarget,
    pub(crate) row: u16,
    pub(crate) columns: Range<u16>,
}

/// A row-local link in a pane, which a popup never is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PaneLink {
    pub(crate) pane_id: String,
    pub(crate) link: RowLink,
}

/// The web address under a point, in a pane or a popup.
pub(crate) fn link_at(
    surface: &PaneSurfaceFrame,
    x: f32,
    y: f32,
    cell_width: f32,
    cell_height: f32,
) -> Option<String> {
    match located(surface, x, y, cell_width, cell_height)?.1.target {
        RowTarget::Web(url) => Some(url),
        RowTarget::Path { .. }
        | RowTarget::External(_)
        | RowTarget::OtherHost(_)
        | RowTarget::Malformed => None,
    }
}

/// The link of either kind under a point in a pane, in surface frame cells.
pub(crate) fn pane_link_at(
    surface: &PaneSurfaceFrame,
    x: f32,
    y: f32,
    cell_width: f32,
    cell_height: f32,
) -> Option<PaneLink> {
    let (pane_id, link) = located(surface, x, y, cell_width, cell_height)?;
    Some(PaneLink {
        pane_id: pane_id?,
        link,
    })
}

fn located(
    surface: &PaneSurfaceFrame,
    x: f32,
    y: f32,
    cell_width: f32,
    cell_height: f32,
) -> Option<(Option<String>, RowLink)> {
    // Share popup isolation, pane bounds, and invalid-geometry handling with input.
    let target = wheel_target(surface, x, y, cell_width, cell_height)?;
    let (frame, x, y, start, end, pane_id) = match target.target {
        InputTarget::Popup(_) => {
            let popup = surface.popup.as_ref()?;
            let origin = popup_origin(&surface.frame, &popup.frame, cell_width, cell_height);
            (
                &popup.frame,
                x - f32::from(origin.x),
                y - f32::from(origin.y),
                0,
                popup.frame.width,
                None,
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
                Some(id),
            )
        }
    };
    let link = frame_link(
        frame,
        (x / cell_width).floor() as u16,
        (y / cell_height).floor() as u16,
        start,
        end,
    )?;
    Some((pane_id, link))
}

/// Whether an explicit hyperlink covers the pane cell under a point, whatever
/// its destination. Herdr reads that destination for plugin link handlers,
/// even one this client would never open itself, such as another host's
/// `file://` link. A popup is never a pane.
pub(crate) fn pane_hyperlink_at(
    surface: &PaneSurfaceFrame,
    x: f32,
    y: f32,
    cell_width: f32,
    cell_height: f32,
) -> bool {
    let Some(InputTarget::Pane(id)) =
        wheel_target(surface, x, y, cell_width, cell_height).map(|hit| hit.target)
    else {
        return false;
    };
    let Some(pane) = surface.panes.iter().find(|pane| pane.pane_id == id) else {
        return false;
    };
    let rect = pane.inner_rect;
    link_cell(
        &surface.frame,
        (x / cell_width).floor() as u16,
        (y / cell_height).floor() as u16,
        rect.x,
        rect.x.saturating_add(rect.width),
    )
    .is_some_and(|(cells, selected, source)| {
        cells[selected]
            .hyperlink
            .or(cells[source].hyperlink)
            .is_some()
    })
}

/// The row's cells from `start` to `end`, the index of the one at `column`,
/// and the index of the cell that draws it, which for a wide glyph's
/// continuation is the glyph's own; `None` off the row or on concealed text.
fn link_cell(
    frame: &FrameData,
    column: u16,
    row: u16,
    start: u16,
    end: u16,
) -> Option<(&[CellData], usize, usize)> {
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
    (cells.get(source)?.modifier & HIDDEN == 0).then_some((cells, selected, source))
}

fn frame_link(frame: &FrameData, column: u16, row: u16, start: u16, end: u16) -> Option<RowLink> {
    let (cells, selected, source) = link_cell(frame, column, row, start, end)?;
    let cell = &cells[source];
    // Cell indexes fit the row, whose width is a `u16`.
    let columns = |cells: Range<usize>| start + cells.start as u16..start + cells.end as u16;
    // An explicit link is authoritative, even if its destination is disallowed.
    if let Some(index) = cells[selected].hyperlink.or(cell.hyperlink) {
        let destination = frame.hyperlinks.get(index as usize)?;
        let target = match web_url(destination) {
            Some(url) => RowTarget::Web(url),
            None => explicit_target(destination),
        };
        let on_link = |i: &usize| cells[*i].skip || cells[*i].hyperlink == Some(index);
        let first = (0..=source)
            .rev()
            .take_while(on_link)
            .last()
            .unwrap_or(source);
        let last = (selected..cells.len())
            .take_while(on_link)
            .last()
            .unwrap_or(selected);
        return Some(RowLink {
            target,
            row,
            columns: columns(first..last + 1),
        });
    }
    // Byte offsets into the row's text, so a link maps back to its cells.
    let mut text = String::new();
    let mut starts = Vec::with_capacity(cells.len());
    for cell in cells {
        starts.push(text.len());
        if cell.skip {
            continue;
        }
        let symbol = shown(cell);
        if text.len() + symbol.len() > MAX_ROW_BYTES {
            return None;
        }
        text.push_str(symbol);
    }
    let hit = starts[source];
    // Plain links are row-local: the surface doesn't distinguish soft wraps
    // from separate lines, so joining rows could silently change the
    // destination. A daemon offering `pane.link.resolve` reads wrapped URLs
    // from its own terminal state instead; see `crate::links`.
    let (range, target) = match plain_url(&text, hit) {
        Some((_, true)) => return None,
        Some((range, false)) => {
            let target = plain_address(&text[range.clone()])?;
            (range, target)
        }
        None => {
            let found = path::plain_path(&text, hit)?;
            // A path starting the row may be the tail of whatever filled the
            // row above, such as a wrapped URL.
            if found.open || (found.range.start == 0 && row_full(frame, row, end)) {
                return None;
            }
            let target = RowTarget::Path {
                path: found.path.into_owned(),
                line: found.line,
                explicit: false,
            };
            (found.range, target)
        }
    };
    let first = (0..cells.len()).find(|&i| !cells[i].skip && starts[i] >= range.start)?;
    let last = (0..cells.len())
        .rev()
        .find(|&i| !cells[i].skip && starts[i] < range.end)?;
    // A wide character's continuation cells belong to the link too.
    let end = (last + 1..cells.len())
        .find(|&i| !cells[i].skip)
        .unwrap_or(cells.len());
    Some(RowLink {
        target,
        row,
        columns: columns(first..end),
    })
}

/// Whether the row above `row` prints up to the pane's right edge, `end`,
/// where its text may wrap onto `row`.
fn row_full(frame: &FrameData, row: u16, end: u16) -> bool {
    let Some(above) = row.checked_sub(1) else {
        return false;
    };
    let index = usize::from(above) * usize::from(frame.width) + usize::from(end) - 1;
    frame
        .cells
        .get(index)
        .is_some_and(|cell| cell.skip || shown(cell) != " ")
}

/// The schemes read from plain text. Any other is plain text.
const PLAIN_SCHEMES: [&str; 5] = ["http://", "https://", "file://", "mailto:", "vscode:"];

/// Where an address with one of the plain schemes starts in `text`, in
/// order. A scheme counts only at the start of a word, except the web's,
/// which are read wherever they stand.
fn address_starts(text: &str) -> Vec<(usize, &'static str)> {
    let mut starts: Vec<_> = PLAIN_SCHEMES
        .into_iter()
        .flat_map(|scheme| {
            text.match_indices(scheme)
                .map(move |(start, _)| (start, scheme))
        })
        .filter(|(start, scheme)| {
            scheme.starts_with("http")
                || !text[..*start]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_alphanumeric)
        })
        .collect();
    starts.sort_unstable();
    starts
}

/// The byte range of the plain address in one row's `text` that covers the
/// byte at `hit`, trimmed of the prose punctuation around it, and whether the
/// address runs to the end of the row, where it may continue off-screen or on
/// the next row.
pub(super) fn plain_url(text: &str, hit: usize) -> Option<(Range<usize>, bool)> {
    for (start, scheme) in address_starts(text) {
        if start > hit {
            break;
        }
        let tail = &text[start..];
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
        // A scheme with nothing after it names nothing.
        if candidate.len() <= scheme.len() {
            continue;
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
