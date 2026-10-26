//! What the view draws for an entry, prepared when the entry changes rather
//! than on every frame: message text parsed as Markdown with its Mermaid
//! fences split out as diagrams, its HTML fences as pages to preview, and
//! its picture links as pictures, pictures asked of [`Visuals`], and a tool
//! call's output and file changes summarized.

use super::{
    rich::Fence,
    visual::{MAX_DIAGRAM_SOURCE, Source, VisualId, Visuals},
};
use crate::release_notes::{self, Line};
use herdr_chat_protocol::{
    Author, EntryId, EntryKind, MAX_QUESTION_ANSWERS, MAX_QUESTION_TEXT, QuestionAnswer,
    ToolContent, Transcript,
};
use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
    rc::Rc,
};

/// Lines of a tool call's text output shown under it.
const TOOL_LINES: usize = 6;
/// Bytes of a diff's sides compared line by line; past them it is counted
/// as changed without its lines.
const DIFF_BYTES: usize = 512 * 1024;
/// How much of one answer is drawn: as much text as the program keeps for
/// an entry, with room for long code. A longer answer ends with a note.
pub(super) const MESSAGE: release_notes::Limits = release_notes::Limits {
    body: 256 * 1024,
    lines: 2_000,
    line: 4_000,
};

/// What ends an answer cut at [`MESSAGE`].
const CUT: &str = "The rest of this answer is too long to show here; Copy takes all of it.";

/// Markdown prose as rich blocks, or as lines when it is too large to be
/// rich.
fn prose(text: &str, visuals: &mut Visuals) -> Part {
    match super::rich::parse(text, visuals) {
        Some(blocks) => Part::Rich(blocks.into()),
        None => Part::Text(message_lines(text)),
    }
}

/// An answer's Markdown lines, ending with a note when some are not shown;
/// Copy still takes the whole answer.
fn message_lines(text: &str) -> Rc<[Line]> {
    let (mut lines, cut) = release_notes::parse_within(text, MESSAGE);
    if cut {
        lines.push(release_notes::note(CUT));
    }
    lines.into()
}

/// Diagrams drawn per message; later fences stay code.
const DIAGRAMS: usize = 8;
/// Pictures shown under one tool call.
const TOOL_IMAGES: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DiffSummary {
    pub(crate) path: String,
    pub(crate) added: usize,
    pub(crate) removed: usize,
}

/// Characters of a tool call's header.
const TOOL_HEADER: usize = 300;
/// Characters of a notice's folded line or the label above its answers.
const NOTICE_LABEL: usize = 240;
/// Characters of a folded thought's first line.
const FOLDED_THOUGHT: usize = 120;

/// What a tool call's title reads as, before it is cut to a header.
fn tool_label(title: &str) -> &str {
    match title {
        "request_user_input" | "request_user_input_async" => "Ask user",
        _ => title,
    }
}

// The view and the selection document both draw these labels from the
// helpers below, so the drawn text always matches its selectable segment.

/// A tool call's header.
pub(super) fn tool_header(title: &str) -> String {
    crate::notifications::safe_text(tool_label(title), TOOL_HEADER)
}

/// A notice's folded line, or the label above its answers.
pub(super) fn notice_label(text: &str) -> String {
    crate::notifications::safe_text(text, NOTICE_LABEL)
}

/// The first line of a folded thought with any text on it.
pub(super) fn folded_thought(text: &str) -> String {
    let first = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");
    crate::notifications::safe_text(first, FOLDED_THOUGHT)
}

/// Expanded tool output comes from the retained transcript, not its preview.
/// Shared by the view and the selection document, like [`tool_header`].
pub(crate) fn tool_text(content: &ToolContent) -> Option<String> {
    match content {
        ToolContent::Text { text } => Some(text.clone()),
        ToolContent::Diff { old, new, path } => Some(match old {
            Some(old) => format!("{path}\nBefore:\n{old}\nAfter:\n{new}"),
            None => format!("{path}\nNew file:\n{new}"),
        }),
        ToolContent::Terminal { id } => Some(format!(
            "Terminal {id}: output is available in the program's terminal view."
        )),
        ToolContent::Image { .. } => None,
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ToolSummary {
    pub(crate) diffs: Vec<DiffSummary>,
    /// The first lines of its text output.
    pub(crate) lines: Vec<String>,
    /// Pictures it returned, the first [`TOOL_IMAGES`] of them.
    pub(crate) images: Vec<VisualId>,
    /// Text lines left out.
    pub(crate) more: usize,
    pub(crate) terminals: usize,
}

/// One stretch of a user or agent message.
#[derive(Debug)]
pub(crate) enum Part {
    Text(Rc<[Line]>),
    Rich(Rc<[super::rich::Block]>),
    Widget(Rc<super::widgets::Document>),
    /// A Mermaid fence, with its source as code lines for when it cannot be
    /// drawn.
    Diagram(VisualId, Rc<[Line]>),
    /// An HTML fence: a page the user may preview.
    Html {
        page: Rc<str>,
    },
    /// A picture on the agent's machine; herdr-acp adds it after the
    /// message, so the message only names it.
    Picture {
        alt: String,
    },
    /// A picture on the web, fetched once the user asks for it.
    Remote {
        id: VisualId,
        url: crate::browser::WebUrl,
        alt: String,
    },
}

#[derive(Debug)]
pub(crate) enum Body {
    Message(Vec<Part>),
    /// Historical Q&A, with its readable copy payload prepared once as well.
    Answers {
        text: Rc<str>,
        parts: Vec<Part>,
    },
    /// An agent's compaction summary, folded.
    Summary(super::summary::Summary),
    Image(VisualId),
    Tool(ToolSummary),
}

impl Body {
    fn visuals(&self) -> impl Iterator<Item = VisualId> + '_ {
        let (parts, images): (&[Part], &[VisualId]) = match self {
            Self::Message(parts) | Self::Answers { parts, .. } => (parts, &[]),
            Self::Summary(_) => (&[], &[]),
            Self::Image(id) => (&[], std::slice::from_ref(id)),
            Self::Tool(summary) => (&[], &summary.images),
        };
        parts
            .iter()
            .flat_map(|part| {
                let (own, blocks): (_, &[super::rich::Block]) = match part {
                    Part::Diagram(id, _) | Part::Remote { id, .. } => (Some(*id), &[]),
                    Part::Rich(blocks) => (None, blocks),
                    Part::Text(_) | Part::Widget(_) | Part::Html { .. } | Part::Picture { .. } => {
                        (None, &[])
                    }
                };
                own.into_iter()
                    .chain(blocks.iter().flat_map(super::rich::Block::visuals))
            })
            .chain(images.iter().copied())
    }
}

/// Bodies by entry, each with the revision it was prepared at, and the
/// pictures they draw.
#[derive(Default)]
pub(crate) struct Prepared {
    bodies: HashMap<EntryId, (u64, Rc<Body>)>,
    /// Files the transcript's tool calls changed, first change first, with
    /// their lines added and removed summed.
    changed: Vec<DiffSummary>,
    text: super::selection::Document,
    pub(crate) visuals: Visuals,
}

impl Prepared {
    pub(crate) fn body(&self, id: EntryId) -> Option<Rc<Body>> {
        self.bodies.get(&id).map(|(_, body)| body.clone())
    }

    /// Whether the message part at `key` is still the card `document`.
    pub(crate) fn shows_widget(
        &self,
        (entry, part): super::widgets::Key,
        document: &super::widgets::Document,
    ) -> bool {
        self.bodies.get(&entry).is_some_and(|(_, body)| {
            matches!(&**body, Body::Message(parts)
                if matches!(parts.get(part), Some(Part::Widget(shown))
                    if shown.source == document.source))
        })
    }

    pub(crate) fn changed(&self) -> &[DiffSummary] {
        &self.changed
    }

    pub(crate) fn text(&self) -> &super::selection::Document {
        &self.text
    }

    /// Remote links display their destination beside the label. Keep the
    /// cached selection projection in the same coordinate system as the view.
    pub(crate) fn set_remote_links(&mut self, remote: bool) {
        self.text.set_remote_links(remote);
    }

    /// Apply before `update`: folded records contribute only their visible
    /// label/gist, never undisclosed source text between selected messages.
    pub(crate) fn set_expanded(&mut self, expanded: &HashSet<EntryId>) {
        self.text.set_expanded(expanded);
    }

    /// The entries that draw `visual`, to measure again once it lands.
    pub(crate) fn drawing(&self, visual: VisualId) -> impl Iterator<Item = EntryId> + '_ {
        self.bodies
            .iter()
            .filter(move |(_, (_, body))| body.visuals().any(|id| id == visual))
            .map(|(id, _)| *id)
    }

    /// Prepares the entries whose revision moved and forgets those gone.
    pub(crate) fn update(&mut self, transcript: &Transcript, keys: &[(EntryId, u64)]) {
        let live: HashSet<EntryId> = keys.iter().map(|(id, _)| *id).collect();
        self.bodies.retain(|id, _| live.contains(id));
        for (entry, (_, revision)) in transcript.entries.iter().zip(keys) {
            if self
                .bodies
                .get(&entry.id)
                .is_some_and(|(prepared, _)| prepared == revision)
            {
                continue;
            }
            let visuals = &mut self.visuals;
            let body = match &entry.kind {
                EntryKind::User { text, .. } => Body::Message(message(text, Author::User, visuals)),
                EntryKind::Notice { answers, .. } if !answers.is_empty() => {
                    let text: Rc<str> = answers_text(answers).into();
                    let parts = vec![prose(&text, visuals)];
                    Body::Answers { text, parts }
                }
                EntryKind::Agent { text } => {
                    let (lines, cut) = release_notes::parse_within(text, MESSAGE);
                    match super::summary::summary(&lines) {
                        Some(mut summary) => {
                            if cut {
                                summary.lines.push(release_notes::note(CUT));
                            }
                            Body::Summary(summary)
                        }
                        None => Body::Message(message(text, Author::Agent, visuals)),
                    }
                }
                EntryKind::Image { image, .. } => {
                    let id = VisualId::of(&image.data);
                    visuals.want(id, Source::Image(image.clone()));
                    Body::Image(id)
                }
                EntryKind::Tool(tool) => Body::Tool(summarize(&tool.content, visuals)),
                _ => {
                    self.bodies.remove(&entry.id);
                    continue;
                }
            };
            self.bodies.insert(entry.id, (*revision, Rc::new(body)));
        }
        self.text.update(transcript, keys, &self.bodies);
        self.changed.clear();
        for entry in &transcript.entries {
            let Some((_, body)) = self.bodies.get(&entry.id) else {
                continue;
            };
            let Body::Tool(summary) = &**body else {
                continue;
            };
            for diff in &summary.diffs {
                match self
                    .changed
                    .iter_mut()
                    .find(|known| known.path == diff.path)
                {
                    Some(known) => {
                        known.added += diff.added;
                        known.removed += diff.removed;
                    }
                    None => self.changed.push(diff.clone()),
                }
            }
        }
        let live: HashSet<VisualId> = self
            .bodies
            .values()
            .flat_map(|(_, body)| body.visuals())
            .collect();
        self.visuals.retain(&live);
    }

    /// Starts over, as after a snapshot that may reuse entry ids.
    pub(crate) fn clear(&mut self) {
        self.bodies.clear();
        self.changed.clear();
        self.text.clear();
        self.visuals.clear();
    }
}

/// Readable history and its copy payload share the same bounded Markdown.
/// Only rich Markdown parses this text; HTML/widget fences remain code.
pub(super) fn answers_text(answers: &[QuestionAnswer]) -> String {
    let mut source = String::new();
    let mut remaining = MAX_QUESTION_TEXT;
    let mut truncated = answers.len() > MAX_QUESTION_ANSWERS;
    'answers: for (index, answer) in answers.iter().take(MAX_QUESTION_ANSWERS).enumerate() {
        if index > 0 {
            source.push_str("\n\n");
        }
        let _ = write!(source, "**Question {}**\n\n", index + 1);
        for (label, text) in [
            ("", &answer.question),
            ("\n\n**Answer**\n\n", &answer.answer),
        ] {
            source.push_str(label);
            let end = text.floor_char_boundary(remaining.min(text.len()));
            source.push_str(&text[..end]);
            remaining -= end;
            if end < text.len() {
                truncated = true;
                break 'answers;
            }
        }
    }
    if truncated {
        source.push_str("\n\n… (answers truncated)");
    }
    source
}

/// A message as Markdown text with its diagrams, pages, and
/// pictures between it. Only a closed fence becomes a diagram or a page, so
/// one still streaming shows as code. A user's own text is shown as Markdown,
/// but only an agent's reply can become a live card or a page preview:
/// pasting an earlier reply must not turn the user's bubble into a form that
/// sends prompts.
fn message(text: &str, author: Author, visuals: &mut Visuals) -> Vec<Part> {
    let end = MESSAGE
        .clip(text)
        .split_inclusive('\n')
        .take(MESSAGE.lines)
        .map(str::len)
        .sum();
    let cut = end < text.len();
    let text = &text[..end];
    let mut parts = Vec::new();
    let mut markdown = String::new();
    let flush = |text: &mut String, parts: &mut Vec<Part>, visuals: &mut Visuals| {
        if !text.trim().is_empty() {
            parts.push(prose(text, visuals));
        }
        text.clear();
    };
    let mut lines = text.split_inclusive('\n');
    let (mut diagrams, mut pages, mut widgets) = (0, 0, 0);
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        let indentation = &line[..line.len() - trimmed.len()];
        if indentation.len() > 3 || !indentation.chars().all(|ch| ch == ' ') {
            markdown.push_str(line);
            continue;
        }
        if let Some(part) = picture(line) {
            flush(&mut markdown, &mut parts, visuals);
            parts.push(part);
            continue;
        }
        let Some(fence) = Fence::open(line) else {
            markdown.push_str(line);
            continue;
        };
        let info = fence.info;
        let kind = if info.eq_ignore_ascii_case("mermaid") && diagrams < DIAGRAMS {
            Embed::Diagram
        } else if author == Author::Agent && info.eq_ignore_ascii_case("html") && pages < PAGES {
            Embed::Page
        } else if author == Author::Agent
            && info.eq_ignore_ascii_case("herdr-ui")
            && widgets < WIDGETS
        {
            Embed::Widget
        } else {
            Embed::Code
        };
        let mut source = String::new();
        let mut closed = false;
        let mut block = line.to_owned();
        for inner in lines.by_ref() {
            block.push_str(inner);
            if fence.closes(inner) {
                closed = true;
                break;
            }
            source.push_str(inner);
        }
        let limit = match kind {
            Embed::Diagram => MAX_DIAGRAM_SOURCE,
            Embed::Page => MAX_PAGE,
            Embed::Widget => super::widgets::MAX_SOURCE,
            Embed::Code => MESSAGE.body,
        };
        if !closed || source.trim().is_empty() || source.len() > limit {
            markdown.push_str(&block);
            continue;
        }
        match kind {
            // Markdown draws code fences itself, in place in the prose.
            Embed::Code => markdown.push_str(&block),
            Embed::Widget => match super::widgets::Document::parse(&source) {
                Ok(document) => {
                    flush(&mut markdown, &mut parts, visuals);
                    widgets += 1;
                    parts.push(Part::Widget(Rc::new(document)));
                }
                Err(_) => markdown.push_str(&block),
            },
            Embed::Diagram => {
                flush(&mut markdown, &mut parts, visuals);
                diagrams += 1;
                let id = VisualId::of(&source);
                let code = release_notes::parse(&block).into();
                visuals.want(id, Source::Diagram(source.into()));
                parts.push(Part::Diagram(id, code));
            }
            Embed::Page => {
                flush(&mut markdown, &mut parts, visuals);
                pages += 1;
                parts.push(Part::Html {
                    page: source.into(),
                });
            }
        }
    }
    if !markdown.trim().is_empty() || parts.is_empty() {
        parts.push(prose(&markdown, visuals));
    }
    if cut {
        let note = release_notes::note(CUT);
        match parts.last_mut() {
            // Lines cut for their own length already end with the note.
            Some(Part::Text(lines)) if lines.last() == Some(&note) => {}
            Some(Part::Text(lines)) => *lines = lines.iter().cloned().chain([note]).collect(),
            _ => parts.push(Part::Text(vec![note].into())),
        }
    }
    parts
}

/// What a closed fence becomes.
enum Embed {
    Code,
    Diagram,
    Page,
    Widget,
}

/// Cards per message; excess cards remain readable source.
const WIDGETS: usize = 4;

/// Pages per message; later HTML fences stay code.
const PAGES: usize = 4;
/// Bytes of an HTML fence shown as a page; a longer one stays code.
pub(crate) const MAX_PAGE: usize = 256 * 1024;

/// A line that is only a Markdown picture, `![alt](target)`: a web address
/// to fetch on request, or a file herdr-acp shows after the message.
fn picture(line: &str) -> Option<Part> {
    let inner = line.trim().strip_prefix("![")?.strip_suffix(')')?;
    let (alt, target) = inner.split_once("](")?;
    if alt.contains(['[', ']']) || target.contains(['(', ')']) {
        return None;
    }
    let target = target.trim();
    let target = target
        .strip_prefix('<')
        .and_then(|inner| inner.strip_suffix('>'))
        .unwrap_or_else(|| target.split_whitespace().next().unwrap_or(""));
    let alt = crate::notifications::safe_text(alt.trim(), 200);
    if target.is_empty() || target.starts_with("data:") {
        return None;
    }
    if target.starts_with("http://") || target.starts_with("https://") {
        let url = crate::browser::WebUrl::try_from(target).ok()?;
        return Some(Part::Remote {
            id: VisualId::of(url.as_str()),
            url,
            alt,
        });
    }
    // herdr-acp reads a path or a file URL; any other scheme stays text. A
    // Windows drive letter looks like a one-letter scheme.
    let scheme = target.split_once(':').map(|(scheme, _)| scheme);
    if scheme.is_some_and(|scheme| {
        scheme.len() > 1
            && scheme != "file"
            && scheme
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || "+-.".contains(ch))
    }) {
        return None;
    }
    Some(Part::Picture { alt })
}

pub(crate) fn summarize(content: &[ToolContent], visuals: &mut Visuals) -> ToolSummary {
    let mut summary = ToolSummary::default();
    for item in content {
        match item {
            ToolContent::Text { text } => {
                for line in text.lines().filter(|line| !line.trim().is_empty()) {
                    if summary.lines.len() < TOOL_LINES {
                        summary
                            .lines
                            .push(crate::notifications::safe_text(line, 400));
                    } else {
                        summary.more += 1;
                    }
                }
            }
            ToolContent::Diff { new, old, path } => {
                let (added, removed) = diff_counts(old.as_deref().unwrap_or(""), new);
                summary.diffs.push(DiffSummary {
                    path: path.clone(),
                    added,
                    removed,
                });
            }
            ToolContent::Image { image } => {
                if summary.images.len() < TOOL_IMAGES {
                    let id = VisualId::of(&image.data);
                    visuals.want(id, Source::Image(image.clone()));
                    summary.images.push(id);
                }
            }
            ToolContent::Terminal { .. } => summary.terminals += 1,
        }
    }
    summary
}

/// Lines added and removed between two versions of a file, by counting
/// each distinct line on both sides: what `+`/`-` totals show, without the
/// cost of an alignment.
pub(crate) fn diff_counts(old: &str, new: &str) -> (usize, usize) {
    if old.len() + new.len() > DIFF_BYTES {
        return (new.lines().count(), old.lines().count());
    }
    let mut counts: HashMap<&str, isize> = HashMap::new();
    for line in old.lines() {
        *counts.entry(line).or_default() -= 1;
    }
    for line in new.lines() {
        *counts.entry(line).or_default() += 1;
    }
    counts
        .values()
        .fold((0, 0), |(added, removed), &count| match count {
            count if count > 0 => (added + count.unsigned_abs(), removed),
            count => (added, removed + count.unsigned_abs()),
        })
}
