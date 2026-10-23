//! `status`, `app`, `window`, `ui`, and `run`.
use super::{Context, Output, send, unexpected};
use crate::{
    cli::control::{AppCommand, UiCommand, WindowCommand},
    control::protocol::{
        Request, Response,
        app::{AppOp, CommandOp, Reply, Status, UiOp, WindowInfo, WindowOp},
    },
};
use std::fmt::Write as _;

fn describe_window(window: &WindowInfo, text: &mut String) {
    let _ = writeln!(
        text,
        "{}{} {} ({}x{}{})",
        if window.active { "*" } else { " " },
        window.id,
        window.title,
        window.width.round(),
        window.height.round(),
        if window.fullscreen {
            ", full screen"
        } else {
            ""
        },
    );
    if let Some(workspace) = &window.workspace {
        let _ = writeln!(text, "    workspace {workspace}");
    }
    for host in &window.hosts {
        let _ = writeln!(
            text,
            "    {} {} ({}): {}",
            if host.selected { ">" } else { " " },
            host.label,
            host.id,
            host.status
        );
    }
}

pub(super) fn status(context: &Context) -> crate::Result<Output> {
    let response = send(&Request::Status(context.caller()?))?;
    let Response::Status(status) = response else {
        return Err(super::rejected(response));
    };
    let Status {
        version,
        protocol,
        pid,
        browser_tabs,
        windows,
    } = &status;
    let mut text = format!(
        "Herdr UI {version} is running (pid {pid}, control protocol {protocol}{})\n",
        if *browser_tabs {
            ""
        } else {
            ", pages open in the system browser"
        }
    );
    if *protocol != crate::control::protocol::PROTOCOL_VERSION {
        let _ = writeln!(
            text,
            "This command line speaks protocol {}; update the older of the two.",
            crate::control::protocol::PROTOCOL_VERSION
        );
    }
    if windows.is_empty() {
        text.push_str("No window is open\n");
    }
    for window in windows {
        describe_window(window, &mut text);
    }
    Output::data(text.trim_end(), &status)
}

pub(super) fn app(command: AppCommand, context: &Context) -> crate::Result<Output> {
    context.act(
        Request::App,
        match command {
            AppCommand::Focus => AppOp::Focus,
            AppCommand::Hide => AppOp::Hide,
            AppCommand::Quit => AppOp::Quit,
        },
    )
}

pub(super) fn window(command: WindowCommand, context: &Context) -> crate::Result<Output> {
    let op = match command {
        WindowCommand::List => WindowOp::List,
        WindowCommand::New { session } => WindowOp::New { session },
        WindowCommand::Focus { id } => {
            let context = Context {
                json: context.json,
                window: Some(id),
            };
            return context.act(Request::Window, WindowOp::Focus);
        }
        WindowCommand::Fullscreen(switch) => WindowOp::Fullscreen { switch },
        WindowCommand::Size(size) => WindowOp::Size { size },
        WindowCommand::Detach => WindowOp::Detach,
        WindowCommand::Reconnect => WindowOp::Reconnect,
    };
    match context.call(Request::Window, op)? {
        Response::Done { message } => Ok(Output::message(message)),
        Response::App {
            reply: Reply::Windows { windows },
        } => {
            let mut text = String::new();
            for window in &windows {
                describe_window(window, &mut text);
            }
            if windows.is_empty() {
                text.push_str("No window is open");
            }
            Output::data(text.trim_end(), &windows)
        }
        Response::App {
            reply: Reply::Size { width, height },
        } => Output::data(
            format!("{}x{}", width.round(), height.round()),
            &serde_json::json!({ "width": width, "height": height }),
        ),
        response => Err(unexpected(response)),
    }
}

pub(super) fn ui(command: UiCommand, context: &Context) -> crate::Result<Output> {
    let op = match command {
        UiCommand::Sidebar(switch) => UiOp::Sidebar { switch },
        UiCommand::StatusBar(switch) => UiOp::StatusBar { switch },
        UiCommand::FontSize(size) => UiOp::FontSize { size },
        UiCommand::Panel(panel) => UiOp::Panel { panel },
    };
    match context.call(Request::Ui, op)? {
        Response::Done { message } => Ok(Output::message(message)),
        Response::App {
            reply: Reply::FontSize { points },
        } => Output::data(
            format!("Terminal font size {points}"),
            &serde_json::json!({ "points": points }),
        ),
        response => Err(unexpected(response)),
    }
}

pub(super) fn run_command(name: &str, context: &Context) -> crate::Result<Output> {
    if !crate::controls::COMMANDS
        .iter()
        .any(|info| info.name == name)
    {
        return Err(crate::control::Error::Rejected {
            code: crate::control::ErrorCode::NotFound,
            message: format!("No command is named {name}; `herdr-gpui commands` lists them"),
        }
        .into());
    }
    context.act(
        Request::Command,
        CommandOp::Run {
            name: name.to_owned(),
        },
    )
}
