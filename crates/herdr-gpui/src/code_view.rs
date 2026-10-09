//! Code tabs: a local source file drawn read-only beside the workspace's
//! terminals and pages, with its definitions listed as an outline, for a
//! quick look without leaving the agent's side. Editing belongs to the
//! terminal editor, which `e` opens at the line in view.
//!
//! A tab keeps only the file's path and the line it opened at; the window
//! keeps each tab's view, made when the tab opens or is first seen after a
//! restart. The file is read, outlined, and coloured on the background
//! executor, colours last, so its text shows before a large file is
//! coloured. Drawing only reads what those produced.

mod render;

use crate::{
    HerdrWindow,
    browser::{Location, Store, TabId},
    code_index::{self, Kind, Language},
    editor::EditorTarget,
    review::highlight::{self, Span},
    window::Flash,
};
use gpui::{prelude::*, *};
use serde::{Deserialize, Serialize};
use std::{cell::Cell, ops::Range, path::Path, rc::Rc, sync::Arc};

/// Lines coloured at most; the rest of a longer file draws plain.
const MAX_COLOURED_LINES: usize = 20_000;

/// The local file a code tab shows, and the line it opened at. Saved with
/// the tabs, so it is checked again whenever it is read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "SavedCodeFile")]
pub(crate) struct CodeFile {
    pub(crate) path: String,
    pub(crate) line: Option<u32>,
}

#[derive(Deserialize)]
struct SavedCodeFile {
    path: String,
    line: Option<u32>,
}

impl TryFrom<SavedCodeFile> for CodeFile {
    type Error = crate::Error;

    fn try_from(saved: SavedCodeFile) -> crate::Result<Self> {
        if saved.path.is_empty()
            || saved.path.len() > 4096
            || saved.path.chars().any(char::is_control)
            || !Path::new(&saved.path).is_absolute()
        {
            return Err(crate::Error::InvalidCodeFile);
        }
        Ok(Self {
            path: saved.path,
            line: saved.line,
        })
    }
}

impl CodeFile {
    /// The file's name, as its tab is titled.
    pub(crate) fn name(&self) -> &str {
        Path::new(&self.path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&self.path)
    }
}

/// One definition in the outline.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    name: String,
    kind: Kind,
    line: u32,
}

/// A file as read: its text, where each line is in it, and its outline.
struct Text {
    source: String,
    lines: Vec<Range<usize>>,
    outline: Vec<Entry>,
    /// The longest line, which sets how far the code scrolls sideways.
    widest: usize,
}

impl Text {
    /// Reads the file at `path`. Its text is untrusted: as in a review,
    /// tabs become spaces and control and direction-override characters go
    /// before anything is shown, coloured, or outlined, so what is drawn is
    /// what was read.
    fn read(path: &Path) -> crate::Result<Self> {
        let raw = code_index::read_source(path).ok_or(crate::Error::CodeFileUnreadable)?;
        let mut source = String::with_capacity(raw.len());
        let mut lines = Vec::new();
        for line in raw.lines() {
            let start = source.len();
            crate::review::push_clean(&mut source, line);
            lines.push(start..source.len());
            source.push('\n');
        }
        let widest = lines
            .iter()
            .enumerate()
            .max_by_key(|(_, line)| line.len())
            .map_or(0, |(index, _)| index);
        let outline = path
            .to_str()
            .and_then(Language::of)
            .map(|language| code_index::scan(language, &source, 5_000))
            .unwrap_or_default()
            .into_iter()
            .map(|found| Entry {
                name: found.name,
                kind: found.kind,
                line: found.line,
            })
            .collect();
        Ok(Self {
            source,
            lines,
            outline,
            widest,
        })
    }

    fn line(&self, index: usize) -> &str {
        self.lines
            .get(index)
            .and_then(|range| self.source.get(range.clone()))
            .unwrap_or_default()
    }
}

enum State {
    Loading,
    Loaded(Arc<Text>),
    Failed(String),
}

pub(crate) struct CodeView {
    pub(crate) focus: FocusHandle,
    state: State,
    /// Each line's colours, once worked out.
    spans: Arc<Vec<Vec<Span>>>,
    scroll: UniformListScrollHandle,
    /// The line brought into view once the text is read, and marked.
    target: Option<u32>,
    marked: Option<u32>,
    outline: bool,
    /// The tab's width as last laid out, which decides whether the outline
    /// fits.
    width: Rc<Cell<f32>>,
    /// The newest read; an older one's result is dropped.
    load: u64,
}

impl CodeView {
    fn text(&self) -> Option<&Arc<Text>> {
        match &self.state {
            State::Loaded(text) => Some(text),
            State::Loading | State::Failed(_) => None,
        }
    }

    /// The marked line, and how many lines the read file has.
    #[cfg(test)]
    pub(crate) fn shown(&self) -> (Option<u32>, Option<usize>) {
        (self.marked, self.text().map(|text| text.lines.len()))
    }

    /// Brings 1-based `line` into view and marks it.
    fn go_to(&mut self, line: u32) {
        self.marked = Some(line);
        let row = usize::try_from(line.saturating_sub(1)).unwrap_or(0);
        // Strict, so a jump centers its line even when already in view.
        self.scroll
            .scroll_to_item_strict(row, ScrollStrategy::Center);
    }

    /// The 1-based line the editor opens at: the marked one, or the top
    /// line in view.
    fn editor_line(&self) -> u32 {
        self.marked.unwrap_or_else(|| {
            u32::try_from(self.scroll.logical_scroll_top_index() + 1).unwrap_or(1)
        })
    }
}

impl HerdrWindow {
    /// Opens `target` in a code tab of the focused workspace, or brings
    /// back the tab already showing that file, at the target's line.
    pub(crate) fn open_code_view(
        &mut self,
        target: &EditorTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((scope, workspace)) = self.browser_key() else {
            self.show_flash(Flash::warning("Open a workspace first"), cx);
            return;
        };
        let Some(path) = target.path.to_str().map(str::to_owned) else {
            self.show_flash(
                Flash::warning(crate::Error::InvalidCodeFile.to_string()),
                cx,
            );
            return;
        };
        let existing = cx.try_global::<Store>().and_then(|store| {
            store
                .in_workspace(&scope, &workspace)
                .find(|tab| {
                    matches!(&tab.location, Some(Location::Code { file }) if file.path == path)
                })
                .map(|tab| tab.id)
        });
        let file = CodeFile {
            path,
            line: target.line,
        };
        let opened = existing.or_else(|| {
            Store::update(cx, |store| {
                store.open(scope, &workspace, Some(Location::Code { file }), None)
            })
        });
        let Some(id) = opened else {
            self.show_flash(Flash::warning("Too many tabs are open"), cx);
            return;
        };
        self.ensure_code_view(id, cx);
        let Some(view) = self.code_views.get_mut(&id) else {
            return;
        };
        if let Some(line) = target.line {
            view.target = Some(line);
            if view.text().is_some() {
                view.go_to(line);
            }
        }
        let focus = view.focus.clone();
        // Read again even when the tab was open: an agent may have changed
        // the file since, and the line came from a newer index. The old text
        // shows until the new one is read.
        self.load_code_view(id, cx);
        self.show_browser_tab(id, window, cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Makes `id`'s view from its tab, if the window has none yet. Whether
    /// it was made.
    fn ensure_code_view(&mut self, id: TabId, cx: &mut Context<Self>) -> bool {
        if self.code_views.contains_key(&id) {
            return false;
        }
        let Some(Location::Code { file }) = cx
            .try_global::<Store>()
            .and_then(|store| store.get(id))
            .and_then(|tab| tab.location.clone())
        else {
            return false;
        };
        self.code_views.insert(
            id,
            CodeView {
                focus: cx.focus_handle(),
                state: State::Loading,
                spans: Arc::default(),
                scroll: UniformListScrollHandle::new(),
                target: file.line,
                marked: None,
                outline: true,
                width: Rc::default(),
                load: 0,
            },
        );
        true
    }

    /// Reads tab `id`'s file off the UI thread, then colours it.
    fn load_code_view(&mut self, id: TabId, cx: &mut Context<Self>) {
        let Some(Location::Code { file }) = cx
            .try_global::<Store>()
            .and_then(|store| store.get(id))
            .and_then(|tab| tab.location.clone())
        else {
            return;
        };
        let Some(view) = self.code_views.get_mut(&id) else {
            return;
        };
        view.load += 1;
        let load = view.load;
        let path = std::path::PathBuf::from(&file.path);
        let read = cx
            .background_executor()
            .spawn(async move { Text::read(&path).map(Arc::new) });
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let text = read.await;
            let coloured = text.as_ref().ok().cloned();
            let current = this
                .update(cx, |this, cx| this.code_view_read(id, load, text, cx))
                .unwrap_or(false);
            let Some(text) = coloured.filter(|_| current) else {
                return;
            };
            let name = file.name().to_owned();
            let spans = executor
                .spawn(
                    async move { highlight::colour_file(&name, &text.source, MAX_COLOURED_LINES) },
                )
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(view) = this.code_views.get_mut(&id)
                    && view.load == load
                {
                    view.spans = Arc::new(spans);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Shows a read file; whether this read is still the newest.
    fn code_view_read(
        &mut self,
        id: TabId,
        load: u64,
        text: crate::Result<Arc<Text>>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(view) = self
            .code_views
            .get_mut(&id)
            .filter(|view| view.load == load)
        else {
            return false;
        };
        match text {
            Ok(text) => {
                // Colours of other text would land on the wrong words.
                if view.text().is_some_and(|old| old.source != text.source) {
                    view.spans = Arc::default();
                }
                view.state = State::Loaded(text);
                if let Some(line) = view.target.take() {
                    view.go_to(line);
                }
            }
            Err(error) => view.state = State::Failed(error.to_string()),
        }
        cx.notify();
        true
    }

    /// Runs every window tick: code tabs of the focused workspace with no
    /// view yet, such as ones restored from the last run, get one; the
    /// views of closed tabs go.
    pub(crate) fn poll_code_views(&mut self, cx: &mut Context<Self>) {
        let Some(store) = cx.try_global::<Store>() else {
            self.code_views.clear();
            return;
        };
        self.code_views.retain(|id, _| store.get(*id).is_some());
        let Some((scope, workspace)) = self.browser_key() else {
            return;
        };
        let missing: Vec<TabId> = store
            .in_workspace(&scope, &workspace)
            .filter(|tab| {
                matches!(tab.location, Some(Location::Code { .. }))
                    && !self.code_views.contains_key(&tab.id)
            })
            .map(|tab| tab.id)
            .collect();
        for id in missing {
            if self.ensure_code_view(id, cx) {
                self.load_code_view(id, cx);
            }
        }
    }

    /// Opens tab `id`'s file in the terminal editor, at the marked line or
    /// the top one in view, beside the focused pane.
    fn open_code_view_in_editor(&mut self, id: TabId, cx: &mut Context<Self>) {
        let Some(Location::Code { file }) = cx
            .try_global::<Store>()
            .and_then(|store| store.get(id))
            .and_then(|tab| tab.location.clone())
        else {
            return;
        };
        let line = self.code_views.get(&id).map(CodeView::editor_line);
        let target = EditorTarget {
            path: file.path.into(),
            line,
        };
        self.open_in_editor(&target, None, cx);
    }

    /// Handles a key in a code tab; whether it was the tab's.
    fn code_view_key(&mut self, id: TabId, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let keystroke = &event.keystroke;
        let modifiers = keystroke.modifiers;
        if modifiers.control || modifiers.alt || modifiers.platform || modifiers.function {
            return false;
        }
        if keystroke.key == "e" && !modifiers.shift {
            self.open_code_view_in_editor(id, cx);
            return true;
        }
        let Some(view) = self.code_views.get_mut(&id) else {
            return false;
        };
        let count = view.text().map_or(0, |text| text.lines.len());
        let top = view.scroll.logical_scroll_top_index();
        let page = 20;
        let row = match (keystroke.key.as_str(), modifiers.shift) {
            ("j" | "down", false) => top + 1,
            ("k" | "up", false) => top.saturating_sub(1),
            ("space", false) | ("pagedown", _) => top + page,
            ("space", true) | ("pageup", _) => top.saturating_sub(page),
            ("home", _) | ("g", false) => 0,
            ("end", _) | ("g", true) => count.saturating_sub(1),
            ("o", false) => {
                view.outline = !view.outline;
                cx.notify();
                return true;
            }
            _ => return false,
        };
        view.scroll
            .scroll_to_item_strict(row.min(count.saturating_sub(1)), ScrollStrategy::Top);
        cx.notify();
        true
    }
}

#[cfg(test)]
mod tests;
