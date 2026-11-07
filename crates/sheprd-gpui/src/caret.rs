//! Where a text field's caret, selection, and clicks fall in a line that may
//! mix directions (docs/i18n.md, Phase 4). GPUI's `x_for_index` and
//! `closest_index_for_x` walk glyphs as if bytes ran left to right, so in a
//! Hebrew or Arabic run the caret stays at the run's left edge while it
//! moves through it, and a selection covers the wrong letters. The shaper
//! already put each glyph where it is drawn; [`Carets`] takes those
//! positions as they are and resolves only which way each character reads,
//! with the Unicode bidirectional algorithm and the paragraph direction of
//! the first strong character, as CoreText and cosmic-text do.
//!
//! Arrows and deletion still move through the text in logical order, as
//! every platform's own fields do; only where that position is drawn
//! changes.

use gpui::{LineLayout, Pixels, px};
use std::ops::Range;

/// The glyphs of one shaping cluster: the bytes `bytes`, drawn between `left`
/// and `right`, and whether the run they are in reads right to left.
#[derive(Clone, Debug, PartialEq)]
struct Cluster {
    bytes: Range<usize>,
    left: f32,
    right: f32,
    rtl: bool,
}

impl Cluster {
    /// Where the caret before byte `index` of this cluster goes: from its
    /// leading edge, the left one left to right and the right one right to
    /// left, across in proportion when a ligature draws several characters.
    fn x(&self, index: usize) -> f32 {
        let length = self.bytes.len().max(1) as f32;
        let k = index.saturating_sub(self.bytes.start).min(self.bytes.len()) as f32 / length;
        let width = self.right - self.left;
        match self.rtl {
            false => self.left + width * k,
            true => self.right - width * k,
        }
    }

    /// The caret after the whole cluster.
    fn trailing(&self) -> f32 {
        match self.rtl {
            false => self.right,
            true => self.left,
        }
    }
}

/// A shaped line's carets in logical order.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Carets {
    clusters: Vec<Cluster>,
}

impl Carets {
    /// The carets of `text` as shaped into `line`.
    pub(crate) fn new(text: &str, line: &LineLayout) -> Self {
        let width = f32::from(line.width);
        let mut glyphs: Vec<(f32, usize)> = line
            .runs
            .iter()
            .flat_map(|run| run.glyphs.iter())
            .map(|glyph| (f32::from(glyph.position.x), glyph.index.min(line.len)))
            .collect();
        if glyphs.is_empty() {
            return Self::default();
        }
        // Each glyph reaches the next one drawn to its right.
        glyphs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let mut spans: Vec<(usize, f32, f32)> = glyphs
            .iter()
            .enumerate()
            .map(|(k, &(x, index))| {
                let right = glyphs.get(k + 1).map_or(width, |next| next.0);
                (index, x, right.max(x))
            })
            .collect();
        // A cluster is every glyph at one byte index, such as a letter and
        // the marks over it, and runs to the next index drawn.
        spans.sort_by_key(|span| span.0);
        let mut clusters: Vec<Cluster> = Vec::new();
        for (index, left, right) in spans {
            match clusters.last_mut() {
                Some(cluster) if cluster.bytes.start == index => {
                    cluster.left = cluster.left.min(left);
                    cluster.right = cluster.right.max(right);
                }
                _ => clusters.push(Cluster {
                    bytes: index..index,
                    left,
                    right,
                    rtl: false,
                }),
            }
        }
        let starts: Vec<usize> = clusters.iter().map(|cluster| cluster.bytes.start).collect();
        for (k, cluster) in clusters.iter_mut().enumerate() {
            cluster.bytes.end = starts.get(k + 1).copied().unwrap_or(line.len);
        }
        // Spaces and punctuation take the direction of the run around them,
        // which only the levels know; the positions alone cannot tell where
        // a Hebrew run ends and the Latin one beside it starts.
        let bidi = unicode_bidi::BidiInfo::new(text, None);
        for cluster in &mut clusters {
            cluster.rtl = bidi
                .levels
                .get(cluster.bytes.start)
                .is_some_and(|level| level.is_rtl());
        }
        Self { clusters }
    }

    /// Where the caret before byte `index` is drawn; past the end, after the
    /// last character in logical order, which may be on the left.
    pub(crate) fn x(&self, index: usize) -> Pixels {
        let Some(last) = self.clusters.last() else {
            return px(0.);
        };
        px(match self.cluster_at(index) {
            Some(cluster) => cluster.x(index),
            None if index < self.start() => self.clusters[0].x(self.start()),
            None => last.trailing(),
        })
    }

    /// The caret position nearest a click at `x`.
    pub(crate) fn index_at(&self, x: Pixels) -> usize {
        let x = f32::from(x);
        let Some(cluster) = self
            .clusters
            .iter()
            .find(|cluster| cluster.left <= x && x < cluster.right)
            .or_else(|| self.outermost(x))
        else {
            return 0;
        };
        let past_middle = x >= (cluster.left + cluster.right) / 2.;
        match past_middle != cluster.rtl {
            true => cluster.bytes.end,
            false => cluster.bytes.start,
        }
    }

    /// The stretches `range` covers, left to right. A selection across a
    /// change of direction is several stretches, as it is in any editor.
    pub(crate) fn spans(&self, range: Range<usize>) -> Vec<Range<Pixels>> {
        let mut spans: Vec<Range<f32>> = self
            .clusters
            .iter()
            .filter(|cluster| cluster.bytes.start < range.end && range.start < cluster.bytes.end)
            .map(|cluster| {
                let from = cluster.x(range.start.max(cluster.bytes.start));
                let to = match range.end >= cluster.bytes.end {
                    true => cluster.trailing(),
                    false => cluster.x(range.end),
                };
                from.min(to)..from.max(to)
            })
            .collect();
        spans.sort_by(|a, b| a.start.total_cmp(&b.start));
        let mut merged: Vec<Range<Pixels>> = Vec::with_capacity(spans.len());
        for span in spans {
            match merged.last_mut() {
                Some(last) if span.start <= f32::from(last.end) + 0.5 => {
                    last.end = last.end.max(px(span.end));
                }
                _ => merged.push(px(span.start)..px(span.end)),
            }
        }
        merged
    }

    /// The leftmost and rightmost edges `range` covers, as one box for the
    /// input method's candidate window; the caret's own place when empty.
    pub(crate) fn extent(&self, range: Range<usize>) -> Range<Pixels> {
        let spans = self.spans(range.clone());
        match (spans.first(), spans.last()) {
            (Some(first), Some(last)) => first.start..last.end,
            _ => {
                let x = self.x(range.start);
                x..x
            }
        }
    }

    fn start(&self) -> usize {
        self.clusters
            .first()
            .map_or(0, |cluster| cluster.bytes.start)
    }

    fn cluster_at(&self, index: usize) -> Option<&Cluster> {
        self.clusters
            .iter()
            .find(|cluster| cluster.bytes.contains(&index))
    }

    /// The cluster at the line's left or right end, for a click beyond it.
    fn outermost(&self, x: f32) -> Option<&Cluster> {
        let leftmost = self
            .clusters
            .iter()
            .min_by(|a, b| a.left.total_cmp(&b.left))?;
        match x < leftmost.left {
            true => Some(leftmost),
            false => self
                .clusters
                .iter()
                .max_by(|a, b| a.right.total_cmp(&b.right)),
        }
    }
}

#[cfg(test)]
mod tests;
