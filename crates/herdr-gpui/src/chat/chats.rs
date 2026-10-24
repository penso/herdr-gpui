//! The chats a window attaches to: one per chat pane of its daemon, each
//! with its connection, retries, and the view state drawn from it.

use super::{
    Error, MAX_FAILURES, Result, Target,
    link::{Dial, Link},
    list::{self, Splice},
    prepared::Prepared,
    session::Session,
};
use crate::search_input::SearchInput;
use gpui::{Entity, ListAlignment, ListState, px};
use herdr_chat_protocol::{ClientMessage, Command, Cursor, EndpointId, EntryId};
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

/// Messages applied per pane per poll, so a burst cannot stall a frame.
const POLL_BUDGET: usize = 512;
/// How far past the viewport the entry list lays rows out.
const OVERDRAW: f32 = 400.;
const FIRST_RETRY: Duration = Duration::from_millis(250);
const LAST_RETRY: Duration = Duration::from_secs(5);

/// Starts a connection to `target`'s program; tests connect elsewhere.
pub(crate) type Connect<'a> =
    dyn FnMut(&Target, Option<Cursor>, Vec<ClientMessage>) -> Result<Link> + 'a;

/// The connector the app uses: the program's endpoint by the id its pane
/// reported, a socket in this user's private directory or a named pipe, or
/// for a chat on an SSH host, `herdr-acp connect` run there over `ssh`.
pub(crate) fn connect(
    target: &Target,
    since: Option<Cursor>,
    resend: Vec<ClientMessage>,
) -> Result<Link> {
    let dial = match &target.route {
        super::Route::Local => Dial::Socket(
            herdr_chat_protocol::endpoint_address(&target.endpoint).ok_or(Error::NoEndpointDir)?,
        ),
        super::Route::Ssh(host) => {
            let script = super::remote_program_script(&format!("connect {}", target.endpoint));
            Dial::Process(herdr_client::script_command(host, &script).map_err(Error::Remote)?)
        }
    };
    Link::start(dial, since, resend)
}

/// The menu open over a chat's header.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Menu {
    /// The agent's older list of session modes.
    Modes,
    /// The choices of one setting, by id.
    Setting(String),
    /// Every setting the header has no button for.
    Settings,
}

pub(crate) struct ChatPane {
    /// Files the next prompt carries, in the order they were added.
    pub(crate) attachments: Vec<std::path::PathBuf>,
    /// Which slash-command completion is selected.
    pub(crate) completion: usize,
    pub(crate) composer: Option<Entity<SearchInput>>,
    /// Lines written above the composer's own, joined into one prompt.
    pub(crate) draft: Vec<String>,
    /// The last command this window could not send.
    pub(crate) error: Option<String>,
    /// Thoughts shown in full.
    pub(crate) expanded: HashSet<EntryId>,
    failures: u32,
    /// Prompts sent from this window, for Up and Down to recall.
    pub(crate) history: herdr_chat_protocol::assist::History,
    /// Bumped for each connection, so nothing of an earlier one is applied.
    generation: u64,
    /// How the list follows a streaming reply.
    pub(crate) glide: super::glide::Glide,
    link: Option<(u64, Link)>,
    pub(crate) list: ListState,
    list_keys: Vec<(EntryId, u64)>,
    list_snapshots: u64,
    pub(crate) menu: Option<Menu>,
    /// The search field over a long menu while one is open, with the
    /// subscription that redraws the menu as it changes.
    pub(crate) menu_search: Option<(Entity<SearchInput>, gpui::Subscription)>,
    /// The web pages live in the chat's messages, oldest first.
    pub(crate) pages: Vec<super::page::ChatPage>,
    /// Trying again cannot help: the program speaks another version.
    permanent: bool,
    pub(crate) prepared: Prepared,
    /// Whether the plan and changed files show beside the entries.
    pub(crate) rail: bool,
    retry_at: Option<Instant>,
    pub(crate) session: Session,
    pub(crate) target: Target,
    /// Files being copied to the chat's SSH host to attach there.
    pub(crate) uploading: usize,
}

impl ChatPane {
    fn new(target: Target) -> Self {
        // `glide` follows the end rather than the list's own tail mode,
        // which jumps there with each chunk.
        let list = ListState::new(0, ListAlignment::Top, px(OVERDRAW));
        let glide = super::glide::Glide::new(&list);
        Self {
            attachments: Vec::new(),
            completion: 0,
            composer: None,
            draft: Vec::new(),
            error: None,
            expanded: HashSet::new(),
            failures: 0,
            generation: 0,
            glide,
            history: Default::default(),
            link: None,
            list,
            list_keys: Vec::new(),
            list_snapshots: 0,
            menu: None,
            menu_search: None,
            pages: Vec::new(),
            permanent: false,
            prepared: Prepared::default(),
            rail: true,
            retry_at: None,
            session: Session::default(),
            target,
            uploading: 0,
        }
    }

    /// Whether the view can stand in for the pane's terminal: it has a
    /// transcript, and its program is reachable or only briefly away.
    pub(crate) fn available(&self) -> bool {
        self.session.synced() && !self.permanent && self.failures < MAX_FAILURES
    }

    pub(crate) fn attached(&self) -> bool {
        self.link.is_some() && self.session.live()
    }

    #[cfg(test)]
    pub(crate) fn has_link(&self) -> bool {
        self.link.is_some()
    }

    #[cfg(test)]
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    /// Queues `command` for the program. It waits for its acknowledgement
    /// either way, so a command sent while reconnecting goes out with the
    /// next connection.
    pub(crate) fn send(&mut self, command: Command) -> Result<()> {
        let message = self.session.submit(command)?;
        let Some((_, link)) = &self.link else {
            return Ok(());
        };
        match link.send(message) {
            Err(Error::NotAttached) | Ok(()) => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn start(&mut self, now: Instant, connect: &mut Connect<'_>) {
        self.retry_at = None;
        self.generation += 1;
        self.session.begin();
        let resend = self.session.resend();
        match connect(&self.target, self.session.since(), resend) {
            Ok(link) => self.link = Some((self.generation, link)),
            Err(error) => self.fail(&error, now),
        }
    }

    fn fail(&mut self, error: &Error, now: Instant) {
        tracing::debug!(%error, pane = %self.target.pane_id, "Chat connection ended");
        self.link = None;
        self.session.lost(error);
        self.menu = None;
        if error.permanent() {
            self.permanent = true;
            return;
        }
        // A stream that stopped fitting is fetched again at once, as a
        // snapshot; anything else backs off.
        let delay = if error.desynced() {
            Duration::ZERO
        } else {
            self.failures = self.failures.saturating_add(1);
            FIRST_RETRY
                .saturating_mul(1 << self.failures.min(5))
                .min(LAST_RETRY)
        };
        self.retry_at = Some(now + delay);
    }

    /// Applies what the link read, returning whether anything shown changed.
    fn poll(&mut self, now: Instant) -> bool {
        let Some((generation, link)) = &self.link else {
            return false;
        };
        if *generation != self.generation {
            self.link = None;
            return false;
        }
        let polled = link.poll(POLL_BUDGET);
        let mut changed = false;
        let mut end = None;
        for message in polled.messages {
            match self.session.receive(message) {
                Ok(shown) => changed |= shown,
                Err(error) => {
                    end = Some(error);
                    break;
                }
            }
        }
        if self.session.live() {
            self.failures = 0;
        }
        if let Some(error) = end.or(polled.end) {
            self.fail(&error, now);
            changed = true;
        }
        if changed {
            self.sync_view();
        }
        changed
    }

    /// Brings the list and the prepared bodies up to the transcript.
    fn sync_view(&mut self) {
        let keys = self.session.keys();
        if self.list_snapshots != self.session.snapshots() {
            self.list_snapshots = self.session.snapshots();
            self.prepared.clear();
            self.list.reset(keys.len());
            self.glide.reset(&self.list);
        } else {
            match list::splices(&self.list_keys, &keys) {
                Some(splices) => {
                    for Splice { range, count } in splices {
                        super::glide::splice_in_place(&self.list, range, count);
                    }
                }
                None => {
                    self.list.reset(keys.len());
                    self.glide.reset(&self.list);
                }
            }
        }
        self.prepared.update(self.session.transcript(), &keys);
        self.list_keys = keys;
    }

    /// Redraws one entry, such as a thought folding open.
    pub(crate) fn remeasure(&self, id: EntryId) {
        if let Some(index) = self.list_keys.iter().position(|(entry, _)| *entry == id) {
            super::glide::splice_in_place(&self.list, index..index + 1, 1);
        }
    }
}

/// A tab strip's deck of agents under "+": how far it is dealt, and where
/// the pointer is. Over "+" or over the cards, it stays dealt.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LauncherDeck {
    pub(crate) group: crate::browser::GroupId,
    pub(crate) over_cards: bool,
    pub(crate) over_plus: bool,
    pub(crate) toggle: crate::motion::Toggle,
    /// The side it deals toward, chosen as it starts to deal.
    pub(crate) toward: Toward,
}

/// The side of "+" a deck deals its agents out to: the right while the
/// window has room there for all of them, else the left, over the tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Toward {
    Left,
    #[default]
    Right,
}

/// Every chat pane of the window's daemon, in its snapshot's order.
#[derive(Default)]
pub(crate) struct Chats {
    /// The agents installed on each host new chats may start on.
    pub(crate) agents: super::harnesses::Harnesses,
    /// The agent a chat was last started with, which the strips' one-button
    /// launcher offers first.
    pub(crate) last_started: Option<herdr_chat_protocol::Agent>,
    /// The strip whose launcher shows its hidden agents under the pointer.
    pub(crate) launcher_open: Option<crate::browser::GroupId>,
    /// The strip whose "+" deals its agents out, and how far it has.
    pub(crate) launcher_deck: Option<LauncherDeck>,
    /// The layout the lists' rows were last measured in.
    measured: crate::config::ChatLayoutMode,
    /// Chat pages opened so far, which numbers the next one.
    #[cfg(any(target_os = "macos", windows))]
    pub(crate) opened_pages: u64,
    /// Pages of panes no longer attached, for the window to close.
    #[cfg(any(target_os = "macos", windows))]
    closed_pages: Vec<crate::browser::TabId>,
    /// The chat pages the window presented this frame, for the check that
    /// asks for a frame when they moved.
    #[cfg(any(target_os = "macos", windows))]
    pub(crate) presented: std::rc::Rc<std::cell::RefCell<Vec<crate::browser::TabId>>>,
    panes: Vec<ChatPane>,
    /// The `herdr-acp` new chats run.
    pub(crate) program: super::Program,
    /// Programs whose panes the user turned back to their terminal.
    raw: HashSet<EndpointId>,
    /// Textures of panes no longer attached, for the window to drop.
    released: Vec<std::sync::Arc<gpui::RenderImage>>,
    /// The second a running turn's timer last redrew at.
    pub(crate) ticked: u64,
    /// A new chat whose tab is being made.
    pub(crate) starting: Option<super::start::Starting>,
}

impl Chats {
    /// Measures every list's rows again once the layout they are drawn in
    /// changed, since each layout gives a row its own height. Says whether
    /// it did.
    pub(crate) fn relayout(&mut self, layout: crate::config::ChatLayoutMode) -> bool {
        if self.measured == layout {
            return false;
        }
        self.measured = layout;
        for pane in &self.panes {
            pane.list.remeasure();
        }
        true
    }

    /// Moves each list a frame's step toward its end while it follows a
    /// reply, or with `smooth` off all the way there, and says whether to
    /// draw another frame.
    pub(crate) fn glide(&mut self, now: Instant, smooth: bool) -> bool {
        let mut more = false;
        for pane in &mut self.panes {
            more |= pane.glide.tick(&pane.list, now, smooth);
        }
        more
    }

    /// Attaches to `targets` and detaches from every other pane, then
    /// starts the connections that are due.
    pub(crate) fn reconcile(
        &mut self,
        targets: Vec<Target>,
        now: Instant,
        connect: &mut Connect<'_>,
    ) -> bool {
        let before = self.panes.len();
        let released = &mut self.released;
        #[cfg(any(target_os = "macos", windows))]
        let closed_pages = &mut self.closed_pages;
        self.panes.retain_mut(|pane| {
            let keep = targets.contains(&pane.target);
            if !keep {
                pane.prepared.visuals.clear();
                released.extend(pane.prepared.visuals.take_released());
                #[cfg(any(target_os = "macos", windows))]
                closed_pages.extend(pane.pages.drain(..).map(|page| page.id));
            }
            keep
        });
        let mut changed = self.panes.len() != before;
        let mut panes = Vec::with_capacity(targets.len());
        for target in targets {
            let index = self.panes.iter().position(|pane| pane.target == target);
            panes.push(match index {
                Some(index) => self.panes.swap_remove(index),
                None => {
                    changed = true;
                    ChatPane::new(target)
                }
            });
        }
        self.panes = panes;
        let endpoints: HashSet<&EndpointId> = self
            .panes
            .iter()
            .map(|pane| &pane.target.endpoint)
            .collect();
        self.raw.retain(|endpoint| endpoints.contains(endpoint));
        for pane in &mut self.panes {
            let due =
                pane.link.is_none() && !pane.permanent && pane.retry_at.is_none_or(|at| at <= now);
            if due {
                pane.start(now, connect);
            }
        }
        changed
    }

    /// Applies what every link read; whether anything shown changed.
    pub(crate) fn poll(&mut self, now: Instant) -> bool {
        self.panes
            .iter_mut()
            .fold(false, |changed, pane| pane.poll(now) | changed)
    }

    /// Chat pages whose panes went away.
    #[cfg(any(target_os = "macos", windows))]
    pub(crate) fn take_closed_pages(&mut self) -> Vec<crate::browser::TabId> {
        std::mem::take(&mut self.closed_pages)
    }

    pub(crate) fn take_released(&mut self) -> Vec<std::sync::Arc<gpui::RenderImage>> {
        std::mem::take(&mut self.released)
    }

    pub(crate) fn panes(&self) -> impl Iterator<Item = &ChatPane> {
        self.panes.iter()
    }

    pub(crate) fn panes_mut(&mut self) -> impl Iterator<Item = &mut ChatPane> {
        self.panes.iter_mut()
    }

    pub(crate) fn pane(&self, pane_id: &str) -> Option<&ChatPane> {
        self.panes
            .iter()
            .find(|pane| pane.target.pane_id == pane_id)
    }

    pub(crate) fn pane_mut(&mut self, pane_id: &str) -> Option<&mut ChatPane> {
        self.panes
            .iter_mut()
            .find(|pane| pane.target.pane_id == pane_id)
    }

    /// Whether `pane_id` shows the chat view rather than its terminal.
    pub(crate) fn shown(&self, pane_id: &str) -> bool {
        self.pane(pane_id)
            .is_some_and(|pane| pane.available() && !self.raw.contains(&pane.target.endpoint))
    }

    /// Whether the user turned `pane_id` back to its terminal.
    pub(crate) fn raw(&self, pane_id: &str) -> bool {
        self.pane(pane_id)
            .is_some_and(|pane| self.raw.contains(&pane.target.endpoint))
    }

    /// Shows the chat view over `pane_id`, or its terminal; `false` when the
    /// pane has no chat to show.
    pub(crate) fn set_raw(&mut self, pane_id: &str, raw: bool) -> bool {
        let Some(endpoint) = self.pane(pane_id).map(|pane| pane.target.endpoint.clone()) else {
            return false;
        };
        if raw {
            self.raw.insert(endpoint);
        } else {
            self.raw.remove(&endpoint);
        }
        true
    }
}
