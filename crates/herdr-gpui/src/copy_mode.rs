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

use crate::{
    scrollback::{match_count, push_matches, push_range, viewport_top},
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
    /// Clears the selection and the search, or leaves copy mode when there
    /// is neither.
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

/// A request for the daemon, built from the cursor as it is now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Request {
    Motion(CopyMotionParams),
    /// A repeated search (`n`, `N`) keeps the direction the query was
    /// entered with, so `N` does not turn `n` around.
    Search {
        params: CopySearchParams,
        repeat: bool,
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
    /// Read this range and copy it, then leave copy mode.
    Copy(TextRange),
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
        repeat: bool,
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
                    repeat: false,
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
                    repeat: true,
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
            Command::Copy => {
                let copied = self
                    .selection(last_col)
                    .or_else(|| self.found.as_ref()?.current_match());
                return match copied {
                    Some(range) => Outcome::Copy(range),
                    None => Outcome::Exit,
                };
            }
            Command::Cancel if self.mark.is_some() || self.found.is_some() => {
                self.mark = None;
                self.found = None;
            }
            Command::Cancel | Command::Exit => return Outcome::Exit,
        }
        Outcome::Moved
    }

    /// The request for `operation` from the cursor. A repeated search moves
    /// past the current match when the cursor is still on it.
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
                repeat,
            } => {
                let previous = self
                    .found
                    .as_ref()
                    .filter(|_| *repeat)
                    .and_then(Found::current_match)
                    .filter(|current| current.start == self.cursor);
                Request::Search {
                    params: CopySearchParams {
                        pane_id: self.pane_id.clone(),
                        query: query.clone(),
                        direction: *direction,
                        cursor: self.cursor,
                        content_revision,
                        previous,
                    },
                    repeat: *repeat,
                }
            }
        }
    }

    pub(crate) fn sent(&mut self, request: String, sent: &Request) {
        self.retry = None;
        let (origin, operation, content_revision) = match sent {
            Request::Motion(params) => (
                params.cursor,
                Operation::Motion(params.motion),
                params.content_revision.unwrap_or_default(),
            ),
            Request::Search { params, repeat } => (
                params.cursor,
                Operation::Search {
                    query: params.query.clone(),
                    direction: params.direction,
                    repeat: *repeat,
                },
                params.content_revision,
            ),
        };
        self.in_flight = Some(InFlight {
            request,
            origin,
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
                    repeat,
                },
            ) => {
                if !current || result.pane_id != self.pane_id {
                    return Ok(false);
                }
                Ok(self.found(query, direction, repeat, result))
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
                self.retry = Some((operation, sent.content_revision));
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
        repeat: bool,
        result: CopySearchResult,
    ) -> bool {
        let current = result
            .current
            .and_then(|index| usize::try_from(index).ok())
            .filter(|index| *index < result.matches.len());
        let direction = match &self.found {
            Some(found) if repeat => found.direction,
            _ => direction,
        };
        let found = Found {
            query,
            direction,
            matches: result.matches,
            current,
            current_global: result.current_global.filter(|_| current.is_some()),
            total: result.total,
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
mod tests {
    use super::*;
    use herdr_client::protocol::{PaneSurfaceScrollMetrics, SurfaceRect};
    use herdr_client::scrollback::CopyMotionResult;

    /// A 20x10 pane with 50 rows of history, scrolled `offset` up.
    fn pane(offset: u64) -> PaneSurfacePane {
        let rect = SurfaceRect {
            x: 2,
            y: 1,
            width: 20,
            height: 10,
        };
        PaneSurfacePane {
            pane_id: "p".into(),
            content_revision: 4,
            rect,
            inner_rect: rect,
            scrollbar_rect: None,
            scroll: Some(PaneSurfaceScrollMetrics {
                offset_from_bottom: offset,
                max_offset_from_bottom: 50,
                viewport_rows: 10,
            }),
            focused: true,
            mouse_reporting: false,
            sgr_pixel_mouse: false,
            alternate_screen_active: false,
            pixel_width: 200,
            pixel_height: 100,
        }
    }

    fn point(row: u32, col: u16) -> TextPoint {
        TextPoint { row, col }
    }

    #[test]
    fn keys_parse_as_herdr_copy_mode_defines_them() {
        use CopyMotion::*;
        let key = |key, shift| Command::from_key(key, shift, false);
        assert_eq!(key("w", false), Some(Command::Motion(NextWordStart)));
        assert_eq!(key("w", true), Some(Command::Motion(NextBigWordStart)));
        assert_eq!(key("b", true), Some(Command::Motion(PreviousBigWordStart)));
        assert_eq!(key("e", true), Some(Command::Motion(NextBigWordEnd)));
        assert_eq!(key("4", true), Some(Command::Motion(LineEnd)));
        assert_eq!(key("6", true), Some(Command::Motion(FirstNonBlank)));
        assert_eq!(key("[", true), Some(Command::Motion(PreviousParagraph)));
        assert_eq!(key("g", true), Some(Command::History { top: false }));
        assert_eq!(key("v", true), Some(Command::Mark { lines: true }));
        assert_eq!(key("x", false), None);
        assert_eq!(
            Command::from_key("u", false, true),
            Some(Command::Page {
                down: false,
                half: true
            })
        );
        assert_eq!(Command::from_key("w", false, true), None);
        // Characters an input method commits read the same way.
        assert_eq!(
            Command::from_char('W'),
            Some(Command::Motion(NextBigWordStart))
        );
        assert_eq!(Command::from_char('$'), Some(Command::Motion(LineEnd)));
        assert_eq!(
            Command::from_char('}'),
            Some(Command::Motion(NextParagraph))
        );
        assert_eq!(
            Command::from_char(' '),
            Some(Command::Mark { lines: false })
        );
        assert_eq!(Command::from_char('界'), None);
    }

    #[test]
    fn starts_at_the_terminal_cursor_or_the_last_row() {
        let shown = pane(5);
        // Rows 45..55 show; the cursor at grid (7, 4) is row 48, column 5.
        let mode = CopyMode::new(&shown, Some((7, 4)));
        assert_eq!(mode.cursor, point(48, 5));
        assert_eq!(mode.entry_offset(), Some(5));
        let mode = CopyMode::new(&shown, Some((0, 0)));
        assert_eq!(mode.cursor, point(54, 0), "a cursor outside the pane");
        assert_eq!(CopyMode::new(&shown, None).cursor, point(54, 0));
    }

    #[test]
    fn local_steps_clamp_to_the_pane_and_its_history() {
        let shown = pane(0);
        let mut mode = CopyMode::new(&shown, None);
        assert_eq!(mode.cursor, point(59, 0));
        let mut run = |command| mode.command(command, &shown);
        assert_eq!(run(Command::Step { rows: 1, cols: -1 }), Outcome::Moved);
        run(Command::Step { rows: 0, cols: 40 });
        run(Command::Page {
            down: false,
            half: true,
        });
        assert_eq!(mode.cursor, point(54, 19));
        mode.command(
            Command::Page {
                down: false,
                half: false,
            },
            &shown,
        );
        assert_eq!(mode.cursor, point(46, 19));
        mode.command(Command::History { top: true }, &shown);
        assert_eq!(mode.cursor, point(0, 0));
        mode.command(Command::Step { rows: -3, cols: -3 }, &shown);
        assert_eq!(mode.cursor, point(0, 0));
        mode.command(Command::History { top: false }, &shown);
        assert_eq!(mode.cursor, point(59, 0));
    }

    #[test]
    fn motions_go_to_the_daemon_one_at_a_time_and_keys_wait_in_order() {
        let shown = pane(0);
        let mut mode = CopyMode::new(&shown, None);
        let Outcome::Send(Request::Motion(params)) =
            mode.command(Command::Motion(CopyMotion::PreviousWordStart), &shown)
        else {
            panic!("a word motion asks the daemon");
        };
        assert_eq!(params.cursor, point(59, 0));
        assert_eq!(params.content_revision, Some(4));
        mode.sent("1".into(), &Request::Motion(params));
        assert_eq!(
            mode.command(Command::Step { rows: 0, cols: 1 }, &shown),
            Outcome::Nothing
        );
        assert_eq!(mode.next_queued(), None, "still in flight");
        let landed = CopyMotionResult {
            pane_id: "p".into(),
            cursor: point(58, 12),
            content_revision: 4,
        };
        let landed = ScrollbackResponse::PaneCopyMotion(landed);
        assert!(!mode.answer("other", Ok(landed.clone())).unwrap());
        assert!(mode.answer("1", Ok(landed)).unwrap());
        assert_eq!(mode.cursor, point(58, 12));
        let next = mode.next_queued().unwrap();
        assert_eq!(mode.command(next, &shown), Outcome::Moved);
        assert_eq!(mode.cursor, point(58, 13));
        // A held key stops queueing at the bound.
        let request = mode.request(&Operation::Motion(CopyMotion::NextWordEnd), 4);
        mode.sent("2".into(), &request);
        for _ in 0..100 {
            mode.command(Command::Step { rows: 1, cols: 0 }, &shown);
        }
        assert_eq!(mode.queued.len(), MAX_QUEUED);
    }

    #[test]
    fn a_stale_motion_retries_on_newer_settled_content() {
        let mut shown = pane(0);
        let mut mode = CopyMode::new(&shown, None);
        let request = mode.request(&Operation::Motion(CopyMotion::NextParagraph), 4);
        mode.sent("1".into(), &request);
        let stale = herdr_client::Error::Endpoint {
            code: EndpointErrorCode::StaleContent,
            message: "pane content changed".into(),
        };
        assert!(!mode.answer("1", Err(stale)).unwrap());
        mode.command(Command::Step { rows: -1, cols: 0 }, &shown);
        assert!(mode.due_retry(&shown).is_none(), "same revision");
        shown.content_revision = 5;
        assert!(mode.due_retry(&shown).is_none(), "mid-write");
        shown.content_revision = 6;
        let retry = mode.due_retry(&shown).unwrap();
        let Request::Motion(params) = &retry else {
            panic!("the motion is retried");
        };
        assert_eq!(params.motion, CopyMotion::NextParagraph);
        assert_eq!(params.content_revision, Some(6));
        mode.sent("2".into(), &retry);
        // Any other failure drops what waited behind it.
        let failure = herdr_client::Error::Endpoint {
            code: EndpointErrorCode::Other("copy_motion_unavailable".into()),
            message: "terminal row is unavailable".into(),
        };
        assert!(mode.answer("2", Err(failure)).is_err());
        assert_eq!(mode.next_queued(), None);
    }

    #[test]
    fn marks_select_by_cell_or_line_and_copy_or_cancel() {
        let shown = pane(0);
        let mut mode = CopyMode::new(&shown, Some((5, 9)));
        assert_eq!(
            mode.command(Command::Copy, &shown),
            Outcome::Exit,
            "nothing marked"
        );
        mode.command(Command::Mark { lines: false }, &shown);
        mode.command(Command::Step { rows: -2, cols: -1 }, &shown);
        assert_eq!(
            mode.command(Command::Copy, &shown),
            Outcome::Copy(TextRange {
                start: point(56, 2),
                end: point(58, 3),
            })
        );
        mode.command(Command::Mark { lines: true }, &shown);
        mode.command(Command::Step { rows: 3, cols: 0 }, &shown);
        assert_eq!(
            mode.command(Command::Copy, &shown),
            Outcome::Copy(TextRange {
                start: point(56, 0),
                end: point(59, 19),
            })
        );
        // Escape clears a selection first, then leaves.
        assert_eq!(mode.command(Command::Cancel, &shown), Outcome::Moved);
        assert_eq!(mode.command(Command::Cancel, &shown), Outcome::Exit);
        assert_eq!(mode.command(Command::Exit, &shown), Outcome::Exit);
    }

    #[test]
    fn the_cursor_is_kept_on_screen_with_the_least_scrolling() {
        let mut shown = pane(0);
        let mut mode = CopyMode::new(&shown, None);
        assert_eq!(mode.reveal(&shown), None);
        mode.command(Command::Step { rows: -12, cols: 0 }, &shown);
        // Row 47 is above rows 50..60: it becomes the top row.
        assert_eq!(mode.reveal(&shown), Some(3));
        assert_eq!(mode.reveal(&shown), None, "asked once");
        shown.scroll.as_mut().unwrap().offset_from_bottom = 3;
        assert_eq!(mode.reveal(&shown), None);
        mode.command(Command::History { top: false }, &shown);
        // Row 59 below rows 47..57: it becomes the bottom row.
        assert_eq!(mode.reveal(&shown), Some(0));
    }

    #[test]
    fn highlights_show_the_selection_and_the_cursor() {
        let shown = pane(0);
        let mut mode = CopyMode::new(&shown, Some((5, 9)));
        assert_eq!(
            mode.highlights(&shown),
            vec![Highlight {
                row: 9,
                columns: 5..6,
                tint: Tint::CopyCursor,
            }]
        );
        mode.command(Command::Mark { lines: true }, &shown);
        let highlights = mode.highlights(&shown);
        assert_eq!(highlights[0].columns, 2..22);
        assert_eq!(highlights[0].tint, Tint::Selection);
        let mut other = shown.clone();
        other.pane_id = "q".into();
        assert!(mode.highlights(&other).is_empty());
    }

    fn matches(rows: &[(u32, u16, u16)], current: Option<u32>, global: u64) -> CopySearchResult {
        CopySearchResult {
            pane_id: "p".into(),
            content_revision: 4,
            matches: rows
                .iter()
                .map(|&(row, start, end)| TextRange {
                    start: point(row, start),
                    end: point(row, end),
                })
                .collect(),
            total: 9,
            current,
            current_global: current.map(|_| global),
        }
    }

    /// Runs `command`, expecting a search, and records it as sent.
    fn search(
        mode: &mut CopyMode,
        command: Command,
        shown: &PaneSurfacePane,
        id: &str,
    ) -> (CopySearchParams, bool) {
        let Outcome::Send(request) = mode.command(command, shown) else {
            panic!("a search asks the daemon");
        };
        mode.sent(id.into(), &request);
        let Request::Search { params, repeat } = request else {
            panic!("not a search: {request:?}");
        };
        (params, repeat)
    }

    fn found(mode: &mut CopyMode, id: &str, result: CopySearchResult) -> bool {
        mode.answer(id, Ok(ScrollbackResponse::PaneCopySearch(result)))
            .unwrap()
    }

    #[test]
    fn search_keys_parse_as_herdr_copy_mode_defines_them() {
        use SearchDirection::*;
        let key = |key, shift| Command::from_key(key, shift, false);
        assert_eq!(key("/", false), Some(Command::Prompt(Forward)));
        // Shift-/ arrives as `/` with shift on some layouts and `?` on others.
        assert_eq!(key("/", true), Some(Command::Prompt(Backward)));
        assert_eq!(key("?", true), Some(Command::Prompt(Backward)));
        assert_eq!(key("n", false), Some(Command::Repeat { reverse: false }));
        assert_eq!(key("n", true), Some(Command::Repeat { reverse: true }));
        assert_eq!(Command::from_key("n", false, true), None);
        assert_eq!(Command::from_char('/'), Some(Command::Prompt(Forward)));
        assert_eq!(Command::from_char('?'), Some(Command::Prompt(Backward)));
        assert_eq!(
            Command::from_char('N'),
            Some(Command::Repeat { reverse: true })
        );
    }

    #[test]
    fn the_prompt_opens_at_once_and_its_search_waits_its_turn() {
        let shown = pane(0);
        let mut mode = CopyMode::new(&shown, None);
        let request = mode.request(&Operation::Motion(CopyMotion::NextWordEnd), 4);
        mode.sent("1".into(), &request);
        // The prompt opens while the motion is out, so typing goes into it.
        assert_eq!(
            mode.command(Command::Prompt(SearchDirection::Forward), &shown),
            Outcome::Prompt(SearchDirection::Forward)
        );
        assert!(mode.queued.is_empty());
        // The submitted search runs after the motion lands, from its cursor.
        let submitted = Command::Search {
            query: "Needle".into(),
            direction: SearchDirection::Forward,
        };
        assert_eq!(mode.command(submitted.clone(), &shown), Outcome::Nothing);
        let landed = CopyMotionResult {
            pane_id: "p".into(),
            cursor: point(59, 7),
            content_revision: 4,
        };
        mode.answer("1", Ok(ScrollbackResponse::PaneCopyMotion(landed)))
            .unwrap();
        assert_eq!(mode.next_queued(), Some(submitted.clone()));
        let (params, repeat) = search(&mut mode, submitted, &shown, "2");
        assert!(!repeat);
        assert_eq!(
            params,
            CopySearchParams {
                pane_id: "p".into(),
                // Sent as typed: the daemon ignores case unless the query
                // has an uppercase letter, as Herdr's copy mode does.
                query: "Needle".into(),
                direction: SearchDirection::Forward,
                cursor: point(59, 7),
                content_revision: 4,
                previous: None,
            }
        );
        // An empty query searches for nothing.
        mode.in_flight = None;
        let empty = Command::Search {
            query: String::new(),
            direction: SearchDirection::Backward,
        };
        assert_eq!(mode.command(empty, &shown), Outcome::Nothing);
        assert_eq!(
            mode.command(Command::Repeat { reverse: false }, &shown),
            Outcome::Nothing,
            "n before any search"
        );
    }

    #[test]
    fn a_match_moves_the_cursor_and_n_repeats_the_way_it_was_entered() {
        let shown = pane(0);
        let mut mode = CopyMode::new(&shown, None);
        let backward = Command::Search {
            query: "err".into(),
            direction: SearchDirection::Backward,
        };
        search(&mut mode, backward, &shown, "1");
        assert!(found(
            &mut mode,
            "1",
            matches(&[(40, 2, 4), (52, 6, 8)], Some(1), 4)
        ));
        assert_eq!(mode.cursor, point(52, 6));
        assert_eq!(mode.search_label().as_deref(), Some("?err 5 of 9"));
        // Matches on screen (rows 50..60) are tinted, the current one more.
        let highlights = mode.highlights(&shown);
        assert_eq!(highlights[0].tint, Tint::CurrentMatch);
        assert_eq!(highlights[0].row, 3);
        assert_eq!(highlights[0].columns, 8..11);
        assert_eq!(highlights.last().unwrap().tint, Tint::CopyCursor);

        // `n` goes on backward, past the match the cursor sits on.
        let (params, repeat) = search(&mut mode, Command::Repeat { reverse: false }, &shown, "2");
        assert!(repeat);
        assert_eq!(params.direction, SearchDirection::Backward);
        assert_eq!(params.query, "err");
        assert_eq!(
            params.previous,
            Some(TextRange {
                start: point(52, 6),
                end: point(52, 8),
            })
        );
        assert!(found(
            &mut mode,
            "2",
            matches(&[(40, 2, 4), (52, 6, 8)], Some(0), 3)
        ));
        assert_eq!(mode.cursor, point(40, 2));

        // `N` goes the other way but does not turn `n` around.
        let (params, _) = search(&mut mode, Command::Repeat { reverse: true }, &shown, "3");
        assert_eq!(params.direction, SearchDirection::Forward);
        assert!(found(
            &mut mode,
            "3",
            matches(&[(40, 2, 4), (52, 6, 8)], Some(1), 4)
        ));
        assert_eq!(mode.search_label().as_deref(), Some("?err 5 of 9"));
        // Off the match, a repeat searches from the cursor itself.
        mode.command(Command::Step { rows: 0, cols: 1 }, &shown);
        let (params, _) = search(&mut mode, Command::Repeat { reverse: false }, &shown, "4");
        assert_eq!(params.direction, SearchDirection::Backward);
        assert_eq!(params.previous, None);
        assert_eq!(params.cursor, point(52, 7));
    }

    #[test]
    fn search_answers_for_a_moved_cursor_or_of_the_wrong_kind_are_dropped() {
        let shown = pane(0);
        let mut mode = CopyMode::new(&shown, None);
        let forward = Command::Search {
            query: "x".into(),
            direction: SearchDirection::Forward,
        };
        search(&mut mode, forward.clone(), &shown, "1");
        mode.cursor = point(3, 3);
        assert!(!found(&mut mode, "1", matches(&[(10, 0, 0)], Some(0), 0)));
        assert_eq!(mode.cursor, point(3, 3));
        assert_eq!(mode.search_label(), None);

        search(&mut mode, forward.clone(), &shown, "2");
        assert!(!found(&mut mode, "2", matches(&[], None, 0)));
        assert_eq!(mode.cursor, point(3, 3), "no match leaves the cursor");
        assert_eq!(mode.search_label().as_deref(), Some("/x 9 found"));

        search(&mut mode, forward, &shown, "3");
        let motion = CopyMotionResult {
            pane_id: "p".into(),
            cursor: point(1, 1),
            content_revision: 4,
        };
        assert!(matches!(
            mode.answer("3", Ok(ScrollbackResponse::PaneCopyMotion(motion))),
            Err(herdr_client::Error::ResponseType)
        ));
        assert_eq!(mode.cursor, point(3, 3));
    }

    #[test]
    fn a_stale_search_retries_as_the_same_search() {
        let mut shown = pane(0);
        let mut mode = CopyMode::new(&shown, None);
        let backward = Command::Search {
            query: "x".into(),
            direction: SearchDirection::Backward,
        };
        search(&mut mode, backward, &shown, "1");
        found(&mut mode, "1", matches(&[(55, 1, 1)], Some(0), 0));
        search(&mut mode, Command::Repeat { reverse: true }, &shown, "2");
        let stale = herdr_client::Error::Endpoint {
            code: EndpointErrorCode::StaleContent,
            message: "pane content changed".into(),
        };
        assert!(!mode.answer("2", Err(stale)).unwrap());
        shown.content_revision = 6;
        let Some(Request::Search { params, repeat }) = mode.due_retry(&shown) else {
            panic!("the search is retried");
        };
        assert!(repeat);
        assert_eq!(params.direction, SearchDirection::Forward);
        assert_eq!(params.content_revision, 6);
        assert_eq!(params.previous.map(|range| range.start), Some(point(55, 1)));
    }

    #[test]
    fn yank_copies_the_current_match_and_escape_clears_the_search_first() {
        let shown = pane(0);
        let mut mode = CopyMode::new(&shown, None);
        let forward = Command::Search {
            query: "x".into(),
            direction: SearchDirection::Forward,
        };
        search(&mut mode, forward, &shown, "1");
        found(&mut mode, "1", matches(&[(55, 1, 4)], Some(0), 0));
        assert_eq!(
            mode.command(Command::Copy, &shown),
            Outcome::Copy(TextRange {
                start: point(55, 1),
                end: point(55, 4),
            })
        );
        // A selection, marked from the match, wins over the match.
        mode.command(Command::Mark { lines: true }, &shown);
        assert_eq!(
            mode.command(Command::Copy, &shown),
            Outcome::Copy(TextRange {
                start: point(55, 0),
                end: point(55, 19),
            })
        );
        assert_eq!(mode.command(Command::Cancel, &shown), Outcome::Moved);
        assert_eq!(mode.search_label(), None);
        assert_eq!(mode.highlights(&shown).len(), 1, "only the cursor");
        assert_eq!(mode.command(Command::Copy, &shown), Outcome::Exit);
        assert_eq!(mode.command(Command::Cancel, &shown), Outcome::Exit);
    }
}
