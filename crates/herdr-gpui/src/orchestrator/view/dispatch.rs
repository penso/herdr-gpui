//! Starting an agent on an item, and the run actions that change things:
//! the dispatch dialog, the confirmations for stopping an agent, removing a
//! worktree, or deleting a bead, and the banner with each action's outcome.

use super::{OrchestratorView, look::agent_icon};
use crate::{
    config::{corners, mix},
    orchestrator::{
        Action, DispatchRequest, Item, Notice, SourceKey,
        actions::valid_model,
        prompt::{self, MAX_EXTRA},
    },
    pull_request::MergeMethod,
    search_input::SearchInput,
    teleport::AgentKind,
};
use gpui::{prelude::*, *};

/// The dispatch dialog's choices.
pub(crate) struct Dialog {
    item: String,
    /// The prompt profile, by index into the snapshot's; `None` is built-in.
    profile: Option<usize>,
    kind: Option<AgentKind>,
    branch: Entity<SearchInput>,
    extra: Entity<SearchInput>,
    /// The model the agent is started with; its own default when empty.
    model: Entity<SearchInput>,
    /// Another host to start on, by endpoint; the tab's own when `None`.
    pub(super) host: Option<String>,
    problem: Option<&'static str>,
    /// A read-only review of a pull request rather than work on an issue.
    review: bool,
}

/// A destructive action waiting for the user to confirm it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Confirm {
    Stop {
        run: String,
        agent: String,
    },
    Remove {
        run: String,
        branch: String,
    },
    DeleteBead {
        source: SourceKey,
        id: String,
    },
    /// Merging the pull request and head commit shown when it was asked.
    Merge {
        method: MergeMethod,
        target: crate::pr_actions::Target,
    },
}

impl Confirm {
    fn text(&self) -> (String, String, &'static str) {
        match self {
            Self::Stop { agent, .. } => (
                format!("Stop {agent}?"),
                "Sends Esc, then Ctrl-C to the agent. The worktree is kept.".into(),
                "Stop",
            ),
            Self::Remove { branch, .. } => (
                "Remove worktree?".into(),
                format!(
                    "Removes the checkout of {branch}, discarding uncommitted changes. Committed work stays on the branch."
                ),
                "Remove",
            ),
            Self::Merge { method, .. } => (
                format!("{}?", method.action()),
                "Merges the pull request at the head commit shown here; GitHub refuses if it moved."
                    .into(),
                "Merge",
            ),
            Self::DeleteBead { id, .. } => (
                format!("Delete {id} permanently?"),
                "Runs bd delete --force. This cannot be undone and removes it for everyone using this .beads."
                    .into(),
                "Delete",
            ),
        }
    }

    /// The worker action it confirms; a merge goes through `pr_actions`.
    fn action(self) -> Option<Action> {
        match self {
            Self::Stop { run, .. } => Some(Action::Stop { run }),
            Self::Remove { run, .. } => Some(Action::Remove { run }),
            Self::DeleteBead { source, id } => Some(Action::DeleteBead { source, id }),
            Self::Merge { .. } => None,
        }
    }
}

impl OrchestratorView {
    /// Opens the dispatch dialog for the item with canonical key `item`.
    pub(super) fn open_dispatch(
        &mut self,
        item: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_dialog(item, false, window, cx);
    }

    /// Opens the dialog for a read-only review of the pull request `item`.
    pub(super) fn open_review(
        &mut self,
        item: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_dialog(item, true, window, cx);
    }

    fn open_dialog(
        &mut self,
        item: String,
        review: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(found) = self.item(&item) else {
            return;
        };
        let branch_name = match (&found.pull_request, review) {
            (Some(pr), true) => format!("review/pr-{}", pr.number),
            _ => prompt::branch(found),
        };
        let field = |placeholder: &str, text: &str, cx: &mut Context<Self>| {
            let (ui, theme) = (self.look.ui.clone(), self.look.theme.clone());
            cx.new(|cx| {
                let mut input = SearchInput::new(cx);
                input.set_placeholder(placeholder, cx);
                input.set_appearance(ui, theme, cx);
                if !text.is_empty() {
                    input.set_text_selected(text, cx);
                }
                input
            })
        };
        let branch = field("Branch name", &branch_name, cx);
        let extra = field("Extra instructions, appended verbatim (optional)", "", cx);
        let model = field("Agent default", "", cx);
        let kind = self
            .snapshot
            .installed
            .as_ref()
            .and_then(|installed| installed.first().copied());
        let focus = extra.read(cx).focus.clone();
        self.dialog = Some(Dialog {
            item,
            profile: None,
            kind,
            branch,
            extra,
            model,
            host: None,
            problem: None,
            review,
        });
        window.focus(&focus, cx);
        cx.notify();
    }

    pub(super) fn close_overlays(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let open = self.dialog.take().is_some() | self.confirm.take().is_some();
        if open {
            window.focus(&self.focus, cx);
            cx.notify();
        }
        open
    }

    fn submit_dispatch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        let Some(kind) = dialog.kind else {
            dialog.problem = Some("Pick an agent installed on this host.");
            cx.notify();
            return;
        };
        let branch = dialog.branch.read(cx).text().trim().to_owned();
        if !prompt::valid_branch(&branch) {
            dialog.problem = Some("That branch name is not one Git accepts.");
            cx.notify();
            return;
        }
        let extra = dialog.extra.read(cx).text().to_owned();
        if extra.len() > MAX_EXTRA {
            dialog.problem = Some("Extra instructions are limited to 16 KiB.");
            cx.notify();
            return;
        }
        let model = dialog.model.read(cx).text().trim().to_owned();
        if !model.is_empty() && !valid_model(&model) {
            dialog.problem = Some("A model name has no spaces and does not start with a dash.");
            cx.notify();
            return;
        }
        let host = dialog.host.clone();
        let Some(item) = self
            .snapshot
            .items
            .iter()
            .find(|item| item.key.canonical() == dialog.item)
        else {
            return;
        };
        let profile = dialog
            .profile
            .and_then(|index| self.snapshot.profiles.get(index));
        let remote = self
            .snapshot
            .repo
            .as_ref()
            .and_then(|info| info.remote.as_ref())
            .map(|remote| remote.url.as_str());
        let text = match (&item.pull_request, dialog.review) {
            (Some(pr), true) => prompt::compose_review(item, pr, remote, profile, &extra),
            _ => prompt::compose(item, profile, &extra),
        };
        let request = DispatchRequest {
            item_key: dialog.item.clone(),
            kind,
            model: (!model.is_empty()).then_some(model),
            prompt: text,
            branch,
            workspace_id: self.request.workspace_id.clone(),
            base: None,
            elsewhere: None,
        };
        match host {
            // The window knows the other host and sets the request up for it.
            Some(endpoint) => cx.emit(super::Event::Dispatch {
                request: Box::new(request),
                endpoint,
            }),
            None => self.act(Action::Dispatch(Box::new(request))),
        }
        self.dialog = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Hands `action` to the worker, or says why it could not take it.
    pub(super) fn act(&mut self, action: Action) {
        let taken = self
            .service
            .as_ref()
            .is_some_and(|service| service.act(action));
        if !taken {
            self.notice = Some(Notice {
                outcome: Err(std::sync::Arc::new(crate::orchestrator::Error::Busy)),
            });
        }
    }

    /// Sends the message field's text to the newest run of the open item.
    pub(super) fn send_message(&mut self, cx: &mut Context<Self>) {
        let text = self.message.read(cx).text().trim().to_owned();
        let Some(detail) = &self.detail else {
            return;
        };
        let runs = super::rows::Runs::new(&self.snapshot.runs, &self.snapshot.sessions, &self.live);
        let Some(run) = runs
            .of_item(&detail.key)
            .first()
            .map(|row| runs.runs[row.run].id.clone())
        else {
            return;
        };
        if text.is_empty() {
            return;
        }
        self.act(Action::Send { run, text });
        self.message.update(cx, |input, cx| input.clear(cx));
    }

    pub(super) fn ask(&mut self, confirm: Confirm, cx: &mut Context<Self>) {
        self.confirm = Some(confirm);
        cx.notify();
    }

    /// The outcome of the newest action, with a dismiss button.
    pub(super) fn render_notice(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let look = &self.look;
        let notice = self.notice.as_ref()?;
        let (hue, text) = match &notice.outcome {
            Ok(text) => (super::look::GREEN, (*text).to_owned()),
            Err(error) => (super::look::RED, error.to_string()),
        };
        Some(
            div()
                .id("orchestrator-notice")
                .flex_none()
                .mx_4()
                .mb_2()
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_2()
                .rounded(px(corners::CONTROL))
                .bg(rgb(look.wash(hue)))
                .child(look.dot(look.hue(hue), 7.))
                .child(div().flex_1().min_w_0().text_size(look.small()).child(text))
                .child(
                    look.icon_button("orchestrator-notice-close", "icons/close.svg")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.notice = None;
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }

    /// The dialog or confirmation over the view, if one is open.
    pub(super) fn render_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let card = match (&self.dialog, &self.confirm) {
            (Some(dialog), _) => self.render_dialog(dialog, cx),
            (None, Some(confirm)) => self.render_confirm(confirm, cx),
            (None, None) => return None,
        };
        let theme = &self.look.theme;
        let backdrop = (mix(theme.background, theme.foreground, 12) << 8) | 0xB0;
        Some(
            div()
                .id("orchestrator-overlay")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(rgba(backdrop))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(card)
                .into_any_element(),
        )
    }

    fn render_confirm(&self, confirm: &Confirm, cx: &mut Context<Self>) -> AnyElement {
        let look = &self.look;
        let (title, body, verb) = confirm.text();
        let confirm = confirm.clone();
        card(look)
            .w(px(420.))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
            .child(
                div()
                    .text_size(look.small())
                    .text_color(rgb(look.theme.subtext()))
                    .child(body),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(div().flex_1())
                    .child(
                        look.button("orchestrator-confirm-cancel", "Cancel", false)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_overlays(window, cx);
                            })),
                    )
                    .child(
                        danger(look, verb).on_click(cx.listener(move |this, _, window, cx| {
                            match confirm.clone() {
                                Confirm::Merge { method, target } => {
                                    this.merge(method, &target, cx)
                                }
                                confirm => {
                                    if let Some(action) = confirm.action() {
                                        this.act(action);
                                    }
                                }
                            }
                            this.close_overlays(window, cx);
                        })),
                    ),
            )
            .into_any_element()
    }

    fn render_dialog(&self, dialog: &Dialog, cx: &mut Context<Self>) -> AnyElement {
        let look = &self.look;
        let theme = &look.theme;
        let item = self
            .snapshot
            .items
            .iter()
            .find(|item| item.key.canonical() == dialog.item);
        let host = match &self.request.target {
            herdr_client::ConnectTarget::Ssh { target, .. } => target.clone(),
            _ => "this machine".into(),
        };
        let section = |title: &'static str, body: AnyElement| {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(look.label(title))
                .child(body)
        };
        let field = |input: &Entity<SearchInput>| {
            div()
                .px_2()
                .py(px(4.))
                .rounded(px(corners::CONTROL))
                .bg(rgb(theme.background))
                .border_1()
                .border_color(rgb(theme.active))
                .child(input.clone())
        };
        let profiles = std::iter::once((None, "built-in".to_owned())).chain(
            self.snapshot
                .profiles
                .iter()
                .enumerate()
                .map(|(index, profile)| (Some(index), profile.name.clone())),
        );
        let segmented = div()
            .flex()
            .flex_wrap()
            .p(px(2.))
            .gap(px(2.))
            .rounded(px(corners::CONTROL))
            .bg(rgb(theme.surface))
            .children(profiles.map(|(index, name)| {
                let on = index == dialog.profile;
                div()
                    .id(SharedString::from(format!("orchestrator-profile-{name}")))
                    .px_3()
                    .py(px(3.))
                    .rounded(px(corners::SMALL))
                    .cursor_pointer()
                    .when(on, |el| el.bg(rgb(theme.background)))
                    .text_size(look.small())
                    .text_color(rgb(if on { theme.foreground } else { theme.muted }))
                    .child(name)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(dialog) = &mut this.dialog {
                            dialog.profile = index;
                        }
                        cx.notify();
                    }))
            }));
        let agents: Vec<AgentKind> = self
            .snapshot
            .installed
            .as_ref()
            .map(|installed| installed.iter().copied().take(8).collect())
            .unwrap_or_default();
        let tiles = if agents.is_empty() {
            look.muted(if self.snapshot.installed.is_none() {
                "Looking for installed agents\u{2026}"
            } else {
                "No supported agent is installed on this host."
            })
            .into_any_element()
        } else {
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .children(agents.into_iter().map(|kind| {
                    let on = dialog.kind == Some(kind);
                    div()
                        .id(SharedString::from(format!(
                            "orchestrator-agent-{}",
                            kind.name()
                        )))
                        .w(px(84.))
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_1()
                        .py_2()
                        .rounded(px(corners::CONTROL))
                        .border_1()
                        .border_color(rgb(if on { theme.primary() } else { theme.active }))
                        .when(on, |el| el.bg(rgb(theme.primary_wash())))
                        .cursor_pointer()
                        .child(look.icon(agent_icon(kind.name()), 20., theme.foreground))
                        .child(div().text_size(look.small()).child(kind.name()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(dialog) = &mut this.dialog {
                                dialog.kind = Some(kind);
                            }
                            cx.notify();
                        }))
                }))
                .into_any_element()
        };
        card(look)
            .w(px(600.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(look.ui.size + 3.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(if dialog.review {
                                "Review with agent"
                            } else {
                                "Dispatch agent"
                            }),
                    )
                    .child(look.chip(format!("on {host}")))
                    .child(
                        look.icon_button("orchestrator-dialog-close", "icons/close.svg")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_overlays(window, cx);
                            })),
                    ),
            )
            .when_some(item, |el, item| el.child(item_line(look, item)))
            .child(section("PROMPT", segmented.into_any_element()))
            .child(section("AGENT", tiles))
            .when_some(self.render_hosts(dialog, cx), |el, hosts| {
                el.child(section("HOST", hosts))
            })
            .child(section("MODEL", field(&dialog.model).into_any_element()))
            .child(section("BRANCH", field(&dialog.branch).into_any_element()))
            .child(section(
                "EXTRA INSTRUCTIONS",
                field(&dialog.extra).into_any_element(),
            ))
            .when_some(dialog.problem, |el, problem| {
                el.child(
                    div()
                        .text_size(look.small())
                        .text_color(rgb(look.hue(super::look::RED)))
                        .child(problem),
                )
            })
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(div().flex_1())
                    .child(
                        look.button("orchestrator-dialog-cancel", "Cancel", false)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_overlays(window, cx);
                            })),
                    )
                    .child(
                        look.button("orchestrator-dialog-dispatch", "Dispatch", true)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.submit_dispatch(window, cx)),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// Keys while the dialog is open: Enter dispatches, Escape cancels.
    pub(super) fn dialog_key(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match (key, self.dialog.is_some(), self.confirm.is_some()) {
            ("escape", true, _) | ("escape", _, true) => self.close_overlays(window, cx),
            ("enter", true, _) => {
                self.submit_dispatch(window, cx);
                true
            }
            _ => false,
        }
    }
}

fn card(look: &super::Look) -> Div {
    let theme = &look.theme;
    div()
        .flex()
        .flex_col()
        .gap_3()
        .p_4()
        .rounded(px(corners::PANEL))
        .bg(rgb(theme.background))
        .border_1()
        .border_color(rgb(theme.active))
        .shadow_lg()
}

fn danger(look: &super::Look, label: &'static str) -> Stateful<Div> {
    let fill = look.hue(super::look::RED);
    div()
        .id(SharedString::from(format!("orchestrator-confirm-{label}")))
        .px_3()
        .py(px(4.))
        .rounded(px(corners::CONTROL))
        .bg(rgb(fill))
        .cursor_pointer()
        .text_color(rgb(look.theme.text_on(fill)))
        .text_size(look.small())
        .child(label)
}

fn item_line(look: &super::Look, item: &Item) -> Div {
    let theme = &look.theme;
    div()
        .flex()
        .items_center()
        .gap_2()
        .p_2()
        .rounded(px(corners::CONTROL))
        .bg(rgb(theme.surface))
        .child(super::list::source_mark(look, item.key.source.provider))
        .child(
            look.mono(item.identifier.clone())
                .text_color(rgb(theme.muted)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(item.title.clone()),
        )
}
