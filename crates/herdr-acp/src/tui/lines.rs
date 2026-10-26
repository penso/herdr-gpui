//! Transcript entries as wrapped, styled terminal lines.

use herdr_chat_protocol::{
    Author, Entry, EntryKind, NoticeLevel, PlanEntry, PlanStatus, Tool, ToolContent, ToolStatus,
};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthChar;

/// Tool output lines shown under a call.
const TOOL_LINES: usize = 6;
/// Thought lines shown before the rest is folded.
const THOUGHT_LINES: usize = 4;

/// Wraps `text` to `width` columns, breaking at spaces where it can and
/// inside words where it must. Every input line yields at least one line.
pub(crate) fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    for line in text.split('\n') {
        let line = line.trim_end_matches('\r').replace('\t', "    ");
        let mut current = String::new();
        let mut used = 0;
        for token in tokens(&line) {
            let token_width: usize = token.chars().map(|ch| ch.width().unwrap_or(0)).sum();
            if used + token_width > width && used > 0 {
                out.push(std::mem::take(&mut current).trim_end().to_owned());
                used = 0;
                if token.starts_with(' ') {
                    continue;
                }
            }
            if token_width <= width {
                current.push_str(token);
                used += token_width;
                continue;
            }
            for ch in token.chars() {
                let ch_width = ch.width().unwrap_or(0);
                if used + ch_width > width && used > 0 {
                    out.push(std::mem::take(&mut current));
                    used = 0;
                }
                current.push(ch);
                used += ch_width;
            }
        }
        out.push(current);
    }
    out
}

/// Alternating runs of spaces and of everything else.
fn tokens(line: &str) -> impl Iterator<Item = &str> {
    let mut rest = line;
    std::iter::from_fn(move || {
        let first = rest.chars().next()?;
        let space = first == ' ';
        let end = rest
            .char_indices()
            .find(|(_, ch)| (*ch == ' ') != space)
            .map_or(rest.len(), |(index, _)| index);
        let (token, tail) = rest.split_at(end);
        rest = tail;
        Some(token)
    })
}

fn prefixed(
    lines: &mut Vec<Line<'static>>,
    first: &'static str,
    rest: &'static str,
    text: &str,
    width: usize,
    style: Style,
) {
    let indent = first.chars().count().max(rest.chars().count());
    for (index, line) in wrap(text, width.saturating_sub(indent))
        .into_iter()
        .enumerate()
    {
        let lead = if index == 0 { first } else { rest };
        lines.push(Line::from(vec![
            Span::styled(lead, style.add_modifier(Modifier::DIM)),
            Span::styled(line, style),
        ]));
    }
}

/// The lines for one entry, followed by a blank separator line.
pub(crate) fn entry(entry: &Entry, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    match &entry.kind {
        EntryKind::User { text, .. } => {
            let style = Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD);
            prefixed(&mut lines, "› ", "  ", text, width, style);
        }
        EntryKind::Agent { text } => agent_text(&mut lines, text, width),
        EntryKind::Thought { text } => {
            let style = Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC);
            let all = wrap(text.trim(), width.saturating_sub(2));
            let hidden = all.len().saturating_sub(THOUGHT_LINES);
            for (index, line) in all.into_iter().take(THOUGHT_LINES).enumerate() {
                let lead = if index == 0 { "∴ " } else { "  " };
                lines.push(Line::styled(format!("{lead}{line}"), style));
            }
            if hidden > 0 {
                lines.push(Line::styled(format!("  … {hidden} more lines"), style));
            }
        }
        // The terminal names a picture; attached clients draw it.
        EntryKind::Image { author, .. } => {
            let (lead, color) = match author {
                Author::User => ("› ", Color::Cyan),
                Author::Agent => ("  ", Color::DarkGray),
            };
            lines.push(Line::styled(
                format!("{lead}[image]"),
                Style::default().fg(color),
            ));
        }
        EntryKind::Tool(tool) => tool_lines(&mut lines, tool, width),
        EntryKind::Turn(summary) => {
            let text = summary.describe();
            let rule = width.saturating_sub(text.chars().count() + 4) / 2;
            lines.push(Line::styled(
                format!(
                    "{} {text} {}",
                    "─".repeat(rule.max(1)),
                    "─".repeat(rule.max(1))
                ),
                Style::default().fg(Color::DarkGray),
            ));
        }
        EntryKind::Notice { level, text, .. } => {
            let color = match level {
                NoticeLevel::Info => Color::Blue,
                NoticeLevel::Warning => Color::Yellow,
                NoticeLevel::Error => Color::Red,
            };
            prefixed(
                &mut lines,
                "! ",
                "  ",
                text,
                width,
                Style::default().fg(color),
            );
        }
    }
    lines.push(Line::default());
    lines
}

/// Agent text with fenced code set apart; the rest is shown as written.
fn agent_text(lines: &mut Vec<Line<'static>>, text: &str, width: usize) {
    let mut code = false;
    for raw in text.trim_end().split('\n') {
        if raw.trim_start().starts_with("```") {
            code = !code;
            lines.push(Line::styled(
                format!("  {}", "─".repeat(width.saturating_sub(2).min(40))),
                Style::default().fg(Color::DarkGray),
            ));
            continue;
        }
        let style = if code {
            Style::default().fg(Color::Green)
        } else if raw.starts_with('#') {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        for line in wrap(raw, width.saturating_sub(2)) {
            lines.push(Line::styled(format!("  {line}"), style));
        }
    }
}

fn tool_lines(lines: &mut Vec<Line<'static>>, tool: &Tool, width: usize) {
    let (mark, color) = match tool.status {
        ToolStatus::Pending => ("○", Color::DarkGray),
        ToolStatus::InProgress => ("◐", Color::Yellow),
        ToolStatus::Completed => ("✓", Color::Green),
        ToolStatus::Failed => ("✗", Color::Red),
    };
    let title = wrap(&tool.title, width.saturating_sub(2));
    for (index, line) in title.into_iter().enumerate() {
        let lead = if index == 0 { mark } else { " " };
        lines.push(Line::from(vec![
            Span::styled(format!("{lead} "), Style::default().fg(color)),
            Span::styled(line, Style::default().add_modifier(Modifier::BOLD)),
        ]));
    }
    let dim = Style::default().fg(Color::DarkGray);
    for location in tool.locations.iter().take(3) {
        lines.push(Line::styled(format!("  {location}"), dim));
    }
    for content in &tool.content {
        match content {
            ToolContent::Diff { new, old, path } => {
                let (added, removed) = diff_counts(old.as_deref().unwrap_or(""), new);
                lines.push(Line::from(vec![
                    Span::styled(format!("  {path} "), dim),
                    Span::styled(format!("+{added}"), Style::default().fg(Color::Green)),
                    Span::raw(" "),
                    Span::styled(format!("-{removed}"), Style::default().fg(Color::Red)),
                ]));
            }
            ToolContent::Text { text } => {
                let all = wrap(text.trim_end(), width.saturating_sub(4));
                let hidden = all.len().saturating_sub(TOOL_LINES);
                for line in all.into_iter().take(TOOL_LINES) {
                    lines.push(Line::styled(format!("  │ {line}"), dim));
                }
                if hidden > 0 {
                    lines.push(Line::styled(format!("  │ … {hidden} more lines"), dim));
                }
            }
            ToolContent::Image { .. } => lines.push(Line::styled("  │ [image]", dim)),
            ToolContent::Terminal { id } => {
                lines.push(Line::styled(format!("  terminal {id}"), dim));
            }
        }
    }
}

/// Lines added and removed, counting each distinct line as often as it occurs.
pub(crate) fn diff_counts(old: &str, new: &str) -> (usize, usize) {
    let mut counts = std::collections::HashMap::<&str, isize>::new();
    for line in old.lines() {
        *counts.entry(line).or_default() -= 1;
    }
    for line in new.lines() {
        *counts.entry(line).or_default() += 1;
    }
    counts.values().fold((0, 0), |(added, removed), count| {
        if *count > 0 {
            (added + count.unsigned_abs(), removed)
        } else {
            (added, removed + count.unsigned_abs())
        }
    })
}

pub(crate) fn plan(plan: &[PlanEntry], width: usize) -> Vec<Line<'static>> {
    plan.iter()
        .map(|entry| {
            let (mark, style) = match entry.status {
                PlanStatus::Completed => (
                    "☑",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::CROSSED_OUT),
                ),
                PlanStatus::InProgress => ("◐", Style::default().fg(Color::Yellow)),
                PlanStatus::Pending => ("☐", Style::default()),
            };
            let text = wrap(&entry.content, width.saturating_sub(2))
                .into_iter()
                .next()
                .unwrap_or_default();
            Line::styled(format!("{mark} {text}"), style)
        })
        .collect()
}

#[cfg(test)]
mod tests;
