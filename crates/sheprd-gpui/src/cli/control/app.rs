//! `app`, `window`, and `ui`.
use super::{Args, CliError, FontSize, Size, Switch, switch, unknown};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppCommand {
    Focus,
    Hide,
    Quit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WindowCommand {
    List,
    New {
        session: Option<String>,
    },
    Focus {
        id: u64,
    },
    Fullscreen(Switch),
    /// Report the size, or set it.
    Size(Option<Size>),
    Detach,
    Reconnect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiCommand {
    Sidebar(Switch),
    StatusBar(Switch),
    FontSize(FontSize),
    /// The activity bar section to show.
    Panel(crate::sidebar::ActivityPanel),
}

pub(super) fn app(args: &mut Args) -> Result<AppCommand, CliError> {
    let verb = args.verb().ok_or(CliError::MissingArgument {
        command: "app",
        what: "focus|hide|quit",
    })?;
    Ok(match verb.to_str() {
        Some("focus") => AppCommand::Focus,
        Some("hide") => AppCommand::Hide,
        Some("quit") => AppCommand::Quit,
        _ => return Err(unknown("app", verb)),
    })
}

pub(super) fn window(args: &mut Args) -> Result<WindowCommand, CliError> {
    let Some(verb) = args.verb() else {
        return Ok(WindowCommand::List);
    };
    Ok(match verb.to_str() {
        Some("list") => WindowCommand::List,
        Some("new") => WindowCommand::New {
            session: args.text_option("--session")?,
        },
        Some("focus") => WindowCommand::Focus {
            id: args
                .required_text("ID")?
                .parse()
                .map_err(|_| CliError::InvalidValue("ID"))?,
        },
        Some("fullscreen") => WindowCommand::Fullscreen(switch(args.positional())?),
        Some("size") => WindowCommand::Size(args.positional().map(size).transpose()?),
        Some("detach") => WindowCommand::Detach,
        Some("reconnect") => WindowCommand::Reconnect,
        _ => return Err(unknown("window", verb)),
    })
}

/// `WIDTHxHEIGHT` in points, each within what a screen can hold.
fn size(text: std::ffi::OsString) -> Result<Size, CliError> {
    text.to_str()
        .and_then(|text| text.split_once(['x', 'X']))
        .and_then(|(width, height)| Some((width.parse().ok()?, height.parse().ok()?)))
        .filter(|(width, height)| (200..=16_384).contains(width) && (150..=16_384).contains(height))
        .map(|(width, height)| Size { width, height })
        .ok_or(CliError::InvalidValue("WIDTHxHEIGHT"))
}

/// The panel names `ui panel` takes, as its errors and help name them.
const PANELS: &str = "workspaces|overview|pull-requests|issues|security|automations|usage";

pub(super) fn ui(args: &mut Args) -> Result<UiCommand, CliError> {
    let verb = args.verb().ok_or(CliError::MissingArgument {
        command: "ui",
        what: "sidebar|statusbar|font-size|panel",
    })?;
    Ok(match verb.to_str() {
        Some("sidebar") => UiCommand::Sidebar(switch(args.positional())?),
        Some("statusbar" | "status-bar") => UiCommand::StatusBar(switch(args.positional())?),
        Some("panel") => {
            let name = args.required_text(PANELS)?;
            UiCommand::Panel(
                crate::sidebar::ActivityPanel::parse(&name)
                    .ok_or(CliError::InvalidValue(PANELS))?,
            )
        }
        Some("font-size") => {
            let value = args.required_text("+|-|reset|POINTS")?;
            UiCommand::FontSize(match value.as_str() {
                "+" | "larger" => FontSize::Larger,
                "-" | "smaller" => FontSize::Smaller,
                "reset" => FontSize::Reset,
                points => FontSize::Points {
                    points: points
                        .parse()
                        .ok()
                        .filter(|points| (8..=48).contains(points))
                        .ok_or(CliError::InvalidValue("POINTS (8-48)"))?,
                },
            })
        }
        _ => return Err(unknown("ui", verb)),
    })
}
