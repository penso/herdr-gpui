//! The trailing badges a row reserves room for: its pull request, change
//! counts, dirty, note, teleported, and muted marks, and the upstream drift
//! counts.

use super::label_text;
use crate::{
    config::{FontConfig, Theme},
    sidebar::{glyph_width, layout::SidebarMetrics, line_height},
};
use gpui::{prelude::*, *};

/// What a row shows on its right edge: the cached pull request, the line
/// counts `[sidebar] change_counts` picks, and whether the checkout has work
/// that is not committed yet.
pub(in crate::sidebar) struct RowBadge {
    pub(in crate::sidebar) pr: Option<PrBadge>,
    pub(in crate::sidebar) counts: Option<Counts>,
    pub(in crate::sidebar) dirty: bool,
    /// The work moved to another host; this checkout stays behind.
    pub(in crate::sidebar) teleported: bool,
    /// The user keeps a note on this checkout. Card layouts leave it to the
    /// note's own line under the row, which leads with the same icon, rather
    /// than squeeze their pull request number out of a narrow card.
    pub(in crate::sidebar) noted: bool,
    /// The user muted the workspace's notifications.
    pub(in crate::sidebar) muted: bool,
}

impl RowBadge {
    /// Whether it says anything about the checkout, beyond the workspace's
    /// mute, which card layouts put on the name's line instead.
    pub(in crate::sidebar) fn checkout_marks(&self) -> bool {
        self.pr.is_some() || self.counts.is_some() || self.dirty || self.teleported || self.noted
    }

    pub(in crate::sidebar) fn lines(&self, layout: &SidebarMetrics) -> usize {
        1 + usize::from(self.counts.is_some() && layout.pr_counts())
    }

    /// Nothing to draw is nothing to reserve, so a row with none keeps its
    /// full label width.
    pub(in crate::sidebar) fn new(
        pr: Option<PrBadge>,
        counts: Option<Counts>,
        dirty: bool,
        teleported: bool,
        noted: bool,
        muted: bool,
    ) -> Option<Self> {
        (pr.is_some() || counts.is_some() || dirty || teleported || noted || muted).then_some(
            Self {
                pr,
                counts,
                dirty,
                teleported,
                noted,
                muted,
            },
        )
    }

    /// The whole column's width: its first line or the counts line under
    /// it, whichever is wider.
    pub(in crate::sidebar) fn width(&self, font: &FontConfig, layout: &SidebarMetrics) -> f32 {
        self.first_width(font, layout)
            .max(self.counts_width(font, layout))
    }

    /// The counts line's width, or nothing when it is not drawn.
    pub(in crate::sidebar) fn counts_width(
        &self,
        font: &FontConfig,
        layout: &SidebarMetrics,
    ) -> f32 {
        let glyph = glyph_width(font);
        let counts = self.counts.as_ref().filter(|_| layout.pr_counts());
        counts.map_or(0., |counts| match counts.local {
            true => counts.chip_width(glyph, chip_icon(font)),
            false => (glyph * counts.glyphs() as f32).ceil(),
        })
    }

    /// The first line's width: the marks and the pull request number. The
    /// label's own first line only yields this much, not the counts' width.
    pub(in crate::sidebar) fn first_width(
        &self,
        font: &FontConfig,
        layout: &SidebarMetrics,
    ) -> f32 {
        let glyph = glyph_width(font);
        let number = self.pr.as_ref().map_or(0, |pr| pr.number.chars().count());
        let pr = (glyph * number as f32).ceil();
        // Reserve the icon and the gap before the PR number, even at small fonts.
        let mark = line_height(font).min(18.) + glyph;
        let marks = [
            self.dirty && !self.chipped(layout),
            self.teleported,
            self.noted,
            self.muted,
        ]
        .into_iter()
        .filter(|mark| *mark)
        .count();
        pr + mark * marks as f32
    }

    /// Local counts take the dirty mark's place, in a chip of its hue under
    /// the number, wherever the counts line shows.
    fn chipped(&self, layout: &SidebarMetrics) -> bool {
        layout.pr_counts() && self.counts.as_ref().is_some_and(|counts| counts.local)
    }

    /// The badge column at a row's trailing edge, `width` wide: the marks and
    /// pull request number on the first line, its counts under them.
    pub(in crate::sidebar) fn element(
        self,
        key: &str,
        width: f32,
        font: &FontConfig,
        theme: &Theme,
        layout: &SidebarMetrics,
    ) -> Div {
        let chipped = self.chipped(layout);
        let Self {
            pr,
            counts,
            dirty,
            teleported,
            noted,
            muted,
        } = self;
        div()
            .debug_selector(|| format!("pr-{key}"))
            .w(px(width))
            .flex_none()
            .flex()
            .flex_col()
            .items_end()
            .overflow_hidden()
            .child(
                div()
                    .debug_selector(|| format!("badge-line-{key}"))
                    .h(px(line_height(font)))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(glyph_width(font)))
                    .overflow_hidden()
                    .when(muted, |line| {
                        line.child(
                            crate::icons::muted(theme, (line_height(font) * 0.75).round().min(15.))
                                .debug_selector(|| format!("muted-{key}")),
                        )
                    })
                    // Uncommitted work, marked the way the titlebar
                    // button marks it.
                    .when(teleported, |line| {
                        line.child(
                            crate::icons::teleported(
                                theme,
                                (line_height(font) * 0.75).round().min(15.),
                            )
                            .debug_selector(|| format!("teleported-{key}")),
                        )
                    })
                    .when(noted, |line| {
                        line.child(
                            crate::icons::note(theme, (line_height(font) * 0.75).round().min(15.))
                                .debug_selector(|| format!("note-{key}")),
                        )
                    })
                    .when(dirty && !chipped, |line| {
                        line.child(
                            // Well under the line height, so marks on
                            // neighbouring rows keep a visible gap.
                            crate::icons::uncommitted(
                                theme,
                                (line_height(font) * 0.75).round().min(15.),
                            )
                            .debug_selector(|| format!("dirty-{key}")),
                        )
                    })
                    .when_some(pr.as_ref(), |line, badge| {
                        line.child(
                            div()
                                .flex_none()
                                .truncate()
                                .text_color(rgb(badge.color))
                                .child(label_text(&badge.number)),
                        )
                    }),
            )
            .when_some(counts.filter(|_| layout.pr_counts()), |column, badge| {
                if badge.local {
                    let colors = (theme.ink(theme.palette[2]), theme.ink(theme.palette[1]));
                    let chip = badge.chip(key, glyph_width(font), chip_icon(font), colors, theme);
                    return column.child(chip.h(px(line_height(font))));
                }
                column.child(
                    div()
                        .flex()
                        .flex_none()
                        .overflow_hidden()
                        .child(
                            div()
                                .flex_none()
                                .text_color(rgb(theme.ink(theme.palette[2])))
                                .child(label_text(&badge.additions)),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_color(rgb(theme.muted))
                                .child(label_text("/")),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_color(rgb(theme.ink(theme.palette[1])))
                                .child(label_text(&badge.deletions)),
                        ),
                )
            })
    }
}

/// Cached pull request state for a worktree row: the number carries the
/// lifecycle/readiness color.
pub(in crate::sidebar) struct PrBadge {
    pub(in crate::sidebar) number: String,
    pub(in crate::sidebar) color: u32,
}

impl PrBadge {
    pub(in crate::sidebar) fn new(pr: &crate::pull_request::PullRequest, theme: &Theme) -> Self {
        Self {
            number: format!("#{}", pr.number),
            color: crate::pull_request::color(pr, theme),
        }
    }
}

/// `+additions` and `-deletions` under a row's badge: the pull request's or
/// the working tree's, whichever `[sidebar] change_counts` picks.
pub(in crate::sidebar) struct Counts {
    pub(in crate::sidebar) additions: String,
    pub(in crate::sidebar) deletions: String,
    /// The checkout's uncommitted lines rather than its pull request's. They
    /// are drawn inside the dirty mark's chip, so the two never read alike.
    pub(in crate::sidebar) local: bool,
}

/// Room inside a local counts chip: padding on each side, its one-pixel
/// border, and the gap after the pencil.
const CHIP_PADDING: f32 = 3.;

impl Counts {
    pub(in crate::sidebar) fn new(additions: u64, deletions: u64) -> Self {
        Self {
            additions: format!("+{}", compact(additions)),
            deletions: format!("-{}", compact(deletions)),
            local: false,
        }
    }

    pub(in crate::sidebar) fn local(additions: u64, deletions: u64) -> Self {
        Self {
            local: true,
            ..Self::new(additions, deletions)
        }
    }

    /// A local chip's width at `glyph`, with an `icon`-wide pencil.
    pub(in crate::sidebar) fn chip_width(&self, glyph: f32, icon: f32) -> f32 {
        (glyph * self.glyphs() as f32).ceil() + icon + CHIP_PADDING * 3. + 2.
    }

    /// The pencil and the counts in the dirty mark's hue: it stands for the
    /// mark, so it carries the mark's selector and the counts the badge's.
    pub(in crate::sidebar) fn chip(
        &self,
        key: &str,
        glyph: f32,
        icon: f32,
        (additions, deletions): (u32, u32),
        theme: &Theme,
    ) -> Div {
        let hue = theme.palette[3];
        div()
            .debug_selector(|| format!("dirty-{key}"))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(CHIP_PADDING))
            .px(px(CHIP_PADDING))
            .overflow_hidden()
            .rounded(px(crate::config::corners::SMALL))
            .bg(rgba((hue << 8) | 0x24))
            .border_1()
            .border_color(rgba((hue << 8) | 0x70))
            .child(
                svg()
                    .flex_none()
                    .size(px(icon))
                    .path("icons/pencil.svg")
                    .text_color(rgb(theme.ink(hue))),
            )
            .child(
                div()
                    .debug_selector(|| format!("counts-{key}"))
                    .flex()
                    .flex_none()
                    .w(px((glyph * self.glyphs() as f32).ceil()))
                    .child(
                        div()
                            .text_color(rgb(additions))
                            .child(label_text(&self.additions)),
                    )
                    .child(div().text_color(rgb(theme.muted)).child(label_text("/")))
                    .child(
                        div()
                            .text_color(rgb(deletions))
                            .child(label_text(&self.deletions)),
                    ),
            )
    }

    /// Both counts and the separator between them. Sidebar labels are
    /// monospace by default and digits are near-uniform elsewhere, so an
    /// em-fraction per glyph bounds the line; a wider face truncates the
    /// counts rather than eating the label.
    pub(in crate::sidebar) fn glyphs(&self) -> usize {
        self.additions.chars().count() + self.deletions.chars().count() + 1
    }
}

/// The pencil inside a local counts chip, a little smaller than the bare
/// mark's so the chip keeps a line's height.
pub(in crate::sidebar) fn chip_icon(font: &FontConfig) -> f32 {
    (line_height(font) * 0.6).round().min(12.)
}

/// How far the checked-out branch has drifted from its upstream, as the
/// daemon's `git_status` token reports it: `↑` commits to push in green, `↓`
/// commits to pull in red, painted the way the TUI paints them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::sidebar) struct Upstream {
    ahead: usize,
    behind: usize,
}

impl Upstream {
    /// Nothing while the branch is in sync, has no upstream, or the daemon's
    /// sidebar config leaves `git_status` out and so never computes it.
    pub(in crate::sidebar) fn new(counts: Option<(usize, usize)>) -> Option<Self> {
        let (ahead, behind) = counts?;
        (ahead > 0 || behind > 0).then_some(Self { ahead, behind })
    }

    /// `↑ahead` and `↓behind`, each only when nonzero.
    fn parts(self) -> impl Iterator<Item = (String, bool)> {
        [
            (self.ahead, '\u{2191}', true),
            (self.behind, '\u{2193}', false),
        ]
        .into_iter()
        .filter(|(count, _, _)| *count > 0)
        .map(|(count, arrow, ahead)| (format!("{arrow}{count}"), ahead))
    }

    /// Width at `glyph`, the two counts a glyph apart as in `↑2 ↓18`.
    pub(in crate::sidebar) fn width(self, glyph: f32) -> f32 {
        let (glyphs, parts) = self.parts().fold((0, 0), |(glyphs, parts), (text, _)| {
            (glyphs + text.chars().count(), parts + 1)
        });
        ((glyphs + parts - 1) as f32 * glyph).ceil()
    }

    pub(in crate::sidebar) fn element(self, key: &str, glyph: f32, theme: &Theme) -> Div {
        let (ahead, behind) = (theme.ink(theme.palette[2]), theme.ink(theme.palette[1]));
        div()
            .debug_selector(|| format!("upstream-{key}"))
            .w(px(self.width(glyph)))
            .flex_none()
            .flex()
            .gap(px(glyph))
            .overflow_hidden()
            .children(self.parts().map(|(text, is_ahead)| {
                div()
                    .flex_none()
                    .text_color(rgb(if is_ahead { ahead } else { behind }))
                    .child(label_text(&text))
            }))
    }
}

/// Four digits of churn is already a big diff; abbreviate past that so the
/// column stays narrow enough to leave the branch readable. The titlebar's Git
/// badge reuses it so one PR reads the same in both places.
pub(crate) fn compact(lines: u64) -> String {
    match lines {
        0..=9999 => lines.to_string(),
        _ => format!("{}k", lines / 1000),
    }
}
