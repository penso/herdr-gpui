//! `theme set`, `font list`, and `font set|reset`: saved like the pickers
//! save them, off the UI thread, then applied by the config watcher.
use super::{super::window_for, answer_when_done, socket};
use crate::{
    Error,
    config::{self, Config, ThemeName},
    control::protocol::{Caller, ErrorCode, Response, settings::Reply},
};
use gpui::App;
use std::time::Duration;

fn failed(error: &Error) -> Response {
    let code = match error {
        Error::Settings(config::Error::ThemeNotFound { .. }) => ErrorCode::NotFound,
        Error::Settings(
            config::Error::EmptyTheme
            | config::Error::InvalidThemePair
            | config::Error::InvalidThemePath,
        ) => ErrorCode::InvalidRequest,
        _ => ErrorCode::Failed,
    };
    Response::error(code, error.to_string())
}

pub(super) fn theme(
    caller: &Caller,
    name: String,
    wait: Duration,
    incoming: socket::Incoming,
    watches: &mut super::Watches,
    cx: &mut App,
) {
    let name = name.trim().to_owned();
    if name.is_empty() || name.len() > 4096 || name.chars().any(char::is_control) {
        incoming.respond(Response::error(
            ErrorCode::InvalidRequest,
            "Invalid theme name",
        ));
        return;
    }
    if let Err(error) = ThemeName::parse(&name) {
        incoming.respond(failed(&error));
        return;
    }
    let handle = match window_for(caller, cx) {
        Ok(handle) => handle,
        Err(response) => {
            incoming.respond(response);
            return;
        }
    };
    let Ok(config) = handle.read_with(cx, |view, _| view.config.clone()) else {
        incoming.respond(Response::error(ErrorCode::NoWindow, "The window closed"));
        return;
    };
    // Saving resolves the theme first, so a name no file matches is refused
    // before anything is written.
    let task = cx.background_executor().spawn(async move {
        match config.save_theme(&name) {
            Ok(()) => Response::done(format!("Theme set to {name}")),
            Err(error) => failed(&error),
        }
    });
    answer_when_done(task, wait, incoming, watches, cx);
}

pub(super) fn fonts(
    wait: Duration,
    incoming: socket::Incoming,
    watches: &mut super::Watches,
    cx: &mut App,
) {
    let text_system = cx.text_system().clone();
    // Listing every installed family is slow; never on the UI thread.
    let task = cx.background_executor().spawn(async move {
        Response::Settings {
            reply: Reply::Fonts {
                fonts: font_names(text_system.all_font_names()),
            },
        }
    });
    answer_when_done(task, wait, incoming, watches, cx);
}

pub(super) fn font(
    name: Option<String>,
    wait: Duration,
    incoming: socket::Incoming,
    watches: &mut super::Watches,
    cx: &mut App,
) {
    let name = name.map(|name| name.trim().to_owned());
    if name.as_deref().is_some_and(|name| {
        name.is_empty() || name.len() > 256 || name.chars().any(char::is_control)
    }) {
        incoming.respond(Response::error(
            ErrorCode::InvalidRequest,
            "Invalid font name",
        ));
        return;
    }
    let text_system = cx.text_system().clone();
    let task = cx.background_executor().spawn(async move {
        if let Some(name) = &name
            && !text_system.all_font_names().iter().any(|font| font == name)
        {
            return Response::error(
                ErrorCode::NotFound,
                format!("No installed font is named {name}; `herdr-gpui font list` lists them"),
            );
        }
        match Config::save_all_font_families(name.as_deref()) {
            Ok(()) => Response::done(match &name {
                Some(name) => format!("Font set to {name}"),
                None => "Fonts set back to the defaults".to_owned(),
            }),
            Err(error) => failed(&error),
        }
    });
    answer_when_done(task, wait, incoming, watches, cx);
}

/// Installed families as `settings fonts` lists them: sorted without regard
/// to case, without duplicates or blank names.
pub(super) fn font_names(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut names: Vec<_> = names
        .into_iter()
        .filter(|name| !name.trim().is_empty())
        .collect();
    names.sort_by_cached_key(|name| (name.to_lowercase(), name.clone()));
    names.dedup();
    names
}
