//! One sidebar row: its icon, tree guides, badges, and the label budget that
//! decides what still fits. Text is elided against measured glyph widths, not
//! guessed, so a long label cannot overflow the row it was laid out in.

#[cfg(any(test, feature = "integration-test"))]
use super::layout_tests;
use super::{
    ARROW_RESERVE, ICON_RESERVE, STATUS_WIDTH, glyph_width,
    layout::{SidebarDensity, SidebarLook},
    line_height, segment_budgets, status_indicator,
};
use crate::config::{FontConfig, Theme};
use gpui::{prelude::*, *};
use herdr_client::protocol::AgentStatus;
use std::sync::Arc;

/// What a row shows in its leading icon slot: a repository owner's avatar when
/// one is cached, the GitHub mark while it is not, and nothing for the child
/// rows that reserve no slot at all.
pub(super) enum RowIcon {
    None,
    Mark,
    Avatar(Arc<Image>),
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

/// Where a row stands while a workspace is dragged: rows the lifted card
/// passes over stop answering hover, so only the drop line marks a place.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RowLift {
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
    pr: Option<PrBadge>,
    dirty: bool,
}

impl RowBadge {
    /// Nothing to draw is nothing to reserve, so a row with neither keeps its
    /// full label width.
    pub(super) fn new(pr: Option<PrBadge>, dirty: bool) -> Option<Self> {
        (pr.is_some() || dirty).then_some(Self { pr, dirty })
    }

    pub(super) fn pr(&self) -> Option<&PrBadge> {
        self.pr.as_ref()
    }

    pub(super) fn dirty(&self) -> bool {
        self.dirty
    }

    pub(super) fn width(&self, font: &FontConfig, layout: &dyn SidebarDensity) -> f32 {
        let pr = self.pr.as_ref().map_or(0., |pr| pr.width(font, layout));
        // Reserve the icon and the gap before the PR number, even at small fonts.
        pr + if self.dirty {
            line_height(font).min(18.) + glyph_width(font)
        } else {
            0.
        }
    }
}

/// Cached pull request state for a worktree row: the number carries the
/// lifecycle/readiness color, the counts sit under it.
pub(super) struct PrBadge {
    number: String,
    color: u32,
    additions: String,
    deletions: String,
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

    /// The number as it prints, `#` included.
    pub(super) fn number(&self) -> &str {
        &self.number
    }

    pub(super) fn color(&self) -> u32 {
        self.color
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
    let lengths: Vec<usize> = segments
        .iter()
        .map(|(text, _)| text.chars().count())
        .collect();
    let available = (width - separators as f32 * separator).max(0.);
    let budgets = segment_budgets(&lengths, (available / glyph).floor() as usize);
    // The last segment takes the rounding remainder, so one segment fills the
    // line exactly as it did before a line could carry several.
    let used: f32 = budgets.iter().map(|budget| *budget as f32 * glyph).sum();
    let slack = (available - used).max(0.);
    let mut line = div()
        .relative()
        .w(px(width))
        .h(px(line_height(font)))
        .flex_none()
        .overflow_hidden();
    let mut x = 0.;
    for (index, ((text, is_primary), budget)) in segments.iter().zip(budgets).enumerate() {
        if index > 0 {
            line = line.child(
                div()
                    .absolute()
                    .left(px(x))
                    .w(px(separator))
                    .text_color(rgb(muted))
                    .child(label_text(" \u{b7} ")),
            );
            x += separator;
        }
        let last = index + 1 == segments.len();
        let segment = budget as f32 * glyph + if last { slack } else { 0. };
        line = line.child(
            div()
                .absolute()
                .left(px(x))
                .w(px(segment))
                .truncate()
                .when(*is_primary, |part| {
                    part.font_weight(weight).text_color(rgb(primary))
                })
                .when(!*is_primary, |part| part.text_color(rgb(muted)))
                .child(label_text(text)),
        );
        x += segment;
    }
    line
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
    removing: bool,
    focused: bool,
    lift: RowLift,
    tree: RowTree,
    reserve_arrow: bool,
    width: f32,
    workspace_icon: RowIcon,
    arrow: Option<Stateful<Div>>,
    badge: Option<RowBadge>,
    look: SidebarLook,
    appearance: (&FontConfig, &Theme),
) -> Div {
    let (font, theme) = appearance;
    let layout = look.density;
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
    let icon_reserve = match workspace_icon {
        RowIcon::None => 0.,
        _ => ICON_RESERVE,
    };
    let muted = theme.muted;
    let indent = if tree == RowTree::None {
        0.
    } else {
        layout.child_indent()
    };
    let arrow_reserve = if reserve_arrow { ARROW_RESERVE } else { 0. };
    let arrow_absent = arrow.is_none();
    let available =
        (look.content_width(width) - STATUS_WIDTH - gap - indent - arrow_reserve).max(0.);
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
    let label_width = (available - pr_reserve).max(0.);
    let agent_icon = match kind {
        RowKind::Agent(icon) => Some(icon),
        RowKind::Workspace => None,
    };
    // An orphan's name is on the first line; normal rows name the agent below
    // its location. Reserve the same fixed icon + gap on whichever line owns it.
    let agent_first = agent_icon.filter(|_| detail.is_empty());
    let agent_detail = agent_icon.filter(|_| !detail.is_empty());
    let agent_size = line_height(font).min(12.);
    let agent_reserve = agent_size + 4.;
    let name_reserve = icon_reserve + agent_first.map_or(0., |_| agent_reserve);
    div()
        .debug_selector(|| format!("row-{key}"))
        .h(px(look.row_height(
            line_height(font) * if show_detail { 2. } else { 1. },
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
        .map(|row| match lift {
            RowLift::Resting => look.hover_group(row),
            RowLift::Passed | RowLift::Lifted => row,
        })
        .child(if lift == RowLift::Lifted {
            look.lifted(key, focused, theme)
        } else {
            look.highlight(key, focused, theme)
        })
        // Tree lines run in the indent the row already reserves, so a child is
        // tied to its parent without box-drawing glyphs in the label.
        .when(tree != RowTree::None && look.style.tree_lines(), |row| {
            let (color, font) = (theme.muted, font.clone());
            let gutter = look.tree_gutter();
            row.child(
                div()
                    .debug_selector(|| format!("tree-{key}"))
                    .absolute()
                    // Between the parent's label column and this row's own dot.
                    .left(px(gutter))
                    .w(px(padding + indent - layout.tree_gutter()))
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
        .child(if removing {
            div()
                .debug_selector(|| "worktree-removing".into())
                .size(px(STATUS_WIDTH))
                .mt(px((line_height(font) - STATUS_WIDTH) / 2.))
                .flex_none()
                .child(
                    div()
                        .size_full()
                        .rounded_full()
                        .bg(rgb(theme.primary()))
                        .with_animation(
                            "worktree-removing-pulse",
                            Animation::new(std::time::Duration::from_secs(1)).repeat(),
                            |dot, delta| {
                                dot.opacity(0.3 + 0.7 * (delta * std::f32::consts::PI).sin())
                            },
                        ),
                )
        } else {
            status_indicator(status, font)
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
                .child(
                    div()
                        .relative()
                        .w(px(label_width))
                        .h(px(line_height(font)))
                        .when_some(agent_first, |line, icon| {
                            line.child(agent_mark(key, icon, agent_size, name_color, font))
                        })
                        .when(!matches!(workspace_icon, RowIcon::None), |title| {
                            title.child(
                                div()
                                    .debug_selector(|| format!("github-{key}"))
                                    .absolute()
                                    .left_0()
                                    .top(px((line_height(font) - 12.) / 2.))
                                    .size(px(12.))
                                    .flex_none()
                                    .overflow_hidden()
                                    .child(match workspace_icon {
                                        RowIcon::Avatar(image) => img(image)
                                            .size_full()
                                            .rounded_full()
                                            .with_fallback(move || {
                                                github_mark(muted).size_full().into_any_element()
                                            })
                                            .with_loading(move || {
                                                github_mark(muted).size_full().into_any_element()
                                            })
                                            .into_any_element(),
                                        _ => github_mark(muted).size_full().into_any_element(),
                                    }),
                            )
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
                    column.child(
                        div()
                            .relative()
                            .w(px(label_width))
                            .h(px(line_height(font)))
                            .when_some(agent_detail, |line, icon| {
                                line.child(agent_mark(key, icon, agent_size, detail_color, font))
                            })
                            .child(
                                div()
                                    .debug_selector(|| format!("detail-{key}"))
                                    .ml(px(if agent_detail.is_some() {
                                        agent_reserve.min(label_width)
                                    } else {
                                        0.
                                    }))
                                    .w(px((label_width
                                        - agent_detail.map_or(0., |_| agent_reserve))
                                    .max(0.)))
                                    .truncate()
                                    .text_color(rgb(detail_color))
                                    .child(label_text(detail)),
                            ),
                    )
                }),
        )
        // The collapse column comes first so the badge can hug the row's edge;
        // a reserved-but-empty column keeps every badge on the same right edge.
        .when_some(arrow, |row, arrow| row.child(arrow))
        .when(arrow_absent && reserve_arrow, |row| {
            row.child(div().w(px(ARROW_RESERVE - gap)).flex_none())
        })
        .when_some(badge, |row, badge| {
            let RowBadge { pr, dirty } = badge;
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
                                        .text_color(rgb(theme.palette[2]))
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
                                        .text_color(rgb(theme.palette[1]))
                                        .child(label_text(&badge.deletions)),
                                ),
                        )
                    }),
            )
        })
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

#[cfg(not(any(test, feature = "integration-test")))]
pub(crate) fn label_text(text: &str) -> SharedString {
    text.to_owned().into()
}

#[cfg(any(test, feature = "integration-test"))]
pub(crate) fn label_text(text: &str) -> layout_tests::ProbeText {
    layout_tests::ProbeText(text.to_owned().into())
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
