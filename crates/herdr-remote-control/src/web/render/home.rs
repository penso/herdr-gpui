//! Home: questions waiting on top, then this computer's workspaces as
//! grouped lists of chats and terminal tabs. The
//! board is the same chats grouped by state, behind the toolbar's eye.

use super::{agent_icon, display_folder, elapsed, nav, status_word};
use crate::hub::{ChatRow, Layout, Summary, Workspace};
use herdr_chat_protocol::Status;
use maud::{Markup, html};
use std::collections::HashMap;

/// What home and the board read: the layout and each chat's row.
pub(crate) struct Lists<'a> {
    pub(crate) computer: &'a str,
    pub(crate) layout: &'a Layout,
    pub(crate) summaries: &'a HashMap<String, Summary>,
    pub(crate) now_ms: u64,
}

impl Lists<'_> {
    fn summary(&self, pane: &str) -> Summary {
        self.summaries.get(pane).cloned().unwrap_or_default()
    }

    pub(crate) fn waiting(&self) -> usize {
        self.summaries
            .values()
            .map(|summary| summary.questions)
            .sum()
    }
}

pub(crate) fn home(lists: &Lists<'_>) -> Markup {
    html! {
        div id="lists" {
        div id="home" class="pb-28" {
            (strip(lists))
            div class="flex items-center gap-2.5 px-1 pt-4 pb-1.5" {
                span class="text-muted" aria-hidden="true" { "▭" }
                b class="text-[19px] font-bold" { (lists.computer) }
                @if !lists.layout.connected {
                    span class="badge" { "Herdr not running" }
                }
                (nav("/settings", "ml-auto text-muted tracking-widest px-2", html! { "•••" }))
            }
            @if lists.layout.connected && lists.layout.workspaces.is_empty() {
                p class="text-muted px-2 py-6" { "No workspaces yet." }
            }
            @for workspace in &lists.layout.workspaces {
                section x-data="fold" class="mt-1" {
                    div class="flex items-center gap-3 px-1.5 pt-3 pb-1.5 text-muted font-semibold text-[13.5px]" {
                        span class="flex-1 truncate" {
                            (workspace.label)
                            @if let Some(branch) = &workspace.branch {
                                span class="font-normal text-faint" { " · " (branch) }
                            }
                        }
                        (nav(&format!("/new-chat?workspace={}", urlencode(&workspace.id)), "text-[17px] px-1", html! { span aria-label="New chat" { "⊕" } }))
                        button type="button" class="text-[15px] px-1 transition" x-bind:class="openClass" x-on:click="toggle" aria-label="Collapse" { "⌄" }
                    }
                    ul class="group-list" x-show="open" {
                        @for chat in &workspace.chats {
                            @let summary = lists.summary(&chat.pane);
                            li {
                                (nav(&format!("/chat/{}", urlencode(&chat.pane)), "row", html! {
                                    span class="relative" {
                                        (agent_icon(chat.agent.as_deref(), "w-[30px] h-[30px] text-base"))
                                        @if summary.questions > 0 { i class="dot-corner bg-amber" {} }
                                    }
                                    span class="flex-1 min-w-0 truncate" { (chat.title) }
                                    @if summary.questions > 0 {
                                        span class="pill-amber" { "Answer" }
                                    } @else if summary.status == Status::Working {
                                        span class="text-muted tracking-widest" aria-label="Working" { "•••" }
                                    }
                                    span class="text-faint text-lg" aria-hidden="true" { "›" }
                                }))
                            }
                        }
                        @for terminal in &workspace.terminals {
                            @if let Some(pane) = terminal.first_pane() {
                                li {
                                    (nav(&format!("/term/{}", urlencode(&pane.pane)), "row", html! {
                                        span class="grid place-items-center w-[30px] h-[30px] rounded-lg bg-raised text-green font-mono text-[11px] font-bold shrink-0" { "›_" }
                                        span class="flex-1 min-w-0 truncate" {
                                            (terminal.label)
                                            @if let Some(cwd) = &terminal.cwd {
                                                small class="text-muted ml-1" { (display_folder(cwd)) }
                                            }
                                        }
                                        @if terminal.panes.len() > 1 {
                                            span class="text-muted text-xs" { (terminal.panes.len()) " panes" }
                                        }
                                        span class="text-faint text-lg" aria-hidden="true" { "›" }
                                    }))
                                }
                            }
                        }
                    }
                }
            }
        }
        (toolbar(lists.waiting(), false))
        }
    }
}

/// Questions waiting anywhere, as cards scrolling sideways.
fn strip(lists: &Lists<'_>) -> Markup {
    let cards: Vec<_> = lists
        .layout
        .workspaces
        .iter()
        .flat_map(|workspace| workspace.chats.iter().map(move |chat| (workspace, chat)))
        .filter_map(|(workspace, chat)| {
            let summary = lists.summary(&chat.pane);
            summary
                .question
                .clone()
                .map(|question| (workspace, chat, question, summary.questions))
        })
        .collect();
    html! {
        @if !cards.is_empty() {
            div class="flex gap-2 overflow-x-auto snap-x pt-2 pb-1 -mx-1 px-1" {
                @for (workspace, chat, question, count) in &cards {
                    (nav(&format!("/chat/{}", urlencode(&chat.pane)), "strip-card snap-start", html! {
                        (agent_icon(chat.agent.as_deref(), "w-[26px] h-[26px] text-[13px]"))
                        span class="flex flex-col min-w-0" {
                            b class="truncate" { (question.title) }
                            small class="text-muted truncate" {
                                (workspace.label) " · " (chat.title)
                                @if *count > 1 { " · " (count) " waiting" }
                            }
                        }
                    }))
                }
            }
        }
    }
}

/// Every chat grouped by state: needs you, working, then the rest.
pub(crate) fn board(lists: &Lists<'_>) -> Markup {
    let mut needs = Vec::new();
    let mut working = Vec::new();
    let mut rest = Vec::new();
    for workspace in &lists.layout.workspaces {
        for chat in &workspace.chats {
            let summary = lists.summary(&chat.pane);
            let group = if summary.questions > 0 {
                &mut needs
            } else if summary.status == Status::Working {
                &mut working
            } else {
                &mut rest
            };
            group.push((workspace, chat, summary));
        }
    }
    let section = |title: &str, tone: &str, items: &[(&Workspace, &ChatRow, Summary)]| -> Markup {
        html! {
            @if !items.is_empty() {
                div class="flex items-center gap-2 px-1.5 pt-4 pb-1.5" {
                    span class="flex-1 font-semibold text-[15px]" { (title) }
                    span class={ "count " (tone) } { (items.len()) }
                }
                ul class="group-list" {
                    @for (workspace, chat, summary) in items {
                        li {
                            (nav(&format!("/chat/{}", urlencode(&chat.pane)), "row py-2.5", html! {
                                (agent_icon(chat.agent.as_deref(), "w-[30px] h-[30px] text-base"))
                                span class="flex-1 min-w-0 flex flex-col" {
                                    span class="truncate" {
                                        @if let Some(question) = &summary.question { (question.title) } @else { (chat.title) }
                                    }
                                    small class="text-muted truncate" {
                                        (lists.computer) " · " (workspace.label) " · " (chat.title)
                                        @if let Some(started) = summary.turn_started_ms {
                                            " · " (elapsed(lists.now_ms, started))
                                        }
                                    }
                                }
                                @let (word, _) = status_word(&summary.status, summary.questions);
                                span class="sr-only" { (word) }
                                span class="text-faint text-lg" aria-hidden="true" { "›" }
                            }))
                        }
                    }
                }
            }
        }
    };
    html! {
        div id="lists" {
        div id="board" class="pb-28" {
            (section("Needs you", "bg-amber", &needs))
            (section("Working", "bg-green", &working))
            (section("Idle", "bg-raised", &rest))
            @if needs.is_empty() && working.is_empty() && rest.is_empty() {
                p class="text-muted px-2 py-10 text-center" { "No chats are running." }
            }
        }
        (toolbar(lists.waiting(), true))
        }
    }
}

/// The floating toolbar: settings, the eye (home or board), new workspace.
fn toolbar(waiting: usize, board: bool) -> Markup {
    html! {
        nav class="toolbar" aria-label="Toolbar" {
            div class="glass" {
                (nav("/settings", "", html! { span aria-label="Settings" { "⚙" } }))
                (nav(if board { "/" } else { "/board" }, if board { "relative text-amber" } else { "relative" }, html! {
                    span aria-label=(if board { "Show workspaces" } else { "Show what needs you" }) { "◉" }
                    @if waiting > 0 && !board { i class="dot-corner bg-amber" {} }
                }))
                (nav("/new", "", html! { span aria-label="New workspace" { "＋" } }))
            }
        }
    }
}

/// Percent-encodes an id for a path or query component.
pub(crate) fn urlencode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(byte).to_string()
            }
            byte => format!("%{byte:02X}"),
        })
        .collect()
}
