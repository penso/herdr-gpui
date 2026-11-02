//! Commands this process answers from the app's files and its own code,
//! whether or not Herdr UI runs.
mod diagnostics;
mod logs;
mod processes;
mod search;

use super::Output;
pub(super) use diagnostics::diagnostics;
pub(super) use logs::logs;
pub(super) use processes::processes;
pub(super) use search::search;

/// Every command `run` accepts, with its name and label.
pub(super) fn commands() -> crate::Result<Output> {
    let commands: Vec<_> = crate::controls::COMMANDS
        .iter()
        .map(|info| serde_json::json!({ "name": info.name, "label": crate::i18n::command_english(info) }))
        .collect();
    let text = crate::controls::COMMANDS
        .iter()
        .map(|info| format!("{:<28}{}", info.name, crate::i18n::command_english(info)))
        .collect::<Vec<_>>()
        .join("\n");
    Output::data(text, &commands)
}
