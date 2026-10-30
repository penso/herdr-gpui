//! The least a row can show: its status and its name. Spacing, highlight, and
//! headings follow the configured density and style, so a narrow sidebar
//! keeps every workspace on one line. Rows Herdr's sidebar config defines
//! take as many lines as it names, beside the same status and marks.

use super::{
    super::{
        ARROW_RESERVE,
        cell::{AgentRow, RowContext, RowLayout, RowState, WorkspaceRow},
        line_height,
        row::{Elbow, RowKind, RowTree, TokenLook, guide, left_behind, row_text, token_column},
        tokens::ResolvedToken,
    },
    parts::{self, Line},
};
use crate::config::Theme;
use crate::direction::Logical;
use gpui::{prelude::*, *};

pub(in super::super) struct Minimal;

/// A child's guide gutter, as `(x, width)` from the sidebar's leading edge:
/// Herdr's own, from the parent's label column, ending at the child's status
/// dot, which these rows place without the wider indicators' extra indent.
fn gutter(cx: &RowContext<'_>) -> (f32, f32) {
    let (x, width) = super::super::row::tree_gutter(cx);
    let extra = cx.indicators.width(cx.font) - super::super::STATUS_WIDTH;
    (x, width - extra)
}

/// A row laid out by the density and style like Herdr's own: one centered
/// line, or `Some` count of configured lines whose pieces align to the first.
fn shell(
    key: &str,
    state: RowState,
    indent: f32,
    (line, lines): (Line<'_>, Option<usize>),
    cx: &RowContext<'_>,
) -> Div {
    let look = cx.look;
    div()
        .debug_selector(|| format!("row-{key}"))
        .relative()
        .w_full()
        .flex_none()
        .h(px(look.row_height(
            line_height(cx.font) * lines.unwrap_or(1) as f32,
        )))
        .ps(px(look.content_x() + indent))
        .row()
        .items_center()
        .cursor_pointer()
        .map(|row| look.mark(row, key, (state, cx.mark), cx.theme))
        .child(
            line.into_div()
                .when(lines.is_some(), |line| line.items_start()),
        )
}

/// Configured lines after the row's status slot, with the status the config
/// leads with, or none. `lead` places any pieces between them.
fn configured<'a>(
    key: &'a str,
    lines: &'a [Vec<ResolvedToken>],
    (look, removing): (TokenLook, bool),
    line: Line<'a>,
    lead: impl FnOnce(Line<'a>) -> Line<'a>,
    cx: &'a RowContext<'a>,
) -> Line<'a> {
    let line = match parts::configured_status(lines, look.status, removing, cx) {
        Some(mark) => line.fixed(cx.indicators.width(cx.font), mark),
        None => line,
    };
    lead(line).fill_with(move |width| token_column(key, lines, look, width, cx))
}

fn name(key: &str, kind: RowKind, state: RowState, teleported: bool, theme: &Theme) -> Div {
    let (color, weight, _) = row_text(kind, state.selected, theme);
    div()
        .debug_selector(|| format!("name-{key}"))
        .font_weight(weight)
        .text_color(rgb(if teleported {
            left_behind(color, theme)
        } else {
            color
        }))
}

impl RowLayout for Minimal {
    fn workspace(&self, row: WorkspaceRow<'_>, state: RowState, cx: &RowContext<'_>) -> Div {
        let theme = cx.theme;
        let density = cx.look.density;
        let indent = cx.nest
            + if row.tree == RowTree::None {
                0.
            } else {
                density.child_indent()
            };
        // Pull requests and uncommitted work stay off these rows, but a
        // teleported checkout is a copy left behind, so it keeps its mark,
        // as a muted workspace keeps its own.
        let teleported = row.badge.as_ref().is_some_and(|badge| badge.teleported);
        let muted = row.badge.as_ref().is_some_and(|badge| badge.muted);
        let mark = (line_height(cx.font) * 0.75).round().min(15.);
        let worktree = parts::worktree(row.label, &row.icon, mark);
        let status = row.status();
        let line = Line::new(cx.look.content_width(cx.width) - indent, density.gap());
        let line = if row.lines.is_empty() {
            line.fixed(
                cx.indicators.width(cx.font),
                parts::status(status, row.removing, cx),
            )
            .when_some(worktree, |line, glyph| line.fixed(mark, glyph))
            .fill(
                name(row.label, RowKind::Workspace, state, teleported, theme),
                row.label,
            )
        } else {
            configured(
                row.label,
                &row.lines,
                (
                    TokenLook {
                        kind: RowKind::Workspace,
                        status,
                        focused: state.selected,
                        teleported,
                    },
                    row.removing,
                ),
                line,
                |line| match worktree {
                    Some(glyph) => line.fixed(mark, parts::first_line(mark, glyph, cx)),
                    None => line,
                },
                cx,
            )
        };
        let line = line
            .when(muted, |line| {
                line.fixed(
                    mark,
                    parts::first_line(mark, parts::muted(row.label, mark, theme), cx),
                )
            })
            .when(teleported, |line| {
                line.fixed(
                    mark,
                    parts::first_line(mark, parts::teleported(row.label, mark, theme), cx),
                )
            })
            .when_some(row.fold, |line, fold| {
                let width = ARROW_RESERVE - density.gap();
                line.fixed(
                    width,
                    parts::first_line(
                        width,
                        fold.element(theme).w(px(width)).text_size(px(16.)),
                        cx,
                    ),
                )
            });
        let lines = (!row.lines.is_empty()).then_some(row.lines.len());
        // The first line sits where Herdr's rows put it, so the tick does too.
        let look = cx.look;
        let elbow = Elbow::At(look.row_padding() + look.spacing() / 2. + line_height(cx.font) / 2.);
        shell(row.label, state, indent, (line, lines), cx).children(guide(
            row.label,
            row.tree,
            gutter(cx),
            elbow,
            cx,
        ))
    }

    fn trunk_below(&self, key: &str, tree: RowTree, cx: &RowContext<'_>) -> Option<Div> {
        super::super::row::trunk_below(key, tree, gutter(cx), cx)
    }

    fn agent(&self, agent: AgentRow<'_>, state: RowState, cx: &RowContext<'_>) -> Div {
        let (theme, font) = (cx.theme, cx.font);
        let gap = cx.look.density.gap();
        let kind = RowKind::Agent(agent.icon);
        let (color, _, _) = row_text(kind, state.selected, theme);
        let icon = line_height(font).min(12.);
        let line = Line::new(cx.look.content_width(cx.width), gap);
        if !agent.lines.is_empty() {
            let icon = parts::first_line(icon, parts::icon(agent.icon.path(), icon, color), cx);
            let lines = agent.lines.len();
            let line = configured(
                &agent.key,
                &agent.lines,
                (
                    TokenLook {
                        kind,
                        status: agent.status,
                        focused: state.selected,
                        teleported: false,
                    },
                    false,
                ),
                line,
                |line| line.fixed(line_height(font).min(12.), icon),
                cx,
            );
            return shell(&agent.key, state, 0., (line, Some(lines)), cx);
        }
        let line = line
            .fixed(
                cx.indicators.width(font),
                parts::status(agent.status, false, cx),
            )
            .fixed(icon, parts::icon(agent.icon.path(), icon, color))
            .lead(
                name(&agent.key, kind, state, false, theme),
                agent.name,
                parts::glyph_at(font, font.size),
                0.4,
            )
            .when_some(agent.pane, |line, pane| {
                line.label(
                    div()
                        .debug_selector(|| format!("pane-{}", agent.key))
                        .text_color(rgb(theme.muted)),
                    pane,
                    parts::glyph_at(font, font.size),
                    0.5,
                )
            })
            .when_some(agent.status_text.as_deref(), |line, text| {
                line.keep(
                    div()
                        .debug_selector(|| format!("status-{}", agent.key))
                        .text_color(rgb(cx.indicators.color(agent.status))),
                    text,
                    parts::glyph_at(font, font.size),
                    0.3,
                )
            });
        shell(&agent.key, state, 0., (line, None), cx)
    }
}
