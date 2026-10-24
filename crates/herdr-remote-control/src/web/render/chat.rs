//! A chat in focus: the latest turn (its prompt, the steps the agent took,
//! and its answer), earlier turns folded into one line that loads the full
//! history, the pending question as a bottom sheet, and the composer.
//! Each region has an id the stream replaces as the chat changes.

use super::{markdown, ring, status_word};
use crate::hub::{ChatRow, View, Workspace};
use herdr_chat_protocol::{
    Author, Entry, EntryKind, NoticeLevel, OptionCategory, OptionKind, Permission, PermissionKind,
    PlanStatus, Status, Tool, ToolContent, ToolStatus, Transcript, display,
};
use maud::{Markup, html};

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
            (turn(row, view))
            (sheet(row, &view.transcript))
            (composer(row, view))
        }
    }
}

/// The regions the stream replaces after a change, by id.
pub(crate) fn chat_patches(
    workspace: &Workspace,
    row: &ChatRow,
    view: &View,
    now_ms: u64,
) -> Vec<(&'static str, Markup)> {
    vec![
        ("chat-head", head(workspace, row, view, now_ms)),
        ("chat-turn", turn(row, view)),
        ("chat-sheet", sheet(row, &view.transcript)),
        ("chat-composer", composer(row, view)),
    ]
}

fn head(workspace: &Workspace, row: &ChatRow, view: &View, now_ms: u64) -> Markup {
    let transcript = &view.transcript;
    let (word, tone) = status_word(&transcript.status, transcript.permissions.len());
    let model = transcript.option(OptionCategory::Model);
    html! {
        header id="chat-head" class="sticky top-0 z-10 bg-bg/90 backdrop-blur border-b border-line" {
            div class="flex items-center gap-2.5 px-3.5 pt-1.5 pb-2" {
                a href="/" data-nav class="text-accent text-[28px] leading-none px-1" aria-label="Back" { "‹" }
                span class="flex flex-col min-w-0 flex-1" {
                    b class="truncate" { (row.title) }
                    small class="text-muted truncate" {
                        span class={ "inline-block w-2 h-2 rounded-full mr-1 align-middle " (tone) } {}
                        (word)
                        @if let Some(turn) = transcript.turn { " · " (display::duration(now_ms.saturating_sub(turn.started_ms))) }
                        " · " (workspace.label)
                        @if !view.connected { " · reconnecting" }
                    }
                }
                @if let Some(usage) = &transcript.usage {
                    span class="flex items-center gap-1.5 text-muted text-xs" {
                        @if let Some(cost) = &usage.cost { (display::cost(cost)) }
                        (ring(usage.percent(), &format!("{}% of the context window", usage.percent())))
                    }
                }
            }
            @if model.is_some() || transcript.modes.is_some() {
                div class="flex gap-2 px-3.5 pb-2 overflow-x-auto text-xs" {
                    @for option in transcript.options.iter().filter(|option| matches!(option.kind, OptionKind::Select { .. })) {
                        @if let OptionKind::Select { choices, current } = &option.kind {
                            label class="picker" {
                                span class="sr-only" { (option.name) }
                                select data-option=(option.id) data-pane=(row.pane) {
                                    @for choice in choices {
                                        option value=(choice.id) selected[&choice.id == current] { (choice.name) }
                                    }
                                }
                            }
                        }
                    }
                    @if let Some(modes) = &transcript.modes {
                        @if transcript.option(OptionCategory::Mode).is_none() {
                            label class="picker" {
                                span class="sr-only" { "Mode" }
                                select data-mode data-pane=(row.pane) {
                                    @for mode in &modes.available {
                                        option value=(mode.id) selected[mode.id == modes.current] { (mode.name) }
                                    }
                                }
                            }
                        }
                    }
                    label class="picker gap-1.5" {
                        input type="checkbox" data-auto-accept data-pane=(row.pane) checked[transcript.auto_accept];
                        "Auto-accept"
                    }
                }
            }
        }
    }
}

fn turn(row: &ChatRow, view: &View) -> Markup {
    let transcript = &view.transcript;
    let start = latest_turn(transcript);
    let earlier = &transcript.entries[..start];
    let latest = &transcript.entries[start..];
    let folded_turns = earlier
        .iter()
        .filter(|entry| matches!(entry.kind, EntryKind::Turn(_)))
        .count();
    html! {
        main id="chat-turn" class="flex-1 flex flex-col gap-3 px-3.5 pt-3 pb-40" {
            @if !earlier.is_empty() {
                div x-data="history" data-pane=(row.pane) {
                    button type="button" class="fold w-full text-left" x-on:click="load" x-show="folded" {
                        "▸ " (folded_turns.max(1)) " earlier turn" @if folded_turns != 1 { "s" }
                        @if transcript.truncated { " (older ones were dropped)" }
                    }
                    div x-ref="body" {}
                }
            }
            @for entry in latest { (entry_view(entry, true)) }
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
        EntryKind::User { text } if focus => html! {
            p class="text-[17px] font-semibold leading-snug whitespace-pre-wrap" { "“" (text) "”" }
        },
        EntryKind::User { text } => html! {
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
        EntryKind::Notice { level, text } => html! {
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
    let commands: Vec<&str> = transcript
        .commands
        .iter()
        .take(8)
        .map(|command| command.name.as_str())
        .collect();
    html! {
        form id="chat-composer" class="composer" x-data="composer" data-pane=(row.pane) x-on:submit="send" {
            @if !commands.is_empty() {
                div class="flex gap-1.5 overflow-x-auto pb-1.5" {
                    @for name in commands {
                        button type="button" class="chip font-mono" data-slash=(name) x-on:click="slash" { "/" (name) }
                    }
                }
            }
            div x-ref="chips" class="flex gap-1.5 flex-wrap pb-1.5 empty:hidden" {}
            div class="flex items-end gap-2 bg-raised rounded-3xl pl-2 pr-1.5 py-1.5" {
                input type="file" accept="image/png,image/jpeg,image/gif,image/webp" multiple x-ref="file" x-on:change="attach" class="hidden";
                button type="button" class="w-8 h-8 grid place-items-center text-muted text-xl shrink-0" x-on:click="pick" aria-label="Attach a picture" { "＋" }
                textarea name="text" rows="1" x-ref="text" x-on:input="grow" x-on:keydown="key"
                    class="flex-1 bg-transparent resize-none outline-none py-1.5 max-h-40 placeholder:text-faint"
                    placeholder=(if working { "Queue a prompt…" } else { "Message the agent…" })
                    disabled[!transcript.status.accepts_prompts() || !view.connected] {}
                @if working {
                    button type="button" class="round-btn bg-red" data-pane=(row.pane) data-command=r#"{"command":"cancel"}"# aria-label="Stop" { "■" }
                }
                button type="submit" class="round-btn bg-accent" aria-label="Send" { "↑" }
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
