//! The project field of a new workspace: a button showing the folder, and
//! a full-screen list with a search field, in place of a native select
//! whose long paths wrap over several lines. app.js's `picker` filters the
//! rows and writes the chosen path into the hidden input the form sends.

use super::{display_folder, icons, pages::Folders};
use maud::{Markup, html};

/// The last part of `path`, its name in bold.
fn name(path: &str) -> &str {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
}

/// Where `path` sits, shortened with `~`, shown dimmed under its name.
fn parent(path: &str) -> String {
    let shown = display_folder(path);
    match shown.trim_end_matches('/').rsplit_once('/') {
        Some(("", _)) => "/".to_owned(),
        Some((parent, _)) => parent.to_owned(),
        None => shown,
    }
}

/// The picker for the form field `field`, starting on the first folder.
pub(super) fn folder_picker(field: &str, folders: &Folders) -> Markup {
    let groups: Vec<(String, &[String])> = (!folders.recent.is_empty())
        .then(|| ("Recent".to_owned(), folders.recent.as_slice()))
        .into_iter()
        .chain(
            folders
                .roots
                .iter()
                .map(|(root, children)| (display_folder(root), children.as_slice())),
        )
        .filter(|(_, children)| !children.is_empty())
        .collect();
    let first = groups
        .first()
        .and_then(|(_, children)| children.first())
        .map(String::as_str);
    html! {
        div x-data="picker" {
            input type="hidden" name=(field) value=[first] x-ref="value";
            button type="button" class="field w-full flex items-center gap-2 text-left" x-on:click="open"
                aria-haspopup="dialog" {
                span class="flex flex-col min-w-0 flex-1" {
                    b class="truncate font-medium" x-ref="name" { (first.map_or("Choose a project", name)) }
                    small class="truncate text-muted" x-ref="parent" { @if let Some(first) = first { (parent(first)) } }
                }
                span class="text-muted shrink-0" { (icons::chevron_down(18)) }
            }
            div class="picker-page" x-ref="page" role="dialog" aria-label="Choose a project" hidden {
                div class="flex items-center gap-2 px-4 pb-2" {
                    input type="search" x-ref="search" x-on:input="filter" class="field flex-1 picker-search"
                        placeholder="Search projects" autocapitalize="off" autocorrect="off" spellcheck="false" enterkeyhint="search";
                    button type="button" class="text-accent text-[16px] font-medium px-1" x-on:click="close" { "Cancel" }
                }
                div class="picker-list" {
                    @for (label, children) in &groups {
                        section data-group {
                            h4 class="settings-label px-4" { (label) }
                            div class="tsheet-list mx-4" {
                                @for folder in children.iter() {
                                    button type="button" class="picker-row" data-value=(folder)
                                        data-find=(display_folder(folder).to_lowercase()) x-on:click="choose"
                                        aria-pressed=(if Some(folder.as_str()) == first { "true" } else { "false" }) {
                                        span class="flex flex-col min-w-0 flex-1" {
                                            span class="truncate" { (name(folder)) }
                                            small class="truncate text-muted text-[13px]" { (parent(folder)) }
                                        }
                                        span class="settings-check" { (icons::check(20)) }
                                    }
                                }
                            }
                        }
                    }
                    p class="text-muted text-center py-10" x-ref="empty" hidden { "No project matches." }
                }
            }
        }
    }
}
