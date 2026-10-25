//! A chat's session settings: the summary under its title and the sheet
//! the title opens, with the model, effort, mode, and Auto-accept. Changes
//! go out as the agent's own commands; the sheet redraws from what the
//! agent reports. The sheet stays open across patches because app.js keeps
//! that on `<html data-sheet>`, outside the region.

use super::icons;
use crate::hub::{ChatRow, Workspace};
use herdr_chat_protocol::{ConfigOption, OptionCategory, OptionKind, Transcript};
use maud::{Markup, html};

/// A select with this many choices or fewer, none described, is a
/// segmented control; longer or described ones are a list.
const SEGMENTS: usize = 4;

/// Whether the chat has anything the sheet would show.
pub(super) fn has_settings(transcript: &Transcript) -> bool {
    !transcript.options.is_empty() || transcript.modes.is_some()
}

/// The current choices, in the agent's order, for the line under the
/// title: "Opus 5.5 · High · Bypass Permissions".
pub(super) fn summary(transcript: &Transcript) -> String {
    let mut parts: Vec<&str> = transcript
        .options
        .iter()
        .filter_map(|option| match &option.kind {
            OptionKind::Select { choices, current } => choices
                .iter()
                .find(|choice| &choice.id == current)
                .map(|choice| choice.name.as_str()),
            OptionKind::Toggle { .. } => None,
        })
        .collect();
    if transcript.option(OptionCategory::Mode).is_none()
        && let Some(mode) = transcript.modes.as_ref().and_then(|modes| modes.current())
    {
        parts.push(&mode.name);
    }
    parts.join(" · ")
}

/// The sheet, hidden until the title is tapped.
pub(super) fn sheet(workspace: &Workspace, row: &ChatRow, transcript: &Transcript) -> Markup {
    html! {
        div id="chat-settings" {
            @if has_settings(transcript) {
                div class="sheet-backdrop settings-only" data-close-settings {}
                // It drops from the title, as the title's chevron points.
                div class="tsheet settings-only settings-sheet" role="dialog" aria-label="Session settings" {
                    div class="flex items-center gap-2 px-0.5 mb-1" {
                        span class="flex flex-col min-w-0 flex-1" {
                            b class="truncate text-[17px]" { (row.title) }
                            small class="text-muted truncate" { (workspace.label) }
                        }
                        button type="button" class="pill-button" data-close-settings { "Done" }
                    }
                    @for option in &transcript.options { (setting(row, option)) }
                    @if transcript.option(OptionCategory::Mode).is_none()
                        && let Some(modes) = &transcript.modes {
                        h4 class="settings-label" { "Mode" }
                        div class="tsheet-list" {
                            @for mode in &modes.available {
                                label class="settings-choice" {
                                    input type="radio" class="sr-only" name="mode" value=(mode.id)
                                        data-mode data-pane=(row.pane) checked[mode.id == modes.current];
                                    (choice_text(&mode.name, mode.description.as_deref()))
                                    span class="settings-check" { (icons::check(20)) }
                                }
                            }
                        }
                    }
                    div class="tsheet-list" {
                        label class="settings-choice" {
                            span { "Auto-accept questions" }
                            input type="checkbox" class="switch shrink-0" data-auto-accept data-pane=(row.pane) checked[transcript.auto_accept];
                        }
                    }
                    div class="grab mt-3" {}
                }
            }
        }
    }
}

fn setting(row: &ChatRow, option: &ConfigOption) -> Markup {
    let name = format!("option-{}", option.id);
    html! {
        @match &option.kind {
            OptionKind::Select { choices, current } => {
                h4 class="settings-label" { (option.name) }
                @if choices.len() <= SEGMENTS && choices.iter().all(|choice| choice.description.is_none()) {
                    div class="segmented" {
                        @for choice in choices {
                            label {
                                input type="radio" class="sr-only" name=(name) value=(choice.id)
                                    data-option=(option.id) data-pane=(row.pane) checked[&choice.id == current];
                                span { (choice.name) }
                            }
                        }
                    }
                } @else {
                    div class="tsheet-list" {
                        @for choice in choices {
                            label class="settings-choice" {
                                input type="radio" class="sr-only" name=(name) value=(choice.id)
                                    data-option=(option.id) data-pane=(row.pane) checked[&choice.id == current];
                                (choice_text(&choice.name, choice.description.as_deref()))
                                span class="settings-check" { (icons::check(20)) }
                            }
                        }
                    }
                }
            }
            OptionKind::Toggle { current } => {
                div class="tsheet-list" {
                    label class="settings-choice" {
                        span { (option.name) }
                        input type="checkbox" class="switch shrink-0" data-toggle=(option.id) data-pane=(row.pane) checked[*current];
                    }
                }
            }
        }
    }
}

fn choice_text(name: &str, description: Option<&str>) -> Markup {
    html! {
        span class="flex flex-col min-w-0" {
            span { (name) }
            @if let Some(description) = description {
                small class="text-muted text-[13px]" { (description) }
            }
        }
    }
}
