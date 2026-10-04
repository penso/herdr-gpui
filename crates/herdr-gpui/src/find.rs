//! Searching a pane's scrollback through the daemon's `pane.copy_search`.
//!
//! The daemon owns the terminal and its history, so every match comes from it;
//! this module only decides what to ask next and maps the answers onto the
//! painted grid. At most one search is in flight: a step asked for meanwhile
//! waits in a single slot and is sent, against the newest content revision,
//! once the answer arrives. Nothing here touches the window or the socket.

use crate::{
    scrollback::{match_count, push_matches, viewport_top},
    terminal_painter::Highlight,
};
use herdr_client::{
    protocol::PaneSurfacePane,
    scrollback::{
        CopySearchParams, CopySearchResult, EndpointErrorCode, SearchDirection, TextPoint,
        TextRange,
    },
};
use std::time::{Duration, Instant};

/// How often a pane whose output keeps changing is searched again. A search
/// scans the whole retained scrollback, so a busy pane must not turn the find
/// bar into a continuous daemon workload.
pub(crate) const REFRESH_INTERVAL: Duration = Duration::from_millis(500);

/// What the next search is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// The query changed: the nearest match above where the bar opened.
    Query,
    /// The match before the current one, toward older output.
    Older,
    /// The match after the current one, toward newer output.
    Newer,
    /// The pane's content changed: find the current match again, leaving the
    /// view where the user has it.
    Refresh,
}

impl Step {
    /// Whether the answer moves the view to its match.
    fn reveals(self) -> bool {
        self != Self::Refresh
    }
}

/// The last answer, kept while a newer one is on its way so a busy pane's
/// highlights do not flicker.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Results {
    query: String,
    matches: Vec<TextRange>,
    current: Option<usize>,
    current_global: Option<u64>,
    total: u64,
    content_revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InFlight {
    request: String,
    step: Step,
    query: String,
    content_revision: u64,
}

/// One pane's search.
#[derive(Debug)]
pub(crate) struct Search {
    pane_id: String,
    /// The bottom row of the viewport when the bar opened. A new query looks
    /// upward from here, as a terminal's own find does.
    origin: TextPoint,
    query: String,
    results: Option<Results>,
    in_flight: Option<InFlight>,
    queued: Option<Step>,
    /// The revision a `stale_content` answer refused. The queued step waits
    /// for a surface showing newer content instead of retrying at once.
    refused_revision: Option<u64>,
    last_sent: Option<Instant>,
    error: Option<String>,
}

impl Search {
    /// A search of `pane`, anchored at the bottom of what it shows now.
    pub(crate) fn new(pane: &PaneSurfacePane) -> Self {
        Self {
            pane_id: pane.pane_id.clone(),
            origin: TextPoint {
                row: viewport_top(pane).saturating_add(u32::from(pane.inner_rect.height)),
                col: 0,
            },
            query: String::new(),
            results: None,
            in_flight: None,
            queued: None,
            refused_revision: None,
            last_sent: None,
            error: None,
        }
    }

    pub(crate) fn pane_id(&self) -> &str {
        &self.pane_id
    }

    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub(crate) fn set_query(&mut self, query: &str) {
        if query == self.query {
            return;
        }
        query.clone_into(&mut self.query);
        self.error = None;
        self.refused_revision = None;
        if query.is_empty() {
            self.results = None;
            self.queued = None;
        } else {
            self.queued = Some(Step::Query);
        }
    }

    /// Asks for the match before or after the current one. A pending new
    /// query is answered first; its answer is the match this step would move
    /// from, so the step adds nothing.
    pub(crate) fn step(&mut self, step: Step) {
        if self.query.is_empty() || self.queued == Some(Step::Query) {
            return;
        }
        self.error = None;
        self.queued = Some(step);
    }

    /// Notices a pane whose content moved on since the shown answer, and
    /// queues a refresh no more often than [`REFRESH_INTERVAL`].
    pub(crate) fn content_changed(&mut self, pane: &PaneSurfacePane, now: Instant) {
        let Some(results) = &self.results else {
            return;
        };
        if results.content_revision == pane.content_revision
            || self.queued.is_some()
            || self.in_flight.is_some()
            || self
                .last_sent
                .is_some_and(|sent| now.saturating_duration_since(sent) < REFRESH_INTERVAL)
        {
            return;
        }
        self.queued = Some(Step::Refresh);
    }

    /// The search to send now, if one is wanted and may go: nothing is in
    /// flight and the pane's content is settled (an odd revision is a write in
    /// progress, which the daemon refuses).
    pub(crate) fn next_request(&self, pane: &PaneSurfacePane) -> Option<(Step, CopySearchParams)> {
        let step = self.queued?;
        if self.in_flight.is_some()
            || pane.pane_id != self.pane_id
            || !pane.content_revision.is_multiple_of(2)
            || self.refused_revision == Some(pane.content_revision)
            || self.query.is_empty()
        {
            return None;
        }
        let current = self
            .results
            .as_ref()
            .filter(|results| results.query == self.query)
            .and_then(|results| results.matches.get(results.current?))
            .copied();
        let (direction, cursor, previous) = match (step, current) {
            (Step::Older, Some(current)) => (SearchDirection::Backward, self.origin, Some(current)),
            (Step::Newer, Some(current)) => (SearchDirection::Forward, self.origin, Some(current)),
            // A refresh looks from just before the current match, so the same
            // match stays current if the pane still has it.
            (Step::Refresh, Some(current)) => {
                (SearchDirection::Forward, before(current.start), None)
            }
            (Step::Newer, None) => (SearchDirection::Forward, self.origin, None),
            (Step::Query | Step::Older | Step::Refresh, _) => {
                (SearchDirection::Backward, self.origin, None)
            }
        };
        Some((
            step,
            CopySearchParams {
                pane_id: self.pane_id.clone(),
                query: self.query.clone(),
                direction,
                cursor,
                content_revision: pane.content_revision,
                previous,
            },
        ))
    }

    /// Records the request `next_request` produced as sent.
    pub(crate) fn sent(
        &mut self,
        request: String,
        params: &CopySearchParams,
        step: Step,
        now: Instant,
    ) {
        self.queued = None;
        self.last_sent = Some(now);
        self.in_flight = Some(InFlight {
            request,
            step,
            query: params.query.clone(),
            content_revision: params.content_revision,
        });
    }

    /// The search could not be queued; the step is dropped and said so.
    pub(crate) fn send_failed(&mut self, error: &herdr_client::Error) {
        self.queued = None;
        self.error = Some(error.to_string());
    }

    /// Applies the answer to `request`, returning the match to scroll into
    /// view when the step that asked moves the view. Answers to anything but
    /// the request in flight, or to a query since edited, are dropped.
    pub(crate) fn answer(
        &mut self,
        request: &str,
        answer: herdr_client::Result<CopySearchResult>,
    ) -> Option<TextRange> {
        if self
            .in_flight
            .as_ref()
            .is_none_or(|sent| sent.request != request)
        {
            return None;
        }
        let sent = self.in_flight.take()?;
        let result = match answer {
            Ok(result) => result,
            Err(herdr_client::Error::Endpoint {
                code: EndpointErrorCode::StaleContent,
                ..
            }) => {
                // Ask again once the surface shows what the daemon has now.
                self.refused_revision = Some(sent.content_revision);
                self.queued = self.queued.or(Some(sent.step));
                return None;
            }
            Err(error) => {
                if sent.query == self.query {
                    self.error = Some(error.to_string());
                }
                return None;
            }
        };
        if sent.query != self.query || result.pane_id != self.pane_id {
            return None;
        }
        self.refused_revision = None;
        self.error = None;
        let current = result
            .current
            .and_then(|index| usize::try_from(index).ok())
            .filter(|index| *index < result.matches.len());
        let reveal = current
            .and_then(|index| result.matches.get(index))
            .copied()
            .filter(|_| sent.step.reveals());
        self.results = Some(Results {
            query: sent.query,
            matches: result.matches,
            current,
            current_global: result.current_global.filter(|_| current.is_some()),
            total: result.total,
            content_revision: result.content_revision,
        });
        reveal
    }

    /// The search waiting for its answer, if any.
    pub(crate) fn in_flight(&self) -> Option<&str> {
        self.in_flight.as_ref().map(|sent| sent.request.as_str())
    }

    /// The count shown beside the query: "3 of 17", or "No results". Empty
    /// until the first answer for the query arrives.
    pub(crate) fn label(&self) -> String {
        let Some(results) = self.results.as_ref().filter(|r| r.query == self.query) else {
            return String::new();
        };
        match_count(results.total, results.current_global)
    }

    /// Whether the shown answer has any match to move between.
    pub(crate) fn has_matches(&self) -> bool {
        self.results
            .as_ref()
            .is_some_and(|results| results.query == self.query && results.total > 0)
    }

    /// The tinted cells of every match `pane` shows, in the surface frame's
    /// grid. Rows are mapped through the pane's current scroll position, so a
    /// match keeps its place as the user scrolls.
    pub(crate) fn highlights(&self, pane: &PaneSurfacePane) -> Vec<Highlight> {
        let Some(results) = self
            .results
            .as_ref()
            .filter(|results| results.query == self.query && pane.pane_id == self.pane_id)
        else {
            return Vec::new();
        };
        let mut highlights = Vec::new();
        push_matches(pane, &results.matches, results.current, &mut highlights);
        highlights
    }
}

/// The cell just before `point` in reading order. The daemon only compares
/// points, so a column past the row's end is a valid stand-in for "the end of
/// the previous row".
fn before(point: TextPoint) -> TextPoint {
    match point.col.checked_sub(1) {
        Some(col) => TextPoint { col, ..point },
        None => TextPoint {
            row: point.row.saturating_sub(1),
            col: u16::MAX,
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::{scrollback::reveal_offset, terminal_painter::Tint};
    use herdr_client::protocol::{PaneSurfaceScrollMetrics, SurfaceRect};

    fn pane(content_revision: u64, offset: u64, max: u64) -> PaneSurfacePane {
        let rect = SurfaceRect {
            x: 2,
            y: 1,
            width: 20,
            height: 10,
        };
        PaneSurfacePane {
            pane_id: "p".into(),
            content_revision,
            rect,
            inner_rect: rect,
            scrollbar_rect: None,
            scroll: Some(PaneSurfaceScrollMetrics {
                offset_from_bottom: offset,
                max_offset_from_bottom: max,
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

    fn range(row: u32, start: u16, end: u16) -> TextRange {
        TextRange {
            start: point(row, start),
            end: point(row, end),
        }
    }

    fn result(
        revision: u64,
        matches: Vec<TextRange>,
        current: u32,
        global: u64,
    ) -> CopySearchResult {
        CopySearchResult {
            pane_id: "p".into(),
            content_revision: revision,
            total: matches.len() as u64,
            matches,
            current: Some(current),
            current_global: Some(global),
        }
    }

    fn send(
        search: &mut Search,
        pane: &PaneSurfacePane,
        id: &str,
        now: Instant,
    ) -> CopySearchParams {
        let (step, params) = search.next_request(pane).expect("a request is due");
        search.sent(id.into(), &params, step, now);
        params
    }

    #[test]
    fn a_query_searches_upward_from_where_the_bar_opened() {
        // 100 history rows, scrolled up 30: rows 70..80 show.
        let shown = pane(4, 30, 100);
        let mut search = Search::new(&shown);
        assert!(
            search.next_request(&shown).is_none(),
            "an empty query asks nothing"
        );
        search.set_query("needle");
        let params = send(&mut search, &shown, "1", Instant::now());
        assert_eq!(params.direction, SearchDirection::Backward);
        assert_eq!(params.cursor, point(80, 0));
        assert_eq!(params.previous, None);
        assert_eq!(params.content_revision, 4);
        assert_eq!(params.pane_id, "p");
    }

    #[test]
    fn one_search_in_flight_and_steps_coalesce_to_the_latest() {
        let shown = pane(2, 0, 50);
        let mut search = Search::new(&shown);
        let now = Instant::now();
        search.set_query("a");
        send(&mut search, &shown, "1", now);
        search.set_query("ab");
        assert!(
            search.next_request(&shown).is_none(),
            "the first is still in flight"
        );
        assert_eq!(search.in_flight(), Some("1"));
        // The edited query's answer arrives for the old one and is dropped.
        let stale = result(2, vec![range(3, 0, 0)], 0, 0);
        assert_eq!(search.answer("1", Ok(stale)), None);
        assert_eq!(search.label(), "");
        let params = send(&mut search, &shown, "2", now);
        assert_eq!(params.query, "ab");
        // An answer to some other request is not this search's.
        assert_eq!(search.answer("9", Ok(result(2, vec![], 0, 0))), None);
        assert_eq!(search.in_flight(), Some("2"));
    }

    #[test]
    fn older_and_newer_move_from_the_current_match_and_reveal_it() {
        let shown = pane(2, 0, 50);
        let mut search = Search::new(&shown);
        let now = Instant::now();
        search.set_query("x");
        // A step before any answer is absorbed by the pending query.
        search.step(Step::Older);
        send(&mut search, &shown, "1", now);
        let matches = vec![range(5, 1, 2), range(20, 0, 1), range(55, 3, 4)];
        let reveal = search.answer("1", Ok(result(2, matches.clone(), 1, 1)));
        assert_eq!(reveal, Some(matches[1]));
        assert_eq!(search.label(), "2 of 3");
        assert!(search.has_matches());

        search.step(Step::Older);
        let params = send(&mut search, &shown, "2", now);
        assert_eq!(params.direction, SearchDirection::Backward);
        assert_eq!(params.previous, Some(matches[1]));
        assert_eq!(
            search.answer("2", Ok(result(2, matches.clone(), 0, 0))),
            Some(matches[0])
        );
        search.step(Step::Newer);
        let params = send(&mut search, &shown, "3", now);
        assert_eq!(params.direction, SearchDirection::Forward);
        assert_eq!(params.previous, Some(matches[0]));
    }

    #[test]
    fn stale_content_waits_for_a_newer_surface_then_retries() {
        let mut shown = pane(2, 0, 50);
        let mut search = Search::new(&shown);
        let now = Instant::now();
        search.set_query("x");
        send(&mut search, &shown, "1", now);
        let refused = herdr_client::Error::Endpoint {
            code: EndpointErrorCode::StaleContent,
            message: "pane content changed".into(),
        };
        assert_eq!(search.answer("1", Err(refused)), None);
        assert!(search.error().is_none(), "a stale answer is not a failure");
        assert!(search.next_request(&shown).is_none(), "same revision: wait");
        shown.content_revision = 3;
        assert!(
            search.next_request(&shown).is_none(),
            "odd revision: mid-write"
        );
        shown.content_revision = 4;
        let params = send(&mut search, &shown, "2", now);
        assert_eq!(params.content_revision, 4);
        assert!(
            search
                .answer("2", Ok(result(4, vec![range(1, 0, 0)], 0, 0)))
                .is_some()
        );
    }

    #[test]
    fn other_errors_are_reported_and_cleared_by_the_next_edit() {
        let shown = pane(2, 0, 50);
        let mut search = Search::new(&shown);
        search.set_query("x");
        send(&mut search, &shown, "1", Instant::now());
        let error = herdr_client::Error::Endpoint {
            code: EndpointErrorCode::PaneNotFound,
            message: "pane not found: p".into(),
        };
        assert_eq!(search.answer("1", Err(error)), None);
        assert_eq!(search.error(), Some("pane not found: p"));
        assert!(search.next_request(&shown).is_none(), "no retry loop");
        search.set_query("xy");
        assert!(search.error().is_none());
        search.send_failed(&herdr_client::Error::Full);
        assert_eq!(search.error(), Some("client command queue is full"));
        assert!(search.next_request(&shown).is_none());
    }

    #[test]
    fn changed_content_refreshes_in_place_at_a_bounded_rate() {
        let mut shown = pane(2, 0, 50);
        let mut search = Search::new(&shown);
        let start = Instant::now();
        search.set_query("x");
        send(&mut search, &shown, "1", start);
        // Rows 50..60 show; the match is on screen.
        let current = range(55, 4, 6);
        search.answer("1", Ok(result(2, vec![current], 0, 0)));

        shown.content_revision = 4;
        search.content_changed(&shown, start + REFRESH_INTERVAL / 2);
        assert!(
            search.next_request(&shown).is_none(),
            "too soon after the last search"
        );
        search.content_changed(&shown, start + REFRESH_INTERVAL);
        let params = send(&mut search, &shown, "2", start + REFRESH_INTERVAL);
        assert_eq!(params.direction, SearchDirection::Forward);
        assert_eq!(params.cursor, point(55, 3));
        assert_eq!(params.previous, None);
        // The old highlights stay up until the refresh lands, which does not
        // move the view.
        assert_eq!(search.highlights(&shown).len(), 1);
        assert_eq!(search.answer("2", Ok(result(4, vec![current], 0, 0))), None);
        search.content_changed(&shown, start + REFRESH_INTERVAL * 3);
        assert!(
            search.next_request(&shown).is_none(),
            "nothing changed since"
        );
    }

    #[test]
    fn a_refresh_from_the_start_of_a_row_looks_from_the_previous_row() {
        assert_eq!(before(point(7, 0)), point(6, u16::MAX));
        assert_eq!(before(point(7, 5)), point(7, 4));
        assert_eq!(before(point(0, 0)), point(0, u16::MAX));
    }

    #[test]
    fn highlights_map_through_the_scroll_and_clip_to_the_pane() {
        // Rows 40..50 show (max 50, offset 10); the pane sits at x 2, y 1.
        let shown = pane(2, 10, 50);
        let mut search = Search::new(&shown);
        search.set_query("x");
        send(&mut search, &shown, "1", Instant::now());
        let wrapped = TextRange {
            start: point(39, 18),
            end: point(41, 3),
        };
        let matches = vec![range(10, 0, 3), wrapped, range(45, 5, 7), range(49, 19, 30)];
        search.answer("1", Ok(result(2, matches, 2, 7)));
        let highlights = search.highlights(&shown);
        assert_eq!(
            highlights,
            vec![
                Highlight {
                    row: 1,
                    columns: 2..22,
                    tint: Tint::Match
                },
                Highlight {
                    row: 2,
                    columns: 2..6,
                    tint: Tint::Match
                },
                Highlight {
                    row: 6,
                    columns: 7..10,
                    tint: Tint::CurrentMatch
                },
                Highlight {
                    row: 10,
                    columns: 21..22,
                    tint: Tint::Match
                },
            ]
        );
        let mut other = shown.clone();
        other.pane_id = "q".into();
        assert!(search.highlights(&other).is_empty());
        search.set_query("");
        assert!(search.highlights(&shown).is_empty());
        assert_eq!(search.label(), "");
    }

    #[test]
    fn reveal_centers_a_match_out_of_view_and_leaves_one_in_view() {
        let shown = pane(2, 10, 50);
        assert_eq!(reveal_offset(&shown, range(45, 0, 1)), None);
        // Row 12 centered in 10 rows puts row 7 on top: offset 50 - 7.
        assert_eq!(reveal_offset(&shown, range(12, 0, 1)), Some(43));
        // Near the top of history the offset stops at its maximum.
        assert_eq!(reveal_offset(&shown, range(2, 0, 1)), Some(50));
        let mut alternate = shown.clone();
        alternate.scroll = None;
        assert_eq!(reveal_offset(&alternate, range(12, 0, 1)), None);
    }

    #[test]
    fn labels_follow_the_answer() {
        let shown = pane(2, 0, 50);
        let mut search = Search::new(&shown);
        search.set_query("x");
        send(&mut search, &shown, "1", Instant::now());
        search.answer(
            "1",
            Ok(CopySearchResult {
                pane_id: "p".into(),
                content_revision: 2,
                matches: vec![],
                total: 0,
                current: None,
                current_global: None,
            }),
        );
        assert_eq!(search.label(), "No results");
        assert!(!search.has_matches());
        search.step(Step::Older);
        // Moving with no current match starts over from the anchor.
        let params = search.next_request(&shown).unwrap().1;
        assert_eq!(params.previous, None);
        assert_eq!(params.direction, SearchDirection::Backward);
    }
}
