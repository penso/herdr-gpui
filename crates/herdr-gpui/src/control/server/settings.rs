//! `config reload|check`, `theme set`, `font`, `keybinds`, `caffeine`,
//! `sound play`, `bell`, `update`, and `integrations`. Files are written on
//! the background executor; the config watcher then applies them as it
//! applies any edit, so a request only waits for the write to finish.
mod caffeine;
mod keys;
mod saves;
mod services;
#[cfg(test)]
mod tests;

use super::{Watches, in_free_window, in_window, socket};
use crate::{
    control::protocol::{
        Call, ErrorCode, Response,
        settings::{Reply, SettingsOp, Sound},
    },
    controls::Command,
};
use gpui::App;
use std::{cell::RefCell, rc::Rc, time::Duration};

pub(super) fn serve(
    call: Call<SettingsOp>,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let wait = Duration::from_secs(call.op.wait_seconds());
    let Call { caller, op } = call;
    match op {
        SettingsOp::ReloadConfig => {
            incoming.respond(in_free_window(&caller, cx, |view, window, cx| {
                view.command(Command::ReloadConfig, window, cx);
                Response::done("Reloading Herdr's config and Herdr UI's")
            }))
        }
        SettingsOp::CheckConfig => {
            incoming.respond(in_window(&caller, cx, |view, _, _| Response::Settings {
                reply: Reply::Config {
                    missing_fonts: view.config.missing_fonts.clone(),
                },
            }))
        }
        SettingsOp::Theme { name } => saves::theme(&caller, name, wait, incoming, watches, cx),
        SettingsOp::Fonts => saves::fonts(wait, incoming, watches, cx),
        SettingsOp::Font { name } => saves::font(name, wait, incoming, watches, cx),
        SettingsOp::Keybinds => incoming.respond(keys::keybinds(&caller, cx)),
        SettingsOp::Caffeine { switch, seconds } => {
            caffeine::serve(switch, seconds, incoming, watches, cx);
        }
        SettingsOp::Sound { sound } => incoming.respond(play(&caller, sound, cx)),
        SettingsOp::Bell => incoming.respond(in_window(&caller, cx, |_, window, _| {
            // As a terminal bell rings when both reactions are on: attention
            // is only asked for while the window is in the background.
            if !window.is_window_active() {
                window.request_attention();
            }
            window.play_system_bell();
            Response::done("Rang the bell")
        })),
        SettingsOp::Update => incoming.respond(services::update(cx)),
        SettingsOp::CheckUpdate => services::check_update(wait, incoming, watches, cx),
        SettingsOp::Integrations => services::integrations(&caller, wait, incoming, watches, cx),
    }
}

fn play(caller: &crate::control::protocol::Caller, sound: Sound, cx: &mut App) -> Response {
    in_window(caller, cx, |view, _, _| {
        if crate::sound::muted() {
            return Response::error(
                ErrorCode::Unsupported,
                "Herdr UI plays no sounds: HERDR_DISABLE_SOUND is set in its environment",
            );
        }
        if view.sound.play(sound.into()) {
            Response::done(format!("Playing the {} sound", sound.name()))
        } else {
            Response::error(
                ErrorCode::Busy,
                "The sound player is unavailable or busy; try again",
            )
        }
    })
}

/// Where a task off the UI thread leaves its answer for a parked request.
type Slot = Rc<RefCell<Option<Response>>>;

/// Answers `incoming` once `task` finishes, or with a timeout after `wait`.
pub(super) fn answer_when_done(
    task: gpui::Task<Response>,
    wait: Duration,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let slot = Slot::default();
    let filled = slot.clone();
    cx.spawn(async move |_| {
        let response = task.await;
        filled.borrow_mut().replace(response);
    })
    .detach();
    watches.add(
        incoming,
        wait,
        move |_| slot.borrow_mut().take(),
        Response::error(ErrorCode::Timeout, "Herdr UI did not finish in time"),
        cx,
    );
}
