// objc 0.2's selectors expand a legacy cargo-clippy cfg in the native test adapter.
#![cfg_attr(feature = "integration-test", allow(unexpected_cfgs))]

mod about;
mod actions;
mod app;
#[cfg(any(target_os = "macos", test))]
mod app_badge;
mod app_icon;
mod avatars;
mod cli;
mod close_modal;
mod config;
mod connection;
mod constants;
mod controls;
mod daemon;
mod diagnostics;
mod endpoint;
mod error;
mod fonts;
mod git;
mod github;
mod icons;
mod input;
mod keymap;
mod kit_theme;
mod log_window;
mod menu;
mod menus;
mod navigation;
mod notifications;
mod palette;
mod pane_menu;
mod preferences;
mod presentation;
mod pull_request;
mod repo_items;
mod sidebar;
mod sound;
mod state;
mod tab_menu;
mod terminal;
mod terminal_painter;
#[cfg(test)]
mod test_support;
mod theme_picker;
mod titlebar;
mod update_panel;
mod updater;
mod window;
mod window_state;
mod worktree;
mod worktree_banner;

#[cfg(feature = "integration-test")]
mod performance;
#[cfg(feature = "integration-test")]
mod smoke;

pub use error::{Error, Result};

pub(crate) use {
    actions::{
        CheckForUpdates, PlaySound, Quit, RunCommand, ShowHerdrNotDetected, ShowLogs,
        ShowUpdatePreview, bind_keys,
    },
    app::open_additional_window,
    constants::{APP_VERSION, RELEASE_BUILD, WINDOW_TITLE},
    menus::menus,
    navigation::{NavigationTarget, OwnedNavigationTarget},
    window::HerdrWindow,
};

// Re-exported at the root because they are referenced crate-wide; the domain
// modules below are where they are defined and changed.
#[cfg(feature = "integration-test")]
pub(crate) use app::open_window;
// Only the macOS smoke checks drive the connection status from the root.
#[cfg(all(feature = "integration-test", target_os = "macos"))]
pub(crate) use state::ConnectionStatus;

pub(crate) use {controls::Command, state::LiveState, terminal::WheelAccumulator};

fn main() -> std::process::ExitCode {
    let exit = app::run();
    if exit != std::process::ExitCode::SUCCESS {
        return exit;
    }
    #[cfg(feature = "integration-test")]
    return std::process::ExitCode::from(
        smoke::EXIT_CODE.load(std::sync::atomic::Ordering::SeqCst),
    );
    #[cfg(not(feature = "integration-test"))]
    std::process::ExitCode::SUCCESS
}
