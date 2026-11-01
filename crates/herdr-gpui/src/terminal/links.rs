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
mod wrap;
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

/// A link read from the rows of a pane or popup, and the frame columns it
/// covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RowLink {
    pub(crate) target: RowTarget,
    /// The first row the link covers, and its columns there.
    pub(crate) row: u16,
    pub(crate) columns: Range<u16>,
    /// The rows below `row` that a link wrapped across goes on to, top to
    /// bottom, with the columns it covers on each; empty for a link on one
    /// row.
    pub(crate) continued: Vec<(u16, Range<u16>)>,
}

impl RowLink {
    /// Every row the link covers, top to bottom, with its columns there.
    pub(crate) fn rows(&self) -> impl Iterator<Item = (u16, Range<u16>)> + '_ {
        std::iter::once((self.row, self.columns.clone())).chain(self.continued.iter().cloned())
    }
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
    let (frame, x, y, columns, rows, pane_id) = match target.target {
        InputTarget::Popup(_) => {
            let popup = surface.popup.as_ref()?;
            let origin = popup_origin(&surface.frame, &popup.frame, cell_width, cell_height);
            (
                &popup.frame,
                x - f32::from(origin.x),
                y - f32::from(origin.y),
                0..popup.frame.width,
                0..popup.frame.height,
                None,
            )
        }
        InputTarget::Pane(id) => {
            let pane = surface.panes.iter().find(|pane| pane.pane_id == id)?;
            let rect = pane.inner_rect;
            (
                &surface.frame,
                x,
                y,
                rect.x..rect.x.saturating_add(rect.width),
                rect.y..rect.y.saturating_add(rect.height),
                Some(id),
            )
        }
    };
    let link = frame_link(
        frame,
        (x / cell_width).floor() as u16,
        (y / cell_height).floor() as u16,
        columns,
        rows,
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

/// The link at cell (`column`, `row`) of the pane or popup that covers
/// `columns` and `rows` of `frame`, which it never leaves.
fn frame_link(
    frame: &FrameData,
    column: u16,
    row: u16,
    columns: Range<u16>,
    rows: Range<u16>,
) -> Option<RowLink> {
    if !rows.contains(&row) {
        return None;
    }
    let (cells, selected, source) = link_cell(frame, column, row, columns.start, columns.end)?;
    let cell = &cells[source];
    // Cell indexes fit the row, whose width is a `u16`.
    let left = columns.start;
    let span = |cells: Range<usize>| left + cells.start as u16..left + cells.end as u16;
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
            columns: span(first..last + 1),
            continued: Vec::new(),
        });
    }
    // Most rows end their line before the right edge and are read alone;
    // text under the point that a neighbouring row may continue is read with
    // the rows it may span, as one line.
    let area = wrap::Area::new(frame, columns, rows);
    let lines = area.around(row, source);
    let first = *lines.start();
    let run = area.read(lines)?;
    let found = Plain::read(&run.text, run.start_of(row, source)?)?;
    // Text past either end of what was read may go on with the link, off
    // screen or beyond the rows read: a destination is never guessed.
    if found.open() || (found.range().start == 0 && area.continued_above(first)) {
        return None;
    }
    let segments = run.segments(found.range());
    let target = found.target(&run.text)?;
    // The surface does not tell a wrapped row from a line that happened to
    // fill it, so text read across rows is a link only where the disk
    // confirms it: a file, which is one only once it is found. A web address
    // the terminal wrapped is the daemon's to read (`pane.link.resolve`),
    // from the terminal's own wrap flags.
    if segments.len() > 1 && !matches!(target, RowTarget::Path { .. }) {
        return None;
    }
    let mut segments = segments.into_iter();
    let (row, columns) = segments.next()?;
    Some(RowLink {
        target,
        row,
        columns,
        continued: segments.collect(),
    })
}

/// Plain text that may be a link: an address with one of the plain schemes,
/// or else a file path.
enum Plain<'a> {
    Address { range: Range<usize>, open: bool },
    Path(path::PlainPath<'a>),
}

impl<'a> Plain<'a> {
    /// What `text` may link to at the byte at `hit`. An address there is
    /// never read as a path, whatever it names.
    fn read(text: &'a str, hit: usize) -> Option<Self> {
        match plain_url(text, hit) {
            Some((range, open)) => Some(Self::Address { range, open }),
            None => path::plain_path(text, hit).map(Self::Path),
        }
    }

    fn range(&self) -> &Range<usize> {
        match self {
            Self::Address { range, .. } => range,
            Self::Path(found) => &found.range,
        }
    }

    /// Whether it runs to the end of the text, where it may continue.
    fn open(&self) -> bool {
        match self {
            Self::Address { open, .. } => *open,
            Self::Path(found) => found.open,
        }
    }

    /// What it points at, read from the `text` it was found in.
    fn target(self, text: &str) -> Option<RowTarget> {
        match self {
            Self::Address { range, .. } => plain_address(text.get(range)?),
            Self::Path(found) => Some(RowTarget::Path {
                path: found.path.into_owned(),
                line: found.line,
                explicit: false,
            }),
        }
    }
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

/// The byte range of the plain address in `text` that covers the byte at
/// `hit`, trimmed of the prose punctuation around it, and whether the address
/// runs to the end of the text, where it may continue off-screen or on the
/// next row.
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
