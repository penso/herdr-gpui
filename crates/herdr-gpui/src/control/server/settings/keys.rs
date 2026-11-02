//! `keybinds`: the window's shortcuts, and whether each binding of Herdr's
//! custom commands reaches this client, as the shortcut reference shows.
use super::super::in_window;
use crate::{
    control::protocol::{
        Caller, Response,
        settings::{BindingInfo, CustomInfo, Reply},
    },
    controls::COMMANDS,
    keymap::Reach,
};
use gpui::App;
use herdr_client::protocol::ClientShellCommand;

pub(super) fn reach_name(reach: Reach) -> &'static str {
    match reach {
        Reach::Runs => "runs",
        Reach::Shadowed => "shadowed",
        Reach::Taken => "taken",
        Reach::NeedsModifier => "needs_modifier",
        Reach::NoPrefix => "no_prefix",
        Reach::OverLimit => "over_limit",
        Reach::Unsupported => "unsupported",
    }
}

/// A custom command's name: its description, or its ID without one, as the
/// shortcut reference names it.
fn command_label(command: &ClientShellCommand) -> &str {
    command
        .description
        .as_deref()
        .filter(|description| !description.trim().is_empty())
        .unwrap_or(&command.command_id)
}

pub(super) fn keybinds(caller: &Caller, cx: &mut App) -> Response {
    in_window(caller, cx, |view, _, _| {
        let keymap = view.keymap();
        let bindings = COMMANDS
            .iter()
            .map(|info| BindingInfo {
                name: info.name.to_owned(),
                label: crate::i18n::command_english(info),
                keys: keymap.shortcuts(info.command).map(str::to_owned).collect(),
            })
            .collect();
        // Custom commands come from the selected host's live snapshot.
        let commands = view
            .live
            .snapshot
            .as_ref()
            .filter(|_| view.live.status.is_connected())
            .map(|snapshot| snapshot.commands.as_slice())
            .unwrap_or_default();
        let custom = commands
            .iter()
            .zip(keymap.custom_bindings(commands))
            .flat_map(|(command, bindings)| {
                bindings.into_iter().map(move |binding| CustomInfo {
                    label: command_label(command).to_owned(),
                    keys: binding.label,
                    reach: reach_name(binding.reach).to_owned(),
                })
            })
            .collect();
        Response::Settings {
            reply: Reply::Keybinds { bindings, custom },
        }
    })
}
