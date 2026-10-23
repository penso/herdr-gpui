//! Blank space between a terminal's cells and the edges of its tab group.
use super::Error;
use crate::Result;
use gpui::{Styled, px};

/// Padding wider than this stops reading as spacing and starts eating the
/// columns and rows the terminal needs, so each side is held to a band a
/// window can afford.
pub(crate) const MAX_PADDING: f32 = 64.;

/// The direction a pair of sides pads along, named by its `[terminal]` key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Axis {
    X,
    Y,
}

impl Axis {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::X => "padding_x",
            Self::Y => "padding_y",
        }
    }
}

/// One of an axis's two sides: left or top first, right or bottom last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Side {
    Start,
    End,
}

/// The padding on the two opposite sides of one axis, in logical pixels:
/// left then right, or top then bottom. Each side is finite and within
/// `0..=MAX_PADDING`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sides {
    start: f32,
    end: f32,
}

impl Sides {
    pub(crate) fn new(start: f32, end: f32) -> Option<Self> {
        let valid = |side: f32| side.is_finite() && (0.0..=MAX_PADDING).contains(&side);
        (valid(start) && valid(end)).then_some(Self { start, end })
    }

    /// These sides with one of them moved by `step`, held inside the band.
    pub(crate) fn stepped(self, side: Side, step: f32) -> Self {
        let moved = |value: f32| (value + step).clamp(0., MAX_PADDING);
        match side {
            Side::Start => Self {
                start: moved(self.start),
                ..self
            },
            Side::End => Self {
                end: moved(self.end),
                ..self
            },
        }
    }

    pub(crate) fn side(self, side: Side) -> f32 {
        match side {
            Side::Start => self.start,
            Side::End => self.end,
        }
    }

    pub(crate) fn start(self) -> f32 {
        self.start
    }

    pub(crate) fn end(self) -> f32 {
        self.end
    }

    /// One number pads both sides alike; a pair pads each side on its own.
    fn parse(value: &toml::Value, axis: Axis) -> Result<Self> {
        let number = |value: &toml::Value| match value {
            toml::Value::Float(value) => Some(*value as f32),
            toml::Value::Integer(value) => Some(*value as f32),
            _ => None,
        };
        let sides = match value {
            toml::Value::Array(pair) => match pair.as_slice() {
                [start, end] => number(start).zip(number(end)),
                _ => None,
            },
            value => number(value).map(|side| (side, side)),
        };
        sides
            .and_then(|(start, end)| Self::new(start, end))
            .ok_or_else(|| Error::InvalidTerminalPadding(axis.key()).into())
    }
}

/// Space kept clear around a terminal's cells. It comes out of the terminal's
/// own size, so the daemon is told about the columns and rows actually drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Padding {
    pub(crate) x: Sides,
    pub(crate) y: Sides,
}

impl Padding {
    /// Reads `[terminal] padding_x` and `padding_y`. The deprecated
    /// `[layout] sidebar_gap` only padded the left side, so it stands in for
    /// that side while `padding_x` is unset.
    pub(super) fn resolve(
        x: Option<&toml::Value>,
        y: Option<&toml::Value>,
        legacy_sidebar_gap: Option<f32>,
    ) -> Result<Self> {
        // The deprecated key is held to its band even when it is not used.
        let legacy = legacy_sidebar_gap
            .map(|gap| Sides::new(gap, 0.).ok_or(Error::InvalidSidebarGap))
            .transpose()?;
        let x = match x {
            Some(x) => Sides::parse(x, Axis::X)?,
            None => legacy.unwrap_or_default(),
        };
        let y = y
            .map(|y| Sides::parse(y, Axis::Y))
            .transpose()?
            .unwrap_or_default();
        Ok(Self { x, y })
    }

    pub(crate) fn axis(self, axis: Axis) -> Sides {
        match axis {
            Axis::X => self.x,
            Axis::Y => self.y,
        }
    }

    /// Pads `element`, so the bounds its children are laid out in exclude
    /// the padding.
    pub(crate) fn pad<E: Styled>(self, element: E) -> E {
        element
            .pl(px(self.x.start))
            .pr(px(self.x.end))
            .pt(px(self.y.start))
            .pb(px(self.y.end))
    }
}
