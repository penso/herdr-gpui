//! The file tree tab: a local checkout's files beside the workspace's
//! terminals, each marked with how Git sees it, so looking around what an
//! agent is working on takes neither a name to remember nor another editor.
//! Choosing a file opens it in a code tab.
//!
//! The tab keeps only the checkout; the window keeps each tab's view, made
//! when the tab opens or is first seen after a restart. The listing and the
//! tree built from it are read on the background executor, as is the filter
//! over them, and read again when the checkout's changes move, as a review
//! is. Drawing only reads what those produced. Like Go to File it is local
//! only: a remote pane's files are not on this machine.

// i18n: migrated
mod render;

use crate::{
    SheprdWindow,
    browser::{Location, ReviewCheckout, Slot, Store, Tab, TabId},
    editor::EditorTarget,
    git::Status,
    i18n::{Localize, fl},
    pull_request::Input,
    review::tree::{Node, Tree},
    search_input::{self, SearchInput},
    window::Flash,
};
use gpui::{prelude::*, *};
use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{CaseMatching, Normalization, Pattern},
};
use sheprd_vcs::file_tree::{Change, Entry, Listing};
use std::{
    collections::HashSet,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// How long finding the checkout may take before the listing's own limits.
const LOCATE_TIMEOUT: Duration = Duration::from_secs(30);
/// How often a listing is read again with no change in the status to say so.
const REREAD: Duration = Duration::from_secs(15);

/// A listing, and what drawing it needs, worked out in the background.
struct Prepared {
    listing: Listing,
    tree: Tree,
    /// By node: whether a folder holds a change.
    changed: Vec<bool>,
}

impl Prepared {
    fn of(listing: Listing) -> Self {
        let tree = Tree::of(listing.entries().iter().map(|entry| entry.path.as_str()));
        let mut changed = vec![false; tree.nodes.len()];
        // The folders above the next node.
        let mut above: Vec<usize> = Vec::new();
        for (index, node) in tree.nodes.iter().enumerate() {
            above.truncate(node.depth());
            match node {
                Node::Folder { .. } => above.push(index),
                Node::File { file, .. } => {
                    if listing.entries()[*file].change.is_some() {
                        for folder in &above {
                            changed[*folder] = true;
                        }
                    }
                }
            }
        }
        Self {
            listing,
            tree,
            changed,
        }
    }

    fn entry(&self, file: usize) -> Option<&Entry> {
        self.listing.entries().get(file)
    }

    /// What identifies `node` across listings.
    fn key(&self, node: usize) -> Option<&str> {
        match self.tree.nodes.get(node)? {
            Node::Folder { path, .. } => Some(path),
            Node::File { file, .. } => self.entry(*file).map(|entry| entry.path.as_str()),
        }
    }

    fn folders(&self) -> impl Iterator<Item = (usize, &String)> {
        self.tree
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| match node {
                Node::Folder { path, .. } => Some((index, path)),
                Node::File { .. } => None,
            })
    }
}

/// By file, whether its path matches `query`; `None` when nothing filters.
/// nucleo's fzf scoring, as Go to File uses, for what it finds, not its
/// order: the tree keeps its own.
fn matches(listing: &Listing, query: &str) -> Option<Vec<bool>> {
    let query = query.trim();
    if query.is_empty() {
        return None;
    }
    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut buffer = Vec::new();
    Some(
        listing
            .entries()
            .iter()
            .map(|entry| {
                pattern
                    .score(Utf32Str::new(&entry.path, &mut buffer), &mut matcher)
                    .is_some()
            })
            .collect(),
    )
}

enum State {
    Loading,
    Ready,
    Failed(String),
}

pub(crate) struct FileTree {
    /// The tab's own handle; typing in it moves to the filter.
    pub(crate) focus: FocusHandle,
    checkout: Input,
    state: State,
    /// What the rows show, and what files `matched` covers: a row's numbers
    /// mean nothing in any other listing.
    prepared: Option<Arc<Prepared>>,
    /// The newest listing, which the next filter reads. It replaces
    /// `prepared` only together with that filter's result.
    latest: Option<Arc<Prepared>>,
    matched: Option<Vec<bool>>,
    /// Folders closed, and every folder known: a new one starts closed
    /// unless it holds a change.
    closed: HashSet<String>,
    known: HashSet<String>,
    /// The nodes the list shows, and the row the arrows are on.
    shown: Vec<usize>,
    picked: Option<usize>,
    filter: Entity<SearchInput>,
    filter_task: Option<Task<()>>,
    scroll: UniformListScrollHandle,
    /// The newest read; an older one's result is dropped.
    load: u64,
    reading: bool,
    /// Stops the read nobody waits for any more.
    cancelled: Arc<AtomicBool>,
    /// The checkout's status when last read, to see its changes move.
    seen: Option<Status>,
    /// When it was last read, for what the status does not count.
    read_at: Option<Instant>,
    _subscription: Subscription,
}

impl Drop for FileTree {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

impl FileTree {
    /// Takes `prepared` and its filter's `matched` as what the rows show,
    /// staying on the row it was on wherever that landed.
    fn adopt(&mut self, prepared: Arc<Prepared>, matched: Option<Vec<bool>>) {
        let kept = self
            .picked
            .and_then(|row| self.prepared.as_ref()?.key(*self.shown.get(row)?))
            .map(str::to_owned);
        let folders: HashSet<String> = prepared.folders().map(|(_, path)| path.clone()).collect();
        for (node, path) in prepared.folders() {
            if !self.known.contains(path) && !prepared.changed[node] {
                self.closed.insert(path.clone());
            }
        }
        self.closed.retain(|path| folders.contains(path));
        self.known = folders;
        self.prepared = Some(prepared);
        self.matched = matched;
        self.state = State::Ready;
        self.refresh_shown();
        self.picked = kept.and_then(|kept| {
            let prepared = self.prepared.as_ref()?;
            self.shown
                .iter()
                .position(|node| prepared.key(*node) == Some(kept.as_str()))
        });
    }

    /// Works out which nodes the list shows, after the listing, the filter
    /// or the folders changed.
    fn refresh_shown(&mut self) {
        let Some(prepared) = &self.prepared else {
            self.shown.clear();
            return;
        };
        let matched = self.matched.as_deref();
        self.shown = prepared
            .tree
            .shown(&self.closed, matched.is_some(), |file| {
                matched.is_none_or(|matched| matched.get(file).copied().unwrap_or(false))
            });
        self.picked = self.picked.filter(|picked| *picked < self.shown.len());
    }

    /// The first row showing a file, which a filter starts on.
    fn first_file(&self) -> Option<usize> {
        let prepared = self.prepared.as_ref()?;
        self.shown
            .iter()
            .position(|node| matches!(prepared.tree.nodes.get(*node), Some(Node::File { .. })))
    }

    fn file_count(&self) -> usize {
        self.prepared
            .as_ref()
            .map_or(0, |prepared| prepared.listing.entries().len())
    }

    /// Opens or closes the folder at `node`.
    fn toggle_folder(&mut self, node: usize) {
        let Some(Node::Folder { path, .. }) = self
            .prepared
            .as_ref()
            .and_then(|prepared| prepared.tree.nodes.get(node))
        else {
            return;
        };
        let path = path.clone();
        if !self.closed.remove(&path) {
            self.closed.insert(path);
        }
        self.refresh_shown();
    }

    /// What row `row` is.
    fn node_at(&self, row: usize) -> Option<&Node> {
        self.prepared
            .as_ref()?
            .tree
            .nodes
            .get(*self.shown.get(row)?)
    }

    /// The folder on `row`, and whether it is closed.
    fn folder_at(&self, row: usize) -> Option<bool> {
        match self.node_at(row)? {
            Node::Folder { path, .. } => Some(self.closed.contains(path)),
            Node::File { .. } => None,
        }
    }

    /// Moves the picked row by one, wrapping.
    fn step(&mut self, back: bool) {
        let count = self.shown.len();
        if count == 0 {
            return;
        }
        let row = match (self.picked, back) {
            (None, false) => 0,
            (None, true) => count - 1,
            (Some(row), false) => (row + 1) % count,
            (Some(row), true) => (row + count - 1) % count,
        };
        self.picked = Some(row);
        self.scroll.scroll_to_item(row, ScrollStrategy::Center);
    }
}

impl SheprdWindow {
    /// Opens the file tree tab of the checkout the Git chip follows, or
    /// brings back the one already open, and reads it again.
    pub(crate) fn open_file_tree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_is_remote() {
            self.show_flash(Flash::warning(fl!("file-tree-local-only")), cx);
            return;
        }
        let Some(checkout) = self
            .git
            .tracked()
            .and_then(|input| ReviewCheckout::try_from(input).ok())
        else {
            self.show_flash(Flash::warning(fl!("file-tree-no-checkout")), cx);
            return;
        };
        let Some((scope, workspace)) = self.browser_key() else {
            self.show_flash(Flash::warning(fl!("file-tree-no-workspace")), cx);
            return;
        };
        let existing = cx.try_global::<Store>().and_then(|store| {
            store
                .in_workspace(&scope, &workspace)
                .find(|tab| {
                    matches!(&tab.location, Some(Location::Files { checkout: open }) if *open == checkout)
                })
                .map(|tab| tab.id)
        });
        let opened = existing.or_else(|| {
            Store::update(cx, |store| {
                store.open(scope, &workspace, Some(Location::Files { checkout }), None)
            })
        });
        let Some(id) = opened else {
            self.show_flash(Flash::warning(fl!("file-tree-too-many-tabs")), cx);
            return;
        };
        self.dismiss_menu(window, cx);
        self.ensure_file_tree(id, cx);
        self.load_file_tree(id, cx);
        self.show_browser_tab(id, window, cx);
        if let Some(view) = self.file_trees.get(&id) {
            let focus = view.filter.read(cx).focus.clone();
            window.focus(&focus, cx);
        }
    }

    /// Makes `id`'s view from its tab, if the window has none yet. Whether
    /// it was made.
    fn ensure_file_tree(&mut self, id: TabId, cx: &mut Context<Self>) -> bool {
        if self.file_trees.contains_key(&id) {
            return false;
        }
        let Some(Location::Files { checkout }) = cx
            .try_global::<Store>()
            .and_then(|store| store.get(id))
            .and_then(|tab| tab.location.clone())
        else {
            return false;
        };
        let (ui, theme) = (self.config.ui.clone(), self.theme.clone());
        let filter = cx.new(|cx| {
            let mut input = SearchInput::new(cx);
            input.set_placeholder_with(|| fl!("file-tree-filter"), cx);
            input.set_appearance(ui, theme, cx);
            input
        });
        let subscription = cx.subscribe(&filter, move |this, _, _: &search_input::Changed, cx| {
            this.filter_file_tree(id, cx);
        });
        self.file_trees.insert(
            id,
            FileTree {
                focus: cx.focus_handle(),
                checkout: Input::from(&checkout),
                state: State::Loading,
                prepared: None,
                latest: None,
                matched: None,
                closed: HashSet::new(),
                known: HashSet::new(),
                shown: Vec::new(),
                picked: None,
                filter,
                filter_task: None,
                scroll: UniformListScrollHandle::new(),
                load: 0,
                reading: false,
                read_at: None,
                cancelled: Arc::default(),
                seen: None,
                _subscription: subscription,
            },
        );
        true
    }

    /// Lists the tab's checkout off the UI thread. A newer read stops an
    /// older one; the rows stay until the new listing lands.
    fn load_file_tree(&mut self, id: TabId, cx: &mut Context<Self>) {
        let status = self.git.status();
        let Some(view) = self.file_trees.get_mut(&id) else {
            return;
        };
        view.load += 1;
        view.reading = true;
        view.seen = status;
        view.read_at = Some(Instant::now());
        view.cancelled.store(true, Ordering::Release);
        view.cancelled = Arc::default();
        let (load, input, cancelled) = (view.load, view.checkout.clone(), view.cancelled.clone());
        let reading = cx.background_executor().spawn(async move {
            let stop = || cancelled.load(Ordering::Acquire);
            let deadline = Instant::now() + LOCATE_TIMEOUT;
            let root = crate::pull_request::local_checkout(&input, deadline, &stop)?;
            let listing = Listing::read(Path::new(&root), &cancelled)?;
            crate::Result::Ok(Arc::new(Prepared::of(listing)))
        });
        cx.spawn(async move |this, cx| {
            let read = reading.await;
            let _ = this.update(cx, |this, cx| this.file_tree_read(id, load, read, cx));
        })
        .detach();
        cx.notify();
    }

    fn file_tree_read(
        &mut self,
        id: TabId,
        load: u64,
        read: crate::Result<Arc<Prepared>>,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self
            .file_trees
            .get_mut(&id)
            .filter(|view| view.load == load)
        else {
            return;
        };
        view.reading = false;
        match read {
            Ok(prepared) => {
                view.latest = Some(prepared);
                self.filter_file_tree(id, cx);
            }
            // Rows already shown are still the best known.
            Err(error) if view.prepared.is_some() => {
                tracing::warn!(%error, "Could not read the file tree again");
            }
            Err(error) => view.state = State::Failed(error.localize()),
        }
        cx.notify();
    }

    /// Filters the newest listing on the background executor, then shows
    /// the two together. A newer filter drops an unfinished one with its
    /// task; it reads the same newest listing, so none is lost.
    fn filter_file_tree(&mut self, id: TabId, cx: &mut Context<Self>) {
        let Some(view) = self.file_trees.get_mut(&id) else {
            return;
        };
        let Some(prepared) = view.latest.clone() else {
            return;
        };
        let query = view.filter.read(cx).text().to_owned();
        // Only a new query moves the pick; a refreshed listing keeps it.
        let refilter = view
            .prepared
            .as_ref()
            .is_some_and(|shown| Arc::ptr_eq(shown, &prepared));
        let read = prepared.clone();
        let filtering = cx
            .background_executor()
            .spawn(async move { matches(&read.listing, &query) });
        view.filter_task = Some(cx.spawn(async move |this, cx| {
            let matched = filtering.await;
            let _ = this.update(cx, |this, cx| {
                let Some(view) = this.file_trees.get_mut(&id) else {
                    return;
                };
                view.adopt(prepared, matched);
                if refilter {
                    view.picked = view.matched.is_some().then(|| view.first_file()).flatten();
                    view.scroll.scroll_to_item(0, ScrollStrategy::Top);
                }
                view.filter_task = None;
                cx.notify();
            });
        }));
    }

    /// Runs every window tick: file tree tabs of the focused workspace with
    /// no view yet, such as ones restored from the last run, get one, the
    /// views of closed tabs go, and a checkout whose changes moved is read
    /// again. So is one left alone for a while: the status counts a new
    /// folder once, however many files then arrive in it.
    pub(crate) fn poll_file_trees(&mut self, cx: &mut Context<Self>) {
        let Some(store) = cx.try_global::<Store>() else {
            self.file_trees.clear();
            return;
        };
        self.file_trees.retain(|id, _| store.get(*id).is_some());
        let Some((scope, workspace)) = self.browser_key() else {
            return;
        };
        let missing: Vec<TabId> = store
            .in_workspace(&scope, &workspace)
            .filter(|tab| {
                matches!(tab.location, Some(Location::Files { .. }))
                    && !self.file_trees.contains_key(&tab.id)
            })
            .map(|tab| tab.id)
            .collect();
        for id in missing {
            if self.ensure_file_tree(id, cx) {
                self.load_file_tree(id, cx);
            }
        }
        let (Some(tracked), Some(status)) = (self.git.tracked(), self.git.status()) else {
            return;
        };
        let now = Instant::now();
        let moved: Vec<TabId> = self
            .file_trees
            .iter()
            .filter(|(_, view)| {
                let stale = view
                    .read_at
                    .is_none_or(|read| now.duration_since(read) >= REREAD);
                view.checkout == *tracked && !view.reading && (stale || view.seen != Some(status))
            })
            .map(|(id, _)| *id)
            .collect();
        for id in moved {
            self.load_file_tree(id, cx);
        }
    }

    /// Opens the file on `row` in a code tab, or opens or closes its folder.
    /// A deleted file has nothing to open.
    fn activate_file_tree_row(
        &mut self,
        id: TabId,
        row: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.file_trees.get_mut(&id) else {
            return;
        };
        view.picked = Some(row);
        let Some(prepared) = view.prepared.clone() else {
            return;
        };
        let Some(&node) = view.shown.get(row) else {
            return;
        };
        match prepared.tree.nodes.get(node) {
            Some(Node::Folder { .. }) => view.toggle_folder(node),
            Some(Node::File { file, .. }) => {
                let Some(entry) = prepared.entry(*file) else {
                    return;
                };
                if entry.change != Some(Change::Deleted) {
                    let target = EditorTarget {
                        path: prepared.listing.root().join(&entry.path),
                        line: None,
                    };
                    self.open_code_view(&target, window, cx);
                }
            }
            None => {}
        }
        cx.notify();
    }

    /// Up and Down pick a row, Enter opens it, Left and Right close and
    /// open a folder while the filter has no text of its own to move
    /// through, and Escape clears the filter. Text typed on the tab itself
    /// goes to the filter. Whether the key was the tab's.
    fn file_tree_key(
        &mut self,
        id: TabId,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(view) = self.file_trees.get_mut(&id) else {
            return false;
        };
        if view.filter.read(cx).is_composing() {
            return false;
        }
        let keystroke = &event.keystroke;
        let modifiers = keystroke.modifiers;
        if modifiers.control || modifiers.alt || modifiers.platform || modifiers.function {
            return false;
        }
        let empty = view.filter.read(cx).text().is_empty();
        match keystroke.key.as_str() {
            key @ ("up" | "down") => view.step(key == "up"),
            "enter" => {
                if let Some(row) = view.picked {
                    self.activate_file_tree_row(id, row, window, cx);
                }
            }
            key @ ("left" | "right") if empty => {
                let closing = key == "left";
                let Some(row) = view.picked else {
                    return false;
                };
                if view.folder_at(row) == Some(!closing) {
                    let node = view.shown[row];
                    view.toggle_folder(node);
                }
            }
            "escape" if !empty => {
                let filter = view.filter.clone();
                filter.update(cx, |input, cx| input.clear(cx));
            }
            _ if view.focus.is_focused(window) => {
                let Some(text) = keystroke.key_char.clone() else {
                    return false;
                };
                let filter = view.filter.clone();
                window.focus(&filter.read(cx).focus.clone(), cx);
                filter.update(cx, |input, cx| {
                    let text = format!("{}{text}", input.text());
                    input.set_text_at(&text, text.len(), cx);
                });
            }
            _ => return false,
        }
        cx.notify();
        true
    }

    /// A file tree tab, drawn in `slot` where a page would be.
    pub(crate) fn render_file_tree_tab(
        &self,
        slot: Slot,
        tab: &Tab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = tab.id;
        let Some(view) = self.file_trees.get(&id) else {
            return div()
                .id(SharedString::from(slot.selector("file-tree-loading")))
                .size_full()
                .into_any_element();
        };
        let focus = view.focus.clone();
        div()
            .id(SharedString::from(slot.selector("file-tree-tab")))
            .size_full()
            .min_w_0()
            .bg(rgb(self.theme.background))
            .track_focus(&focus)
            // The tab holds the keyboard while used, so typing never
            // reaches a terminal in another group.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    if let Some(view) = this.file_trees.get(&id) {
                        let focus = view.filter.read(cx).focus.clone();
                        window.focus(&focus, cx);
                    }
                }),
            )
            // Before the filter, which keeps Left and Right for its text.
            .capture_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if this.file_tree_key(id, event, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .child(self.render_file_tree(id, cx))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;
