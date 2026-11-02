//! The panes one herdr-core tab splits into. herdr-core keeps no layout, so
//! the app does: a binary tree of terminals, saved with the tab. Pure data
//! and geometry with no GPUI, so splitting, closing, resizing, and moving
//! focus are tested without a window.

mod error;

pub(crate) use error::Error;

use herdr_backend::TerminalId;
use serde::{Deserialize, Serialize};

/// The most panes one tab splits into.
pub(crate) const MAX_PANES: usize = 16;

/// A split's first side's share, in thousandths, kept where both sides stay
/// usable.
const MIN_SHARE: u16 = 100;
const MAX_SHARE: u16 = 900;
const HALF: u16 = 500;

/// How far one resize step moves a split, in thousandths.
const RESIZE_STEP: i32 = 50;

/// Rects closer than this touch, absorbing rounding in the unit square.
const EPSILON: f32 = 1e-4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Axis {
    /// Side by side, as a split to the right makes.
    Row,
    /// Stacked, as a split downward makes.
    Column,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    fn axis(self) -> Axis {
        match self {
            Self::Left | Self::Right => Axis::Row,
            Self::Up | Self::Down => Axis::Column,
        }
    }

    /// Whether it points away from the top-left corner.
    fn forward(self) -> bool {
        matches!(self, Self::Right | Self::Down)
    }
}

/// A split's first side's share of it, in thousandths. Any saved value is
/// brought back into range rather than refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "u16", into = "u16")]
pub(crate) struct Share(u16);

impl Share {
    /// The share at `fraction` of the split, as a drag reports it.
    pub(crate) fn at(fraction: f32) -> Self {
        // Clamped to 0..=1000 first, so the cast cannot truncate.
        Self::from((fraction.clamp(0., 1.) * 1000.).round() as u16)
    }

    pub(crate) fn fraction(self) -> f32 {
        f32::from(self.0) / 1000.
    }

    fn moved(self, by: i32) -> Self {
        let moved = (i32::from(self.0) + by).clamp(i32::from(MIN_SHARE), i32::from(MAX_SHARE));
        Self(u16::try_from(moved).unwrap_or(HALF))
    }
}

impl Default for Share {
    fn default() -> Self {
        Self(HALF)
    }
}

impl From<u16> for Share {
    fn from(share: u16) -> Self {
        Self(share.clamp(MIN_SHARE, MAX_SHARE))
    }
}

impl From<Share> for u16 {
    fn from(share: Share) -> Self {
        share.0
    }
}

/// One terminal, or a split of two parts. A single terminal saves as its
/// ID alone, which is also how tabs saved before splits read back.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum Node {
    Leaf(TerminalId),
    Split {
        axis: Axis,
        #[serde(default)]
        share: Share,
        first: Box<Node>,
        second: Box<Node>,
    },
}

impl Node {
    fn leaves<'a>(&'a self, out: &mut Vec<&'a TerminalId>) {
        match self {
            Self::Leaf(id) => out.push(id),
            Self::Split { first, second, .. } => {
                first.leaves(out);
                second.leaves(out);
            }
        }
    }

    fn contains(&self, id: &TerminalId) -> bool {
        match self {
            Self::Leaf(leaf) => leaf == id,
            Self::Split { first, second, .. } => first.contains(id) || second.contains(id),
        }
    }

    /// Replaces the leaf `at` with a split keeping it first and `new` second.
    fn split(&mut self, at: &TerminalId, axis: Axis, new: &TerminalId) -> bool {
        match self {
            Self::Leaf(leaf) if leaf == at => {
                let old = Self::Leaf(leaf.clone());
                *self = Self::Split {
                    axis,
                    share: Share::default(),
                    first: Box::new(old),
                    second: Box::new(Self::Leaf(new.clone())),
                };
                true
            }
            Self::Leaf(_) => false,
            Self::Split { first, second, .. } => {
                first.split(at, axis, new) || second.split(at, axis, new)
            }
        }
    }

    /// Removes the leaf `id`; its sibling takes the split's place.
    fn remove(&mut self, id: &TerminalId) -> bool {
        let Self::Split { first, second, .. } = self else {
            return false;
        };
        let kept = match (&**first, &**second) {
            (Self::Leaf(leaf), _) if leaf == id => Some((**second).clone()),
            (_, Self::Leaf(leaf)) if leaf == id => Some((**first).clone()),
            _ => None,
        };
        if let Some(kept) = kept {
            *self = kept;
            return true;
        }
        first.remove(id) || second.remove(id)
    }

    /// The split at `path`, following first sides for `false`.
    fn split_at(&mut self, path: &[bool]) -> Option<&mut Share> {
        let Self::Split {
            share,
            first,
            second,
            ..
        } = self
        else {
            return None;
        };
        match path.split_first() {
            None => Some(share),
            Some((false, rest)) => first.split_at(rest),
            Some((true, rest)) => second.split_at(rest),
        }
    }

    /// The innermost split along `axis` holding `id`, and whether `id` is in
    /// its first side.
    fn nearest(&mut self, id: &TerminalId, axis: Axis) -> Option<&mut Share> {
        let Self::Split {
            axis: own,
            share,
            first,
            second,
        } = self
        else {
            return None;
        };
        let side = if first.contains(id) {
            first
        } else if second.contains(id) {
            second
        } else {
            return None;
        };
        if side.contains_split(id, axis) {
            return side.nearest(id, axis);
        }
        (*own == axis).then_some(share)
    }

    /// Whether a split along `axis` inside this node holds `id`.
    fn contains_split(&self, id: &TerminalId, axis: Axis) -> bool {
        match self {
            Self::Leaf(_) => false,
            Self::Split {
                axis: own,
                first,
                second,
                ..
            } => {
                (*own == axis && self.contains(id))
                    || first.contains_split(id, axis)
                    || second.contains_split(id, axis)
            }
        }
    }

    fn rects(&self, rect: Rect, out: &mut Vec<(TerminalId, Rect)>) {
        match self {
            Self::Leaf(id) => out.push((id.clone(), rect)),
            Self::Split {
                axis,
                share,
                first,
                second,
            } => {
                let (a, b) = rect.cut(*axis, share.fraction());
                first.rects(a, out);
                second.rects(b, out);
            }
        }
    }
}

/// A part of the unit square a pane takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Rect {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

impl Rect {
    const UNIT: Self = Self {
        x: 0.,
        y: 0.,
        width: 1.,
        height: 1.,
    };

    fn cut(self, axis: Axis, fraction: f32) -> (Self, Self) {
        match axis {
            Axis::Row => {
                let width = self.width * fraction;
                (
                    Self { width, ..self },
                    Self {
                        x: self.x + width,
                        width: self.width - width,
                        ..self
                    },
                )
            }
            Axis::Column => {
                let height = self.height * fraction;
                (
                    Self { height, ..self },
                    Self {
                        y: self.y + height,
                        height: self.height - height,
                        ..self
                    },
                )
            }
        }
    }

    /// How far `other` lies past this rect's edge toward `direction`, when
    /// it touches that edge and they share some of it.
    fn reach(&self, other: &Self, direction: Direction) -> Option<f32> {
        let overlap =
            |a: f32, a_len: f32, b: f32, b_len: f32| (a + a_len).min(b + b_len) - a.max(b);
        let (gap, shared) = match direction {
            Direction::Right => (
                other.x - (self.x + self.width),
                overlap(self.y, self.height, other.y, other.height),
            ),
            Direction::Left => (
                self.x - (other.x + other.width),
                overlap(self.y, self.height, other.y, other.height),
            ),
            Direction::Down => (
                other.y - (self.y + self.height),
                overlap(self.x, self.width, other.x, other.width),
            ),
            Direction::Up => (
                self.y - (other.y + other.height),
                overlap(self.x, self.width, other.x, other.width),
            ),
        };
        (gap.abs() < EPSILON && shared > EPSILON).then_some(shared)
    }
}

/// A tab's panes: at least one, at most [`MAX_PANES`], each terminal once.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Node", into = "Node")]
pub(crate) struct Panes {
    root: Node,
}

impl TryFrom<Node> for Panes {
    type Error = Error;

    fn try_from(root: Node) -> Result<Self, Error> {
        let mut leaves = Vec::new();
        root.leaves(&mut leaves);
        if leaves.len() > MAX_PANES {
            return Err(Error::Full);
        }
        for (index, id) in leaves.iter().enumerate() {
            if leaves[..index].contains(id) {
                return Err(Error::Duplicate((*id).clone()));
            }
        }
        Ok(Self { root })
    }
}

impl From<Panes> for Node {
    fn from(panes: Panes) -> Self {
        panes.root
    }
}

impl Panes {
    pub(crate) fn single(id: TerminalId) -> Self {
        Self {
            root: Node::Leaf(id),
        }
    }

    pub(crate) fn root(&self) -> &Node {
        &self.root
    }

    /// Every terminal, left to right and top to bottom as the tree holds them.
    pub(crate) fn terminals(&self) -> Vec<&TerminalId> {
        let mut leaves = Vec::new();
        self.root.leaves(&mut leaves);
        leaves
    }

    pub(crate) fn len(&self) -> usize {
        self.terminals().len()
    }

    pub(crate) fn first(&self) -> &TerminalId {
        let mut node = &self.root;
        loop {
            match node {
                Node::Leaf(id) => return id,
                Node::Split { first, .. } => node = first,
            }
        }
    }

    pub(crate) fn contains(&self, id: &TerminalId) -> bool {
        self.root.contains(id)
    }

    /// Splits the pane `at`, putting `new` right of or below it.
    pub(crate) fn split(
        &mut self,
        at: &TerminalId,
        axis: Axis,
        new: TerminalId,
    ) -> Result<(), Error> {
        if self.len() >= MAX_PANES {
            return Err(Error::Full);
        }
        if self.contains(&new) {
            return Err(Error::Duplicate(new));
        }
        if !self.root.split(at, axis, &new) {
            return Err(Error::Missing(at.clone()));
        }
        Ok(())
    }

    /// Removes the pane `id`. The last pane stays: a tab always shows one.
    pub(crate) fn remove(&mut self, id: &TerminalId) -> bool {
        self.root.remove(id)
    }

    /// Sets the share of the split at `path`; true when it changed.
    pub(crate) fn set_share(&mut self, path: &[bool], share: Share) -> bool {
        match self.root.split_at(path) {
            Some(current) if *current != share => {
                *current = share;
                true
            }
            _ => false,
        }
    }

    /// Moves the split nearest `id` along `direction`'s axis one step that
    /// way; true when it moved.
    pub(crate) fn resize(&mut self, id: &TerminalId, direction: Direction) -> bool {
        let step = if direction.forward() {
            RESIZE_STEP
        } else {
            -RESIZE_STEP
        };
        let Some(share) = self.root.nearest(id, direction.axis()) else {
            return false;
        };
        let moved = share.moved(step);
        let changed = moved != *share;
        *share = moved;
        changed
    }

    /// Each pane's part of the unit square.
    pub(crate) fn rects(&self) -> Vec<(TerminalId, Rect)> {
        let mut out = Vec::new();
        self.root.rects(Rect::UNIT, &mut out);
        out
    }

    /// The pane beside `id` toward `direction`: of those touching that edge,
    /// the one sharing most of it.
    pub(crate) fn neighbor(&self, id: &TerminalId, direction: Direction) -> Option<TerminalId> {
        let rects = self.rects();
        let (_, from) = rects.iter().find(|(pane, _)| pane == id)?;
        rects
            .iter()
            .filter(|(pane, _)| pane != id)
            .filter_map(|(pane, rect)| from.reach(rect, direction).map(|shared| (pane, shared)))
            // Ties go to the first in layout order: the top or left one.
            .fold(
                None,
                |best: Option<(&TerminalId, f32)>, candidate| match best {
                    Some(best) if best.1 >= candidate.1 => Some(best),
                    _ => Some(candidate),
                },
            )
            .map(|(pane, _)| pane.clone())
    }
}
