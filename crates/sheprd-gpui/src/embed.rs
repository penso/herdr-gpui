//! The main view, for another GPUI app's window to host. [`prepare`] reads
//! what [`crate::app`] reads before its event loop, and [`view`] installs it
//! and makes the view on the Local target.
//!
//! Not yet the whole app: the host owns the window, so what finds a main window
//! by its root view (the control socket, system notifications, opening links)
//! is left out here, as are the updater and the app menus.

use crate::{
    Hide, HideOthers, Quit, SheprdWindow, ShowAll, ShowLogs,
    app::{InitialAppearance, StartupAppearance, Stores, light_appearance, shared_sound},
    bind_keys,
    config::Config,
};
use gpui::{AnyView, App, AppContext as _, Window};
#[cfg(feature = "integration-test")]
use gpui::{
    Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput,
    point, px,
};
use sheprd_client::ConnectTarget;
#[cfg(feature = "integration-test")]
use std::time::Duration;

/// What is read before the event loop; see [`prepare`].
pub struct Startup {
    appearance: StartupAppearance,
    stores: Stores,
}

/// Reads settings and stores. Call it before the host's event loop, so the
/// first frame does no disk work on the UI thread.
pub fn prepare() -> Startup {
    sheprd_client::set_command_environment(crate::login_env::apply);
    sheprd_remote::config::set_loader(Config::load_remote);
    // Before anything resolves a config path or the daemon's socket.
    crate::login_env::adopt_config_home();
    Startup {
        appearance: StartupAppearance::load(Config::load_startup),
        stores: Stores::load(true),
    }
}

fn install(startup: Startup, cx: &mut App) {
    startup.stores.install(cx);
    cx.set_global(startup.appearance.select(light_appearance(cx)));
    let config = &cx.global::<InitialAppearance>().config;
    let (ssh, language) = (config.ssh.clone(), config.language.clone());
    crate::device_tags::DeviceTags::install(cx);
    crate::config::ssh_hosts::install(&ssh, cx);
    crate::i18n::install(&language, cx);
    crate::caffeine::install(cx);
    crate::endpoint::watch_network();
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(|_: &ShowLogs, cx| crate::log_window::open(cx));
    bind_keys(cx);
}

/// Makes the main view for the host to draw. `startup` is `None` for every
/// view after the first, which share what the first installed.
pub fn view(startup: Option<Startup>, window: &mut Window, cx: &mut App) -> AnyView {
    if let Some(startup) = startup {
        install(startup, cx);
    }
    let sound = shared_sound(cx);
    cx.new(|cx| {
        let mut view = SheprdWindow::new(
            ConnectTarget::Local,
            window,
            cx,
            #[cfg(feature = "integration-test")]
            false,
        );
        view.use_sound(sound);
        view
    })
    .into()
}

/// What [`drive`] does to the hosted view, in window coordinates.
#[cfg(feature = "integration-test")]
pub enum Drive {
    /// Presses at `from`, drags to `to`, and releases.
    Drag { from: (f32, f32), to: (f32, f32) },
    /// Types each keystroke in turn, a space between them.
    Keys(String),
    /// Types a keystroke such as `cmd-shift-p`.
    Key(String),
}

/// Sends `input` through the window as the platform would, after `delay`, for
/// checking that a pane offset and a host's focus leave input working.
#[cfg(feature = "integration-test")]
pub fn drive(view: AnyView, input: Drive, delay: Duration, window: &Window, cx: &mut App) {
    window
        .spawn(cx, async move |cx| {
            cx.background_executor().timer(delay).await;
            let _ = cx.update(|window, cx| match input {
                Drive::Drag { from, to } => {
                    let from = point(px(from.0), px(from.1));
                    let to = point(px(to.0), px(to.1));
                    let events = [
                        PlatformInput::MouseMove(MouseMoveEvent {
                            position: from,
                            pressed_button: None,
                            modifiers: Modifiers::default(),
                        }),
                        PlatformInput::MouseDown(MouseDownEvent {
                            button: MouseButton::Left,
                            position: from,
                            modifiers: Modifiers::default(),
                            click_count: 1,
                            first_mouse: false,
                        }),
                        PlatformInput::MouseMove(MouseMoveEvent {
                            position: to,
                            pressed_button: Some(MouseButton::Left),
                            modifiers: Modifiers::default(),
                        }),
                        PlatformInput::MouseUp(MouseUpEvent {
                            button: MouseButton::Left,
                            position: to,
                            modifiers: Modifiers::default(),
                            click_count: 1,
                        }),
                    ];
                    for (step, event) in events.into_iter().enumerate() {
                        let result = window.dispatch_event(event, cx);
                        if let Ok(view) = view.clone().downcast::<SheprdWindow>() {
                            let view = view.read(cx);
                            tracing::info!(
                                step,
                                prevented = result.default_prevented,
                                selection = view.selection.is_some(),
                                menu = view.menu.page.is_some(),
                                "drive: after event"
                            );
                        }
                    }
                    if let Ok(view) = view.downcast::<SheprdWindow>() {
                        let view = view.read(cx);
                        tracing::info!(
                            bounds = ?view.bounds,
                            grid = ?view.grid_position(from),
                            ready = view.live.surface_ready(),
                            selection = view.selection.is_some(),
                            menu = view.menu.page.is_some(),
                            "drive: terminal geometry"
                        );
                    }
                }
                Drive::Keys(text) => {
                    for key in text.split_whitespace() {
                        if let Ok(keystroke) = Keystroke::parse_with_os(key, crate::keymap::HOST_OS)
                        {
                            let handled = window.dispatch_keystroke(keystroke, cx);
                            tracing::info!(key, handled, "drive: keystroke");
                        }
                    }
                }
                Drive::Key(text) => match Keystroke::parse_with_os(&text, crate::keymap::HOST_OS) {
                    Ok(keystroke) => {
                        let handled = window.dispatch_keystroke(keystroke, cx);
                        tracing::info!(text, handled, "drive: keystroke");
                    }
                    Err(error) => tracing::warn!(%error, "drive: bad keystroke"),
                },
            });
        })
        .detach();
}
