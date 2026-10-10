//! Wheel scrolling drawn where the OS's deltas put it, between the daemon's
//! whole-row surfaces. Uncovered rows are filled from earlier surfaces, and
//! rows no surface showed are never drawn. The pane rests where the gesture
//! stops, and hit testing follows (`HerdrWindow::grid_position`).

use herdr_client::protocol::{FrameData, PaneSurfaceFrame, PaneSurfaceScrollMetrics, SurfaceRect};
use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};

/// Each delta slides in over this long: the daemon's 16 ms render interval
/// plus transport.
const SPREAD: Duration = Duration::from_millis(48);
const MAX_BEHIND: usize = 4;
const MAX_RECENT: usize = 64;

#[derive(Clone)]
pub(crate) struct Slide {
    pub(crate) pane_id: String,
    pub(crate) rect: SurfaceRect,
    /// Rows the content is drawn below its grid position.
    pub(crate) offset: f32,
    /// Earlier frames, nearest first, with the rows moved since each.
    pub(crate) behind: Vec<(Arc<PaneSurfaceFrame>, i32)>,
}

impl Slide {
    pub(crate) fn paints_like(&self, other: &Self) -> bool {
        self.pane_id == other.pane_id
            && self.rect == other.rect
            && self.offset == other.offset
            && self.behind.len() == other.behind.len()
            && (self.behind.iter().zip(&other.behind))
                .all(|((a, s), (b, t))| s == t && Arc::ptr_eq(a, b))
    }
}

/// One pane's gesture, in rows from the bottom of its scrollback.
struct Motion {
    pane_id: String,
    rect: SurfaceRect,
    shown: u64,
    target: f64,
    /// The row the motion is entering, already asked of the daemon.
    requested: i64,
    /// Every request not yet fully shown, oldest first, as the rows it moves
    /// between: the daemon answers in order, anywhere along the way.
    pending: VecDeque<(i64, i64)>,
    recent: VecDeque<(Instant, f64)>,
    last: Instant,
    /// The offset last painted, which hit testing follows.
    painted: f32,
    /// Earlier surfaces with their offsets, newest first.
    behind: Vec<(Arc<PaneSurfaceFrame>, u64)>,
}

impl Motion {
    fn scrolled(&self, surface: &PaneSurfaceFrame) -> Option<PaneSurfaceScrollMetrics> {
        surface
            .panes
            .iter()
            .find(|pane| pane.pane_id == self.pane_id && pane.inner_rect == self.rect)?
            .scroll
    }

    fn drawn(&self, now: Instant) -> f64 {
        let pending: f64 = self
            .recent
            .iter()
            .map(|(at, rows)| {
                let slid = now.saturating_duration_since(*at).as_secs_f64() / SPREAD.as_secs_f64();
                rows * (1. - slid).max(0.)
            })
            .sum();
        let shown = self.shown as f64;
        let wanted = self.target - pending - shown;
        // Only rows some surface showed can be drawn.
        let reach = self
            .behind
            .iter()
            .map(|(_, offset)| *offset as f64 - shown)
            .filter(|rows| rows.signum() == wanted.signum())
            .fold(0., |reach: f64, rows| reach.max(rows.abs()));
        shown + wanted.clamp(-reach, reach)
    }

    fn offset(&self, now: Instant) -> f32 {
        (self.drawn(now) - self.shown as f64) as f32
    }
}

#[derive(Default)]
pub(crate) struct SmoothScroll {
    motion: Option<Motion>,
}

impl SmoothScroll {
    /// Returns the lines to send for `rows` (up is positive), or `None` when
    /// an application reads the wheel instead of the scrollback.
    pub(crate) fn wheel(
        &mut self,
        presented: &PaneSurfaceFrame,
        pane_id: &str,
        rows: f32,
        now: Instant,
    ) -> Option<i16> {
        let Some((pane, scroll)) = presented
            .panes
            .iter()
            .find(|pane| pane.pane_id == pane_id)
            .filter(|pane| !pane.mouse_reporting && !pane.alternate_screen_active)
            .and_then(|pane| Some((pane, pane.scroll?)))
        else {
            self.motion = None;
            return None;
        };
        let rows = f64::from(rows);
        if !rows.is_finite() {
            return Some(0);
        }
        let continued = self
            .motion
            .take()
            .filter(|motion| motion.pane_id == pane_id && motion.rect == pane.inner_rect)
            .map(|mut motion| {
                // Turning back starts from what is drawn. Same-direction
                // motion keeps its target: rows still on their way are owed.
                if motion
                    .recent
                    .back()
                    .is_some_and(|(_, last)| last * rows < 0.)
                {
                    motion.target = motion.drawn(now);
                    motion.recent.clear();
                }
                motion
            });
        let offset = scroll.offset_from_bottom;
        let motion = self.motion.insert(continued.unwrap_or_else(|| Motion {
            pane_id: pane_id.into(),
            rect: pane.inner_rect,
            shown: offset,
            target: offset as f64,
            requested: offset as i64,
            pending: VecDeque::new(),
            recent: VecDeque::new(),
            last: now,
            painted: 0.,
            behind: Vec::new(),
        }));
        let max = scroll.max_offset_from_bottom as f64;
        let rows = (motion.target + rows).clamp(0., max) - motion.target;
        if rows == 0. {
            return Some(0);
        }
        motion.target += rows;
        motion.recent.push_back((now, rows));
        while motion.recent.len() > MAX_RECENT
            || motion
                .recent
                .front()
                .is_some_and(|(at, _)| now.saturating_duration_since(*at) >= SPREAD)
        {
            motion.recent.pop_front();
        }
        motion.last = now;
        let entering = match rows > 0. {
            true => motion.target.ceil(),
            false => motion.target.floor(),
        } as i64;
        let lines = (entering - motion.requested).clamp(-128, 128);
        if lines * rows.signum() as i64 <= 0 {
            return Some(0);
        }
        motion
            .pending
            .push_back((motion.requested, motion.requested + lines));
        motion.requested += lines;
        // Merged, the oldest two requests still cover every row either asked.
        if motion.pending.len() > MAX_RECENT
            && let (Some((a, b)), Some((_, c))) =
                (motion.pending.pop_front(), motion.pending.pop_front())
        {
            let (low, high) = (a.min(b).min(c), a.max(b).max(c));
            let merged = if a - low <= high - a {
                (low, high)
            } else {
                (high, low)
            };
            motion.pending.push_front(merged);
        }
        Some(lines as i16)
    }

    pub(crate) fn observe(&mut self, previous: &Arc<PaneSurfaceFrame>, next: &PaneSurfaceFrame) {
        let Some(motion) = &mut self.motion else {
            return;
        };
        let (Some(from), Some(to), true) = (
            motion.scrolled(previous),
            motion.scrolled(next),
            previous.boot_id == next.boot_id,
        ) else {
            self.motion = None;
            return;
        };
        let grown = to.max_offset_from_bottom as i64 - from.max_offset_from_bottom as i64;
        let (from, to) = (from.offset_from_bottom, to.offset_from_bottom);
        // Output grows the history and moves a scrolled-back offset without
        // moving the picture, so the picture's expected shift, then the
        // nearest ones, are tried first: repeated rows can match several.
        let moved = to as i64 - from as i64;
        let height = i64::from(motion.rect.height);
        let nearest = (moved - grown.max(0)).max(1 - height).min(height - 1);
        let picture = (0..2 * height)
            .flat_map(|distance| [nearest - distance, nearest + distance])
            .skip(1) // `nearest` itself comes twice
            .filter(|shift| shift.abs() < height)
            .filter_map(|shift| i32::try_from(shift).ok())
            .find(|shift| rows_shifted(&previous.frame, &next.frame, motion.rect, *shift));
        // A scroll to a row never asked for, such as the keyboard's, ends the
        // motion where the daemon put it, however late the answer lands.
        let landed = picture.map(|shift| from as i64 + i64::from(shift));
        let answer = landed
            .filter(|_| picture != Some(0))
            .map(|landed| answered(&motion.pending, landed));
        if let Some(None) = answer {
            self.motion = None;
            return;
        }
        if let (Some(shift), Some(landed)) = (picture, landed) {
            let output = moved - i64::from(shift);
            // Requests served together, even ones that cancel out, land on
            // the last row asked for; none may wait to answer a later scroll.
            if landed == motion.requested {
                motion.pending.clear();
            } else if let Some(Some(answer)) = answer {
                motion.pending.drain(..answer);
                if let Some(first) = motion.pending.front_mut() {
                    first.0 = landed;
                }
            }
            motion.target += output as f64;
            motion.requested += output;
            for (start, end) in &mut motion.pending {
                *start += output;
                *end += output;
            }
            for (_, at) in &mut motion.behind {
                *at = at.saturating_add_signed(output);
            }
            if shift != 0 {
                let at = from.saturating_add_signed(output);
                motion.behind.insert(0, (previous.clone(), at));
                motion.behind.truncate(MAX_BEHIND);
            }
        } else {
            motion.behind.clear();
            motion.recent.clear();
            motion.target = to as f64;
            motion.requested = to as i64;
            motion.pending.clear();
        }
        motion.shown = to;
    }

    pub(crate) fn moving(&self, now: Instant) -> bool {
        self.motion
            .as_ref()
            .is_some_and(|motion| now < motion.last + SPREAD)
    }

    /// The pane's rect and the offset last painted, for hit testing.
    pub(crate) fn offset(&self) -> Option<(SurfaceRect, f32)> {
        let motion = self.motion.as_ref()?;
        Some((motion.rect, motion.painted))
    }

    pub(crate) fn slide(&mut self, now: Instant) -> Option<Slide> {
        let moving = self.moving(now);
        let motion = self.motion.as_mut()?;
        if motion.target == motion.shown as f64 && !moving {
            self.motion = None;
            return None;
        }
        let offset = motion.offset(now);
        motion.painted = offset;
        let mut behind: Vec<_> = motion
            .behind
            .iter()
            .filter_map(|(frame, at)| {
                let shift = i32::try_from(i128::from(motion.shown) - i128::from(*at)).ok()?;
                (shift.signum() == -offset.signum() as i32).then(|| (frame.clone(), shift))
            })
            .collect();
        behind.sort_by_key(|(_, shift)| shift.unsigned_abs());
        if let Some(needed) = behind
            .iter()
            .position(|(_, shift)| shift.unsigned_abs() as f32 >= offset.abs())
        {
            behind.truncate(needed + 1);
        }
        Some(Slide {
            pane_id: motion.pane_id.clone(),
            rect: motion.rect,
            offset,
            behind,
        })
    }

    pub(crate) fn clear(&mut self) {
        self.motion = None;
    }
}

/// The first request along which the daemon `landed`, which retires every
/// one before it: the daemon answers in order.
fn answered(pending: &VecDeque<(i64, i64)>, landed: i64) -> Option<usize> {
    pending
        .iter()
        .position(|&(a, b)| a.min(b) <= landed && landed <= a.max(b))
}

/// Whether most of at least half the rows of `rect` match `shift` rows down,
/// so a few blank rows never pass for a scroll; a status line may differ.
fn rows_shifted(old: &FrameData, new: &FrameData, rect: SurfaceRect, shift: i32) -> bool {
    if old.width != new.width || old.height != new.height {
        return false;
    }
    let width = usize::from(new.width);
    let (left, right) = (
        usize::from(rect.x),
        usize::from(rect.x) + usize::from(rect.width),
    );
    let (top, bottom) = (
        i32::from(rect.y),
        i32::from(rect.y) + i32::from(rect.height),
    );
    if right > width || bottom > i32::from(new.height) {
        return false;
    }
    // Rows between `top` and `bottom`, so never negative.
    let row = |y: i32| {
        let start = y as usize * width;
        start + left..start + right
    };
    let overlap = top.max(top + shift)..bottom.min(bottom + shift);
    let rows = overlap.len();
    if rows == 0 || rows * 2 < usize::from(rect.height) {
        return false;
    }
    let same = overlap
        .filter(|y| {
            let new = new.cells.get(row(*y));
            new.is_some() && new == old.cells.get(row(y - shift))
        })
        .count();
    same * 4 >= rows * 3
}

#[cfg(test)]
mod tests;
