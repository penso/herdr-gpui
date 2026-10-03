//! One sidebar row: its icon, tree guides, badges, and the label budget that
//! decides what still fits. Text is elided against measured glyph widths, not
//! guessed, so a long label cannot overflow the row it was laid out in.

#[cfg(any(test, feature = "integration-test"))]
use super::layout_tests;
use super::{
    ARROW_RESERVE, ICON_RESERVE, STATUS_WIDTH,
    agents::{Indicators, status_indicator, status_mark},
    cell::{RowContext, RowState},
    glyph_width,
    layout::SidebarDensity,
    line_height, segment_budgets,
    tokens::{ResolvedToken, TextRole, TokenKind, budgets, separator},
};
use crate::config::{FontConfig, Theme};
use gpui::{prelude::*, *};
use herdr_client::protocol::AgentStatus;
use std::sync::Arc;
use unicode_width::UnicodeWidthStr;

/// What a row shows in its leading icon slot: a repository owner's avatar when
/// one is cached, the GitHub mark while it is not, and nothing for the child
/// rows that reserve no slot at all.
pub(super) enum RowIcon {
    None,
    Mark,
    Avatar(Arc<Image>),
}

impl RowIcon {
    fn slot(self, key: &str, font: &FontConfig, color: u32) -> Div {
        div()
            .debug_selector(|| format!("github-{key}"))
            .absolute()
            .left_0()
            .top(px((line_height(font) - 12.) / 2.))
            .size(px(12.))
            .flex_none()
            .overflow_hidden()
            .child(self.element(color))
    }

    /// The icon filling its parent, or nothing for rows without one. An
    /// avatar still loading or failing to decode shows the mark instead.
    pub(super) fn element(self, color: u32) -> AnyElement {
        match self {
            Self::None => Empty.into_any_element(),
            Self::Mark => github_mark(color).size_full().into_any_element(),
            Self::Avatar(image) => img(image)
                .size_full()
                .rounded_full()
                .with_fallback(move || github_mark(color).size_full().into_any_element())
                .with_loading(move || github_mark(color).size_full().into_any_element())
                .into_any_element(),
        }
    }
}

/// The mark paints as vector rather than a rasterized image, so it stays sharp
/// at every size it stands in for an avatar.
pub(crate) fn github_mark(color: u32) -> Svg {
    svg()
        .path("icons/github.svg")
        .flex_none()
        .text_color(rgb(color))
}
/// What a row lists, which decides how its two lines are weighted: upstream
/// keeps agent names bold throughout and reserves bold workspaces for the
/// current one, with the branch picking up the accent while it is focused.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RowKind {
    Workspace,
    Agent(crate::icons::AgentIcon),
}

/// Name color, name weight, and detail color for a row.
pub(super) fn row_text(kind: RowKind, focused: bool, theme: &Theme) -> (u32, FontWeight, u32) {
    let weight = if focused || matches!(kind, RowKind::Agent(_)) {
        FontWeight::BOLD
    } else {
        FontWeight::NORMAL
    };
    let name = if focused {
        theme.foreground
    } else {
        theme.subtext()
    };
    let detail = if focused && kind == RowKind::Workspace {
        theme.primary()
    } else {
        theme.muted
    };
    (name, weight, detail)
}

/// A teleported checkout's name: its work lives on another host now, so the
/// name fades toward the background to read as the copy not to use. The fade
/// stops at the standard mark contrast even when high contrast is on: high
/// contrast already lifts dim labels to its own floor, so honoring it here
/// would leave the fade invisible, and the teleport icon beside the name
/// keeps the meaning readable.
pub(super) fn left_behind(color: u32, theme: &Theme) -> u32 {
    crate::contrast::ink_on_chrome(
        crate::config::mix(theme.background, color, 45),
        [theme.background, theme.sidebar_background(), theme.active],
        crate::contrast::Contrast::Standard.mark_ratio(),
    )
}

/// Where a row stands while a workspace is dragged: rows the lifted card
/// passes over stop answering hover, so only the drop line marks a place.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum RowLift {
    #[default]
    Resting,
    Passed,
    Lifted,
}

/// Where a row sits in its worktree group, which decides whether the gutter
/// carries a trunk through the row or ends in an elbow.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RowTree {
    None,
    Child,
    LastChild,
}

/// Trunk and tick for a child row, given the gutter the row reserves between
/// the parent's label column and the child's status dot. Snapped to whole
/// device pixels and painted as quads rather than borders: a bordered box
/// rounds each edge on its own, which left the trunk thinner than its tick.
pub(super) fn tree_lines(
    gutter: Bounds<Pixels>,
    tree: RowTree,
    font: &FontConfig,
    padding: f32,
    scale: f32,
) -> [Bounds<Pixels>; 2] {
    let device = |value: Pixels| f32::from(value) * scale;
    let logical = |value: f32| px(value / scale);
    let weight = scale.round().max(1.);
    let snap = |value: Pixels| logical(device(value).round());
    // The trunk runs down the gutter's leading edge; the tick crosses it at the
    // status dot's middle row and stops where the dot begins.
    let x = snap(gutter.origin.x);
    let middle = logical(
        (device(gutter.origin.y + px(padding + line_height(font) / 2.)) - weight / 2.).round(),
    );
    let end = if tree == RowTree::LastChild {
        middle + logical(weight)
    } else {
        snap(gutter.bottom())
    };
    [
        Bounds::from_corners(
            point(x, snap(gutter.origin.y)),
            point(x + logical(weight), end),
        ),
        Bounds::from_corners(
            point(x, middle),
            point(snap(gutter.right()), middle + logical(weight)),
        ),
    ]
}

/// What a row shows on its right edge: the cached pull request, and whether
/// the checkout has work that is not committed yet.
pub(super) struct RowBadge {
    pub(super) pr: Option<PrBadge>,
    pub(super) dirty: bool,
    /// The work moved to another host; this checkout stays behind.
    pub(super) teleported: bool,
}

impl RowBadge {
    pub(super) fn lines(&self, layout: &dyn SidebarDensity) -> usize {
        1 + usize::from(self.pr.is_some() && layout.pr_counts())
    }

    /// Nothing to draw is nothing to reserve, so a row with neither keeps its
    /// full label width.
    pub(super) fn new(pr: Option<PrBadge>, dirty: bool, teleported: bool) -> Option<Self> {
        (pr.is_some() || dirty || teleported).then_some(Self {
            pr,
            dirty,
            teleported,
        })
    }

    pub(super) fn width(&self, font: &FontConfig, layout: &dyn SidebarDensity) -> f32 {
        let pr = self.pr.as_ref().map_or(0., |pr| pr.width(font, layout));
        // Reserve the icon and the gap before the PR number, even at small fonts.
        let mark = line_height(font).min(18.) + glyph_width(font);
        pr + mark * f32::from(u8::from(self.dirty) + u8::from(self.teleported))
    }
}

/// Cached pull request state for a worktree row: the number carries the
/// lifecycle/readiness color, the counts sit under it.
pub(super) struct PrBadge {
    pub(super) number: String,
    pub(super) color: u32,
    pub(super) additions: String,
    pub(super) deletions: String,
}

impl PrBadge {
    pub(super) fn new(pr: &crate::pull_request::PullRequest, theme: &Theme) -> Self {
        Self {
            number: format!("#{}", pr.number),
            color: pr.color(theme),
            additions: format!("+{}", compact(pr.additions)),
            deletions: format!("-{}", compact(pr.deletions)),
        }
    }

    /// Reserved width. Sidebar labels are monospace by default and digits are
    /// near-uniform elsewhere, so an em-fraction per glyph bounds both lines;
    /// a wider face truncates the counts rather than eating the label.
    pub(super) fn width(&self, font: &FontConfig, layout: &dyn SidebarDensity) -> f32 {
        let mut glyphs = self.number.chars().count();
        if layout.pr_counts() {
            glyphs =
                glyphs.max(self.additions.chars().count() + self.deletions.chars().count() + 1);
        }
        (glyph_width(font) * glyphs as f32).ceil()
    }
}

/// How far the checked-out branch has drifted from its upstream, as the
/// daemon's `git_status` token reports it: `↑` commits to push in green, `↓`
/// commits to pull in red, painted the way the TUI paints them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Upstream {
    ahead: usize,
    behind: usize,
}

impl Upstream {
    /// Nothing while the branch is in sync, has no upstream, or the daemon's
    /// sidebar config leaves `git_status` out and so never computes it.
    pub(super) fn new(counts: Option<(usize, usize)>) -> Option<Self> {
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
    pub(super) fn width(self, glyph: f32) -> f32 {
        let (glyphs, parts) = self.parts().fold((0, 0), |(glyphs, parts), (text, _)| {
            (glyphs + text.chars().count(), parts + 1)
        });
        ((glyphs + parts - 1) as f32 * glyph).ceil()
    }

    pub(super) fn element(self, key: &str, glyph: f32, theme: &Theme) -> Div {
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

/// A row's first line: segments joined by upstream's separator, the primary one
/// carrying the row's weight and color while the rest stay muted. Segments are
/// placed at measured offsets rather than flexed, because GPUI 0.2.2 only
/// ellipsizes text whose width its parent already fixed.
pub(super) fn name_line(
    segments: &[(&str, bool)],
    appearance: (u32, FontWeight, u32),
    width: f32,
    font: &FontConfig,
) -> Div {
    let (primary, weight, muted) = appearance;
    let glyph = glyph_width(font);
    let separator = 3. * glyph;
    let separators = segments.len().saturating_sub(1);
    let lengths: Vec<usize> = segments.iter().map(|(text, _)| text.width()).collect();
    let available = (width - separators as f32 * separator).max(0.);
    let budgets = segment_budgets(&lengths, (available / glyph).floor() as usize);
    // The last segment takes the rounding remainder, so one segment fills the
    // line exactly as it did before a line could carry several.
    let used: f32 = budgets.iter().map(|budget| *budget as f32 * glyph).sum();
    let slack = (available - used).max(0.);
    let parts =
        segments
            .iter()
            .zip(budgets)
            .enumerate()
            .map(|(index, ((text, is_primary), budget))| {
                let advance = budget as f32 * glyph
                    + if index + 1 == segments.len() {
                        slack
                    } else {
                        0.
                    };
                let cell = div()
                    .truncate()
                    .when(*is_primary, |part| {
                        part.font_weight(weight).text_color(rgb(primary))
                    })
                    .when(!*is_primary, |part| part.text_color(rgb(muted)))
                    .child(label_text(text));
                (if index == 0 { "" } else { " · " }, advance, cell)
            });
    place_line(parts, width, font, muted)
}

fn place_line(
    parts: impl IntoIterator<Item = (&'static str, f32, Div)>,
    width: f32,
    font: &FontConfig,
    muted: u32,
) -> Div {
    let mut line = div()
        .relative()
        .w(px(width))
        .h(px(line_height(font)))
        .flex_none()
        .overflow_hidden();
    let mut x = 0.;
    for (separator, advance, cell) in parts {
        if !separator.is_empty() {
            let gap = separator.chars().count() as f32 * glyph_width(font);
            line = line.child(
                div()
                    .absolute()
                    .left(px(x))
                    .w(px(gap))
                    .text_color(rgb(muted))
                    .child(label_text(separator)),
            );
            x += gap;
        }
        line = line.child(cell.absolute().left(px(x)).w(px(advance)));
        x += advance;
    }
    line
}

pub(super) fn removing_dot(selector: &'static str, theme: &Theme) -> Div {
    div()
        .debug_selector(move || selector.into())
        .size(px(STATUS_WIDTH))
        .flex_none()
        .child(
            div()
                .size_full()
                .rounded_full()
                .bg(rgb(theme.primary()))
                .with_animation(
                    SharedString::from(format!("{selector}-pulse")),
                    Animation::new(std::time::Duration::from_secs(1)).repeat(),
                    |dot, delta| dot.opacity(0.3 + 0.7 * (delta * std::f32::consts::PI).sin()),
                ),
        )
}

/// What colors a configured row's tokens: the row they paint and its state.
#[derive(Clone, Copy)]
pub(super) struct TokenLook {
    pub(super) kind: RowKind,
    pub(super) status: AgentStatus,
    pub(super) focused: bool,
    /// A teleported checkout fades its name, as the native rows do.
    pub(super) teleported: bool,
}

fn token_appearance(kind: &TokenKind, look: TokenLook, cx: &RowContext<'_>) -> (u32, FontWeight) {
    let theme = cx.theme;
    let (name, weight, secondary) = row_text(look.kind, look.focused, theme);
    match kind {
        TokenKind::StateIcon | TokenKind::Text(_, TextRole::Status) => {
            (cx.indicators.color(look.status), FontWeight::NORMAL)
        }
        TokenKind::Text(_, TextRole::Workspace) if look.teleported => {
            (left_behind(name, theme), weight)
        }
        TokenKind::Text(_, TextRole::Workspace) => (name, weight),
        TokenKind::Text(_, TextRole::Secondary | TextRole::Agent) => {
            (secondary, FontWeight::NORMAL)
        }
        _ => (theme.muted, FontWeight::NORMAL),
    }
}

fn styled(
    (color, weight): (u32, FontWeight),
    style: crate::config::TokenStyle,
    theme: &Theme,
) -> (u32, FontWeight) {
    let color = style.fg.unwrap_or(color);
    let color = if style.dim == Some(true) {
        theme.dimmed(color)
    } else {
        color
    };
    let weight = match style.bold {
        Some(true) => FontWeight::BOLD,
        Some(false) => FontWeight::NORMAL,
        None => weight,
    };
    (color, weight)
}

/// Status icons and git counters retain their full width when text truncates.
fn fixed_glyphs(kind: &TokenKind, glyph: f32, status_width: f32) -> usize {
    match kind {
        TokenKind::StateIcon => (status_width / glyph).ceil() as usize,
        TokenKind::GitStatus { ahead, behind } => {
            let count = |n: &usize| format!("{n}").chars().count() + 1;
            usize::from(*ahead > 0) * count(ahead)
                + usize::from(*behind > 0) * count(behind)
                + usize::from(*ahead > 0 && *behind > 0)
        }
        _ => 0,
    }
}

/// Fixed token widths preserve GPUI's text truncation during layout.
fn token_line(row: &[ResolvedToken], look: TokenLook, width: f32, cx: &RowContext<'_>) -> Div {
    let status = look.status;
    let (font, theme) = (cx.font, cx.theme);
    let glyph = glyph_width(font);
    let budgets = budgets(
        row,
        |kind| fixed_glyphs(kind, glyph, cx.indicators.width(font)),
        (width / glyph).floor() as usize,
    );
    let visible: Vec<_> = row
        .iter()
        .zip(budgets)
        .filter_map(|(token, width)| Some((token, width?)))
        .collect();
    let used = visible.iter().map(|(_, width)| width).sum::<usize>()
        + visible
            .windows(2)
            .map(|pair| separator(pair[0].0, pair[1].0).chars().count())
            .sum::<usize>();
    let slack = (width - used as f32 * glyph).max(0.);
    let last_text = visible
        .iter()
        .rposition(|(token, _)| token.kind.text().is_some());
    let parts = visible.iter().enumerate().map(|(index, (token, budget))| {
        let gap = if index == 0 {
            ""
        } else {
            separator(visible[index - 1].0, token)
        };
        let advance = *budget as f32 * glyph + if last_text == Some(index) { slack } else { 0. };
        let (color, weight) = styled(token_appearance(&token.kind, look, cx), token.style, theme);
        let cell = div();
        let cell = match &token.kind {
            TokenKind::StateIcon => cell
                .h(px(line_height(font)))
                .flex()
                .justify_center()
                .debug_selector(|| "inline-status-cell".into())
                .child(
                    status_mark(
                        status,
                        font,
                        cx.indicators,
                        color,
                        weight == FontWeight::BOLD,
                    )
                    .debug_selector(|| "inline-status-mark".into()),
                ),
            TokenKind::GitStatus { ahead, behind } => cell.flex().gap(px(glyph)).children(
                [("↑", *ahead, 2), ("↓", *behind, 1)]
                    .into_iter()
                    .filter(|(_, count, _)| *count > 0)
                    .map(|(arrow, count, palette)| {
                        let (color, weight) = styled(
                            (theme.ink(theme.palette[palette]), FontWeight::NORMAL),
                            token.style,
                            theme,
                        );
                        div()
                            .flex_none()
                            .text_color(rgb(color))
                            .font_weight(weight)
                            .child(label_text(&format!("{arrow}{count}")))
                    }),
            ),
            // Only text the budget cuts short truncates. An emoji's font is wider
            // than two estimated glyphs, so a label that fits by its display
            // width may overhang its cell slightly rather than collapse to "…".
            TokenKind::Text(text, _) if text.width() <= *budget => cell
                .whitespace_nowrap()
                .font_weight(weight)
                .text_color(rgb(color))
                .child(shared_label_text(text.clone())),
            TokenKind::Text(text, _) => cell
                .truncate()
                .font_weight(weight)
                .text_color(rgb(color))
                .child(shared_label_text(text.clone())),
        };
        (gap, advance, cell)
    });
    place_line(parts, width, font, theme.muted)
}

/// The `state_icon` leading a configured row's first line. It takes the row's
/// status slot; a configured row without one shows no status of its own.
pub(super) fn leading_status(lines: &[Vec<ResolvedToken>]) -> Option<&ResolvedToken> {
    lines
        .first()
        .and_then(|line| line.first())
        .filter(|token| matches!(token.kind, TokenKind::StateIcon))
}

/// The color, and whether bold, a leading `state_icon` paints the status in.
pub(super) fn configured_status_style(
    token: &ResolvedToken,
    status: AgentStatus,
    cx: &RowContext<'_>,
) -> (u32, bool) {
    let (color, weight) = styled(
        (cx.indicators.color(status), FontWeight::NORMAL),
        token.style,
        cx.theme,
    );
    (color, weight == FontWeight::BOLD)
}

/// The status mark a leading `state_icon` styles, offset onto the first line.
pub(super) fn configured_status(
    token: &ResolvedToken,
    status: AgentStatus,
    cx: &RowContext<'_>,
) -> Div {
    let (color, bold) = configured_status_style(token, status, cx);
    status_mark(status, cx.font, cx.indicators, color, bold)
}

/// A line's tokens, less the leading `state_icon` the status slot draws.
fn line_tokens(index: usize, line: &[ResolvedToken]) -> &[ResolvedToken] {
    match line.split_first() {
        Some((first, rest)) if index == 0 && matches!(first.kind, TokenKind::StateIcon) => rest,
        _ => line,
    }
}

fn line_selector(key: &str, index: usize) -> String {
    match index {
        0 => format!("name-{key}"),
        1 => format!("detail-{key}"),
        index => format!("line-{key}-{index}"),
    }
}

/// Configured lines stacked at `width`, for layouts that draw their own
/// status and icons beside the text.
pub(super) fn token_column(
    key: &str,
    lines: &[Vec<ResolvedToken>],
    look: TokenLook,
    width: f32,
    cx: &RowContext<'_>,
) -> Div {
    lines.iter().enumerate().fold(
        div().w(px(width)).flex_none().flex().flex_col(),
        |column, (index, line)| {
            column.child(
                token_line(line_tokens(index, line), look, width, cx)
                    .debug_selector(|| line_selector(key, index)),
            )
        },
    )
}

fn configured_lines(
    mut column: Div,
    key: &str,
    lines: &[Vec<ResolvedToken>],
    look: TokenLook,
    label_width: f32,
    mut workspace_icon: RowIcon,
    cx: &RowContext<'_>,
) -> Div {
    let (font, theme) = (cx.font, cx.theme);
    let agent_icon = match look.kind {
        RowKind::Agent(icon) => Some(icon),
        RowKind::Workspace => None,
    };
    let agent_at = lines.iter().position(|line| {
        line.iter()
            .any(|token| matches!(token.kind, TokenKind::Text(_, TextRole::Agent)))
    });
    let (agent_size, agent_reserve) = agent_icon_size(font);
    let workspace_reserve = if matches!(workspace_icon, RowIcon::None) {
        0.
    } else {
        ICON_RESERVE
    };
    for (index, line) in lines.iter().enumerate() {
        let agent_here = agent_at == Some(index);
        let reserve = if index == 0 { workspace_reserve } else { 0. }
            + if agent_here { agent_reserve } else { 0. };
        let text_width = (label_width - reserve).max(0.);
        let selector = line_selector(key, index);
        let icon_color = line
            .iter()
            .find(|token| matches!(token.kind, TokenKind::Text(_, TextRole::Agent)))
            .map(|token| styled(token_appearance(&token.kind, look, cx), token.style, theme).0)
            .unwrap_or(theme.muted);
        let mut text = div().relative().w(px(label_width)).h(px(line_height(font)));
        if agent_here && let Some(icon) = agent_icon {
            text = text.child(agent_mark(key, icon, agent_size, icon_color, font));
        }
        if index == 0 && !matches!(workspace_icon, RowIcon::None) {
            text = text.child(std::mem::replace(&mut workspace_icon, RowIcon::None).slot(
                key,
                font,
                theme.muted,
            ));
        }
        text = text.child(
            token_line(line_tokens(index, line), look, text_width, cx)
                .debug_selector(|| selector.clone())
                .ml(px(reserve.min(label_width))),
        );
        column = column.child(text);
    }
    column
}

#[allow(clippy::too_many_arguments)]
pub(super) fn row(
    // Rows are probed by key, not by label: an agent names its workspace, which
    // already names a row of its own.
    key: &str,
    name: &[(&str, bool)],
    detail: &str,
    kind: RowKind,
    status: AgentStatus,
    indicators: Indicators,
    removing: bool,
    state: RowState,
    tree: RowTree,
    reserve_arrow: bool,
    workspace_icon: RowIcon,
    arrow: Option<Stateful<Div>>,
    badge: Option<RowBadge>,
    upstream: Option<Upstream>,
    // The status word the daemon's `state_text` token asks to show, when its
    // sidebar config names it. Painted at the row's trailing edge in the dot's
    // color so a status reads at a glance, not only by hue.
    status_text: Option<&str>,
    lines: &[Vec<ResolvedToken>],
    cx: &RowContext<'_>,
) -> Div {
    let (font, theme, look, width) = (cx.font, cx.theme, cx.look, cx.width);
    let focused = state.selected;
    let layout = look.density;
    let configured = !lines.is_empty();
    let teleported = badge.as_ref().is_some_and(|badge| badge.teleported);
    let leading_icon = leading_status(lines);
    let show_status = !configured || leading_icon.is_some() || removing;
    let status_text = status_text.filter(|_| !configured);
    let text_lines = if configured {
        let badge_lines = badge.as_ref().map_or(0, |badge| badge.lines(layout));
        lines.len().max(badge_lines)
    } else {
        0
    };
    let padding = layout.padding();
    let content_x = look.content_x();
    let gap = layout.gap();
    let vertical_padding = look.row_padding();
    // Content starts below the highlight's edge, which sits half the row
    // spacing in from the row's own.
    let content_top = vertical_padding + look.spacing() / 2.;
    let show_detail = matches!(kind, RowKind::Agent(_))
        || if tree == RowTree::None {
            layout.workspace_details()
        } else {
            layout.child_details()
        };
    let (name_color, weight, detail_color) = row_text(kind, focused, theme);
    let name_color = if teleported {
        left_behind(name_color, theme)
    } else {
        name_color
    };
    let icon_reserve = match workspace_icon {
        RowIcon::None => 0.,
        _ => ICON_RESERVE,
    };
    let muted = theme.muted;
    let status_width = indicators.width(font);
    let extra_status_width = status_width - STATUS_WIDTH;
    let indent = if tree == RowTree::None {
        0.
    } else {
        layout.child_indent() + extra_status_width
    };
    let arrow_reserve = if reserve_arrow { ARROW_RESERVE } else { 0. };
    let arrow_absent = arrow.is_none();
    let status_gutter = if show_status { status_width + gap } else { 0. };
    let available = (look.content_width(width) - status_gutter - indent - arrow_reserve).max(0.);
    // Narrow sidebars and large fonts can leave less room than a badge needs.
    // Clip its column within the row rather than painting over the terminal.
    let badge_width = badge.as_ref().map_or(0., |badge| {
        badge.width(font, layout).min((available - gap).max(0.))
    });
    let pr_reserve = if badge.is_some() {
        badge_width + gap
    } else {
        0.
    };
    // The status word keeps its own trailing column, so the label yields to it
    // the same way it yields to a badge, and like the badge it is clipped to
    // the room left rather than painting past the row.
    let status_width = status_text.map_or(0., |text| {
        (text.width() as f32 * glyph_width(font))
            .ceil()
            .min((available - pr_reserve - gap).max(0.))
    });
    let status_reserve = if status_text.is_some() {
        status_width + gap
    } else {
        0.
    };
    let status_color = indicators.color(status);
    let glyph = glyph_width(font);
    // A workspace's second line carries the counts after its branch, as the
    // TUI does. A one-line row has no branch line, so a trailing column keeps
    // them visible in compact densities and on worktree children.
    // Configured rows show the counts only through their `git_status` token.
    let upstream = upstream.filter(|_| !configured);
    let upstream_inline = upstream.filter(|_| show_detail && kind == RowKind::Workspace);
    let upstream_trailing = upstream.filter(|_| upstream_inline.is_none());
    let upstream_width = upstream_trailing.map_or(0., |upstream| {
        upstream
            .width(glyph)
            .min((available - pr_reserve - status_reserve - gap).max(0.))
    });
    let upstream_reserve = if upstream_trailing.is_some() {
        upstream_width + gap
    } else {
        0.
    };
    let label_width = (available - pr_reserve - status_reserve - upstream_reserve).max(0.);
    let agent_icon = match kind {
        RowKind::Agent(icon) => Some(icon),
        RowKind::Workspace => None,
    };
    // An orphan's name is on the first line; normal rows name the agent below
    // its location. Reserve the same fixed icon + gap on whichever line owns it.
    let agent_first = agent_icon.filter(|_| detail.is_empty());
    let agent_detail = agent_icon.filter(|_| !detail.is_empty());
    let (agent_size, agent_reserve) = agent_icon_size(font);
    let name_reserve = icon_reserve + agent_first.map_or(0., |_| agent_reserve);
    div()
        .debug_selector(|| format!("row-{key}"))
        .h(px(look.row_height(
            line_height(font)
                * if configured {
                    text_lines as f32
                } else if show_detail {
                    2.
                } else {
                    1.
                },
        )))
        .w_full()
        .min_w_0()
        .flex_none()
        .relative()
        .pl(px(content_x + indent))
        .pr(px(content_x))
        .flex()
        .items_start()
        .gap(px(gap))
        .py(px(content_top))
        .cursor_pointer()
        .map(|row| look.mark(row, key, state, theme))
        // Tree lines run in the indent the row already reserves, so a child is
        // tied to its parent without box-drawing glyphs in the label.
        .when(tree != RowTree::None && look.style.tree_lines(), |row| {
            let (color, font) = (theme.muted, font.clone());
            let gutter = look.tree_gutter() + extra_status_width;
            row.child(
                div()
                    .debug_selector(|| format!("tree-{key}"))
                    .absolute()
                    // Between the parent's label column and this row's own dot.
                    .left(px(gutter))
                    .w(px(padding + indent
                        - layout.tree_gutter()
                        - extra_status_width))
                    .top_0()
                    .bottom_0()
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |bounds, _, window, _| {
                                for line in tree_lines(
                                    bounds,
                                    tree,
                                    &font,
                                    content_top,
                                    window.scale_factor(),
                                ) {
                                    window.paint_quad(fill(line, rgb(color)));
                                }
                            },
                        )
                        .size_full(),
                    ),
            )
        })
        .when(show_status, |row| {
            row.child(if removing {
                div()
                    .w(px(indicators.width(font)))
                    .mt(px((line_height(font) - STATUS_WIDTH) / 2.))
                    .flex_none()
                    .flex()
                    .justify_center()
                    .child(removing_dot("worktree-removing", theme))
            } else if let Some(token) = leading_icon {
                configured_status(token, status, cx)
            } else {
                status_indicator(status, font, indicators)
            })
        })
        .child(
            div()
                .flex()
                .flex_col()
                // Avoid zero-basis measurement: GPUI 0.2.2 mutates text run
                // lengths when truncating and reuses them on wider measurements.
                .w(px(label_width))
                .flex_none()
                .overflow_hidden()
                .debug_selector(|| format!("column-{key}"))
                .map(|column| {
                    if configured {
                        return configured_lines(
                            column,
                            key,
                            lines,
                            TokenLook {
                                kind,
                                status,
                                focused,
                                teleported,
                            },
                            label_width,
                            workspace_icon,
                            cx,
                        );
                    }
                    column
                        .child(
                            div()
                                .relative()
                                .w(px(label_width))
                                .h(px(line_height(font)))
                                .when_some(agent_first, |line, icon| {
                                    line.child(agent_mark(key, icon, agent_size, name_color, font))
                                })
                                .when(!matches!(workspace_icon, RowIcon::None), |title| {
                                    title.child(workspace_icon.slot(key, font, muted))
                                })
                                .child(
                                    name_line(
                                        name,
                                        (name_color, weight, theme.muted),
                                        (label_width - name_reserve).max(0.),
                                        font,
                                    )
                                    .debug_selector(|| format!("name-{key}"))
                                    .ml(px(name_reserve.min(label_width))),
                                ),
                        )
                        .when(show_detail, |column| {
                            let detail_x =
                                agent_detail.map_or(0., |_| agent_reserve.min(label_width));
                            let room = (label_width - detail_x).max(0.);
                            // The counts follow the branch and win the room it would
                            // take, so a long branch ellipsizes before they do.
                            let (detail_width, upstream_x) = match upstream_inline {
                                None => (room, room),
                                Some(_) if detail.is_empty() => (0., 0.),
                                Some(upstream) => {
                                    let natural = (detail.chars().count() as f32 * glyph).ceil();
                                    let width =
                                        natural.min((room - upstream.width(glyph) - glyph).max(0.));
                                    (width, width + glyph)
                                }
                            };
                            column.child(
                                div()
                                    .relative()
                                    .w(px(label_width))
                                    .h(px(line_height(font)))
                                    .when_some(agent_detail, |line, icon| {
                                        line.child(agent_mark(
                                            key,
                                            icon,
                                            agent_size,
                                            detail_color,
                                            font,
                                        ))
                                    })
                                    .child(
                                        div()
                                            .debug_selector(|| format!("detail-{key}"))
                                            .ml(px(detail_x))
                                            .w(px(detail_width))
                                            .truncate()
                                            .text_color(rgb(detail_color))
                                            .child(label_text(detail)),
                                    )
                                    .when_some(upstream_inline, |line, upstream| {
                                        line.child(
                                            div()
                                                .absolute()
                                                .top_0()
                                                .left(px(detail_x + upstream_x))
                                                .w(px((room - upstream_x).max(0.)))
                                                .overflow_hidden()
                                                .child(upstream.element(key, glyph, theme)),
                                        )
                                    }),
                            )
                        })
                }),
        )
        .when_some(status_text, |row, text| {
            row.child(
                div()
                    .debug_selector(|| format!("status-{key}"))
                    .w(px(status_width))
                    .flex_none()
                    .h(px(line_height(font)))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .text_color(rgb(status_color))
                    .child(div().w(px(status_width)).truncate().child(label_text(text))),
            )
        })
        .when_some(upstream_trailing, |row, upstream| {
            row.child(
                div()
                    .w(px(upstream_width))
                    .flex_none()
                    .h(px(line_height(font)))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .child(upstream.element(key, glyph, theme)),
            )
        })
        // The collapse column comes first so the badge can hug the row's edge;
        // a reserved-but-empty column keeps every badge on the same right edge.
        .when_some(arrow, |row, arrow| row.child(arrow))
        .when(arrow_absent && reserve_arrow, |row| {
            row.child(div().w(px(ARROW_RESERVE - gap)).flex_none())
        })
        .when_some(badge, |row, badge| {
            let RowBadge {
                pr,
                dirty,
                teleported,
            } = badge;
            row.child(
                div()
                    .debug_selector(|| format!("pr-{key}"))
                    .w(px(badge_width))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .items_end()
                    .overflow_hidden()
                    .child(
                        div()
                            .h(px(line_height(font)))
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(glyph_width(font)))
                            .overflow_hidden()
                            // Uncommitted work, marked the way the titlebar
                            // button marks it: the counts beside it are the
                            // pull request's, not the working tree's.
                            .when(teleported, |line| {
                                line.child(
                                    crate::icons::teleported(
                                        theme,
                                        (line_height(font) * 0.75).round().min(15.),
                                    )
                                    .debug_selector(|| format!("teleported-{key}")),
                                )
                            })
                            .when(dirty, |line| {
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
                    .when_some(pr.filter(|_| layout.pr_counts()), |column, badge| {
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
                    }),
            )
        })
}

fn agent_icon_size(font: &FontConfig) -> (f32, f32) {
    let size = line_height(font).min(12.);
    (size, size + 4.)
}

fn agent_mark(
    key: &str,
    icon: crate::icons::AgentIcon,
    size: f32,
    color: u32,
    font: &FontConfig,
) -> Div {
    div()
        .debug_selector(|| format!("agent-icon-{key}"))
        .absolute()
        .left_0()
        .top(px((line_height(font) - size) / 2.))
        .size(px(size))
        .child(svg().path(icon.path()).size_full().text_color(rgb(color)))
}

pub(crate) fn label_text(text: &str) -> impl IntoElement {
    shared_label_text(SharedString::from(text))
}

#[cfg(not(any(test, feature = "integration-test")))]
fn shared_label_text(text: SharedString) -> SharedString {
    text
}

#[cfg(any(test, feature = "integration-test"))]
fn shared_label_text(text: SharedString) -> layout_tests::ProbeText {
    layout_tests::ProbeText(text)
}

pub(super) fn first_text<'a>(
    values: impl IntoIterator<Item = Option<&'a str>>,
    fallback: &'a str,
) -> &'a str {
    values
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|s| !s.is_empty())
        .unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::{
        super::{agents::Indicators, cell::RowContext, layout},
        RowKind, TextRole, TokenKind, TokenLook, left_behind, token_appearance,
    };
    use crate::config::{FontConfig, LayoutMode, Theme};
    use herdr_client::protocol::AgentStatus;

    #[core::prelude::v1::test]
    fn teleported_rows_fade_only_their_workspace_tokens() {
        let font = FontConfig {
            family: "Menlo".into(),
            size: 12.,
            fallbacks: None,
        };
        let theme = Theme::default();
        let cx = RowContext {
            indicators: Indicators::new(None, false, &theme),
            font: &font,
            theme: &theme,
            look: layout::for_mode(LayoutMode::default()),
            width: 232.,
            host: None,
        };
        let here = TokenLook {
            kind: RowKind::Workspace,
            status: AgentStatus::Idle,
            focused: false,
            teleported: false,
        };
        let away = TokenLook {
            teleported: true,
            ..here
        };
        let workspace = TokenKind::Text("repo".into(), TextRole::Workspace);
        let (name, weight) = token_appearance(&workspace, here, &cx);
        assert_eq!(
            token_appearance(&workspace, away, &cx),
            (left_behind(name, &theme), weight)
        );
        assert_ne!(left_behind(name, &theme), name);
        for other in [
            TokenKind::Text("idle".into(), TextRole::Status),
            TokenKind::Text("main".into(), TextRole::Secondary),
            TokenKind::StateIcon,
        ] {
            assert_eq!(
                token_appearance(&other, away, &cx),
                token_appearance(&other, here, &cx)
            );
        }
    }
}
