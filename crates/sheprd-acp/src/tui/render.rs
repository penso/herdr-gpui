//! Draws the view.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use sheprd_chat_protocol::{OptionCategory, PermissionKind, Status, display};

use super::{lines, view::View};

const SPINNER: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];
const COMPOSER_ROWS: usize = 8;

pub(crate) fn draw(frame: &mut Frame<'_>, view: &mut View, tick: usize) {
    let area = frame.area();
    let width = usize::from(area.width);
    let plan = lines::plan(&view.transcript.plan, width);
    let plan_height = plan.len().min(6);
    let queued = view.transcript.queued.len().min(3);
    let (bottom_height, bottom) = if !view.transcript.permissions.is_empty() {
        permission_height(view, width)
    } else if view.settings.is_some() {
        (view.transcript.options.len() + 2, Bottom::Settings)
    } else {
        composer_height(view, width)
    };
    let completions = if bottom == Bottom::Composer {
        view.completions().len()
    } else {
        0
    };
    let [
        header,
        body,
        plan_area,
        queue_area,
        completion_area,
        bottom_area,
        footer,
    ] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(height(plan_height)),
        Constraint::Length(height(queued)),
        Constraint::Length(height(completions)),
        Constraint::Length(height(bottom_height)),
        Constraint::Length(1),
    ])
    .areas(area);
    frame.render_widget(Paragraph::new(header_line(view, tick)), header);
    draw_body(frame, view, body);
    frame.render_widget(
        Paragraph::new(plan.into_iter().take(plan_height).collect::<Vec<_>>()),
        plan_area,
    );
    let queue: Vec<Line<'_>> = view
        .transcript
        .queued
        .iter()
        .take(3)
        .map(|queued| {
            let first = queued.text.lines().next().unwrap_or_default();
            Line::styled(
                format!("↳ queued: {first}"),
                Style::default().fg(Color::DarkGray),
            )
        })
        .collect();
    frame.render_widget(Paragraph::new(queue), queue_area);
    draw_completions(frame, view, completion_area);
    match bottom {
        Bottom::Composer => draw_composer(frame, view, bottom_area),
        Bottom::Permission => draw_permission(frame, view, bottom_area),
        Bottom::Settings => draw_settings(frame, view, bottom_area),
    }
    let footer_line = match &view.flash {
        Some(flash) => Line::styled(flash.clone(), Style::default().fg(Color::Yellow)),
        None => {
            let hints: &[&str] = if bottom == Bottom::Settings {
                &[
                    "↑/↓ setting · ←/→ change · Esc close",
                    "←/→ change · Esc close",
                ]
            } else {
                &[
                    "Enter send · Alt+Enter newline · Esc cancel · Shift+Tab mode · Ctrl+O settings · Ctrl+D quit",
                    "Enter send · Esc cancel · Ctrl+O settings · Ctrl+D quit",
                    "Enter send · Ctrl+D quit",
                ]
            };
            let hint = hints
                .iter()
                .copied()
                .find(|hint| hint.chars().count() <= width)
                .unwrap_or_default();
            Line::styled(hint, Style::default().fg(Color::DarkGray))
        }
    };
    frame.render_widget(Paragraph::new(footer_line), footer);
}

fn height(rows: usize) -> u16 {
    u16::try_from(rows).unwrap_or(u16::MAX)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Bottom {
    Composer,
    Permission,
    Settings,
}

fn draw_completions(frame: &mut Frame<'_>, view: &View, area: Rect) {
    let rows: Vec<Line<'_>> = view
        .completions()
        .into_iter()
        .enumerate()
        .map(|(index, command)| {
            let style = if index == view.completion {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            Line::from(vec![
                Span::styled(format!(" /{} ", command.name), style.fg(Color::Cyan)),
                Span::styled(
                    format!(" {}", command.description),
                    Style::default().fg(Color::DarkGray),
                ),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(rows), area);
}

fn draw_settings(frame: &mut Frame<'_>, view: &View, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Settings ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let selected = view.settings.unwrap_or(0);
    let rows: Vec<Line<'_>> = view
        .transcript
        .options
        .iter()
        .enumerate()
        .map(|(index, option)| {
            let marker = if index == selected { "› " } else { "  " };
            let value = Style::default()
                .fg(Color::Cyan)
                .add_modifier(if index == selected {
                    Modifier::REVERSED
                } else {
                    Modifier::empty()
                });
            Line::from(vec![
                Span::raw(marker),
                Span::styled(format!("{}: ", option.name), Style::default()),
                Span::styled(format!(" ‹ {} › ", option.current_name()), value),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(rows), inner);
}

fn header_line(view: &View, tick: usize) -> Line<'static> {
    let transcript = &view.transcript;
    let (status, color) = match &transcript.status {
        _ if transcript.is_blocked() => ("waiting for you".to_owned(), Color::Magenta),
        Status::Starting => (
            format!("{} starting", SPINNER[tick % SPINNER.len()]),
            Color::Yellow,
        ),
        Status::Idle => ("ready".to_owned(), Color::Green),
        Status::Working => {
            let elapsed = transcript.turn.map(|turn| {
                display::duration(crate::session::now_ms().saturating_sub(turn.started_ms))
            });
            let elapsed = elapsed
                .filter(|elapsed| !elapsed.ends_with("ms"))
                .map_or_else(String::new, |elapsed| format!(" {elapsed}"));
            (
                format!("{} working{elapsed}", SPINNER[tick % SPINNER.len()]),
                Color::Yellow,
            )
        }
        Status::Failed { .. } => ("failed".to_owned(), Color::Red),
        Status::Exited { .. } => ("exited".to_owned(), Color::Red),
    };
    let dim = Style::default().fg(Color::DarkGray);
    let mut spans = vec![
        Span::styled(
            format!(" {} ", transcript.agent),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(status, Style::default().fg(color)),
    ];
    // The settings the agent offers come first; its older modes only when it
    // offers no mode setting.
    for category in [
        OptionCategory::Model,
        OptionCategory::ThoughtLevel,
        OptionCategory::Mode,
    ] {
        if let Some(option) = transcript.option(category) {
            spans.push(Span::styled(" · ", dim));
            spans.push(Span::raw(option.current_name()));
        }
    }
    if transcript.option(OptionCategory::Mode).is_none()
        && let Some(mode) = transcript.modes.as_ref().and_then(|modes| modes.current())
    {
        spans.push(Span::styled(" · ", dim));
        spans.push(Span::raw(mode.name.clone()));
    }
    if transcript.auto_accept {
        spans.push(Span::styled(" · ", dim));
        spans.push(Span::styled(
            "auto-accepting",
            Style::default().fg(Color::Yellow),
        ));
    }
    if let Some(title) = transcript.session.as_ref().and_then(|s| s.title.clone()) {
        spans.push(Span::styled(" · ", dim));
        spans.push(Span::styled(title, dim));
    }
    if let Some(usage) = transcript.usage.as_ref().filter(|usage| usage.size > 0) {
        let percent = usage.percent();
        let color = match percent {
            90.. => Color::Red,
            75..90 => Color::Yellow,
            _ => Color::DarkGray,
        };
        spans.push(Span::styled(" · ", dim));
        spans.push(Span::styled(
            format!(
                "{}/{} context ({percent}%)",
                display::tokens(usage.used),
                display::tokens(usage.size)
            ),
            Style::default().fg(color),
        ));
        if let Some(cost) = usage.cost.as_ref().filter(|cost| cost.micros > 0) {
            spans.push(Span::styled(format!(" · {}", display::cost(cost)), dim));
        }
    }
    if transcript.clients > 0 {
        let attached = match transcript.clients {
            1 => "1 client attached".to_owned(),
            count => format!("{count} clients attached"),
        };
        spans.push(Span::styled(" · ", dim));
        spans.push(Span::styled(attached, Style::default().fg(Color::Cyan)));
    }
    Line::from(spans)
}

fn draw_body(frame: &mut Frame<'_>, view: &mut View, area: Rect) {
    let lines = view.lines(usize::from(area.width));
    let rows = usize::from(area.height);
    let most = lines.len().saturating_sub(rows);
    view.scroll = view.scroll.min(most);
    let start = most - view.scroll;
    let visible: Vec<Line<'static>> = lines.into_iter().skip(start).take(rows).collect();
    frame.render_widget(Paragraph::new(visible), area);
}

fn composer_height(view: &View, width: usize) -> (usize, Bottom) {
    let (rows, _) = view.composer.layout(width.saturating_sub(2));
    (rows.len().clamp(1, COMPOSER_ROWS) + 2, Bottom::Composer)
}

fn draw_composer(frame: &mut Frame<'_>, view: &View, area: Rect) {
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    // Files the prompt carries name themselves on the composer's border.
    if !view.attachments.is_empty() {
        let names: Vec<&str> = view.attachments.iter().map(|file| file.name()).collect();
        block = block.title(Line::styled(
            format!(" \u{1f4ce} {} ", names.join(" \u{b7} ")),
            Style::default().fg(Color::Cyan),
        ));
    }
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if view.composer.text.is_empty() {
        let hint = match view.transcript.status {
            Status::Working => "Type to queue a prompt…".to_owned(),
            Status::Failed { .. } | Status::Exited { .. } => {
                "The agent is not running. Ctrl+D quits.".to_owned()
            }
            _ => format!("Ask {}…", view.transcript.agent),
        };
        frame.render_widget(
            Paragraph::new(Line::styled(hint, Style::default().fg(Color::DarkGray))),
            inner,
        );
        frame.set_cursor_position(Position::new(inner.x, inner.y));
        return;
    }
    let (rows, (row, column)) = view.composer.layout(usize::from(inner.width));
    let visible = usize::from(inner.height).max(1);
    let first = row.saturating_sub(visible - 1);
    let text: Vec<Line<'_>> = rows
        .into_iter()
        .skip(first)
        .take(visible)
        .map(Line::from)
        .collect();
    frame.render_widget(Paragraph::new(text), inner);
    let x = inner.x.saturating_add(height(column));
    let y = inner.y.saturating_add(height(row - first));
    frame.set_cursor_position(Position::new(x, y));
}

fn permission_height(view: &View, width: usize) -> (usize, Bottom) {
    let Some(permission) = view.transcript.permissions.first() else {
        return (0, Bottom::Permission);
    };
    let title = lines::wrap(&permission.title, width.saturating_sub(2))
        .len()
        .min(3);
    (title + permission.options.len() + 2, Bottom::Permission)
}

fn draw_permission(frame: &mut Frame<'_>, view: &View, area: Rect) {
    let Some(permission) = view.transcript.permissions.first() else {
        return;
    };
    let more = view.transcript.permissions.len() - 1;
    let title = if more > 0 {
        format!(" Permission (+{more} more) ")
    } else {
        " Permission ".to_owned()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Magenta));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut text: Vec<Line<'_>> = lines::wrap(&permission.title, usize::from(inner.width))
        .into_iter()
        .take(3)
        .map(|line| Line::styled(line, Style::default().add_modifier(Modifier::BOLD)))
        .collect();
    for (index, option) in permission.options.iter().enumerate() {
        let color = match option.kind {
            PermissionKind::AllowOnce | PermissionKind::AllowAlways => Color::Green,
            PermissionKind::RejectOnce | PermissionKind::RejectAlways => Color::Red,
            PermissionKind::Other => Color::Reset,
        };
        let mut style = Style::default().fg(color);
        if index == view.choice {
            style = style.add_modifier(Modifier::REVERSED);
        }
        text.push(Line::from(vec![
            Span::raw(if index == view.choice { "› " } else { "  " }),
            Span::styled(format!("{}. {}", index + 1, option.name), style),
        ]));
    }
    frame.render_widget(Paragraph::new(text), inner);
}

#[cfg(test)]
mod tests;
