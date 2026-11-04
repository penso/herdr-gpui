//! Agents that compact their context, such as OpenCode, write the summary
//! into the conversation as an answer: a run of fixed sections (Objective,
//! Work State, Next Move, ...) that mostly read `(none)`, repeated every
//! time they compact. The view folds such an answer into one card; this
//! recognizes it and keeps only the sections that say something.

use crate::release_notes::{Kind, Line};

// They match what an agent writes, in English.
/// Section titles compaction summaries use; an answer with
/// [`MIN_SECTIONS`] of them as headings is taken for one.
const SECTIONS: &[&str] = &[
    "active",
    "blocked",
    "completed",
    "important details", // i18n: data
    "next move",         // i18n: data
    "next steps",        // i18n: data
    "objective",
    "relevant files", // i18n: data
    "work state",     // i18n: data
];
const MIN_SECTIONS: usize = 3;
/// A line that says a section is empty.
const NONE: &str = "(none)";

/// A folded summary: its gist, and its lines without empty sections.
#[derive(Debug, PartialEq)]
pub(crate) struct Summary {
    pub(crate) gist: String,
    pub(crate) lines: Vec<Line>,
}

fn is_section(line: &Line) -> bool {
    line.kind == Kind::Heading
        && SECTIONS.iter().any(|title| {
            line.text
                .trim()
                .trim_end_matches(':')
                .eq_ignore_ascii_case(title)
        })
}

/// `lines` as a summary, when they read like one.
pub(crate) fn summary(lines: &[Line]) -> Option<Summary> {
    if lines.iter().filter(|line| is_section(line)).count() < MIN_SECTIONS {
        return None;
    }
    let said = |line: &&Line| {
        !(matches!(
            line.kind,
            Kind::Bullet | Kind::Numbered(_) | Kind::Paragraph
        ) && line.text.trim_start().starts_with(NONE))
    };
    let kept: Vec<&Line> = lines.iter().filter(said).collect();
    // From the end, a heading stays when something stays under it; a
    // heading over a kept heading stays too, as Work State over Completed.
    let mut keep = vec![true; kept.len()];
    let mut content = false;
    for (index, line) in kept.iter().enumerate().rev() {
        match line.kind {
            Kind::Heading => keep[index] = content,
            Kind::Blank => {}
            _ => content = true,
        }
    }
    let lines: Vec<Line> = kept
        .into_iter()
        .zip(keep)
        .filter(|(_, keep)| *keep)
        .map(|(line, _)| line.clone())
        .collect();
    let objective = lines
        .iter()
        .position(|line| line.kind == Kind::Heading && line.text.eq_ignore_ascii_case("objective"));
    let gist = objective
        .map_or(&lines[..], |index| &lines[index + 1..])
        .iter()
        .find(|line| !matches!(line.kind, Kind::Heading | Kind::Blank))
        .map(|line| line.text.clone())
        .unwrap_or_default();
    Some(Summary { gist, lines })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
