//! `mod list`, `mod reload`, and `mod logs`.
use crate::control::protocol::{
    Call, ErrorCode, Response,
    mods::{MAX_LOG_LINES, ModInfo, ModOp, Reply},
};
use gpui::App;

pub(super) fn serve(call: Call<ModOp>, cx: &mut App) -> Response {
    use super::{in_window, not_found};

    let Call { caller, op } = call;
    match op {
        ModOp::List => in_window(&caller, cx, |view, _, _| Response::Mod {
            reply: Reply::Mods {
                dir: view.mods.dir().map(|dir| dir.display().to_string()),
                mods: view
                    .mods
                    .entries()
                    .map(|(name, entry)| ModInfo {
                        name: name.to_owned(),
                        places: entry
                            .places()
                            .iter()
                            .map(|place| place.name())
                            .chain(entry.activity().map(|_| "activity_bar"))
                            .map(str::to_owned)
                            .collect(),
                        commands: entry
                            .manifest
                            .iter()
                            .flat_map(|manifest| &manifest.commands)
                            .map(|command| command.id.clone())
                            .collect(),
                        drawn: entry.drawn,
                        error: entry.error().map(ToString::to_string),
                    })
                    .collect(),
            },
        }),
        ModOp::Reload { name } => {
            in_window(&caller, cx, |view, _, _| match view.mods.reload(&name) {
                Ok(()) => Response::Mod {
                    reply: Reply::Reloaded { name },
                },
                Err(error) => not_found(error.to_string()),
            })
        }
        ModOp::Open { name } => in_window(&caller, cx, |view, window, cx| {
            if view
                .mods
                .get(&name)
                .is_none_or(|entry| entry.activity().is_none())
            {
                return not_found(format!(
                    "No running mod named {name} has an activity bar app"
                ));
            }
            if !view.activity_bar_shown() {
                return Response::error(
                    ErrorCode::Unsupported,
                    "The activity bar is off in Settings",
                );
            }
            view.select_mod_app(&name, window, cx);
            Response::Mod {
                reply: Reply::Opened { name },
            }
        }),
        ModOp::Panel { switch } => in_window(&caller, cx, |view, _, cx| {
            if let Some(switch) = switch {
                let shown = switch.apply(!view.mods.panel_hidden());
                view.mods.set_panel_hidden(!shown);
                cx.notify();
            }
            Response::Mod {
                reply: Reply::Panel {
                    shown: !view.mods.panel_hidden(),
                    sections: view
                        .mods
                        .panel_sections()
                        .into_iter()
                        .map(|(name, _)| name.to_owned())
                        .collect(),
                },
            }
        }),
        ModOp::Logs { name, lines } => in_window(&caller, cx, |view, _, _| {
            let Some(entry) = view.mods.get(&name) else {
                return not_found(format!("No mod named {name} is running"));
            };
            let lines = lines.min(MAX_LOG_LINES);
            let skip = entry.log.len().saturating_sub(lines);
            Response::Mod {
                reply: Reply::Logs {
                    name,
                    lines: entry
                        .log
                        .iter()
                        .skip(skip)
                        .map(|line| line.text.clone())
                        .collect(),
                },
            }
        }),
    }
}
