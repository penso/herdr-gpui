//! Home: questions waiting on top, then this computer's workspaces as
//! grouped lists of chats and terminal tabs. The
//! board is the same chats grouped by state, behind the toolbar's eye. Both
//! open under a search field that filters their rows in place.

use super::{agent_icon, elapsed, icons, nav, status_mark};
use crate::hub::{ChatRow, Layout, Summary, Workspace};
use crate::web::Indicators;
use herdr_chat_protocol::Status;
use maud::{Markup, html};
use std::collections::HashMap;

/// What home and the board read: the layout and each chat's row.
pub(crate) struct Lists<'a> {
    pub(crate) computer: &'a str,
    pub(crate) layout: &'a Layout,
    pub(crate) summaries: &'a HashMap<String, Summary>,
    pub(crate) now_ms: u64,
    /// The status marks the user picked in Herdr UI.
    pub(crate) indicators: Indicators,
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
                span class="grid text-muted" { (icons::computer(22)) }
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
                section x-data="fold" class="mt-1" data-group {
                    div class="flex items-center gap-3 px-1.5 pt-3 pb-1.5 text-muted font-semibold text-[13.5px]" {
                        span class="flex-1 truncate" {
                            (workspace.label)
                            @if let Some(branch) = &workspace.branch {
                                span class="font-normal text-faint" { " · " (branch) }
                            }
                        }
                        (nav(&format!("/new-chat?workspace={}", urlencode(&workspace.id)), "text-[17px] px-1", html! { span class="grid" aria-label="New chat" { (icons::plus_circle(20)) } }))
                        button type="button" class="grid px-1 transition" x-bind:class="openClass" x-on:click="toggle" aria-label="Collapse" { (icons::chevron_down(18)) }
                    }
                    ul class="group-list" x-show="open" {
                        @for chat in &workspace.chats {
                            @let summary = lists.summary(&chat.pane);
                            li data-find=(find(workspace, &[&chat.title, chat.agent.as_deref().unwrap_or_default()])) {
                                (nav(&format!("/chat/{}", urlencode(&chat.pane)), "row", html! {
                                    span class="relative" {
                                        (agent_icon(chat.agent.as_deref(), "w-[30px] h-[30px] text-base"))
                                        @if summary.questions > 0 { i class="dot-corner bg-amber" {} }
                                    }
                                    span class="flex-1 min-w-0 truncate" { (chat.title) }
                                    @if summary.questions > 0 {
                                        span class="pill-amber" { "Answer" }
                                    }
                                    (status_mark(chat.status, lists.indicators))
                                    span class="text-faint" { (icons::chevron_right(18)) }
                                }))
                            }
                        }
                        @for terminal in &workspace.terminals {
                            @if let Some(pane) = terminal.first_pane() {
                                li data-find=(find(workspace, &[&terminal.label, terminal.cwd.as_deref().unwrap_or_default()])) {
                                    (nav(&format!("/term/{}", urlencode(&pane.pane)), "row", html! {
                                        @if let Some((agent, _)) = &terminal.agent {
                                            (agent_icon(agent.as_deref(), "w-[30px] h-[30px] text-base"))
                                        } @else {
                                            span class="grid place-items-center w-[30px] h-[30px] rounded-lg bg-raised text-green font-mono text-[11px] font-bold shrink-0" { "›_" }
                                        }
                                        span class="flex-1 min-w-0 truncate" { (terminal.label) }
                                        @if terminal.panes.len() > 1 {
                                            span class="text-muted text-xs" { (terminal.panes.len()) " panes" }
                                        }
                                        @if let Some((_, status)) = terminal.agent {
                                            (status_mark(status, lists.indicators))
                                        }
                                        span class="text-faint" { (icons::chevron_right(18)) }
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
            div class="flex gap-2 overflow-x-auto snap-x pt-2 pb-1 -mx-1 px-1" data-strip {
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
                section data-group {
                div class="flex items-center gap-2 px-1.5 pt-4 pb-1.5" {
                    span class="flex-1 font-semibold text-[15px]" { (title) }
                    span class={ "count " (tone) } { (items.len()) }
                }
                ul class="group-list" {
                    @for (workspace, chat, summary) in items {
                        li data-find=(find(workspace, &[&chat.title, chat.agent.as_deref().unwrap_or_default(), summary.question.as_ref().map_or("", |question| question.title.as_str())])) {
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
                                (status_mark(chat.status, lists.indicators))
                                span class="text-faint" { (icons::chevron_right(18)) }
                            }))
                        }
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

/// The search field above home and the board. It sits outside `#lists`,
/// which every update replaces, so a search being typed survives updates;
/// app.js hides the rows whose `data-find` text lacks a searched word.
pub(crate) fn search() -> Markup {
    html! {
        div class="sticky top-[env(safe-area-inset-top)] z-10 -mx-3.5 px-3.5 pt-3 pb-1.5 bg-bg" {
            input id="search" type="search" class="field w-full" placeholder="Search" aria-label="Search chats and terminals" autocomplete="off" autocapitalize="off" spellcheck="false" enterkeyhint="search";
        }
    }
}

/// What a search that hides every row shows, under the lists.
pub(crate) fn no_match() -> Markup {
    html! {
        p id="no-match" class="text-muted px-2 py-10 text-center" hidden { "Nothing matches." }
    }
}

/// What a search matches on a row: its workspace, then its own words.
fn find(workspace: &Workspace, words: &[&str]) -> String {
    let mut text = workspace.label.clone();
    for word in workspace
        .branch
        .iter()
        .map(String::as_str)
        .chain(words.iter().copied())
    {
        if !word.is_empty() {
            text.push(' ');
            text.push_str(word);
        }
    }
    text
}

/// The floating toolbar: settings, the eye (home or board), new workspace.
fn toolbar(waiting: usize, board: bool) -> Markup {
    html! {
        nav class="toolbar" aria-label="Toolbar" {
            div class="glass" {
                (nav("/settings", "", html! { span class="grid" aria-label="Settings" { (icons::settings(24)) } }))
                (nav(if board { "/" } else { "/board" }, if board { "relative text-amber" } else { "relative" }, html! {
                    span class="grid" aria-label=(if board { "Show workspaces" } else { "Show what needs you" }) { (icons::target(24)) }
                    @if waiting > 0 && !board { i class="dot-corner bg-amber" {} }
                }))
                (nav("/new", "", html! { span class="grid" aria-label="New workspace" { (icons::plus(24)) } }))
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
