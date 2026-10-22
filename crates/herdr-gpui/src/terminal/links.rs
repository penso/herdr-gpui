//! Resolve user-selected links within the painted pane or popup only: web
//! addresses, the local file paths a pane prints or names, which open only
//! on the machine that runs the pane, and the other addresses with a defined
//! scheme. Printed text is read across the rows it wraps onto, by the
//! terminal or by an application's word wrap, and a link that reaches the
//! pane's edge with nothing continuing it ends there; see `wrap`. Reading
//! text decides nothing about the disk: whether a path is there is asked
//! once the link modifier is held over it, and every link opens only on a
//! link-modifier click.
use super::{HIDDEN, InputTarget, popup_origin, wheel_target};
use herdr_client::protocol::{CellData, FrameData, PaneSurfaceFrame};
use std::ops::Range;

mod external;
mod path;
mod wrap;
pub(crate) use path::{escape_into, escaped_len};
use wrap::Bounds;

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
    let named = |path: String, query: Option<String>| RowTarget::Path {
        path,
        line: query.as_deref().and_then(path::fragment_line),
        query,
        explicit: true,
        unjoined: None,
    };
    let url = match url::Url::parse(destination) {
        Ok(url) => url,
        // No scheme: a path as a program would print it, with `~/` and
        // relative paths left for the click to resolve, and a page's query
        // split off as printed text's is.
        Err(url::ParseError::RelativeUrlWithoutBase)
            if !destination.is_empty() && !destination.chars().any(char::is_control) =>
        {
            return match path::split_suffix(destination, path::drive_path(destination)) {
                Some((path, query)) if path.len() <= MAX_PATH_BYTES => {
                    named(path.to_owned(), query.map(str::to_owned))
                }
                _ => RowTarget::Malformed,
            };
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
    let query = file_suffix(&url);
    if query
        .as_ref()
        .is_some_and(|query| query.len() > path::MAX_SUFFIX_BYTES)
    {
        return RowTarget::Malformed;
    }
    url.to_file_path()
        .ok()
        .and_then(|path| path.into_os_string().into_string().ok())
        .map_or(RowTarget::Malformed, |path| named(path, query))
}

/// The `?query` and `#fragment` a `file://` address ends in, as the address
/// serializes them, already percent-encoded. An empty one names nothing.
fn file_suffix(url: &url::Url) -> Option<String> {
    let mut suffix = String::new();
    if let Some(query) = url.query().filter(|query| !query.is_empty()) {
        suffix.push('?');
        suffix.push_str(query);
    }
    if let Some(fragment) = url.fragment().filter(|fragment| !fragment.is_empty()) {
        suffix.push('#');
        suffix.push_str(fragment);
    }
    (!suffix.is_empty()).then_some(suffix)
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
        RowTarget::Path {
            path, line, query, ..
        } if text.starts_with("file://") => Some(RowTarget::Path {
            path,
            line,
            query,
            explicit: false,
            unjoined: None,
        }),
        _ => None,
    }
}

/// What a link read from the terminal points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RowTarget {
    /// A web address that passed `WebUrl` validation.
    Web(String),
    /// A local path as the pane printed it, with `~/` and relative paths left
    /// for the click to resolve, or the path an explicit hyperlink names, and
    /// the line a `:line` suffix or a `#L10` fragment named. An `explicit`
    /// one is a link whether or not its file exists; printed text is one only
    /// when it does.
    Path {
        path: String,
        line: Option<u32>,
        /// The `?query` or `#fragment` a page's address ends in, with its
        /// `?` or `#`, which a page opened in the default browser keeps.
        query: Option<String>,
        explicit: bool,
        /// The path the first row alone reads, when the terminal's own wrap
        /// joined the next row's text to it: a word that merely ended at the
        /// pane's last column reads the same way.
        unjoined: Option<String>,
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

/// A link and the frame cells it covers: one `(row, columns)` per row it
/// spans, top to bottom, with half-open frame columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RowLink {
    pub(crate) target: RowTarget,
    pub(crate) rows: Vec<(u16, Range<u16>)>,
}

impl RowLink {
    /// Whether this link covers every cell of `other`, given in the same
    /// shape, and more besides.
    pub(crate) fn strictly_contains(&self, other: &[(u16, Range<u16>)]) -> bool {
        let cells = |rows: &[(u16, Range<u16>)]| {
            rows.iter()
                .map(|(_, columns)| usize::from(columns.end.saturating_sub(columns.start)))
                .sum::<usize>()
        };
        other.iter().all(|(row, columns)| {
            self.rows.iter().any(|(own, covered)| {
                own == row && covered.start <= columns.start && columns.end <= covered.end
            })
        }) && cells(&self.rows) > cells(other)
    }
}

/// A link in a pane, which a popup never is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PaneLink {
    pub(crate) pane_id: String,
    pub(crate) link: RowLink,
}

/// How plain text is read across rows. A daemon offering
/// `pane.link.resolve` reads an address that wraps raw onto the next row
/// from its own terminal state, so a client of one leaves that address to
/// it; paths, which the daemon does not read, join either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LinkReading {
    /// Whether an address joins a raw terminal wrap.
    pub(crate) raw_url_wraps: bool,
    /// Whether the pane's host writes Windows paths, so a backslash in a
    /// relative path separates folders when a `?query` or `#fragment` is
    /// split off.
    pub(crate) windows_paths: bool,
}

impl LinkReading {
    /// The web address under a point, in a pane or a popup.
    pub(crate) fn link_at(
        self,
        surface: &PaneSurfaceFrame,
        x: f32,
        y: f32,
        cell_width: f32,
        cell_height: f32,
    ) -> Option<String> {
        match self
            .located(surface, x, y, cell_width, cell_height)?
            .1
            .target
        {
            RowTarget::Web(url) => Some(url),
            RowTarget::Path { .. }
            | RowTarget::External(_)
            | RowTarget::OtherHost(_)
            | RowTarget::Malformed => None,
        }
    }

    /// The link of either kind under a point in a pane, in surface frame cells.
    pub(crate) fn pane_link_at(
        self,
        surface: &PaneSurfaceFrame,
        x: f32,
        y: f32,
        cell_width: f32,
        cell_height: f32,
    ) -> Option<PaneLink> {
        let (pane_id, link) = self.located(surface, x, y, cell_width, cell_height)?;
        Some(PaneLink {
            pane_id: pane_id?,
            link,
        })
    }

    fn located(
        self,
        surface: &PaneSurfaceFrame,
        x: f32,
        y: f32,
        cell_width: f32,
        cell_height: f32,
    ) -> Option<(Option<String>, RowLink)> {
        // Share popup isolation, pane bounds, and invalid-geometry handling with input.
        let target = wheel_target(surface, x, y, cell_width, cell_height)?;
        let (frame, x, y, bounds, pane_id) = match target.target {
            InputTarget::Popup(_) => {
                let popup = surface.popup.as_ref()?;
                let origin = popup_origin(&surface.frame, &popup.frame, cell_width, cell_height);
                let bounds = Bounds {
                    columns: 0..popup.frame.width,
                    rows: 0..popup.frame.height,
                };
                (
                    &popup.frame,
                    x - f32::from(origin.x),
                    y - f32::from(origin.y),
                    bounds,
                    None,
                )
            }
            InputTarget::Pane(id) => {
                let pane = surface.panes.iter().find(|pane| pane.pane_id == id)?;
                let rect = pane.inner_rect;
                let bounds = Bounds {
                    columns: rect.x..rect.x.saturating_add(rect.width),
                    rows: rect.y..rect.y.saturating_add(rect.height),
                };
                (&surface.frame, x, y, bounds, Some(id))
            }
        };
        let link = self.frame_link(
            frame,
            (x / cell_width).floor() as u16,
            (y / cell_height).floor() as u16,
            &bounds,
        )?;
        Some((pane_id, link))
    }

    fn frame_link(
        self,
        frame: &FrameData,
        column: u16,
        row: u16,
        bounds: &Bounds,
    ) -> Option<RowLink> {
        if !bounds.rows.contains(&row) {
            return None;
        }
        let (start, end) = (bounds.columns.start, bounds.columns.end);
        let (cells, selected, source) = link_cell(frame, column, row, start, end)?;
        // Cell indexes fit the row, whose width is a `u16`.
        let at = |index: usize| start + index as u16;
        // An explicit link is authoritative, even if its destination is disallowed.
        if let Some(index) = cells[selected].hyperlink.or(cells[source].hyperlink) {
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
            let rows = wrap::hyperlink_rows(frame, bounds, row, at(first)..at(last + 1), |cell| {
                cell.hyperlink == Some(index)
            });
            return Some(RowLink { target, rows });
        }
        self.plain_link(frame, bounds, row, at(source))
    }

    /// The plain address or path whose text covers the cell at `column`,
    /// read across the rows it wraps onto.
    fn plain_link(
        self,
        frame: &FrameData,
        bounds: &Bounds,
        row: u16,
        column: u16,
    ) -> Option<RowLink> {
        let joined = wrap::read(frame, bounds, row, column)?;
        let text = joined.text.as_str();
        let hit = joined.byte_at(row, column)?;
        let (range, open, target) = match plain_url(text, hit) {
            Some((range, open)) => {
                // The daemon reads an address the terminal wrapped itself.
                if !self.raw_url_wraps && joined.crosses_raw_join(&range) {
                    return None;
                }
                let target = plain_address(&text[range.clone()])?;
                (range, open, target)
            }
            None => {
                let found = path::plain_path(text, hit, self.windows_paths)?;
                // Read the first row's part as the whole path is read, so its
                // suffix and escapes go the same way.
                let unjoined = joined
                    .first_raw_join(&found.range)
                    .and_then(|join| {
                        path::plain_path(&text[..join], found.range.start, self.windows_paths)
                    })
                    .map(|first| first.path.into_owned())
                    .filter(|first| found.path != first.as_str());
                let target = RowTarget::Path {
                    path: found.path.into_owned(),
                    line: found.line,
                    query: found.query.map(str::to_owned),
                    explicit: false,
                    unjoined,
                };
                (found.range, found.open, target)
            }
        };
        // Text running to the end of the pane's bottom row may continue
        // off-screen, and text cut off with an ellipsis is not all there.
        if (open && joined.ends_at_bottom) || joined.cut_off(&range) {
            return None;
        }
        Some(RowLink {
            target,
            rows: joined.segments(&range)?,
        })
    }
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

/// The symbols terminal applications draw around text as decoration, such
/// as bullets, tool-result corners, prompts, and the ellipsis that marks
/// shortened text. An address never holds one.
const DECORATIONS: [char; 7] = ['●', '⏺', '⎿', '❯', '›', '•', '…'];

/// The byte range of the plain address in `text` that covers the byte at
/// `hit`, trimmed of the prose punctuation around it, and whether the
/// address runs to the end of the text, where it may continue off-screen.
pub(super) fn plain_url(text: &str, hit: usize) -> Option<(Range<usize>, bool)> {
    for (start, scheme) in address_starts(text) {
        if start > hit {
            break;
        }
        let tail = &text[start..];
        let end = tail
            .find(|c: char| {
                c.is_whitespace()
                    || c.is_control()
                    || matches!(c, '<' | '>' | '"' | '\'' | '`')
                    || wrap::frame_char(c)
                    || DECORATIONS.contains(&c)
            })
            .unwrap_or(tail.len());
        // Check the original token: punctuation at the end may be part of a
        // destination continuing off-screen.
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
