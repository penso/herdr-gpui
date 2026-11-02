//! Keyboard copy mode: a cursor that walks a pane's text, scrollback
//! included, marks a selection, and copies it, as Herdr's own copy mode does.
//!
//! Cell steps, line starts, pages, and the ends of history are plain
//! arithmetic on screen-buffer coordinates and happen here. Motions that
//! depend on the text (words, line ends, paragraphs) and searches are the
//! daemon's: they go out as `pane.copy_motion` and `pane.copy_search`, one at
//! a time, and keys typed meanwhile wait in a bounded queue so a fast
//! typist's sequence still runs in order. Nothing here touches the window or
//! the socket.
// i18n: migrated

use crate::{
    scrollback::{match_count, push_matches, push_range, refresh_from, viewport_top},
    terminal_painter::{Highlight, Tint},
};
use herdr_client::{
    protocol::PaneSurfacePane,
    scrollback::{
        CopyMotion, CopyMotionParams, CopySearchParams, CopySearchResult, EndpointErrorCode,
        ScrollbackResponse, SearchDirection, TextPoint, TextRange,
    },
};
use std::collections::VecDeque;

/// Keys typed while a motion is in flight. Past this, a held key stops
/// queueing rather than replaying long after it was released.
const MAX_QUEUED: usize = 32;

/// One copy-mode key, already parsed from the keystroke.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    /// Rows and columns to step; negative moves up or left.
    Step {
        rows: i32,
        cols: i32,
    },
    LineStart,
    /// The first row of history, or the last row of the screen.
    History {
        top: bool,
    },
    /// A page, or half of one, up or down, scrolling the pane with it.
    Page {
        down: bool,
        half: bool,
    },
    Motion(CopyMotion),
    /// Opens the search prompt: `/` searches toward newer output, `?` toward
    /// older.
    Prompt(SearchDirection),
    /// Searches for a submitted query from the cursor. The daemon matches it
    /// literally, ignoring case unless the query has an uppercase letter.
    Search {
        query: String,
        direction: SearchDirection,
    },
    /// Repeats the last search: `n` the same way, `N` the opposite way.
    Repeat {
        reverse: bool,
    },
    /// Starts (or restarts) a selection at the cursor: by cell, or by line.
    Mark {
        lines: bool,
    },
    /// Copies the selection, or else the current search match, and leaves
    /// copy mode.
    Copy,
    /// Drops a request still waiting for the daemon and the keys queued
    /// behind it; otherwise clears the selection and the search, or leaves
    /// copy mode when there is neither.
    Cancel,
    Exit,
}

impl Command {
    /// The vi key for `key` with shift and control as given. Keys with any
    /// other modifier are not copy-mode keys.
    pub(crate) fn from_key(key: &str, shift: bool, control: bool) -> Option<Self> {
        use CopyMotion::*;
        let step = |rows, cols| Some(Self::Step { rows, cols });
        if control {
            return match key {
                "u" => Some(Self::Page {
                    down: false,
                    half: true,
                }),
                "d" => Some(Self::Page {
                    down: true,
                    half: true,
                }),
                "b" => Some(Self::Page {
                    down: false,
                    half: false,
                }),
                "f" => Some(Self::Page {
                    down: true,
                    half: false,
                }),
                _ => None,
            };
        }
        match (key, shift) {
            ("left", _) | ("h", false) => step(0, -1),
            ("down", _) | ("j", false) => step(1, 0),
            ("up", _) | ("k", false) => step(-1, 0),
            ("right", _) | ("l", false) => step(0, 1),
            ("pageup", _) => Some(Self::Page {
                down: false,
                half: false,
            }),
            ("pagedown", _) => Some(Self::Page {
                down: true,
                half: false,
            }),
            ("home", _) | ("0", false) => Some(Self::LineStart),
            ("end", _) | ("$", _) | ("4", true) => Some(Self::Motion(LineEnd)),
            ("^", _) | ("6", true) => Some(Self::Motion(FirstNonBlank)),
            ("w", false) => Some(Self::Motion(NextWordStart)),
            ("b", false) => Some(Self::Motion(PreviousWordStart)),
            ("e", false) => Some(Self::Motion(NextWordEnd)),
            ("w", true) => Some(Self::Motion(NextBigWordStart)),
            ("b", true) => Some(Self::Motion(PreviousBigWordStart)),
            ("e", true) => Some(Self::Motion(NextBigWordEnd)),
            ("{", _) | ("[", true) => Some(Self::Motion(PreviousParagraph)),
            ("}", _) | ("]", true) => Some(Self::Motion(NextParagraph)),
            ("/", false) => Some(Self::Prompt(SearchDirection::Forward)),
            ("?", _) | ("/", true) => Some(Self::Prompt(SearchDirection::Backward)),
            ("n", reverse) => Some(Self::Repeat { reverse }),
            ("g", false) => Some(Self::History { top: true }),
            ("g", true) => Some(Self::History { top: false }),
            ("v", false) | ("space", false) => Some(Self::Mark { lines: false }),
            ("v", true) => Some(Self::Mark { lines: true }),
            ("y", false) | ("enter", _) => Some(Self::Copy),
            ("escape", _) => Some(Self::Cancel),
            ("q", false) => Some(Self::Exit),
            _ => None,
        }
    }

    /// The command a committed character stands for, for text that arrives
    /// through an input method rather than as a keystroke.
    pub(crate) fn from_char(c: char) -> Option<Self> {
        let lower = c.to_ascii_lowercase().to_string();
        let shift = c.is_ascii_uppercase();
        match c {
            ' ' => Self::from_key("space", false, false),
            '$' | '^' | '{' | '}' | '0' | '/' | '?' => Self::from_key(&c.to_string(), false, false),
            _ => Self::from_key(&lower, shift, false),
        }
    }
}

/// Where a selection was started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    Cell(TextPoint),
    Line(u32),
}

/// Why a search is sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SearchKind {
    /// A query submitted from the prompt.
    New,
    /// `n` or `N`, which keep the direction the query was entered with, so
    /// `N` does not turn `n` around.
    Repeat,
    /// The same search again, looking so the current match stays current,
    /// because the pane changed since it was found and `y` is about to copy
    /// it. A refresh refused as stale is not retried: the copy goes ahead at
    /// the old revision, which the daemon refuses with a reason the user sees.
    Refresh,
}

/// A request for the daemon, built from the cursor as it is now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Request {
    Motion(CopyMotionParams),
    Search {
        params: CopySearchParams,
        kind: SearchKind,
    },
}

/// What the window has to do after a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// Nothing visible changed, or the command waits behind a request.
    Nothing,
    /// The cursor, the selection, or the search moved; repaint and keep the
    /// cursor shown.
    Moved,
    /// Show the search prompt for this direction.
    Prompt(SearchDirection),
    /// Ask the daemon.
    Send(Request),
    /// Escape dropped the request in flight, if any, and the keys behind it;
    /// its answer is no longer wanted.
    Abandoned(Option<String>),
    /// Read this range and copy it, then leave copy mode. A search match is
    /// read at the revision it was found in, so the daemon refuses it rather
    /// than copy other text if the pane has changed since.
    Copy {
        range: TextRange,
        content_revision: Option<u64>,
    },
    /// Leave copy mode without copying.
    Exit,
}

/// What a request in flight, or refused as stale, was for.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Operation {
    Motion(CopyMotion),
    Search {
        query: String,
        direction: SearchDirection,
        kind: SearchKind,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InFlight {
    request: String,
    origin: TextPoint,
    operation: Operation,
    content_revision: u64,
}

/// The last search the daemon answered, kept for `n`, `N`, its highlights,
/// and its count.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Found {
    query: String,
    direction: SearchDirection,
    matches: Vec<TextRange>,
    current: Option<usize>,
    current_global: Option<u64>,
    total: u64,
    /// The pane's revision the matches were found in.
    content_revision: u64,
    /// Whether this answer refreshed an older one for `y`, which then copies
    /// it rather than refreshing again behind output that keeps changing.
    refreshed: bool,
}

impl Found {
    fn current_match(&self) -> Option<TextRange> {
        self.matches.get(self.current?).copied()
    }
}

#[derive(Debug)]
pub(crate) struct CopyMode {
    pane_id: String,
    cursor: TextPoint,
    mark: Option<Mark>,
    found: Option<Found>,
    /// The pane's offset when copy mode began, restored when it ends.
    entry_offset: Option<u64>,
    /// The offset last asked for, so a held key asks once per change.
    asked_offset: Option<u64>,
    in_flight: Option<InFlight>,
    queued: VecDeque<Command>,
    /// A request the daemon refused as stale, sent again once the surface
    /// shows a newer revision than this.
    retry: Option<(Operation, u64)>,
}

impl CopyMode {
    /// Starts at the terminal's cursor when the pane shows it, otherwise at
    /// the start of the pane's last visible row.
    pub(crate) fn new(pane: &PaneSurfacePane, cursor: Option<(u16, u16)>) -> Self {
        let inner = pane.inner_rect;
        let top = viewport_top(pane);
        let cursor = cursor
            .filter(|(x, y)| {
                (inner.x..inner.x.saturating_add(inner.width)).contains(x)
                    && (inner.y..inner.y.saturating_add(inner.height)).contains(y)
            })
            .map(|(x, y)| TextPoint {
                row: top.saturating_add(u32::from(y - inner.y)),
                col: x - inner.x,
            })
            .unwrap_or(TextPoint {
                row: top.saturating_add(u32::from(inner.height.saturating_sub(1))),
                col: 0,
            });
        Self {
            pane_id: pane.pane_id.clone(),
            cursor,
            mark: None,
            found: None,
            entry_offset: pane.scroll.map(|scroll| scroll.offset_from_bottom),
            asked_offset: None,
            in_flight: None,
            queued: VecDeque::new(),
            retry: None,
        }
    }

    pub(crate) fn pane_id(&self) -> &str {
        &self.pane_id
    }

    pub(crate) fn entry_offset(&self) -> Option<u64> {
        self.entry_offset
    }

    pub(crate) fn in_flight(&self) -> Option<&str> {
        self.in_flight.as_ref().map(|sent| sent.request.as_str())
    }

    /// Runs `command` against `pane` as the surface shows it now. Opening the
    /// prompt never waits, so what is typed next goes into it; the search it
    /// submits then queues behind a request in flight like any other key.
    pub(crate) fn command(&mut self, command: Command, pane: &PaneSurfacePane) -> Outcome {
        if let Command::Prompt(direction) = command {
            return Outcome::Prompt(direction);
        }
        if self.in_flight.is_some() || self.retry.is_some() {
            // Escape and `q` never wait: a daemon that keeps answering
            // "stale" must not hold the user in copy mode.
            match command {
                Command::Cancel => {
                    self.retry = None;
                    self.queued.clear();
                    let request = self.in_flight.take().map(|sent| sent.request);
                    return Outcome::Abandoned(request);
                }
                Command::Exit => return Outcome::Exit,
                _ => {}
            }
            if self.queued.len() < MAX_QUEUED {
                self.queued.push_back(command);
            }
            return Outcome::Nothing;
        }
        let inner = pane.inner_rect;
        let last_col = inner.width.saturating_sub(1);
        let last_row = last_row(pane);
        match command {
            Command::Step { rows, cols } => {
                self.cursor.row = offset(self.cursor.row, rows).min(last_row);
                self.cursor.col = u16::try_from(offset(u32::from(self.cursor.col), cols))
                    .unwrap_or(u16::MAX)
                    .min(last_col);
            }
            Command::LineStart => self.cursor.col = 0,
            Command::History { top } => {
                self.cursor.row = if top { 0 } else { last_row };
                self.cursor.col = 0;
            }
            Command::Page { down, half } => {
                let height = inner.height;
                let lines = if height <= 2 {
                    1
                } else if half {
                    height / 2
                } else {
                    height - 2
                };
                let lines = i32::from(lines);
                self.cursor.row =
                    offset(self.cursor.row, if down { lines } else { -lines }).min(last_row);
            }
            Command::Motion(motion) => {
                return Outcome::Send(
                    self.request(&Operation::Motion(motion), pane.content_revision),
                );
            }
            Command::Prompt(direction) => return Outcome::Prompt(direction),
            Command::Search { query, direction } => {
                if query.is_empty() {
                    return Outcome::Nothing;
                }
                let operation = Operation::Search {
                    query,
                    direction,
                    kind: SearchKind::New,
                };
                return Outcome::Send(self.request(&operation, pane.content_revision));
            }
            Command::Repeat { reverse } => {
                let Some(found) = &self.found else {
                    return Outcome::Nothing;
                };
                let direction = if reverse {
                    opposite(found.direction)
                } else {
                    found.direction
                };
                let operation = Operation::Search {
                    query: found.query.clone(),
                    direction,
                    kind: SearchKind::Repeat,
                };
                return Outcome::Send(self.request(&operation, pane.content_revision));
            }
            Command::Mark { lines } => {
                self.mark = Some(if lines {
                    Mark::Line(self.cursor.row)
                } else {
                    Mark::Cell(self.cursor)
                });
            }
            Command::Copy => return self.copy(last_col, pane.content_revision),
            Command::Cancel if self.mark.is_some() || self.found.is_some() => {
                self.mark = None;
                self.found = None;
            }
            Command::Cancel | Command::Exit => return Outcome::Exit,
        }
        Outcome::Moved
    }

    /// Copies the selection, read live as the user sees it, or else the
    /// current match. A match the pane may have rewritten since it was found
    /// is searched for again first, with the copy waiting behind it.
    fn copy(&mut self, last_col: u16, content_revision: u64) -> Outcome {
        if let Some(range) = self.selection(last_col) {
            return Outcome::Copy {
                range,
                content_revision: None,
            };
        }
        let Some(found) = &self.found else {
            return Outcome::Exit;
        };
        let Some(range) = found.current_match() else {
            // A refresh that lost the match keeps copy mode, showing so.
            return if found.refreshed {
                Outcome::Moved
            } else {
                Outcome::Exit
            };
        };
        if found.content_revision == content_revision || found.refreshed {
            return Outcome::Copy {
                range,
                content_revision: Some(found.content_revision),
            };
        }
        let operation = Operation::Search {
            query: found.query.clone(),
            direction: found.direction,
            kind: SearchKind::Refresh,
        };
        let request = self.request(&operation, content_revision);
        self.queued.push_front(Command::Copy);
        Outcome::Send(request)
    }

    /// The request for `operation` from the cursor. A repeated search moves
    /// past the current match when the cursor is still on it; a refresh looks
    /// so the same match stays current if the pane still has it.
    fn request(&self, operation: &Operation, content_revision: u64) -> Request {
        match operation {
            Operation::Motion(motion) => Request::Motion(CopyMotionParams {
                pane_id: self.pane_id.clone(),
                cursor: self.cursor,
                motion: *motion,
                content_revision: Some(content_revision),
            }),
            Operation::Search {
                query,
                direction,
                kind,
            } => {
                let current = self.found.as_ref().and_then(Found::current_match);
                let (direction, cursor, previous) = match (kind, current) {
                    (SearchKind::Refresh, Some(current)) => {
                        let (direction, cursor) = refresh_from(current);
                        (direction, cursor, None)
                    }
                    (SearchKind::Repeat, Some(current)) if current.start == self.cursor => {
                        (*direction, self.cursor, Some(current))
                    }
                    _ => (*direction, self.cursor, None),
                };
                Request::Search {
                    params: CopySearchParams {
                        pane_id: self.pane_id.clone(),
                        query: query.clone(),
                        direction,
                        cursor,
                        content_revision,
                        previous,
                    },
                    kind: *kind,
                }
            }
        }
    }

    /// Records `sent` as in flight. Requests are built from the cursor just
    /// before they go, so the cursor now is what their answer must still
    /// find.
    pub(crate) fn sent(&mut self, request: String, sent: &Request) {
        self.retry = None;
        let (operation, content_revision) = match sent {
            Request::Motion(params) => (
                Operation::Motion(params.motion),
                params.content_revision.unwrap_or_default(),
            ),
            Request::Search { params, kind } => (
                Operation::Search {
                    query: params.query.clone(),
                    // A refresh is a search the way the query was entered,
                    // whichever way it looks this time.
                    direction: match (kind, &self.found) {
                        (SearchKind::Refresh, Some(found)) => found.direction,
                        _ => params.direction,
                    },
                    kind: *kind,
                },
                params.content_revision,
            ),
        };
        self.in_flight = Some(InFlight {
            request,
            origin: self.cursor,
            operation,
            content_revision,
        });
    }

    /// The request could not be queued: drop it and what waited behind it.
    pub(crate) fn send_failed(&mut self) {
        self.in_flight = None;
        self.retry = None;
        self.queued.clear();
    }

    /// Applies the daemon's answer to `request`. `true` when the cursor
    /// moved and should be kept in view. A stale answer is retried once the
    /// surface moves on; an answer for a cursor that has since moved is
    /// dropped.
    pub(crate) fn answer(
        &mut self,
        request: &str,
        answer: herdr_client::Result<ScrollbackResponse>,
    ) -> herdr_client::Result<bool> {
        if self
            .in_flight
            .as_ref()
            .is_none_or(|sent| sent.request != request)
        {
            return Ok(false);
        }
        let Some(sent) = self.in_flight.take() else {
            return Ok(false);
        };
        let current = self.cursor == sent.origin;
        match (answer, sent.operation) {
            (Ok(ScrollbackResponse::PaneCopyMotion(result)), Operation::Motion(_)) => {
                if !current || result.pane_id != self.pane_id {
                    return Ok(false);
                }
                let moved = self.cursor != result.cursor;
                self.cursor = result.cursor;
                Ok(moved)
            }
            (
                Ok(ScrollbackResponse::PaneCopySearch(result)),
                Operation::Search {
                    query,
                    direction,
                    kind,
                },
            ) => {
                if !current || result.pane_id != self.pane_id {
                    return Ok(false);
                }
                Ok(self.found(query, direction, kind, result))
            }
            (Ok(_), _) => {
                self.queued.clear();
                Err(herdr_client::Error::ResponseType)
            }
            (
                Err(herdr_client::Error::Endpoint {
                    code: EndpointErrorCode::StaleContent,
                    ..
                }),
                operation,
            ) => {
                if let Operation::Search {
                    kind: SearchKind::Refresh,
                    ..
                } = operation
                {
                    if let Some(found) = &mut self.found {
                        found.refreshed = true;
                    }
                } else {
                    self.retry = Some((operation, sent.content_revision));
                }
                Ok(false)
            }
            (Err(error), _) => {
                self.queued.clear();
                Err(error)
            }
        }
    }

    /// Keeps a search's answer and moves the cursor to its current match.
    fn found(
        &mut self,
        query: String,
        direction: SearchDirection,
        kind: SearchKind,
        result: CopySearchResult,
    ) -> bool {
        let current = result
            .current
            .and_then(|index| usize::try_from(index).ok())
            .filter(|index| *index < result.matches.len());
        let direction = match &self.found {
            Some(found) if kind == SearchKind::Repeat => found.direction,
            _ => direction,
        };
        let found = Found {
            query,
            direction,
            matches: result.matches,
            current,
            current_global: result.current_global.filter(|_| current.is_some()),
            total: result.total,
            content_revision: result.content_revision,
            refreshed: kind == SearchKind::Refresh,
        };
        let target = found.current_match();
        self.found = Some(found);
        match target {
            Some(target) => {
                self.cursor = target.start;
                true
            }
            None => false,
        }
    }

    /// A refused request to send again, once `pane` shows settled content
    /// newer than the revision it was refused at.
    pub(crate) fn due_retry(&self, pane: &PaneSurfacePane) -> Option<Request> {
        let (operation, refused) = self.retry.as_ref()?;
        (self.in_flight.is_none()
            && pane.content_revision != *refused
            && pane.content_revision.is_multiple_of(2))
        .then(|| self.request(operation, pane.content_revision))
    }

    /// The next key that waited behind a request, once none is in flight.
    pub(crate) fn next_queued(&mut self) -> Option<Command> {
        if self.in_flight.is_some() || self.retry.is_some() {
            return None;
        }
        self.queued.pop_front()
    }

    /// The offset that brings the cursor on screen, scrolling as little as
    /// possible, when it is off screen and not already asked for.
    pub(crate) fn reveal(&mut self, pane: &PaneSurfacePane) -> Option<u64> {
        let scroll = pane.scroll?;
        let top = viewport_top(pane);
        let height = u32::from(pane.inner_rect.height.max(1));
        let wanted_top = if self.cursor.row < top {
            self.cursor.row
        } else if self.cursor.row >= top.saturating_add(height) {
            self.cursor.row.saturating_sub(height - 1)
        } else {
            self.asked_offset = None;
            return None;
        };
        let offset = scroll
            .max_offset_from_bottom
            .saturating_sub(u64::from(wanted_top));
        if self.asked_offset == Some(offset) || offset == scroll.offset_from_bottom {
            return None;
        }
        self.asked_offset = Some(offset);
        Some(offset)
    }

    /// The selection as a range of cells: by cell from the mark to the
    /// cursor, or whole rows by line.
    fn selection(&self, last_col: u16) -> Option<TextRange> {
        Some(match self.mark? {
            Mark::Cell(mark) => TextRange {
                start: mark.min(self.cursor),
                end: mark.max(self.cursor),
            },
            Mark::Line(row) => TextRange {
                start: TextPoint {
                    row: row.min(self.cursor.row),
                    col: 0,
                },
                end: TextPoint {
                    row: row.max(self.cursor.row),
                    col: last_col,
                },
            },
        })
    }

    /// The marked cells, when this is the pane copy mode walks and a mark
    /// is set; a cursor alone selects nothing.
    pub(crate) fn marked(&self, pane: &PaneSurfacePane) -> Option<TextRange> {
        (pane.pane_id == self.pane_id)
            .then(|| self.selection(pane.inner_rect.width.saturating_sub(1)))
            .flatten()
    }

    /// The last search and its count, as the badge shows it: "/needle 2 of
    /// 5", with `?` for a search toward older output.
    pub(crate) fn search_label(&self) -> Option<String> {
        let found = self.found.as_ref()?;
        let marker = match found.direction {
            SearchDirection::Forward => '/',
            SearchDirection::Backward => '?',
        };
        Some(format!(
            "{marker}{} {}",
            found.query,
            match_count(found.total, found.current_global)
        ))
    }

    /// Search matches, the selection, and the cursor, tinted, in the surface
    /// frame's grid; later highlights paint over earlier ones.
    pub(crate) fn highlights(&self, pane: &PaneSurfacePane) -> Vec<Highlight> {
        let mut highlights = Vec::new();
        if pane.pane_id != self.pane_id {
            return highlights;
        }
        if let Some(found) = &self.found {
            push_matches(pane, &found.matches, found.current, &mut highlights);
        }
        if let Some(range) = self.selection(pane.inner_rect.width.saturating_sub(1)) {
            push_range(pane, range, Tint::Selection, &mut highlights);
        }
        push_range(
            pane,
            TextRange {
                start: self.cursor,
                end: self.cursor,
            },
            Tint::CopyCursor,
            &mut highlights,
        );
        highlights
    }
}

fn opposite(direction: SearchDirection) -> SearchDirection {
    match direction {
        SearchDirection::Forward => SearchDirection::Backward,
        SearchDirection::Backward => SearchDirection::Forward,
    }
}

/// The last row the pane holds: its history plus its screen.
fn last_row(pane: &PaneSurfacePane) -> u32 {
    let history = pane.scroll.map_or(0, |scroll| {
        u32::try_from(scroll.max_offset_from_bottom).unwrap_or(u32::MAX)
    });
    history.saturating_add(u32::from(pane.inner_rect.height.saturating_sub(1)))
}

fn offset(value: u32, by: i32) -> u32 {
    if by < 0 {
        value.saturating_sub(by.unsigned_abs())
    } else {
        value.saturating_add(by.unsigned_abs())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
