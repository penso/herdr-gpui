//! Forms and settings: a new workspace, a new chat in a workspace, and this
//! phone's settings.

use super::{display_folder, home::urlencode, icons, nav};
use crate::hub::Workspace;
use herdr_chat_protocol::Agent;
use maud::{Markup, html};

/// The folders a new workspace may start in, grouped for the picker.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Folders {
    /// Folders workspaces already use.
    pub(crate) recent: Vec<String>,
    /// Each project root with the folders in it.
    pub(crate) roots: Vec<(String, Vec<String>)>,
}

impl Folders {
    /// Whether `path` is one this form offers, so a request cannot name
    /// another folder.
    pub(crate) fn offers(&self, path: &str) -> bool {
        self.recent.iter().any(|folder| folder == path)
            || self
                .roots
                .iter()
                .any(|(_, folders)| folders.iter().any(|folder| folder == path))
    }
}

fn agents(selected: Agent) -> Markup {
    html! {
        div class="grid grid-cols-2 gap-1.5" role="radiogroup" aria-label="Agent" {
            @for agent in Agent::ALL {
                label class="agent-choice" {
                    input type="radio" name="agent" value=(agent.name()) checked[*agent == selected] class="sr-only";
                    (super::agent_icon(Some(agent.name()), "w-[22px] h-[22px] text-xs"))
                    (agent.display_name())
                }
            }
        }
    }
}

fn header(title: &str, submit: &str) -> Markup {
    html! {
        header class="flex items-center gap-2 px-4 pt-1.5 pb-2" {
            a href="/" data-nav class="pill-button" { "Cancel" }
            h1 class="flex-1 text-center text-[17px] font-bold" { (title) }
            button type="submit" class="pill-button primary" { (submit) }
        }
    }
}

fn label(text: &str) -> Markup {
    html! { span class="block text-xs text-muted mt-3 mb-1" { (text) } }
}

pub(crate) fn new_workspace(folders: &Folders) -> Markup {
    html! {
        form id="new-workspace" data-form="/api/workspaces" class="pb-10" {
            (header("New workspace", "Create"))
            div class="px-4" {
                (label("Project"))
                (super::picker::folder_picker("cwd", folders))
                (label("New branch as a worktree (optional)"))
                input name="branch" class="field w-full" autocapitalize="off" autocorrect="off" spellcheck="false" placeholder="feat/remote-push" maxlength="200";
                (label("Agent"))
                (agents(Agent::Claude))
                p class="form-error text-red text-sm mt-3" {}
            }
        }
    }
}

pub(crate) fn new_chat(workspace: &Workspace) -> Markup {
    html! {
        form id="new-chat" data-form={ "/api/workspaces/" (urlencode(&workspace.id)) "/chats" } class="pb-10" {
            (header("New chat", "Start"))
            div class="px-4" {
                p class="text-muted text-sm" { "In " b class="text-text" { (workspace.label) } " · " (display_folder(&workspace.cwd)) }
                (label("Agent"))
                (agents(Agent::Claude))
                p class="form-error text-red text-sm mt-3" {}
            }
        }
    }
}

/// This phone's settings: notifications and the addresses it may use.
pub(crate) fn devices(computer: &str, origins: &[String], fingerprint: &str) -> Markup {
    html! {
        div id="settings" class="px-4 pb-16" x-data="settings" {
            header class="flex items-center gap-2 pt-1.5 pb-2" {
                a href="/" data-nav class="grid place-items-center text-accent w-8 h-9 -ml-1.5 shrink-0" aria-label="Back" { (icons::chevron_left(28)) }
                h1 class="text-[22px] font-bold" { "Settings" }
            }
            h2 class="section-title" { "Notifications" }
            div class="group-list" {
                label class="row justify-between" {
                    span { "Push notifications" }
                    // The CSP build cannot write through x-model; the handler reads the box.
                    input type="checkbox" x-bind:checked="push" x-bind:disabled="pushOff" x-on:change="togglePush" class="switch";
                }
                p class="px-3 pb-3 text-xs text-muted" x-text="pushHint" {}
            }
            h2 class="section-title" { (computer) }
            div class="group-list" {
                @for origin in origins { div class="row text-sm font-mono" { span class="min-w-0 break-all" { (origin) } } }
                div class="row items-start text-xs text-muted" { span class="shrink-0" { "Certificate" } span class="min-w-0 break-all font-mono" { (fingerprint) } }
                // `data-computer`: the app points these at the computer it
                // shows, which need not be the one it was installed from.
                a class="row text-accent" href="/ca.mobileconfig" data-computer { "Install the certificate (iPhone, iPad)" }
                a class="row text-accent" href="/ca.pem" download="herdr-remote-ca.pem" data-computer { "Download the certificate (Android)" }
            }
            h2 class="section-title" { "This phone" }
            div class="group-list" {
                (nav("/computers", "row text-accent", html! { "Computers" }))
                button type="button" class="row text-red w-full text-left" x-on:click="forget" { "Forget this computer" }
            }
        }
    }
}
