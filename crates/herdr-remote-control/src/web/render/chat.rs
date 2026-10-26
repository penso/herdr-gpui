//! A chat in focus: the latest turn (its prompt, the steps the agent took,
//! and its answer), earlier turns folded into one line that loads the full
//! history, the pending question as a bottom sheet, and the composer.
//! Each region has an id the stream replaces as the chat changes.

use super::{icons, markdown, ring, session, status_word};
use crate::hub::{ChatRow, View, Workspace};
use herdr_chat_protocol::{
    Author, Entry, EntryKind, NoticeLevel, Permission, PermissionKind, PlanStatus, Status, Tool,
    ToolContent, ToolStatus, Transcript, display,
};
use maud::{Markup, PreEscaped, html};

/// The most commands the composer offers after "/".
const MAX_COMMANDS: usize = 64;

/// Where the latest turn starts: its user prompt, else the beginning.
fn latest_turn(transcript: &Transcript) -> usize {
    transcript
        .entries
        .iter()
        .rposition(|entry| matches!(entry.kind, EntryKind::User { .. }))
        .unwrap_or(0)
}

/// The whole chat fragment.
pub(crate) fn chat(workspace: &Workspace, row: &ChatRow, view: &View, now_ms: u64) -> Markup {
    html! {
        div id="chat" class="flex flex-col min-h-full" data-pane=(row.pane) {
            (head(workspace, row, view, now_ms))
            (PreEscaped(Turn::new(row, view).whole()))
            (sheet(row, &view.transcript))
            (session::sheet(workspace, row, &view.transcript))
            (composer(row, view))
        }
    }
}

/// The regions the stream replaces after a change, by id, apart from the
/// turn, which [`Turn::patch`] sends in pieces.
pub(crate) fn chat_patches(
    workspace: &Workspace,
    row: &ChatRow,
    view: &View,
    now_ms: u64,
) -> Vec<(&'static str, Markup)> {
    vec![
        ("chat-head", head(workspace, row, view, now_ms)),
        ("chat-sheet", sheet(row, &view.transcript)),
        (
            "chat-settings",
            session::sheet(workspace, row, &view.transcript),
        ),
        ("chat-composer", composer(row, view)),
    ]
}

fn head(workspace: &Workspace, row: &ChatRow, view: &View, now_ms: u64) -> Markup {
    let transcript = &view.transcript;
    let (word, tone) = status_word(&transcript.status, transcript.permissions.len());
    let summary = session::summary(transcript);
    let status = html! {
        small class="text-muted truncate" {
            span class={ "inline-block w-2 h-2 rounded-full mr-1 align-middle " (tone) } {}
            (word)
            @if let Some(turn) = transcript.turn { " · " (display::duration(now_ms.saturating_sub(turn.started_ms))) }
            " · " @if summary.is_empty() { (workspace.label) } @else { (summary) }
            @if !view.connected { " · reconnecting" }
        }
    };
    html! {
        header id="chat-head" class="sticky top-0 z-10 bg-bg/90 backdrop-blur border-b border-line" {
            div class="flex items-center gap-2.5 px-3.5 pt-1.5 pb-2" {
                a href="/" data-nav class="grid place-items-center text-accent w-8 h-9 -ml-1.5 shrink-0" aria-label="Back" { (icons::chevron_left(28)) }
                @if session::has_settings(transcript) {
                    // The title opens the session's settings; the chevron
                    // says so.
                    button type="button" class="flex flex-col min-w-0 flex-1 text-left" data-open-settings
                        aria-haspopup="dialog" aria-label={ (row.title) ", session settings" } {
                        span class="flex items-center gap-1 min-w-0" {
                            b class="truncate" { (row.title) }
                            span class="grid place-items-center w-5 h-5 rounded-full bg-raised text-muted shrink-0" { (icons::chevron_down(14)) }
                        }
                        (status)
                    }
                } @else {
                    span class="flex flex-col min-w-0 flex-1" {
                        b class="truncate" { (row.title) }
                        (status)
                    }
                }
                @if let Some(usage) = &transcript.usage {
                    (ring(usage.percent(), &format!("{}% of the context window", usage.percent())))
                }
            }
        }
    }
}

/// The latest turn in the pieces the stream compares, so that a change to
/// one entry, such as streamed text, sends that entry alone rather than
/// the whole turn again: a long turn of tool calls is far larger than the
/// few words each change adds.
#[derive(Debug)]
pub(crate) struct Turn {
    /// The fold over earlier turns, empty when there are none.
    history: String,
    /// Each entry of the latest turn in order: its region id and markup.
    entries: Vec<(String, String)>,
    /// The plan, queued prompts, and the empty or stopped notice.
    extras: String,
}

/// What the phone applies to bring its turn up to date.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum TurnPatch {
    /// The whole `chat-turn` region, when the turn is not the one the phone
    /// holds plus additions: a new prompt, or dropped entries.
    Whole(String),
    Parts {
        /// Entries and `chat-extras` that changed, by region id.
        regions: Vec<(String, String)>,
        /// New entries, added at the end of `chat-entries`.
        appended: Option<String>,
    },
}

impl Turn {
    pub(crate) fn new(row: &ChatRow, view: &View) -> Self {
        let transcript = &view.transcript;
        let start = latest_turn(transcript);
        let earlier = &transcript.entries[..start];
        let latest = &transcript.entries[start..];
        let folded_turns = earlier
            .iter()
            .filter(|entry| matches!(entry.kind, EntryKind::Turn(_)))
            .count();
        let history = if earlier.is_empty() {
            String::new()
        } else {
            html! {
                div x-data="history" data-pane=(row.pane) {
                    button type="button" class="fold w-full text-left" x-on:click="load" x-show="folded" {
                        "▸ " (folded_turns.max(1)) " earlier turn" @if folded_turns != 1 { "s" }
                        @if transcript.truncated { " (older ones were dropped)" }
                    }
                    div x-ref="body" {}
                }
            }
            .into_string()
        };
        let entries = latest
            .iter()
            .map(|entry| {
                let id = format!("entry-{}", entry.id.0);
                let markup = html! { div id=(id) class="contents" { (entry_view(entry, true)) } };
                (id, markup.into_string())
            })
            .collect();
        Self {
            history,
            entries,
            extras: extras(row, transcript).into_string(),
        }
    }

    /// The `chat-turn` region.
    pub(crate) fn whole(&self) -> String {
        html! {
            main id="chat-turn" class="flex-1 flex flex-col gap-3 px-3.5 pt-3 pb-40" {
                (PreEscaped(&self.history))
                div id="chat-entries" class="contents" {
                    @for (_, entry) in &self.entries { (PreEscaped(entry)) }
                }
                (PreEscaped(&self.extras))
            }
        }
        .into_string()
    }

    /// What turns `before`, the turn the phone was last sent, into this one.
    pub(crate) fn patch(&self, before: Option<&Self>) -> TurnPatch {
        let Some(before) = before.filter(|before| {
            before.history == self.history
                && before.entries.len() <= self.entries.len()
                && before
                    .entries
                    .iter()
                    .zip(&self.entries)
                    .all(|((was, _), (is, _))| was == is)
        }) else {
            return TurnPatch::Whole(self.whole());
        };
        let mut regions: Vec<(String, String)> = before
            .entries
            .iter()
            .zip(&self.entries)
            .filter(|((_, was), (_, is))| was != is)
            .map(|(_, (id, is))| (id.clone(), is.clone()))
            .collect();
        if before.extras != self.extras {
            regions.push(("chat-extras".into(), self.extras.clone()));
        }
        let added = &self.entries[before.entries.len()..];
        let appended = (!added.is_empty()).then(|| {
            added
                .iter()
                .map(|(_, entry)| entry.as_str())
                .collect::<String>()
        });
        TurnPatch::Parts { regions, appended }
    }
}

/// What follows the entries: the plan, queued prompts, and notices.
fn extras(row: &ChatRow, transcript: &Transcript) -> Markup {
    html! {
        div id="chat-extras" class="contents" {
            @if transcript.entries.is_empty() {
                p class="text-muted text-center py-10" {
                    @match &transcript.status {
                        Status::Starting => "The agent is starting…",
                        _ => "Send a prompt to start.",
                    }
                }
            }
            @if !transcript.plan.is_empty() {
                details class="card" open {
                    summary class="font-semibold" {
                        "Plan "
                        span class="text-muted font-normal" {
                            (transcript.plan.iter().filter(|step| step.status == PlanStatus::Completed).count())
                            "/" (transcript.plan.len())
                        }
                    }
                    ol class="steps mt-2" {
                        @for step in &transcript.plan {
                            li class={ "st " (match step.status { PlanStatus::Completed => "ok", PlanStatus::InProgress => "run", PlanStatus::Pending => "todo" }) } { (step.content) }
                        }
                    }
                }
            }
            @for queued in &transcript.queued {
                div class="flex items-center gap-2 card text-sm" {
                    span class="text-muted" { "Queued" }
                    span class="flex-1 truncate" { (queued.text) }
                    button type="button" class="text-accent" data-command={ r#"{"command":"unqueue","id":""# (queued.id.0) r#""}"# } data-pane=(row.pane) { "Remove" }
                }
            }
            @match &transcript.status {
                Status::Failed { message } | Status::Exited { message } => {
                    div class="card border-red/50 text-sm" { b class="text-red" { "The agent stopped. " } (message) }
                }
                _ => {}
            }
        }
    }
}

/// One entry. In the latest turn the user's prompt is the heading and tool
/// calls are steps; in the history everything is a plain line.
fn entry_view(entry: &Entry, focus: bool) -> Markup {
    match &entry.kind {
        EntryKind::User { text, .. } if focus => html! {
            p class="text-[17px] font-semibold leading-snug whitespace-pre-wrap" { "“" (text) "”" }
        },
        EntryKind::User { text, .. } => html! {
            div class="self-end max-w-[85%] bg-accent text-white px-3 py-2 rounded-2xl rounded-br-md whitespace-pre-wrap" { (text) }
        },
        EntryKind::Agent { text } => html! {
            div class={ "prose " @if focus { "text-[15.5px]" } } { (markdown(text)) }
        },
        EntryKind::Thought { text } => html! {
            details class="text-muted text-sm" {
                summary { "Thinking" }
                div class="prose mt-1" { (markdown(text)) }
            }
        },
        EntryKind::Image { author, image } => html! {
            img class={ "rounded-xl max-h-80 " @if *author == Author::User { "self-end" } }
                src={ "data:" (image.kind.mime()) ";base64," (image.data) } alt="Picture";
        },
        EntryKind::Tool(tool) => tool_view(tool),
        EntryKind::Turn(summary) => html! {
            div class="text-[11px] text-faint text-center border-t border-dashed border-line pt-1.5" { (summary.describe()) }
        },
        EntryKind::Notice { level, text, .. } => html! {
            div class={ "text-sm " (match level { NoticeLevel::Error => "text-red", NoticeLevel::Warning => "text-amber", NoticeLevel::Info => "text-muted" }) } { (text) }
        },
    }
}

fn tool_view(tool: &Tool) -> Markup {
    let (mark, class) = match tool.status {
        ToolStatus::Completed => ("✓", "text-green"),
        ToolStatus::Failed => ("✕", "text-red"),
        ToolStatus::InProgress => ("◌", "text-amber"),
        ToolStatus::Pending => ("·", "text-muted"),
    };
    html! {
        details class="tool" {
            summary class="flex items-center gap-2" {
                span class=(class) { (mark) }
                span class="truncate flex-1" { (tool.title) }
            }
            @for content in &tool.content {
                @match content {
                    ToolContent::Text { text } => pre class="code" { (text) },
                    ToolContent::Diff { new, old, path } => {
                        div class="text-xs text-muted mt-2" { (path) }
                        pre class="code" {
                            @for line in old.as_deref().unwrap_or_default().lines() { span class="text-red" { "- " (line) "\n" } }
                            @for line in new.lines() { span class="text-green" { "+ " (line) "\n" } }
                        }
                    },
                    ToolContent::Image { image } => img class="rounded-lg mt-2 max-h-64" src={ "data:" (image.kind.mime()) ";base64," (image.data) } alt="Picture";,
                    ToolContent::Terminal { .. } => p class="text-xs text-muted mt-2" { "Terminal output is shown in the pane." },
                }
            }
        }
    }
}

/// The oldest waiting question, as a bottom sheet.
fn sheet(row: &ChatRow, transcript: &Transcript) -> Markup {
    let Some(permission) = transcript.permissions.first() else {
        return html! { div id="chat-sheet" {} };
    };
    let more = transcript.permissions.len() - 1;
    html! {
        div id="chat-sheet" {
            div class="sheet" role="dialog" aria-label="The agent asks" {
                div class="w-10 h-1.5 rounded-full bg-line self-center mb-1.5" {}
                small class="uppercase tracking-wider font-bold text-amber" { (row.agent.as_deref().unwrap_or("The agent")) " asks" }
                p class="text-[15px] font-semibold" { (permission.title) }
                @for option in primary(permission) { (answer(row, permission, option.0, option.1)) }
                @for option in permission.options.iter().filter(|option| matches!(option.kind, PermissionKind::AllowAlways | PermissionKind::RejectAlways)) {
                    button type="button" class="btn-subtle" data-pane=(row.pane) data-command=(answer_json(permission, &option.id)) { (option.name) }
                }
                @if more > 0 { small class="text-muted text-center" { (more) " more waiting" } }
            }
        }
    }
}

/// The once-options and others, allowing first.
fn primary(permission: &Permission) -> Vec<(&herdr_chat_protocol::PermissionOption, bool)> {
    let mut options: Vec<_> = permission
        .options
        .iter()
        .filter(|option| {
            !matches!(
                option.kind,
                PermissionKind::AllowAlways | PermissionKind::RejectAlways
            )
        })
        .map(|option| (option, option.kind.allows()))
        .collect();
    options.sort_by_key(|(_, allows)| !allows);
    options
}

fn answer(
    row: &ChatRow,
    permission: &Permission,
    option: &herdr_chat_protocol::PermissionOption,
    allows: bool,
) -> Markup {
    html! {
        button type="button" class=(if allows { "btn btn-primary" } else { "btn" }) data-pane=(row.pane) data-command=(answer_json(permission, &option.id)) { (option.name) }
    }
}

fn answer_json(permission: &Permission, option: &str) -> String {
    serde_json::json!({"command": "answer", "option": option, "permission": permission.id.0})
        .to_string()
}

fn composer(row: &ChatRow, view: &View) -> Markup {
    let transcript = &view.transcript;
    let working = transcript.status == Status::Working;
    html! {
        form id="chat-composer" class="composer" x-data="composer" data-pane=(row.pane) data-working[working] x-on:submit="send" {
            // Typing "/" lists the commands whose names start with what
            // follows; app.js shows the first few.
            @if !transcript.commands.is_empty() {
                div x-ref="slash" class="slash-list" hidden {
                    @for command in transcript.commands.iter().take(MAX_COMMANDS) {
                        button type="button" data-slash=(command.name) x-on:click="slash" {
                            b class="font-mono" { "/" (command.name) }
                            @if !command.description.is_empty() {
                                small class="text-muted truncate" { (command.description) }
                            }
                        }
                    }
                }
            }
            div x-ref="chips" class="flex gap-1.5 flex-wrap pb-1.5 empty:hidden" {}
            div class="flex items-end gap-2 bg-raised rounded-3xl pl-1.5 pr-1.5 py-1.5" {
                input type="file" accept="image/png,image/jpeg,image/gif,image/webp" multiple x-ref="file" x-on:change="attach" class="hidden";
                button type="button" class="w-9 h-9 grid place-items-center text-muted shrink-0" x-on:click="pick" aria-label="Attach a picture" { (icons::plus(22)) }
                textarea name="text" rows="1" x-ref="text" x-on:input="grow" x-on:keydown="key" x-on:paste="paste"
                    class="flex-1 min-w-0 bg-transparent resize-none outline-none py-1.5 max-h-40 placeholder:text-faint"
                    placeholder=(if working { "Queue a prompt…" } else { "Message the agent…" })
                    disabled[!transcript.status.accepts_prompts() || !view.connected] {}
                // One button: Stop while the agent works and the draft is
                // empty, Send once there is something to send, and a
                // disabled Send otherwise. app.js flips it as the draft and
                // the agent's status change; the markup is the first frame.
                button type="button" class="round-btn bg-red" data-pane=(row.pane) data-command=r#"{"command":"cancel"}"# aria-label="Stop"
                    x-show="stopShown" style=[(!working).then_some("display:none")] { (icons::stop(16)) }
                button type="submit" class="round-btn bg-accent disabled:opacity-40" aria-label="Send"
                    x-show="sendShown" x-bind:disabled="sendOff" disabled style=[working.then_some("display:none")] { (icons::arrow_up(20)) }
            }
        }
    }
}

/// Every entry before the latest turn, for the folded history.
pub(crate) fn chat_history(view: &View) -> Markup {
    let start = latest_turn(&view.transcript);
    html! {
        div class="flex flex-col gap-3" {
            @for entry in &view.transcript.entries[..start] { (entry_view(entry, false)) }
        }
    }
}
