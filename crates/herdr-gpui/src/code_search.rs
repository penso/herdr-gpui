//! Go to Symbol and Go to File: a picker over the focused pane's checkout,
//! as an IDE's quick open. The checkout is found and indexed on the
//! background executor (see `code_index`), kept for the next search, and
//! read again in the background when it has aged, so the picker opens at
//! once on a known checkout. Choosing a row opens it in the terminal editor
//! beside the focused pane (see `editor`), or with the secondary modifier in
//! a code viewer tab.

mod rank;
mod render;

pub(crate) use rank::Mode;

use crate::{
    HerdrWindow,
    code_index::{self, Index},
    editor::EditorTarget,
    menu::Page,
    search_input::{Changed, SearchInput},
};
use gpui::{prelude::*, *};
use rank::{Hit, Query};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// How many checkouts' indexes are kept between searches.
const CACHED: usize = 4;
/// An index older than this is read again when a search opens on it.
const FRESH: Duration = Duration::from_secs(30);

/// Indexes of recently searched checkouts, newest first.
#[derive(Default)]
pub(crate) struct Indexes {
    entries: Vec<(Arc<Index>, Instant)>,
}

impl Indexes {
    fn get(&self, root: &Path) -> Option<&(Arc<Index>, Instant)> {
        self.entries.iter().find(|(index, _)| index.root() == root)
    }

    fn insert(&mut self, index: Arc<Index>, now: Instant) {
        self.entries.retain(|(kept, _)| kept.root() != index.root());
        self.entries.insert(0, (index, now));
        self.entries.truncate(CACHED);
    }
}

/// Where the search's checkout stands.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Status {
    Locating,
    Indexing,
    Ready,
    Failed(String),
}

pub(crate) struct CodeSearch {
    search: Entity<SearchInput>,
    mode: Mode,
    query: String,
    /// The index `hits` were ranked against, which shows them and opens
    /// them: a hit's item number means nothing in any other index.
    index: Option<Arc<Index>>,
    /// The newest index, which the next ranking reads. It replaces `index`
    /// only together with that ranking's hits.
    latest: Option<Arc<Index>>,
    status: Status,
    hits: Vec<Hit>,
    selected: usize,
    scroll: UniformListScrollHandle,
    /// The pane the editor opens beside: the one focused when it opened.
    beside: Option<String>,
    /// Stops a background read nobody waits for any more.
    cancelled: Arc<AtomicBool>,
    rank_task: Option<Task<()>>,
    _subscription: Subscription,
}

impl Drop for CodeSearch {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

impl CodeSearch {
    fn query(&self) -> Query {
        Query::parse(&self.query, self.mode)
    }

    /// The file and line the selected row opens at.
    fn target(&self, hit: &Hit) -> Option<EditorTarget> {
        let index = self.index.as_ref()?;
        Some(match self.mode {
            Mode::Symbols => {
                let symbol = index.symbols().get(hit.item)?;
                EditorTarget {
                    path: index.path(symbol.file)?,
                    line: Some(symbol.line),
                }
            }
            Mode::Files => EditorTarget {
                path: index.path(hit.item)?,
                line: self.query().line,
            },
        })
    }
}

impl HerdrWindow {
    /// Opens the picker in `mode` on the focused pane's checkout.
    pub(crate) fn open_code_search(
        &mut self,
        mode: Mode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.open_menu(window, cx) {
            return;
        }
        self.menu.page = Some(Page::CodeSearch);
        let search = cx.new(SearchInput::new);
        let subscription = cx.subscribe(&search, |this, search, _: &Changed, cx| {
            let query = search.read(cx).text().to_owned();
            if let Some(code) = &mut this.menu.code_search {
                code.query = query;
            }
            this.rank_code_search(cx);
        });
        search.update(cx, |input, cx| {
            input.set_placeholder(placeholder(mode), cx);
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
            window.focus(&input.focus, cx);
        });
        let snapshot = self.live.snapshot.clone();
        let focused = snapshot.as_deref().and_then(|snapshot| {
            let id = snapshot.focused_pane_id.as_deref()?;
            snapshot.panes.iter().find(|pane| pane.pane_id == id)
        });
        let cwd = focused.and_then(|pane| pane.foreground_cwd.clone().or_else(|| pane.cwd.clone()));
        let mut code = CodeSearch {
            search,
            mode,
            query: String::new(),
            index: None,
            latest: None,
            status: Status::Locating,
            hits: Vec::new(),
            selected: 0,
            scroll: UniformListScrollHandle::new(),
            beside: focused.map(|pane| pane.pane_id.clone()),
            cancelled: Arc::default(),
            rank_task: None,
            _subscription: subscription,
        };
        match cwd.filter(|_| !self.selected_is_remote()) {
            Some(cwd) => self.locate_checkout(&code, cwd, cx),
            None => code.status = Status::Failed(crate::Error::CodeIndexRoot.to_string()),
        }
        self.menu.code_search = Some(code);
        cx.notify();
    }

    /// Finds the checkout holding `cwd`, then shows its index.
    fn locate_checkout(&self, code: &CodeSearch, cwd: String, cx: &mut Context<Self>) {
        let cancelled = code.cancelled.clone();
        let token = code.search.clone();
        let found = cx
            .background_executor()
            .spawn(async move { code_index::checkout_root(&cwd, &cancelled) });
        cx.spawn(async move |this, cx| {
            let root = found.await;
            let _ = this.update(cx, |this, cx| {
                if this.code_search_current(&token) {
                    this.checkout_located(root, cx);
                }
            });
        })
        .detach();
    }

    fn code_search_current(&self, token: &Entity<SearchInput>) -> bool {
        self.menu
            .code_search
            .as_ref()
            .is_some_and(|code| code.search == *token)
    }

    fn checkout_located(&mut self, root: crate::Result<PathBuf>, cx: &mut Context<Self>) {
        let root = match root {
            Ok(root) => root,
            Err(error) => return self.fail_code_search(error, cx),
        };
        let cached = self.code_indexes.get(&root).cloned();
        let Some(code) = &mut self.menu.code_search else {
            return;
        };
        let stale = cached
            .as_ref()
            .is_none_or(|(_, built)| built.elapsed() >= FRESH);
        if let Some((index, _)) = cached {
            code.latest = Some(index);
            code.status = Status::Ready;
        } else {
            code.status = Status::Indexing;
        }
        if stale {
            let cancelled = code.cancelled.clone();
            let token = code.search.clone();
            let built = cx
                .background_executor()
                .spawn(async move { Index::build(&root, &cancelled).map(Arc::new) });
            cx.spawn(async move |this, cx| {
                let index = built.await;
                let _ = this.update(cx, |this, cx| this.checkout_indexed(&token, index, cx));
            })
            .detach();
        }
        self.rank_code_search(cx);
    }

    fn checkout_indexed(
        &mut self,
        token: &Entity<SearchInput>,
        index: crate::Result<Arc<Index>>,
        cx: &mut Context<Self>,
    ) {
        // Kept for the next search even when this one has closed.
        let index = match index {
            Ok(index) => {
                self.code_indexes.insert(index.clone(), Instant::now());
                index
            }
            Err(error) if self.code_search_current(token) => {
                return self.fail_code_search(error, cx);
            }
            Err(_) => return,
        };
        if !self.code_search_current(token) {
            return;
        }
        if let Some(code) = &mut self.menu.code_search {
            code.latest = Some(index);
            code.status = Status::Ready;
        }
        self.rerank_code_search(cx);
    }

    fn fail_code_search(&mut self, error: crate::Error, cx: &mut Context<Self>) {
        if let Some(code) = &mut self.menu.code_search
            && code.latest.is_none()
        {
            code.status = Status::Failed(error.to_string());
        }
        cx.notify();
    }

    /// Ranks the index against the query on the background executor. A
    /// newer ranking drops an unfinished one with its task.
    fn rank_code_search(&mut self, cx: &mut Context<Self>) {
        self.rank_code_search_keeping(false, cx);
    }

    /// [`Self::rank_code_search`] for a refreshed index: the query has not
    /// changed, so the row the user moved to stays selected wherever it
    /// lands, rather than Enter opening another one after a background read.
    fn rerank_code_search(&mut self, cx: &mut Context<Self>) {
        self.rank_code_search_keeping(true, cx);
    }

    fn rank_code_search_keeping(&mut self, keep: bool, cx: &mut Context<Self>) {
        let Some(code) = &mut self.menu.code_search else {
            return;
        };
        let Some(index) = code.latest.clone() else {
            return;
        };
        let (mode, query, token) = (code.mode, code.query(), code.search.clone());
        let ranked = index.clone();
        let ranking = cx
            .background_executor()
            .spawn(async move { rank::rank(&ranked, mode, &query) });
        code.rank_task = Some(cx.spawn(async move |this, cx| {
            let hits = ranking.await;
            let _ = this.update(cx, |this, cx| {
                let Some(code) = &mut this.menu.code_search else {
                    return;
                };
                if code.search != token || code.mode != mode {
                    return;
                }
                let chosen = keep
                    .then(|| {
                        code.hits
                            .get(code.selected)
                            .and_then(|hit| code.target(hit))
                    })
                    .flatten();
                code.index = Some(index);
                code.hits = hits;
                code.rank_task = None;
                let kept = chosen.and_then(|chosen| {
                    code.hits
                        .iter()
                        .position(|hit| code.target(hit).as_ref() == Some(&chosen))
                });
                code.selected = kept.unwrap_or(0);
                match kept {
                    Some(row) => code.scroll.scroll_to_item(row, ScrollStrategy::Center),
                    None => code.scroll.scroll_to_item(0, ScrollStrategy::Top),
                }
                cx.notify();
            });
        }));
    }

    pub(crate) fn set_code_search_mode(
        &mut self,
        mode: Mode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(code) = &mut self.menu.code_search else {
            return;
        };
        code.mode = mode;
        code.hits.clear();
        let search = code.search.clone();
        search.update(cx, |input, cx| {
            input.set_placeholder(placeholder(mode), cx);
            window.focus(&input.focus, cx);
        });
        self.rank_code_search(cx);
        cx.notify();
    }

    /// Opens row `row`: in the editor, or with `in_viewer` in a code tab.
    fn activate_code_search(
        &mut self,
        row: usize,
        in_viewer: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(code) = &self.menu.code_search else {
            return;
        };
        let Some(target) = code.hits.get(row).and_then(|hit| code.target(hit)) else {
            return;
        };
        let beside = code.beside.clone();
        self.dismiss_menu(window, cx);
        if in_viewer || !crate::editor::SUPPORTED {
            self.open_code_view(&target, window, cx);
        } else {
            self.open_in_editor(&target, beside.as_deref(), cx);
        }
    }

    pub(crate) fn code_search_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(code) = &mut self.menu.code_search else {
            return;
        };
        if code.search.read(cx).is_composing() {
            return;
        }
        let keystroke = &event.keystroke;
        let handled = match keystroke.key.as_str() {
            "escape" => {
                self.dismiss_menu(window, cx);
                true
            }
            key @ ("up" | "down") if !code.hits.is_empty() => {
                let count = code.hits.len();
                let step = if key == "up" { count - 1 } else { 1 };
                code.selected = (code.selected + step) % count;
                code.scroll
                    .scroll_to_item(code.selected, ScrollStrategy::Center);
                cx.notify();
                true
            }
            "tab" => {
                let mode = match code.mode {
                    Mode::Symbols => Mode::Files,
                    Mode::Files => Mode::Symbols,
                };
                self.set_code_search_mode(mode, window, cx);
                true
            }
            "enter" => {
                let row = code.selected;
                self.activate_code_search(row, keystroke.modifiers.secondary(), window, cx);
                true
            }
            _ => false,
        };
        if handled {
            cx.stop_propagation();
            window.prevent_default();
        }
    }
}

fn placeholder(mode: Mode) -> &'static str {
    match mode {
        Mode::Symbols => "Go to a function, type, or module...",
        Mode::Files => "Go to a file, or file:line...",
    }
}

#[cfg(test)]
mod tests;
