//! One line of agent prose as styled spans. Markdown's inline marks
//! (`code`, **strong**, [links](url)) are drawn rather than shown, and a
//! line's block mark (heading, bullet, number, quote) becomes its lead. A
//! mark left unclosed stays as written, so plain text is never eaten.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthChar;

/// Columns every prose line is indented by, under the entry's lead column.
const INDENT: &str = "  ";

/// `raw`, one line of agent text, wrapped to `width` columns.
pub(super) fn prose(lines: &mut Vec<Line<'static>>, raw: &str, width: usize) {
    let raw = raw.trim_end_matches('\r').replace('\t', "    ");
    let block = Block::of(&raw);
    let lead_width = text_width(&block.lead);
    let room = width.saturating_sub(INDENT.len() + lead_width).max(1);
    let mut styled = Vec::new();
    inline(block.body, block.style, &mut styled);
    for (index, row) in wrap(&styled, room).into_iter().enumerate() {
        let mut spans = vec![Span::raw(INDENT)];
        if index == 0 {
            spans.push(Span::styled(block.lead.clone(), block.lead_style));
        } else if lead_width > 0 {
            spans.push(Span::raw(" ".repeat(lead_width)));
        }
        spans.extend(runs(row));
        lines.push(Line::from(spans));
    }
}

/// What a line's leading block mark makes of it.
struct Block<'a> {
    body: &'a str,
    lead: String,
    lead_style: Style,
    style: Style,
}

impl<'a> Block<'a> {
    fn of(raw: &'a str) -> Self {
        let muted = Style::default().fg(Color::DarkGray);
        let plain = |body| Self {
            body,
            lead: String::new(),
            lead_style: Style::default(),
            style: Style::default(),
        };
        let trimmed = raw.trim_start();
        let indent = " ".repeat(raw.len() - trimmed.len());
        if let Some(body) = heading(trimmed) {
            return Self {
                style: Style::default().add_modifier(Modifier::BOLD),
                ..plain(body)
            };
        }
        if let Some(body) = ["- ", "* ", "+ "]
            .into_iter()
            .find_map(|mark| trimmed.strip_prefix(mark))
        {
            return Self {
                lead: format!("{indent}• "),
                lead_style: muted,
                ..plain(body)
            };
        }
        if let Some((number, body)) = numbered(trimmed) {
            return Self {
                lead: format!("{indent}{number} "),
                lead_style: muted,
                ..plain(body)
            };
        }
        if let Some(body) = trimmed
            .strip_prefix("> ")
            .or_else(|| (trimmed == ">").then_some(""))
        {
            return Self {
                lead: "│ ".to_owned(),
                lead_style: muted,
                style: muted.add_modifier(Modifier::ITALIC),
                ..plain(body)
            };
        }
        plain(raw)
    }
}

/// The text of an ATX heading: one to six `#` and a space.
fn heading(line: &str) -> Option<&str> {
    let marks = line.chars().take_while(|ch| *ch == '#').count();
    (1..=6)
        .contains(&marks)
        .then(|| line[marks..].strip_prefix(' '))
        .flatten()
}

/// An ordered-list item's number with its mark, and the rest.
fn numbered(line: &str) -> Option<(&str, &str)> {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || digits > 3 {
        return None;
    }
    let mark = line[digits..].chars().next()?;
    if !matches!(mark, '.' | ')') {
        return None;
    }
    let body = line[digits + 1..].strip_prefix(' ')?;
    Some((&line[..=digits], body))
}

/// Appends `text`'s characters to `out`, each with the style its inline
/// marks give it over `base`.
fn inline(text: &str, base: Style, out: &mut Vec<(char, Style)>) {
    let code = base.fg(Color::Cyan);
    let mut rest = text;
    while let Some(ch) = rest.chars().next() {
        if ch == '`'
            && let Some(end) = rest[1..].find('`').filter(|end| *end > 0)
        {
            out.extend(rest[1..=end].chars().map(|ch| (ch, code)));
            rest = &rest[end + 2..];
            continue;
        }
        if let Some(inner) = rest.strip_prefix("**")
            && let Some(end) = inner.find("**").filter(|end| *end > 0)
        {
            inline(&inner[..end], base.add_modifier(Modifier::BOLD), out);
            rest = &inner[end + 2..];
            continue;
        }
        if ch == '['
            && let Some((label, target, tail)) = link(rest)
        {
            inline(label, base.add_modifier(Modifier::UNDERLINED), out);
            // The terminal cannot open it, so the address stays readable.
            if !target.is_empty() && target != label {
                let quiet = base.fg(Color::DarkGray);
                out.extend(format!(" ({target})").chars().map(|ch| (ch, quiet)));
            }
            rest = tail;
            continue;
        }
        out.push((ch, base));
        rest = &rest[ch.len_utf8()..];
    }
}

/// A `[label](target)` at the start of `text`: its label, its target, and
/// what follows.
fn link(text: &str) -> Option<(&str, &str, &str)> {
    let close = text.find("](")?;
    let label = &text[1..close];
    if label.is_empty() || label.contains('[') {
        return None;
    }
    let after = &text[close + 2..];
    let end = after.find(')')?;
    Some((label, after[..end].trim(), &after[end + 1..]))
}

/// Breaks styled characters into rows of at most `width` columns, at
/// spaces where it can and inside words where it must, as [`super::wrap`]
/// does for plain text. Every input yields at least one row.
fn wrap(chars: &[(char, Style)], width: usize) -> Vec<Vec<(char, Style)>> {
    let mut rows = Vec::new();
    let mut row: Vec<(char, Style)> = Vec::new();
    let mut used = 0;
    for token in chars.chunk_by(|a, b| (a.0 == ' ') == (b.0 == ' ')) {
        let token_width: usize = token.iter().map(|(ch, _)| char_width(*ch)).sum();
        let space = token.first().is_some_and(|(ch, _)| *ch == ' ');
        if used + token_width > width && used > 0 {
            while row.last().is_some_and(|(ch, _)| *ch == ' ') {
                row.pop();
            }
            rows.push(std::mem::take(&mut row));
            used = 0;
            if space {
                continue;
            }
        }
        if token_width <= width {
            row.extend_from_slice(token);
            used += token_width;
            continue;
        }
        for &(ch, style) in token {
            let ch_width = char_width(ch);
            if used + ch_width > width && used > 0 {
                rows.push(std::mem::take(&mut row));
                used = 0;
            }
            row.push((ch, style));
            used += ch_width;
        }
    }
    rows.push(row);
    rows
}

/// A row's characters as spans, one per run of a single style.
fn runs(row: Vec<(char, Style)>) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (ch, style) in row {
        match spans.last_mut() {
            Some(span) if span.style == style => span.content.to_mut().push(ch),
            _ => spans.push(Span::styled(ch.to_string(), style)),
        }
    }
    spans
}

fn char_width(ch: char) -> usize {
    ch.width().unwrap_or(0)
}

fn text_width(text: &str) -> usize {
    text.chars().map(char_width).sum()
}

#[cfg(test)]
mod tests;
