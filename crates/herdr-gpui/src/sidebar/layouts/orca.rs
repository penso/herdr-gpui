//! Orca's worktree cards: a status column beside the name, a quieter meta line
//! with the host, branch, and pull request, and compact single-line agents.
//! Cards are inset and rounded; the focused one is washed and outlined.
//! Rows Herdr's sidebar config defines fill the card with its lines instead,
//! beside the status column only when those lines lead with the status.

use super::{
    super::{
        cell::{AgentRow, RowContext, RowLayout, RowState, WorkspaceRow},
        line_height,
        row::{Elbow, RowBadge, RowKind, RowTree, TokenLook, guide, left_behind, token_column},
        tokens::ResolvedToken,
        wash::HostMark,
    },
    parts::{self, Line, glyph_at, wash},
};
use crate::config::Theme;
use crate::direction::Logical;
use crate::i18n::fl;
use gpui::{prelude::*, *};
use herdr_client::protocol::AgentStatus;

pub(in super::super) struct Orca;

/// Space between a card and the sidebar's edges.
const MARGIN: f32 = 4.;
const BORDER: f32 = 1.;
/// The status column, wider than its dot so the dot has air on both sides.
const STATUS_COLUMN: f32 = 12.;
const GAP: f32 = 6.;

/// The card or agent strip: inset, rounded, and marked by state. The border
/// is always present, transparent unless selected, so selecting a row never
/// shifts its content, and it keeps neighbouring cards apart without vertical
/// margins, which a dragged row's shifts could not see.
fn card(
    key: &str,
    state: RowState,
    indent: f32,
    radius: f32,
    (theme, mark): (&Theme, HostMark),
) -> Div {
    let card = div()
        .debug_selector(|| format!("row-{key}"))
        .relative()
        .flex_none()
        .ms(px(MARGIN + indent))
        .me(px(MARGIN))
        .rounded(px(radius))
        .border_1()
        .border_color(rgba(0))
        .when(state.selected, |card| {
            card.border_color(wash(theme.foreground, 0x2e))
        })
        .row()
        .gap(px(GAP))
        .px(px(GAP))
        .cursor_pointer();
    let card = mark.fill(card, key, radius);
    parts::mark(
        card,
        state,
        (wash(theme.foreground, 0x1a), wash(theme.foreground, 0x0a)),
        theme,
    )
}

/// A child's guide gutter, as `(x, width)` from the sidebar's leading edge:
/// the trunk drops from the leading edge of the parent's status mark, and the
/// tick spans the indent to the child's mark, as the tree's tick meets its
/// dot. The mark sits centred in its status column.
fn gutter(cx: &RowContext<'_>) -> (f32, f32) {
    let mark = (status_width(cx) - cx.indicators.width(cx.font)) / 2.;
    (
        MARGIN + cx.nest + BORDER + GAP + mark,
        cx.look.density.card_indent(),
    )
}

/// A child card's guide, drawn from the card itself: it lies in the indent
/// to the card's left, and spans the card's border box so the trunk runs
/// unbroken from card to card. `padding` is the card's top padding.
fn card_guide(
    key: &str,
    tree: RowTree,
    indent: f32,
    padding: f32,
    cx: &RowContext<'_>,
) -> Option<Div> {
    let (x, width) = gutter(cx);
    let line = line_height(cx.font);
    guide(
        key,
        tree,
        (x - MARGIN - indent - BORDER, width),
        Elbow::At(BORDER + padding + line / 2.),
        cx,
    )
    .map(|guide| guide.top(px(-BORDER)).bottom(px(-BORDER)))
}

/// Width inside a card's margins, border, and padding, less its status
/// column and the gap after it.
fn inner(cx: &RowContext<'_>, indent: f32) -> f32 {
    cx.width - 1. - 2. * MARGIN - indent - 2. * BORDER - 2. * GAP - status_width(cx) - GAP
}

fn status_width(cx: &RowContext<'_>) -> f32 {
    STATUS_COLUMN.max(cx.indicators.width(cx.font))
}

/// A configured card's status column, offset onto its first line, and the
/// room its absence returns to the text.
fn configured_status(
    lines: &[Vec<ResolvedToken>],
    status: AgentStatus,
    removing: bool,
    cx: &RowContext<'_>,
) -> (Option<Div>, f32) {
    match parts::configured_status(lines, status, removing, cx) {
        Some(mark) => (
            Some(
                status_column(mark, cx)
                    .when(cx.indicators.centered(), |column| column.self_center()),
            ),
            0.,
        ),
        None => (None, status_width(cx) + GAP),
    }
}

fn status_column(child: impl IntoElement, cx: &RowContext<'_>) -> Div {
    div()
        .w(px(status_width(cx)))
        .flex_none()
        .flex()
        .justify_center()
        .child(child)
}

/// A card being removed fades to this while its worktree goes.
const REMOVING_OPACITY: f32 = 0.5;

impl RowLayout for Orca {
    fn workspace(&self, row: WorkspaceRow<'_>, state: RowState, cx: &RowContext<'_>) -> Div {
        // Its marks fade with it, though the moving ones draw outside it.
        let faded = row.removing.then(|| RowContext {
            indicators: cx.indicators.faded(REMOVING_OPACITY),
            ..*cx
        });
        let cx = faded.as_ref().unwrap_or(cx);
        let (branch, status, upstream) = (row.branch(), row.status(), row.upstream());
        let WorkspaceRow {
            label,
            tree,
            icon,
            fold,
            grouped,
            badge,
            removing,
            lines,
            ..
        } = row;
        let (theme, font) = (cx.theme, cx.font);
        let line = line_height(font);
        let small = (font.size * 0.85).round();
        let glyph = glyph_at(font, small);
        let indent = cx.nest
            + if tree == RowTree::None {
                0.
            } else {
                cx.look.density.card_indent()
            };
        let width = inner(cx, indent);
        // Children are named by their branch already; repeating it is noise.
        let branch = branch.filter(|branch| *branch != label);
        let pr = badge.as_ref().and_then(|badge| badge.pr.as_ref());
        let dirty = badge.as_ref().is_some_and(|badge| badge.dirty);
        let teleported = badge.as_ref().is_some_and(|badge| badge.teleported);
        let muted = badge.as_ref().is_some_and(|badge| badge.muted);
        let dirty_size = (line * 0.7).round().min(14.);
        let worktree = parts::worktree(label, &icon, dirty_size);
        let meta = branch.is_some()
            || cx.host.is_some()
            || badge.as_ref().is_some_and(RowBadge::checkout_marks)
            || upstream.is_some();
        // The repository's own checkout is the group's primary card.
        let primary = (grouped && tree == RowTree::None).then(|| {
            div()
                .flex()
                .justify_center()
                .rounded(px(4.))
                .border_1()
                .border_color(wash(theme.foreground, 0x33))
                .bg(wash(theme.foreground, 0x0f))
                .text_size(px(small))
                .text_color(rgb(theme.subtext()))
                .child(super::super::label_text(&fl!("sidebar-primary")))
        });
        if !lines.is_empty() {
            let (mark, room) = configured_status(&lines, status, removing, cx);
            let text = Line::new(width + room, GAP)
                .when_some(worktree, |text, glyph| {
                    text.fixed(dirty_size, parts::first_line(dirty_size, glyph, cx))
                })
                .fill_with(|width| {
                    token_column(
                        label,
                        &lines,
                        TokenLook {
                            kind: RowKind::Workspace,
                            status,
                            focused: state.selected,
                            teleported,
                        },
                        width,
                        cx,
                    )
                })
                .when(muted, |text| {
                    let mark = parts::muted(label, dirty_size, theme);
                    text.fixed(dirty_size, parts::first_line(dirty_size, mark, cx))
                })
                .when_some(primary, |text, primary| {
                    let width = (7. * glyph).ceil() + 10.;
                    text.fixed(width, parts::first_line(width, primary.w(px(width)), cx))
                })
                .when(teleported, |text| {
                    let mark = parts::teleported(label, dirty_size, theme);
                    text.fixed(dirty_size, parts::first_line(dirty_size, mark, cx))
                })
                .when(dirty, |text| {
                    let mark = parts::dirty(label, dirty_size, theme);
                    text.fixed(dirty_size, parts::first_line(dirty_size, mark, cx))
                })
                .when_some(pr, |text, pr| {
                    let (width, number) = parts::pr_number(label, pr, small, glyph);
                    text.shrink(width, number.h(px(line)).text_size(px(small)))
                })
                .when_some(fold, |text, fold| {
                    let arrow = fold.element(theme).w(px(14.)).text_size(px(14.));
                    text.fixed(14., parts::first_line(14., arrow, cx))
                });
            return card(label, state, indent, 8., (theme, cx.mark))
                .items_start()
                .py(px(5.))
                .when(removing, |card| card.opacity(REMOVING_OPACITY))
                .children(card_guide(label, tree, indent, 5., cx))
                .children(mark)
                .child(text.into_div().items_start());
        }
        let title = Line::new(width, GAP)
            .when_some(worktree, |line, glyph| line.fixed(dirty_size, glyph))
            .fill(
                div()
                    .debug_selector(|| format!("name-{label}"))
                    .h(px(line))
                    .text_color(rgb(if teleported {
                        left_behind(theme.foreground, theme)
                    } else {
                        theme.foreground
                    }))
                    .when(state.selected, |title| {
                        title.font_weight(FontWeight::SEMIBOLD)
                    }),
                label,
            )
            .when(muted, |line| {
                line.fixed(dirty_size, parts::muted(label, dirty_size, theme))
            })
            .when_some(primary, |line, primary| {
                let width = (7. * glyph).ceil() + 10.;
                line.fixed(width, primary.w(px(width)))
            })
            .when_some(fold, |line, fold| {
                line.fixed(14., fold.element(theme).w(px(14.)).text_size(px(14.)))
            });
        let details = Line::new(width, GAP)
            .when_some(cx.host, |line, host| {
                let chip = div().rounded(px(4.)).bg(rgb(theme.active));
                line.padded(chip, host, glyph, 1. / 3., 5.)
            })
            .map(|line| match branch {
                Some(branch) => {
                    line.fill(div().debug_selector(|| format!("detail-{label}")), branch)
                }
                None => line.spacer(),
            })
            .when_some(upstream, |line, upstream| {
                line.fixed(upstream.width(glyph), upstream.element(label, glyph, theme))
            })
            .when(teleported, |line| {
                line.fixed(dirty_size, parts::teleported(label, dirty_size, theme))
            })
            .when(dirty, |line| {
                line.fixed(dirty_size, parts::dirty(label, dirty_size, theme))
            })
            .when_some(pr, |line, pr| {
                let (width, number) = parts::pr_number(label, pr, small, glyph);
                line.shrink(width, number)
            });
        let padding = if meta { 5. } else { 7. };
        card(label, state, indent, 8., (theme, cx.mark))
            .items_start()
            .py(px(padding))
            .when(removing, |card| card.opacity(REMOVING_OPACITY))
            .children(card_guide(label, tree, indent, padding, cx))
            .child({
                let column = status_column(parts::status(status, removing, cx), cx);
                if cx.indicators.centered() {
                    column.self_center()
                } else {
                    column.h(px(line)).items_center()
                }
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(title.into_div().h(px(line)))
                    .when(meta, |column| {
                        column.child(
                            details
                                .into_div()
                                .h(px(line))
                                .text_size(px(small))
                                .text_color(rgb(theme.muted)),
                        )
                    }),
            )
    }

    fn trunk_below(&self, key: &str, tree: RowTree, cx: &RowContext<'_>) -> Option<Div> {
        super::super::row::trunk_below(key, tree, gutter(cx), cx)
    }

    fn agent(&self, agent: AgentRow<'_>, state: RowState, cx: &RowContext<'_>) -> Div {
        let (theme, font) = (cx.theme, cx.font);
        let key = agent.key.as_str();
        let line = line_height(font);
        let icon = (line * 0.85).round();
        let color = if state.selected {
            theme.foreground
        } else {
            theme.subtext()
        };
        if !agent.lines.is_empty() {
            let (mark, room) = configured_status(&agent.lines, agent.status, false, cx);
            let column = token_column(
                key,
                &agent.lines,
                TokenLook {
                    kind: RowKind::Agent(agent.icon),
                    status: agent.status,
                    focused: state.selected,
                    teleported: false,
                },
                inner(cx, 0.) - icon - GAP + room,
                cx,
            );
            let icon = parts::first_line(icon, parts::icon(agent.icon.path(), icon, color), cx);
            return card(key, state, 0., 4., (theme, cx.mark))
                .py(px(4.))
                .items_start()
                .children(mark)
                .child(icon)
                .child(column);
        }
        // The agent leads, then its pane, then where it runs, all on one line.
        let text = Line::new(inner(cx, 0.) - icon - GAP, GAP)
            .lead(
                div()
                    .debug_selector(|| format!("name-{key}"))
                    .text_color(rgb(color)),
                agent.name,
                glyph_at(font, font.size),
                0.4,
            )
            .when_some(agent.pane, |line, pane| {
                line.label(
                    div()
                        .debug_selector(|| format!("pane-{key}"))
                        .text_color(rgb(theme.muted)),
                    pane,
                    glyph_at(font, font.size),
                    0.5,
                )
            })
            .when_some(parts::agent_place(agent.place, cx.host), |line, place| {
                line.label(
                    div().text_color(rgb(theme.muted)),
                    place,
                    glyph_at(font, font.size),
                    0.5,
                )
            })
            .when_some(agent.status_text.as_deref(), |line, text| {
                line.keep(
                    div()
                        .debug_selector(|| format!("status-{key}"))
                        .text_color(rgb(cx.indicators.color(agent.status))),
                    text,
                    glyph_at(font, font.size),
                    0.3,
                )
            });
        card(key, state, 0., 4., (theme, cx.mark))
            .h(px(line + 8.))
            .items_center()
            .child(status_column(parts::status(agent.status, false, cx), cx))
            .child(parts::icon(agent.icon.path(), icon, color))
            .child(text.into_div())
    }
}
