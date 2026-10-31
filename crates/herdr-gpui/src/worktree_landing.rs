//! Where a new worktree lands: its shell, or an agent the window starts in
//! that shell. The new worktree dialog offers the agents installed on the
//! checkout's host, and the last choice made is offered first next time, in
//! every window and across runs.

use crate::{
    HerdrWindow,
    i18n::{Localize, fl},
    state_file,
    teleport::{AgentKind, Host},
};
use gpui::{
    AnyElement, App, Context, Global, InteractiveElement, IntoElement, ParentElement, Role,
    SharedString, StatefulInteractiveElement, Styled, Toggled, div, prelude::FluentBuilder, px,
    rgb,
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// The file only ever holds one short agent name.
const MAX_FILE_BYTES: u64 = 256;
const FILE: &str = "worktree-landing.json";
/// How long `herdr agent start` may wait for the agent to be ready, as fan-out allows.
const AGENT_START_TIMEOUT_MS: &str = "90000";

/// What a new worktree's first pane shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Landing {
    /// The shell Herdr opens in the checkout.
    #[default]
    Terminal,
    /// `kind`, started in that shell.
    Agent(AgentKind),
}

impl Landing {
    fn agent(self) -> Option<AgentKind> {
        match self {
            Self::Terminal => None,
            Self::Agent(kind) => Some(kind),
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Saved {
    /// The agent kind's name; see [`AgentKind::parse`]. Absent for the terminal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    agent: Option<String>,
}

impl From<Landing> for Saved {
    fn from(landing: Landing) -> Self {
        Self {
            agent: landing.agent().map(|kind| kind.name().to_owned()),
        }
    }
}

/// An unknown agent, from a newer or edited file, lands in the terminal.
fn parse(bytes: &[u8]) -> crate::Result<Landing> {
    let saved: Saved = serde_json::from_slice(bytes)?;
    Ok(saved
        .agent
        .as_deref()
        .and_then(AgentKind::parse)
        .map_or(Landing::Terminal, Landing::Agent))
}

/// The landing chosen last, kept for the next dialog.
#[derive(Default)]
pub(crate) struct Remembered {
    landing: Landing,
    writer: Option<state_file::Writer<Saved>>,
    quitting: bool,
}

impl Global for Remembered {}

impl Remembered {
    fn path() -> Option<PathBuf> {
        crate::preferences::state_dir().map(|dir| dir.join(FILE))
    }

    /// Called before starting GPUI. A missing or damaged file lands in the terminal.
    pub(crate) fn load() -> Self {
        Self::at(Self::path())
    }

    fn at(path: Option<PathBuf>) -> Self {
        let landing = path
            .as_deref()
            .map(|path| {
                state_file::read(path, MAX_FILE_BYTES)
                    .and_then(|bytes| bytes.as_deref().map_or(Ok(Landing::Terminal), parse))
            })
            .transpose()
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "Cannot restore the new worktree landing");
                None
            })
            .unwrap_or_default();
        let writer = path.and_then(|path| {
            state_file::Writer::start("worktree-landing", path)
                .inspect_err(|error| {
                    tracing::warn!(%error, "Cannot start the worktree landing worker");
                })
                .ok()
        });
        Self {
            landing,
            writer,
            quitting: false,
        }
    }

    pub(crate) fn install(self, cx: &mut App) {
        cx.set_global(self);
        cx.on_app_quit(|cx| {
            let remembered = cx.global_mut::<Self>();
            remembered.quitting = true;
            let writer = remembered.writer.take();
            cx.background_executor().spawn(async move {
                if let Some(writer) = writer {
                    writer.finish();
                }
            })
        })
        .detach();
    }

    /// The landing the next dialog offers first.
    pub(crate) fn landing(cx: &App) -> Landing {
        cx.try_global::<Self>()
            .map_or(Landing::Terminal, |remembered| remembered.landing)
    }

    /// Remember `landing`, creating an unsaved store for fixtures that never
    /// installed one.
    pub(crate) fn record(landing: Landing, cx: &mut App) {
        if !cx.has_global::<Self>() {
            cx.set_global(Self::default());
        }
        let remembered = cx.global_mut::<Self>();
        if remembered.landing == landing {
            return;
        }
        remembered.landing = landing;
        if remembered.quitting {
            return;
        }
        if let Some(writer) = &remembered.writer {
            writer.save(landing.into());
        }
    }

    #[cfg(test)]
    fn finish(&mut self) {
        if let Some(writer) = self.writer.take() {
            writer.finish();
        }
    }
}

/// The dialog's landing row: what is chosen, and the agents its host has.
pub(crate) struct Picker {
    pub(crate) chosen: Landing,
    /// The installed agents, in [`AgentKind::ALL`] order; `None` while the
    /// host is being looked at.
    installed: Option<Vec<AgentKind>>,
    host: Host,
    /// Set when the dialog closes, ending a lookup still running.
    cancelled: Arc<AtomicBool>,
}

impl Drop for Picker {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl Picker {
    pub(crate) fn new(host: Host, chosen: Landing) -> Self {
        Self {
            chosen,
            installed: None,
            host,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Whether the host is still being looked at.
    pub(crate) fn looking(&self) -> bool {
        self.installed.is_none()
    }

    /// Take the host's agents. One the host lacks cannot be chosen, so a
    /// remembered one that is missing lands in the terminal instead. A host
    /// that could not be looked at offers every agent rather than none; the
    /// start then reports one that is missing.
    pub(crate) fn offer(&mut self, installed: herdr_client::Result<Vec<AgentKind>>) {
        let installed = installed.unwrap_or_else(|error| {
            tracing::warn!(%error, "Cannot look up the host's agents");
            AgentKind::ALL.to_vec()
        });
        if self
            .chosen
            .agent()
            .is_some_and(|kind| !installed.contains(&kind))
        {
            self.chosen = Landing::Terminal;
        }
        self.installed = Some(installed);
    }

    /// The landings to show, the terminal first. While the host is looked
    /// at, the chosen agent shows alone so the remembered pick is visible.
    pub(crate) fn offered(&self) -> Vec<Landing> {
        let agents = match &self.installed {
            Some(installed) => installed.clone(),
            None => self.chosen.agent().into_iter().collect(),
        };
        std::iter::once(Landing::Terminal)
            .chain(agents.into_iter().map(Landing::Agent))
            .collect()
    }

    /// Choose `landing` when it is offered.
    pub(crate) fn choose(&mut self, landing: Landing) -> bool {
        let offered = self.offered().contains(&landing);
        if offered {
            self.chosen = landing;
        }
        offered
    }
}

/// The agents installed on `host`, in [`AgentKind::ALL`] order.
fn installed(host: &Host, cancelled: &AtomicBool) -> herdr_client::Result<Vec<AgentKind>> {
    let binaries = AgentKind::ALL.map(AgentKind::binary);
    let found = host.installed_programs(&binaries, cancelled)?;
    Ok(AgentKind::ALL
        .into_iter()
        .filter(|kind| found.iter().any(|binary| binary == kind.binary()))
        .collect())
}

/// A Herdr agent name for `kind` that no other agent uses: Herdr's names
/// start with a lowercase letter and hold at most 32 lowercase letters,
/// digits, and dashes.
fn agent_name(kind: AgentKind) -> String {
    let hex = uuid::Uuid::new_v4().simple().to_string();
    format!("{}-{}", kind.name(), &hex[..8])
}

/// The script line starting `kind` in `pane`. Herdr's CLI waits while the
/// new shell is still starting, which a bare `agent.start` refuses as busy.
fn start_line(kind: AgentKind, pane: &str) -> String {
    Host::herdr_line(&[
        "agent",
        "start",
        &agent_name(kind),
        "--kind",
        kind.name(),
        "--pane",
        pane,
        "--timeout",
        AGENT_START_TIMEOUT_MS,
    ])
}

impl HerdrWindow {
    /// The landing row for a new worktree dialog on the shown host, looking
    /// up its agents off the UI thread. `None` where the host cannot be
    /// scripted, so only its shell could be offered.
    pub(crate) fn open_landing(&mut self, cx: &mut Context<Self>) -> Option<Picker> {
        let selected = &self.endpoints[self.selected_endpoint];
        if !cfg!(any(target_os = "linux", target_os = "macos")) {
            return None;
        }
        let host = crate::teleport::host_for(&selected.connection.target).ok()?;
        let picker = Picker::new(host.clone(), Remembered::landing(cx));
        let cancelled = picker.cancelled.clone();
        let lookup = cx.background_executor().spawn({
            let cancelled = cancelled.clone();
            async move { installed(&host, &cancelled) }
        });
        cx.spawn(async move |this, cx| {
            let result = lookup.await;
            let _ = this.update(cx, |this, cx| {
                // Only the dialog that asked: a newer one has its own lookup.
                if let Some(picker) = &mut this.menu.landing
                    && Arc::ptr_eq(&picker.cancelled, &cancelled)
                {
                    picker.offer(result);
                    cx.notify();
                }
            });
        })
        .detach();
        Some(picker)
    }

    /// Land a created checkout as `picker` chose: remember the choice, and
    /// start the agent in `pane` off the UI thread, reporting a failure.
    pub(crate) fn land_worktree(&mut self, picker: &Picker, pane: &str, cx: &mut Context<Self>) {
        Remembered::record(picker.chosen, cx);
        let Some(kind) = picker.chosen.agent() else {
            return;
        };
        let (host, line) = (picker.host.clone(), start_line(kind, pane));
        // Not the dialog's flag: the dialog closes as the checkout opens.
        let started = cx
            .background_executor()
            .spawn(async move { host.capture(&line, &AtomicBool::new(false)).map(drop) });
        cx.spawn(async move |this, cx| {
            let Err(error) = started.await else {
                return;
            };
            tracing::warn!(%error, agent = kind.name(), "Cannot start the new worktree's agent");
            let _ = this.update(cx, |this, cx| {
                this.local_error = Some(fl!(
                    "worktree-landing-failed",
                    agent = kind.name(),
                    error = error.localize()
                ));
                cx.notify();
            });
        })
        .detach();
    }
}

impl HerdrWindow {
    /// The dialog's landing choices as one strip: the terminal, then each
    /// agent the host has, wrapping on a narrow dialog.
    pub(crate) fn render_landing(&self, picker: &Picker, cx: &mut Context<Self>) -> AnyElement {
        let theme = &self.theme;
        div()
            .id("worktree-landing")
            .debug_selector(|| "worktree-landing".into())
            .role(Role::RadioGroup)
            .aria_label(fl!("worktree-landing-label"))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(4.))
            .children(picker.offered().into_iter().map(|landing| {
                let on = picker.chosen == landing;
                let (key, label): (&str, SharedString) = match landing {
                    Landing::Terminal => ("terminal", fl!("worktree-landing-terminal").into()),
                    Landing::Agent(kind) => (kind.name(), kind.name().into()),
                };
                div()
                    .id(SharedString::from(format!("worktree-landing-{key}")))
                    .debug_selector(move || format!("worktree-landing-{key}"))
                    .role(Role::RadioButton)
                    .aria_toggled(if on { Toggled::True } else { Toggled::False })
                    .px(px(10.))
                    .py(px(3.))
                    .rounded(px(crate::config::corners::SMALL))
                    .border_1()
                    .border_color(rgb(theme.active))
                    .whitespace_nowrap()
                    .text_color(rgb(if on {
                        theme.foreground
                    } else {
                        theme.subtext()
                    }))
                    .when(on, |chip| chip.bg(rgb(theme.active)))
                    .when(!on, |chip| {
                        chip.cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(picker) = &mut this.menu.landing
                                    && picker.choose(landing)
                                {
                                    cx.notify();
                                }
                            }))
                    })
                    .child(label)
            }))
            .when(picker.looking(), |strip| {
                strip.child(
                    div()
                        .text_color(rgb(theme.muted))
                        .child(fl!("worktree-landing-looking")),
                )
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;
