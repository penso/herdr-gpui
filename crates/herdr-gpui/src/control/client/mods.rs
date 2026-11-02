//! `mod`: list, reload, and read the logs of the window's mods; `check`
//! runs here and needs no app.
use super::{Context, Output, unexpected};
use crate::{
    cli::control::ModCommand,
    control::protocol::{
        Request, Response,
        mods::{ModOp, Reply},
    },
};
use std::{path::Path, time::SystemTime};

pub(super) fn run(command: ModCommand, context: &Context) -> crate::Result<Output> {
    let op = match command {
        ModCommand::Check { dir } => return check(&dir),
        ModCommand::List => ModOp::List,
        ModCommand::Reload { name } => ModOp::Reload { name },
        ModCommand::Logs { name, lines } => ModOp::Logs { name, lines },
        ModCommand::Open { name } => ModOp::Open { name },
        ModCommand::Panel { switch } => ModOp::Panel { switch },
    };
    match context.call(Request::Mod, op)? {
        Response::Mod {
            reply: Reply::Mods { dir, mods },
        } => {
            let json = serde_json::json!({ "dir": dir, "mods": mods });
            if mods.is_empty() {
                let text = match &dir {
                    Some(dir) => format!("No mods in {dir}"),
                    None => "No mods".to_owned(),
                };
                return Ok(Output::data(text, &json)?.nothing());
            }
            let text = mods
                .iter()
                .map(|info| {
                    let places = if info.places.is_empty() {
                        "palette only".to_owned()
                    } else {
                        info.places.join(", ")
                    };
                    match &info.error {
                        Some(error) => format!("{}  {places}  error: {error}", info.name),
                        None if info.drawn => format!("{}  {places}", info.name),
                        None => format!("{}  {places}  loading", info.name),
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            Output::data(text, &json)
        }
        Response::Mod {
            reply: Reply::Reloaded { name },
        } => Output::data(
            format!("Reloading {name}"),
            &serde_json::json!({ "name": name, "reloading": true }),
        ),
        Response::Mod {
            reply: Reply::Panel { shown, sections },
        } => {
            let json = serde_json::json!({ "shown": shown, "sections": sections });
            let text = match (shown, sections.is_empty()) {
                (_, true) => "The mod panel has no sections".to_owned(),
                (true, false) => format!("The mod panel shows {}", sections.join(", ")),
                (false, false) => format!("The mod panel is hidden ({})", sections.join(", ")),
            };
            Output::data(text, &json)
        }
        Response::Mod {
            reply: Reply::Opened { name },
        } => Output::data(
            format!("Showing {name}"),
            &serde_json::json!({ "name": name, "shown": true }),
        ),
        Response::Mod {
            reply: Reply::Logs { name, lines },
        } => {
            let json = serde_json::json!({ "name": name, "lines": lines });
            if lines.is_empty() {
                return Ok(Output::data(format!("{name} has logged nothing"), &json)?.nothing());
            }
            Output::data(lines.join("\n"), &json)
        }
        response => Err(unexpected(response)),
    }
}

/// Loads the mod, runs `init` and `view` once against an empty state, and
/// reports what it would draw; any error is the command's failure.
fn check(dir: &Path) -> crate::Result<Output> {
    let failed = |error: herdr_mods::Error| crate::Error::from(crate::mods::Error::from(error));
    let sources = herdr_mods::Sources::read(dir).map_err(failed)?;
    let now = SystemTime::now();
    let mut module =
        herdr_mods::Mod::load(&sources, herdr_mods::Limits::default(), now).map_err(failed)?;
    let tree = module
        .view(&herdr_mods::State::default(), now)
        .map_err(failed)?;
    let manifest = &sources.manifest;
    let mut places: Vec<_> = manifest.places.iter().map(|place| place.name()).collect();
    if manifest.activity.is_some() {
        places.push("activity_bar");
    }
    let log: Vec<_> = module
        .take_log()
        .into_iter()
        .map(|line| line.text)
        .collect();
    // A view that returns nil draws nothing, which is valid.
    let elements = tree.as_ref().map_or(0, herdr_mods::Node::count);
    let mut text = format!(
        "{} is valid: {} elements, drawn in {}",
        manifest.name,
        elements,
        if places.is_empty() {
            "no place (palette commands only)".to_owned()
        } else {
            places.join(", ")
        },
    );
    if !manifest.needs.is_empty() {
        text.push_str("\nIt asks for access that needs approval; mods cannot use it yet.");
    }
    for line in &log {
        text.push_str("\nlog: ");
        text.push_str(line);
    }
    Output::data(
        text,
        &serde_json::json!({
            "name": manifest.name,
            "valid": true,
            "elements": elements,
            "places": places,
            "hash": sources.hash,
            "log": log,
        }),
    )
}
