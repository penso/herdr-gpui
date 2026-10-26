//! A cached readable projection of prepared bodies, in transcript order.

use super::{Caret, Cell, Key, Position, Region};
use crate::{
    chat::{
        prepared::{Body, MESSAGE, Part, folded_thought, notice_label, tool_header, tool_text},
        rich::{Block, CodeText, Inline, Kind, MAX_CODE_LINES, remote_content},
    },
    release_notes::Line,
};
use herdr_chat_protocol::{Entry, EntryId, EntryKind, Transcript};
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    ops::Range,
    rc::Rc,
    sync::Arc,
};

enum Text {
    Owned(String),
    Shared(Arc<str>),
    Lines(Rc<[Line]>, usize),
    /// A code line as drawn, and the block's source it starts at `start` of,
    /// which copy takes with its original tabs.
    Code {
        line: CodeText,
        source: Arc<str>,
        start: usize,
    },
}

pub(crate) struct Segment {
    position: Position,
    text: Text,
    atoms: Vec<Range<usize>>,
    pub(super) separator: &'static str,
}

impl Segment {
    pub(crate) fn text(&self) -> &str {
        match &self.text {
            Text::Owned(text) => text,
            Text::Shared(text) => text,
            Text::Lines(lines, index) => &lines[*index].text,
            Text::Code { line, .. } => &line.text,
        }
    }

    /// The drawn code line this segment selects, when it is one.
    pub(in crate::chat) fn code(&self) -> Option<&CodeText> {
        match &self.text {
            Text::Code { line, .. } => Some(line),
            _ => None,
        }
    }

    pub(crate) fn atomic(&self) -> &[Range<usize>] {
        &self.atoms
    }

    /// Original byte offset relative to this code line; ordinary prose is an
    /// identity map. A caret inside expanded tab spaces maps to the tab start.
    pub(crate) fn source_offset(&self, offset: usize) -> usize {
        let offset = self
            .text()
            .floor_char_boundary(offset.min(self.text().len()));
        self.code().map_or(offset, |line| line.source(offset))
    }

    pub(super) fn caret(&self, offset: usize) -> usize {
        let offset = self
            .text()
            .floor_char_boundary(offset.min(self.text().len()));
        self.atoms
            .iter()
            .find(|atom| atom.start < offset && offset < atom.end)
            .map_or(offset, |atom| {
                if offset - atom.start < atom.end - offset {
                    atom.start
                } else {
                    atom.end
                }
            })
    }

    pub(super) fn copy(&self, offsets: Range<usize>) -> &str {
        if offsets.is_empty() {
            return "";
        }
        let Text::Code {
            line,
            source,
            start: line_start,
        } = &self.text
        else {
            return &self.text()[offsets];
        };
        let start = self.source_offset(offsets.start);
        let end = line.source_end(offsets.end);
        &source[line_start + start..line_start + end.max(start)]
    }
}

pub(super) struct EntryText {
    pub(super) id: EntryId,
    pub(super) revision: u64,
    pub(super) version: u64,
    expanded: bool,
    pub(super) segments: Vec<Segment>,
    positions: HashMap<Position, usize>,
}

/// A segment's place in transcript order: its entry's index, then its own
/// index within the entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Order {
    pub(super) entry: usize,
    pub(super) segment: usize,
}

#[derive(Default)]
pub(crate) struct Document {
    pub(super) entries: Vec<Rc<EntryText>>,
    positions: HashMap<EntryId, usize>,
    epoch: u64,
    remote_links: bool,
    expanded: HashSet<EntryId>,
    version: u64,
}

impl Document {
    pub(crate) fn key(&self, entry: EntryId, region: Region, cell: Cell) -> Option<Key> {
        let projection = self.entries.get(*self.positions.get(&entry)?)?;
        let position = Position { region, cell };
        projection.positions.get(&position)?;
        Some(Key {
            entry,
            revision: projection.version,
            epoch: self.epoch,
            position,
        })
    }

    pub(crate) fn segment(&self, key: Key) -> Option<&Segment> {
        let order = self.order(key)?;
        self.entries.get(order.entry)?.segments.get(order.segment)
    }

    pub(super) fn order(&self, key: Key) -> Option<Order> {
        if key.epoch != self.epoch {
            return None;
        }
        let entry = *self.positions.get(&key.entry)?;
        let projection = self.entries.get(entry)?;
        (projection.version == key.revision).then_some(())?;
        Some(Order {
            entry,
            segment: *projection.positions.get(&key.position)?,
        })
    }

    pub(super) fn ends(&self) -> Option<(Caret, Caret)> {
        let first = self
            .entries
            .iter()
            .find(|entry| !entry.segments.is_empty())?;
        let last = self
            .entries
            .iter()
            .rev()
            .find(|entry| !entry.segments.is_empty())?;
        let caret = |entry: &EntryText, segment: &Segment, offset| Caret {
            key: Key {
                entry: entry.id,
                revision: entry.version,
                epoch: self.epoch,
                position: segment.position,
            },
            offset,
        };
        Some((
            caret(first, first.segments.first()?, 0),
            caret(
                last,
                last.segments.last()?,
                last.segments.last()?.text().len(),
            ),
        ))
    }

    pub(in crate::chat) fn set_remote_links(&mut self, remote: bool) {
        if self.remote_links != remote {
            self.clear();
            self.remote_links = remote;
        }
    }

    pub(in crate::chat) fn clear(&mut self) {
        self.entries.clear();
        self.positions.clear();
        self.epoch = self.epoch.wrapping_add(1);
    }

    pub(in crate::chat) fn set_expanded(&mut self, expanded: &HashSet<EntryId>) {
        self.expanded.clone_from(expanded);
    }

    pub(in crate::chat) fn update(
        &mut self,
        transcript: &Transcript,
        keys: &[(EntryId, u64)],
        bodies: &HashMap<EntryId, (u64, Rc<Body>)>,
    ) {
        let mut cached: HashMap<_, _> = self
            .entries
            .drain(..)
            .map(|entry| (entry.id, entry))
            .collect();
        self.positions.clear();
        for (entry, &(id, revision)) in transcript.entries.iter().zip(keys) {
            if entry.id != id {
                continue;
            }
            let expanded = self.expanded.contains(&id);
            let projected = match cached.remove(&id) {
                Some(projected)
                    if projected.revision == revision && projected.expanded == expanded =>
                {
                    projected
                }
                _ => {
                    self.version = self.version.wrapping_add(1);
                    Rc::new(project(
                        entry,
                        revision,
                        self.version,
                        bodies.get(&id).map(|(_, body)| body.as_ref()),
                        self.remote_links,
                        expanded,
                    ))
                }
            };
            self.positions.insert(id, self.entries.len());
            self.entries.push(projected);
        }
    }
}

fn project(
    entry: &Entry,
    revision: u64,
    version: u64,
    body: Option<&Body>,
    remote: bool,
    expanded: bool,
) -> EntryText {
    let mut builder = Builder {
        segments: Vec::new(),
        remote,
    };
    match body {
        Some(Body::Message(parts)) => builder.parts(parts),
        Some(Body::Answers { parts, .. }) => {
            if let EntryKind::Notice { text, .. } = &entry.kind {
                builder.plain(Region::Label, Cell::default(), notice_label(text));
            }
            builder.parts(parts);
        }
        Some(Body::Summary(summary)) => {
            if expanded {
                for (index, line) in summary.lines.iter().enumerate() {
                    builder.plain(Region::Body(0), Cell::row(index), line.text.clone());
                }
            } else {
                builder.plain(Region::Body(0), Cell::default(), summary.gist.clone());
            }
        }
        Some(Body::Tool(_)) => {
            if let EntryKind::Tool(tool) = &entry.kind {
                builder.plain(Region::Label, Cell::default(), tool_header(&tool.title));
                if expanded {
                    builder.plain(Region::ToolTitle, Cell::default(), tool.title.clone());
                    for (row, location) in tool.locations.iter().enumerate() {
                        builder.plain(Region::ToolLocations, Cell::row(row), location.clone());
                    }
                    for (block, content) in tool.content.iter().enumerate() {
                        if let Some(text) = tool_text(content) {
                            builder.plain(Region::ToolContent, Cell::block(block), text);
                        }
                    }
                }
            }
        }
        Some(Body::Image(_)) => {}
        None => match &entry.kind {
            EntryKind::Thought { text } if !expanded => {
                builder.plain(Region::Body(0), Cell::default(), folded_thought(text));
            }
            EntryKind::Notice {
                text,
                details: Some(_),
                ..
            } => builder.plain(Region::Body(0), Cell::default(), notice_label(text)),
            EntryKind::User { text, .. }
            | EntryKind::Agent { text }
            | EntryKind::Thought { text }
            | EntryKind::Notice { text, .. } => {
                builder.plain(
                    Region::Body(0),
                    Cell::default(),
                    MESSAGE.clip(text).to_owned(),
                );
            }
            EntryKind::Turn(turn) => {
                builder.plain(Region::Body(0), Cell::default(), turn.describe())
            }
            EntryKind::Tool(_) | EntryKind::Image { .. } => {}
        },
    }
    if expanded
        && let EntryKind::Notice {
            details: Some(details),
            ..
        } = &entry.kind
    {
        builder.plain(
            Region::Details,
            Cell::default(),
            MESSAGE.clip(details).to_owned(),
        );
    }
    let positions = builder
        .segments
        .iter()
        .enumerate()
        .map(|(index, segment)| (segment.position, index))
        .collect();
    EntryText {
        id: entry.id,
        revision,
        version,
        expanded,
        segments: builder.segments,
        positions,
    }
}

struct Builder {
    segments: Vec<Segment>,
    remote: bool,
}

impl Builder {
    fn push(
        &mut self,
        position: Position,
        text: Text,
        atoms: Vec<Range<usize>>,
        separator: &'static str,
    ) {
        self.segments.push(Segment {
            position,
            text,
            atoms,
            separator,
        });
    }

    fn plain(&mut self, region: Region, cell: Cell, text: String) {
        self.push(
            Position { region, cell },
            Text::Owned(text),
            Vec::new(),
            "\n",
        );
    }

    fn lines(&mut self, region: Region, lines: &Rc<[Line]>) {
        for (row, _) in lines.iter().enumerate() {
            self.push(
                Position {
                    region,
                    cell: Cell::row(row),
                },
                Text::Lines(lines.clone(), row),
                Vec::new(),
                "\n",
            );
        }
    }

    fn parts(&mut self, parts: &[Part]) {
        for (index, content) in parts.iter().enumerate() {
            let region = Region::Body(index);
            let first = self.segments.len();
            match content {
                Part::Rich(blocks) => self.blocks(region, blocks),
                Part::Text(lines) => self.lines(region, lines),
                // Source owned by a preview/card is not visible transcript
                // text. A range across it must never reveal the hidden payload.
                Part::Diagram(_, _) | Part::Html { .. } | Part::Widget(_) => {}
                Part::Picture { alt } | Part::Remote { alt, .. } => {
                    self.plain(region, Cell::default(), alt.clone())
                }
            }
            if let Some(segment) = self.segments.get_mut(first) {
                segment.separator = "\n\n";
            }
        }
    }

    fn blocks(&mut self, region: Region, blocks: &[Block]) {
        for (block, content) in blocks.iter().enumerate() {
            let position = Position {
                region,
                cell: Cell::block(block),
            };
            match content {
                Block::Paragraph { kind, content, .. } => {
                    if *kind != Kind::Rule {
                        self.inline(position, content, "\n\n");
                    }
                }
                Block::Math { source, .. } => self.push(
                    position,
                    Text::Shared(source.clone()),
                    std::iter::once(0..source.len()).collect(),
                    "\n\n",
                ),
                Block::Code { source, .. } => {
                    let mut offset = 0;
                    for (row, raw) in source
                        .split_inclusive('\n')
                        .take(MAX_CODE_LINES)
                        .enumerate()
                    {
                        let line = raw
                            .strip_suffix('\n')
                            .map(|line| line.strip_suffix('\r').unwrap_or(line))
                            .unwrap_or(raw);
                        self.push(
                            Position {
                                cell: Cell {
                                    block,
                                    row,
                                    column: 0,
                                },
                                ..position
                            },
                            Text::Code {
                                line: CodeText::expand(line),
                                source: source.clone(),
                                start: offset,
                            },
                            Vec::new(),
                            if row == 0 { "\n\n" } else { "\n" },
                        );
                        offset += raw.len();
                    }
                }
                Block::Table { rows, .. } => {
                    for (row, cells) in rows.iter().enumerate() {
                        for (column, content) in cells.iter().enumerate() {
                            self.inline(
                                Position {
                                    cell: Cell { block, row, column },
                                    ..position
                                },
                                content,
                                if column > 0 {
                                    "\t"
                                } else if row > 0 {
                                    "\n"
                                } else {
                                    "\n\n"
                                },
                            );
                        }
                    }
                }
            }
        }
    }

    fn inline(&mut self, position: Position, content: &[Inline], separator: &'static str) {
        let content = if self.remote {
            remote_content(content)
        } else {
            Cow::Borrowed(content)
        };
        let mut text = String::new();
        let mut atoms = Vec::new();
        for inline in content.iter() {
            match inline {
                Inline::Text(value, _) => text.push_str(value),
                Inline::Math { source, .. } => {
                    let start = text.len();
                    text.push_str(source);
                    atoms.push(start..text.len());
                }
                Inline::Break => text.push('\n'),
            }
        }
        self.push(position, Text::Owned(text), atoms, separator);
    }
}
