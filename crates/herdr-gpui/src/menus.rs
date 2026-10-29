//! Shared definitions for the macOS native and the Linux and Windows
//! in-window menu bars.
//! Every item dispatches the same action the palette and the keymap use.
// i18n: migrated

pub(crate) mod in_window;

use crate::i18n::fl;
use crate::{
    CheckForUpdates, PlaySound, Quit, RunCommand, ShowHerdrNotDetected, ShowLogs,
    ShowUpdatePreview,
    actions::{
        Copy, Cut, Paste, RingBellPreview, SelectAll, SetLayout, SetTabLayout, ShowToastPreview,
        ShowUpdateDownloadPreview, ShowUpdateHomebrewPreview, TogglePseudoLocale,
        ToggleSlowAnimations,
    },
    config::{Config, LayoutMode, TabLayoutMode},
    controls::Command,
};
#[cfg(target_os = "macos")]
use crate::{Hide, HideOthers, Minimize, ShowAll};
use gpui::{App, Menu, MenuItem, OsAction};
use herdr_client::protocol::SemanticNotificationKind;
use herdr_i18n::Pseudo;

/// Linux and Windows have no global menu bar, so each window draws its own.
pub(crate) const IN_WINDOW: bool = cfg!(any(target_os = "linux", windows));

/// Only headings are needed while the in-window menus are closed.
struct Headings(Vec<(gpui::SharedString, bool)>);
impl gpui::Global for Headings {}

/// Installs the menu bar, checking the layout the latest config picked.
pub(crate) fn install(cx: &mut App) {
    let menus = cx
        .try_global::<crate::app::InitialAppearance>()
        .map_or_else(
            || menus(&Config::default()),
            |appearance| menus(&appearance.config),
        );
    cx.set_global(Headings(
        menus
            .iter()
            .map(|menu| (menu.name.clone(), menu.disabled))
            .collect(),
    ));
    cx.set_menus(menus);
}

/// View > Layout: Herdr's densities, their rounded versions, the layouts
/// with a design of their own, then Devices, which lists agents by device;
/// the one in use checked.
fn layout_menu(current: LayoutMode) -> MenuItem {
    let items = LayoutMode::ALL
        .iter()
        .enumerate()
        .flat_map(|(index, &mode)| {
            let group = matches!(index, 3 | 6 | 9).then(MenuItem::separator);
            group.into_iter().chain([
                MenuItem::action(mode.label(), SetLayout { mode }).checked(mode == current)
            ])
        });
    MenuItem::submenu(Menu::new(fl!("menus-layout")).items(items))
}

/// View > Tab Layout: how tab strips draw their tabs, the one in use checked.
fn tab_layout_menu(current: TabLayoutMode) -> MenuItem {
    let items = TabLayoutMode::ALL.iter().map(|&mode| {
        MenuItem::action(mode.label(), SetTabLayout { mode }).checked(mode == current)
    });
    MenuItem::submenu(Menu::new(fl!("menus-tab-layout")).items(items))
}

/// Whether the menu bar built from `new` differs from the one built from
/// `old`, so a config reload knows to install it again.
pub(crate) fn changed(old: &Config, new: &Config) -> bool {
    old.layout.mode != new.layout.mode
        || old.tab_layout != new.tab_layout
        || old.features.qa_menu != new.features.qa_menu
}

/// The menu bar, with the config's layout checked under View > Layout and
/// its tab layout under View > Tab Layout, and QA last when
/// `features.qa_menu` is on.
pub(crate) fn menus(config: &Config) -> Vec<Menu> {
    let mut menus = vec![
        Menu {
            name: fl!("menus-herdr").into(),
            disabled: false,
            items: vec![
                MenuItem::action(
                    fl!("menus-about-herdr"),
                    RunCommand {
                        command: Command::About,
                    },
                ),
                MenuItem::separator(),
                MenuItem::action(
                    fl!("menus-command-palette"),
                    RunCommand {
                        command: Command::Palette,
                    },
                ),
                MenuItem::action(
                    fl!("menus-settings"),
                    RunCommand {
                        command: Command::Settings,
                    },
                ),
                MenuItem::action(
                    fl!("menus-keyboard-shortcuts"),
                    RunCommand {
                        command: Command::Keybinds,
                    },
                ),
                MenuItem::action(fl!("menus-check-for-updates"), CheckForUpdates),
                // macOS reserves Hide for every app; without these items
                // Cmd-H has no key equivalent and does nothing.
                #[cfg(target_os = "macos")]
                MenuItem::separator(),
                #[cfg(target_os = "macos")]
                MenuItem::action(fl!("menus-hide-herdr"), Hide),
                #[cfg(target_os = "macos")]
                MenuItem::action(fl!("menus-hide-others"), HideOthers),
                #[cfg(target_os = "macos")]
                MenuItem::action(fl!("menus-show-all"), ShowAll),
                MenuItem::separator(),
                MenuItem::action(fl!("menus-quit-herdr"), Quit),
            ],
        },
        Menu {
            name: fl!("menus-file").into(),
            disabled: false,
            items: vec![
                MenuItem::action(
                    fl!("menus-new-workspace"),
                    RunCommand {
                        command: Command::Workspace,
                    },
                ),
                MenuItem::action(
                    fl!("menus-new-worktree"),
                    RunCommand {
                        command: Command::NewWorktree,
                    },
                ),
                MenuItem::action(
                    fl!("menus-new-tab"),
                    RunCommand {
                        command: Command::Tab,
                    },
                ),
                MenuItem::action(
                    fl!("menus-go-to"),
                    RunCommand {
                        command: Command::WorkspacePicker,
                    },
                ),
                MenuItem::action(
                    fl!("menus-go-to-symbol"),
                    RunCommand {
                        command: Command::GoToSymbol,
                    },
                ),
                MenuItem::action(
                    fl!("menus-go-to-file"),
                    RunCommand {
                        command: Command::GoToFile,
                    },
                ),
                MenuItem::separator(),
                MenuItem::action(
                    fl!("menus-close-pane"),
                    RunCommand {
                        command: Command::ClosePane,
                    },
                ),
                MenuItem::action(
                    fl!("menus-close-tab"),
                    RunCommand {
                        command: Command::CloseTab,
                    },
                ),
                MenuItem::action(
                    fl!("menus-reopen-closed-tab"),
                    RunCommand {
                        command: Command::ReopenClosedTab,
                    },
                ),
            ],
        },
        // OS actions also route these items to native text fields, such as
        // the file panels, the way every macOS app's Edit menu does.
        Menu {
            name: fl!("menus-edit").into(),
            disabled: false,
            items: vec![
                MenuItem::os_action(fl!("menus-cut"), Cut, OsAction::Cut),
                MenuItem::os_action(fl!("menus-copy"), Copy, OsAction::Copy),
                MenuItem::os_action(fl!("menus-paste"), Paste, OsAction::Paste),
                MenuItem::separator(),
                MenuItem::os_action(fl!("menus-select-all"), SelectAll, OsAction::SelectAll),
            ],
        },
        Menu {
            name: fl!("menus-view").into(),
            disabled: false,
            items: vec![
                MenuItem::action(
                    fl!("menus-increase-font-size"),
                    RunCommand {
                        command: Command::IncreaseFontSize,
                    },
                ),
                MenuItem::action(
                    fl!("menus-decrease-font-size"),
                    RunCommand {
                        command: Command::DecreaseFontSize,
                    },
                ),
                MenuItem::separator(),
                MenuItem::action(
                    fl!("menus-reset-font-size"),
                    RunCommand {
                        command: Command::ResetFontSize,
                    },
                ),
                MenuItem::separator(),
                layout_menu(config.layout.mode),
                tab_layout_menu(config.tab_layout),
                MenuItem::action(
                    fl!("menus-devices-overview"),
                    RunCommand {
                        command: Command::DevicesOverview,
                    },
                ),
                MenuItem::separator(),
                MenuItem::action(
                    fl!("menus-toggle-full-screen"),
                    RunCommand {
                        command: Command::ToggleFullScreen,
                    },
                ),
                MenuItem::action(
                    fl!("menus-toggle-zen-mode"),
                    RunCommand {
                        command: Command::ToggleZenMode,
                    },
                ),
            ],
        },
        Menu {
            name: fl!("menus-terminal").into(),
            disabled: false,
            items: vec![
                MenuItem::action(
                    fl!("menus-split-vertically-right"),
                    RunCommand {
                        command: Command::SplitRight,
                    },
                ),
                MenuItem::action(
                    fl!("menus-split-horizontally-down"),
                    RunCommand {
                        command: Command::SplitDown,
                    },
                ),
                MenuItem::separator(),
                MenuItem::action(
                    fl!("menus-next-tab"),
                    RunCommand {
                        command: Command::NextTab,
                    },
                ),
                MenuItem::action(
                    fl!("menus-previous-tab"),
                    RunCommand {
                        command: Command::PreviousTab,
                    },
                ),
                MenuItem::action(
                    fl!("menus-back"),
                    RunCommand {
                        command: Command::Back,
                    },
                ),
                MenuItem::action(
                    fl!("menus-forward"),
                    RunCommand {
                        command: Command::Forward,
                    },
                ),
                MenuItem::action(
                    fl!("menus-toggle-pane-zoom"),
                    RunCommand {
                        command: Command::Zoom,
                    },
                ),
                MenuItem::action(
                    fl!("menus-clear-pane"),
                    RunCommand {
                        command: Command::ClearPane,
                    },
                ),
                MenuItem::action(
                    fl!("menus-find"),
                    RunCommand {
                        command: Command::Find,
                    },
                ),
                MenuItem::action(
                    fl!("menus-find-next"),
                    RunCommand {
                        command: Command::FindNext,
                    },
                ),
                MenuItem::action(
                    fl!("menus-find-previous"),
                    RunCommand {
                        command: Command::FindPrevious,
                    },
                ),
                MenuItem::action(
                    fl!("menus-copy-mode"),
                    RunCommand {
                        command: Command::CopyMode,
                    },
                ),
                MenuItem::action(
                    fl!("menus-open-scrollback-in-editor"),
                    RunCommand {
                        command: Command::EditScrollback,
                    },
                ),
                MenuItem::action(
                    fl!("menus-open-notification-target"),
                    RunCommand {
                        command: Command::OpenNotificationTarget,
                    },
                ),
                MenuItem::action(
                    fl!("menus-toggle-sidebar"),
                    RunCommand {
                        command: Command::ToggleSidebar,
                    },
                ),
                MenuItem::action(
                    fl!("menus-toggle-status-bar"),
                    RunCommand {
                        command: Command::ToggleStatusBar,
                    },
                ),
                MenuItem::action(
                    fl!("menus-open-vs-code"),
                    RunCommand {
                        command: Command::OpenCode,
                    },
                ),
                MenuItem::action(
                    fl!("menus-open-px0"),
                    RunCommand {
                        command: Command::OpenPx0,
                    },
                ),
                MenuItem::separator(),
                MenuItem::action(
                    fl!("menus-reconnect"),
                    RunCommand {
                        command: Command::Reconnect,
                    },
                ),
            ],
        },
        Menu {
            name: fl!("menus-window").into(),
            disabled: false,
            items: vec![
                // Without this item Cmd-M has no key equivalent and does
                // nothing; the pane zoom above is unrelated to the window.
                #[cfg(target_os = "macos")]
                MenuItem::action(fl!("menus-minimize"), Minimize),
                #[cfg(target_os = "macos")]
                MenuItem::separator(),
                MenuItem::action(
                    fl!("menus-new-window"),
                    RunCommand {
                        command: Command::NewWindow,
                    },
                ),
                MenuItem::action(
                    fl!("menus-cycle-through-windows"),
                    RunCommand {
                        command: Command::CycleWindows,
                    },
                ),
                MenuItem::action(
                    fl!("menus-close-window"),
                    RunCommand {
                        command: Command::CloseWindow,
                    },
                ),
                MenuItem::separator(),
                MenuItem::action(fl!("menus-logs"), ShowLogs),
            ],
        },
    ];
    if config.features.qa_menu {
        menus.push(qa_menu());
    }
    menus
}

/// Previews and test actions for checking the app's surfaces by hand.
fn qa_menu() -> Menu {
    let pseudo = crate::i18n::shown_pseudo();
    Menu {
        name: fl!("menus-qa").into(),
        disabled: false,
        items: vec![
            MenuItem::action(fl!("menus-qa-herdr-not-detected"), ShowHerdrNotDetected),
            MenuItem::action(fl!("menus-qa-update-available"), ShowUpdatePreview),
            MenuItem::action(fl!("menus-qa-update-download"), ShowUpdateDownloadPreview),
            MenuItem::action(fl!("menus-qa-update-homebrew"), ShowUpdateHomebrewPreview),
            MenuItem::action(fl!("menus-qa-play-sound"), PlaySound),
            MenuItem::action(fl!("menus-qa-ring-bell"), RingBellPreview),
            MenuItem::action(fl!("menus-qa-slow-animations"), ToggleSlowAnimations)
                .checked(crate::motion::is_slow()),
            MenuItem::action(
                fl!("menus-qa-pseudo-accented"),
                TogglePseudoLocale {
                    pseudo: Pseudo::Accented,
                },
            )
            .checked(pseudo == Some(Pseudo::Accented)),
            MenuItem::action(
                fl!("menus-qa-pseudo-bidi"),
                TogglePseudoLocale {
                    pseudo: Pseudo::Bidi,
                },
            )
            .checked(pseudo == Some(Pseudo::Bidi)),
            #[cfg(any(target_os = "macos", windows))]
            MenuItem::action(
                fl!("menus-qa-badge-enable"),
                crate::actions::SetBadgePreview { enabled: true },
            ),
            #[cfg(any(target_os = "macos", windows))]
            MenuItem::action(
                fl!("menus-qa-badge-disable"),
                crate::actions::SetBadgePreview { enabled: false },
            ),
            MenuItem::separator(),
            MenuItem::action(
                fl!("menus-qa-toast-needs-attention"),
                ShowToastPreview {
                    kind: SemanticNotificationKind::NeedsAttention,
                },
            ),
            MenuItem::action(
                fl!("menus-qa-toast-finished"),
                ShowToastPreview {
                    kind: SemanticNotificationKind::Finished,
                },
            ),
            MenuItem::action(
                fl!("menus-qa-toast-update-installed"),
                ShowToastPreview {
                    kind: SemanticNotificationKind::UpdateInstalled,
                },
            ),
            MenuItem::action(
                fl!("menus-qa-toast-custom"),
                ShowToastPreview {
                    kind: SemanticNotificationKind::Custom,
                },
            ),
            MenuItem::separator(),
            MenuItem::action(
                fl!("menus-qa-notification"),
                crate::actions::ShowSystemNotificationPreview,
            ),
        ],
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
