// objc 0.2's selectors expand a legacy cargo-clippy cfg in the native test adapter.
#![cfg_attr(feature = "integration-test", allow(unexpected_cfgs))]
// Tests start real processes; production code builds them with `new_command`.
#![cfg_attr(test, allow(clippy::disallowed_methods))]

mod about;
mod accessibility;
mod actions;
mod agent_notes;
mod agent_skill;
mod app;
#[cfg(any(target_os = "macos", windows, test))]
mod app_badge;
mod app_icon;
mod automations;
mod avatars;
mod backdrop;
mod bell;
mod browser;
mod bundled;
mod caffeine;
mod caption;
mod chat;
mod cli;
mod close_modal;
// `cloud` is the machinery every provider shares; alone it offers nothing to add.
#[cfg(all(
    feature = "cloud",
    not(any(
        feature = "azure",
        feature = "boat",
        feature = "coder",
        feature = "container",
        feature = "daytona",
        feature = "opensandbox",
        feature = "railway",
        feature = "sprites"
    ))
))]
compile_error!(
    "the `cloud` feature is enabled through a provider feature, such as `azure`, `boat`, `coder`, `container`, `daytona`, `opensandbox`, `railway`, or `sprites`"
);
#[cfg(feature = "cloud")]
mod cloud;
mod cloud_setup;
mod code_search;
mod code_server;
mod code_view;
mod config;
mod config_diagnostic;
mod connection;
mod constants;
mod contrast;
mod control;
mod copy_mode;
mod daemon;
mod devices_overview;
mod dialog_input;
mod dispatch;
mod editor;
mod endpoint;
mod error;
mod fan_out;
mod find;
mod font_picker;
mod fonts;
mod github;
mod group_menu;
mod group_terminals;
mod herdr_settings;
mod icon_font_notice;
mod icons;
mod input;
mod integrations;
#[cfg(windows)]
mod jump_list;
mod keymap;
#[cfg(windows)]
mod launch_at_login;
mod lenient;
mod link;
mod links;
mod listening_ports;
mod log_targets;
mod log_window;
mod mark_layer;
mod menu;
mod menus;
mod merged_cleanup;
#[cfg(feature = "mockup")]
mod mockup;
mod motion;
mod navigation;
mod network_quality;
mod notifications;
mod orchestrator;
mod osc52;
mod overview;
mod owned_process;
mod palette;
mod pane_menu;
mod pane_move;
mod panel_resize;
mod plugin_values;
mod port_forward;
mod preferences;
mod presentation;
mod processes;
mod progress;
mod pull_request;
mod px0;
mod release_notes;
mod remote;
mod remote_browser;
mod reorder;
mod repo_items;
mod restart_manager;
mod review;
mod scrollback;
mod search_input;
mod security;
mod sessions;
mod settings_panel;
mod settings_window;
#[cfg(unix)]
mod setup_relay;
mod sidebar;
mod smooth_scroll;
mod sound;
mod state;
mod state_file;
mod storage_warning;
mod system_load;
mod tab_layout;
mod tab_menu;
mod taskbar;
mod teleport;
mod terminal;
mod terminal_notes;
mod terminal_painter;
mod theme_picker;
mod titlebar;
mod toggles;
mod update_panel;
mod usage;
mod window;
mod window_state;
mod words;
mod worktree;
mod worktree_banner;
mod worktree_identity;
mod worktree_notes;
mod worktree_scripts;

#[cfg(feature = "integration-test")]
mod performance;
#[cfg(feature = "integration-test")]
mod smoke;
#[cfg(all(test, unix))]
#[path = "../../test-support/executable.rs"]
mod test_executable;

pub use error::{Error, Result};

// Its own crate so it builds and is tested without GPUI; the alias keeps the
// app's `crate::updater` paths.
use herdr_updater as updater;
// GitHub's pull request actions; `github` and `pull_request` are the app's
// own modules, which add the avatars, the shared store, and badge colors.
use herdr_github::pr_actions;
// The orchestrator's records, readers, and worker, and the app's cache
// database; `orchestrator` is the app's own module, which adds the views.
use herdr_orchestrator::cache;
// The command catalog and its default shortcuts, without GPUI.
use herdr_commands as controls;
// The log file and crash reports, without GPUI.
use herdr_diagnostics::{self as diagnostics, crash};
// Git status and writes, checkpoints, and the code index, without GPUI.
use herdr_vcs::{checkpoint, code_index, git};
// Shared with herdr-client's own SSH and probe children.
use herdr_client::{local_path, login_env};
// Cloud providers, host scripts, and Teleport's core; `cloud` and `teleport`
// are the app's own modules, which add the jobs and views GPUI draws.
#[cfg(feature = "azure")]
use herdr_remote::azure;
#[cfg(feature = "boat")]
use herdr_remote::boat;
#[cfg(feature = "coder")]
use herdr_remote::coder;
#[cfg(feature = "container")]
use herdr_remote::container;
#[cfg(feature = "daytona")]
use herdr_remote::daytona;
#[cfg(feature = "opensandbox")]
use herdr_remote::opensandbox;
#[cfg(feature = "railway")]
use herdr_remote::railway;
#[cfg(feature = "sprites")]
use herdr_remote::sprites;

pub(crate) use {
    actions::{
        CheckForUpdates, Hide, HideOthers, Minimize, PlaySound, Quit, RunCommand, ShowAll,
        ShowHerdrNotDetected, ShowLogs, ShowUpdatePreview, ZoomWindow, bind_keys,
    },
    app::open_additional_window,
    constants::{APP_VERSION, RELEASE_BUILD, TAB_HEIGHT, TAB_WIDTH, WINDOW_TITLE},
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
