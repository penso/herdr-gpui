//! The native menu bar. Every item dispatches the same `Command` the palette
//! and the keymap use, so a command exists in one place only.

use crate::{
    CheckForUpdates, Quit, RunCommand, ShowLogs,
    actions::{Copy, Cut, Paste, SelectAll},
    controls::Command,
};
#[cfg(feature = "qa-menu")]
use crate::{
    PlaySound, ShowHerdrNotDetected, ShowUpdatePreview,
    actions::{ShowToastPreview, ShowUpdateDownloadPreview, ShowUpdateHomebrewPreview},
};
use gpui::{Menu, MenuItem, OsAction};
#[cfg(feature = "qa-menu")]
use herdr_client::protocol::SemanticNotificationKind;

pub(crate) fn menus() -> Vec<Menu> {
    vec![
        Menu {
            name: "Herdr".into(),
            disabled: false,
            items: vec![
                MenuItem::action(
                    "About Herdr",
                    RunCommand {
                        command: Command::About,
                    },
                ),
                MenuItem::separator(),
                MenuItem::action(
                    "Command Palette",
                    RunCommand {
                        command: Command::Palette,
                    },
                ),
                MenuItem::action(
                    "Settings",
                    RunCommand {
                        command: Command::Settings,
                    },
                ),
                MenuItem::action(
                    "Keyboard Shortcuts",
                    RunCommand {
                        command: Command::Keybinds,
                    },
                ),
                MenuItem::action("Check for Updates...", CheckForUpdates),
                MenuItem::separator(),
                MenuItem::action("Quit Herdr", Quit),
            ],
        },
        Menu {
            name: "File".into(),
            disabled: false,
            items: vec![
                MenuItem::action(
                    "New Workspace",
                    RunCommand {
                        command: Command::Workspace,
                    },
                ),
                MenuItem::action(
                    "New Worktree...",
                    RunCommand {
                        command: Command::NewWorktree,
                    },
                ),
                MenuItem::action(
                    "New Tab",
                    RunCommand {
                        command: Command::Tab,
                    },
                ),
                MenuItem::action(
                    "Go To",
                    RunCommand {
                        command: Command::WorkspacePicker,
                    },
                ),
                MenuItem::separator(),
                MenuItem::action(
                    "Close Pane...",
                    RunCommand {
                        command: Command::ClosePane,
                    },
                ),
                MenuItem::action(
                    "Close Tab...",
                    RunCommand {
                        command: Command::CloseTab,
                    },
                ),
            ],
        },
        // OS actions also route these items to native text fields, such as
        // the file panels, the way every macOS app's Edit menu does.
        Menu {
            name: "Edit".into(),
            disabled: false,
            items: vec![
                MenuItem::os_action("Cut", Cut, OsAction::Cut),
                MenuItem::os_action("Copy", Copy, OsAction::Copy),
                MenuItem::os_action("Paste", Paste, OsAction::Paste),
                MenuItem::separator(),
                MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
            ],
        },
        Menu {
            name: "View".into(),
            disabled: false,
            items: vec![
                MenuItem::action(
                    "Increase Font Size",
                    RunCommand {
                        command: Command::IncreaseFontSize,
                    },
                ),
                MenuItem::action(
                    "Decrease Font Size",
                    RunCommand {
                        command: Command::DecreaseFontSize,
                    },
                ),
                MenuItem::separator(),
                MenuItem::action(
                    "Reset Font Size",
                    RunCommand {
                        command: Command::ResetFontSize,
                    },
                ),
                MenuItem::separator(),
                MenuItem::action(
                    "Collapse or Expand Sidebar",
                    RunCommand {
                        command: Command::CollapseSidebar,
                    },
                ),
                MenuItem::action(
                    "Show or Hide Sidebar",
                    RunCommand {
                        command: Command::ToggleSidebar,
                    },
                ),
            ],
        },
        Menu {
            name: "Terminal".into(),
            disabled: false,
            items: vec![
                MenuItem::action(
                    "Split Vertically (Right)",
                    RunCommand {
                        command: Command::SplitRight,
                    },
                ),
                MenuItem::action(
                    "Split Horizontally (Down)",
                    RunCommand {
                        command: Command::SplitDown,
                    },
                ),
                MenuItem::separator(),
                MenuItem::action(
                    "Next Tab",
                    RunCommand {
                        command: Command::NextTab,
                    },
                ),
                MenuItem::action(
                    "Previous Tab",
                    RunCommand {
                        command: Command::PreviousTab,
                    },
                ),
                MenuItem::action(
                    "Toggle Pane Zoom",
                    RunCommand {
                        command: Command::Zoom,
                    },
                ),
                MenuItem::action(
                    "Clear Pane",
                    RunCommand {
                        command: Command::ClearPane,
                    },
                ),
                MenuItem::action(
                    "Open Notification Target",
                    RunCommand {
                        command: Command::OpenNotificationTarget,
                    },
                ),
                MenuItem::separator(),
                MenuItem::action(
                    "Reconnect",
                    RunCommand {
                        command: Command::Reconnect,
                    },
                ),
            ],
        },
        Menu {
            name: "Window".into(),
            disabled: false,
            items: vec![
                MenuItem::action(
                    "New Window",
                    RunCommand {
                        command: Command::NewWindow,
                    },
                ),
                MenuItem::separator(),
                MenuItem::action("Logs", ShowLogs),
            ],
        },
        #[cfg(feature = "qa-menu")]
        Menu {
            name: "QA".into(),
            disabled: false,
            items: vec![
                MenuItem::action("Show herdr non-detected modal", ShowHerdrNotDetected),
                MenuItem::action("Show app update available", ShowUpdatePreview),
                MenuItem::action(
                    "Show update download progress (50%)",
                    ShowUpdateDownloadPreview,
                ),
                MenuItem::action("Show Homebrew update progress", ShowUpdateHomebrewPreview),
                MenuItem::action("Play Sound", PlaySound),
                #[cfg(target_os = "macos")]
                MenuItem::action(
                    "Enable badge",
                    crate::actions::SetBadgePreview { enabled: true },
                ),
                #[cfg(target_os = "macos")]
                MenuItem::action(
                    "Disable badge preview",
                    crate::actions::SetBadgePreview { enabled: false },
                ),
                MenuItem::separator(),
                MenuItem::action(
                    "Show NeedsAttention toast",
                    ShowToastPreview {
                        kind: SemanticNotificationKind::NeedsAttention,
                    },
                ),
                MenuItem::action(
                    "Show Finished toast",
                    ShowToastPreview {
                        kind: SemanticNotificationKind::Finished,
                    },
                ),
                MenuItem::action(
                    "Show UpdateInstalled toast",
                    ShowToastPreview {
                        kind: SemanticNotificationKind::UpdateInstalled,
                    },
                ),
                MenuItem::action(
                    "Show Custom toast",
                    ShowToastPreview {
                        kind: SemanticNotificationKind::Custom,
                    },
                ),
            ],
        },
    ]
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "qa-menu")]
    fn badge_preview_is_available_only_in_the_macos_qa_menu() {
        let menus = menus();
        let qa = menus
            .iter()
            .find(|menu| menu.name.as_ref() == "QA")
            .unwrap();
        for (label, enabled) in [("Enable badge", true), ("Disable badge preview", false)] {
            let action = qa.items.iter().find_map(|item| match item {
                MenuItem::Action { name, action, .. } if name.as_ref() == label => Some(action),
                _ => None,
            });
            if cfg!(target_os = "macos") {
                assert!(
                    action
                        .unwrap()
                        .partial_eq(&crate::actions::SetBadgePreview { enabled })
                );
            } else {
                assert!(action.is_none());
            }
        }
    }

    #[test]
    #[cfg(feature = "qa-menu")]
    fn qa_menu_carries_update_progress_previews() {
        let menus = menus();
        let qa = menus
            .iter()
            .find(|menu| menu.name.as_ref() == "QA")
            .unwrap();
        for (label, expected) in [
            (
                "Show update download progress (50%)",
                Box::new(ShowUpdateDownloadPreview) as Box<dyn gpui::Action>,
            ),
            (
                "Show Homebrew update progress",
                Box::new(ShowUpdateHomebrewPreview),
            ),
        ] {
            assert!(
                qa.items.iter().any(|item| matches!(item,
                    MenuItem::Action { name, action, .. }
                        if name.as_ref() == label && action.partial_eq(expected.as_ref())
                )),
                "{label}"
            );
        }
    }

    #[test]
    fn qa_menu_requires_explicit_feature() {
        let menus = menus();
        let names: Vec<_> = menus.iter().map(|menu| menu.name.as_ref()).collect();
        let mut expected = vec!["Herdr", "File", "Edit", "View", "Terminal", "Window"];
        if cfg!(feature = "qa-menu") {
            expected.push("QA");
        }
        assert_eq!(names, expected);
    }

    fn edit_action(label: &str) -> Box<dyn gpui::Action> {
        menus()
            .into_iter()
            .find(|menu| menu.name.as_ref() == "Edit")
            .unwrap()
            .items
            .into_iter()
            .find_map(|item| match item {
                MenuItem::Action { name, action, .. } if name.as_ref() == label => Some(action),
                _ => None,
            })
            .unwrap()
    }

    /// Standard Edit items: native selectors for OS text fields, the shortcuts
    /// every macOS app shows, and labels that do not claim the keystrokes.
    #[gpui::test]
    fn edit_menu_carries_standard_items_and_shortcut_labels(cx: &mut gpui::TestAppContext) {
        let items = &menus()
            .into_iter()
            .find(|menu| menu.name.as_ref() == "Edit")
            .unwrap()
            .items;
        let expected = [
            ("Cut", OsAction::Cut, "cmd-x"),
            ("Copy", OsAction::Copy, "cmd-c"),
            ("Paste", OsAction::Paste, "cmd-v"),
            ("Select All", OsAction::SelectAll, "cmd-a"),
        ];
        let actions: Vec<_> = items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Action {
                    name, os_action, ..
                } => Some((name.as_ref(), *os_action)),
                _ => None,
            })
            .collect();
        assert!(
            actions
                .iter()
                .map(|(name, os)| (*name, *os))
                .eq(expected.iter().map(|(name, os, _)| (*name, Some(*os)))),
            "Edit menu items or their native selectors changed"
        );
        cx.update(|cx| {
            crate::bind_keys(cx);
            for (name, _, keystroke) in expected {
                let action = edit_action(name);
                let keymap = cx.key_bindings();
                let keymap = keymap.borrow();
                let bindings: Vec<_> = keymap.bindings_for_action(action.as_ref()).collect();
                assert_eq!(bindings.len(), 1, "{name}");
                assert_eq!(
                    bindings[0].keystrokes()[0].inner(),
                    &gpui::Keystroke::parse(keystroke).unwrap()
                );
                // No element sets the context, so the keystroke never dispatches
                // the action ahead of the focused element's own key handler.
                assert!(bindings[0].predicate().is_some());
            }
        });
    }

    /// Each item is enabled only where it acts, and choosing it does what its
    /// shortcut does in the focused element.
    #[gpui::test]
    fn edit_menu_items_follow_focus(cx: &mut gpui::TestAppContext) {
        use crate::{
            dialog_input::DialogInput,
            menu::{Page, WorkspaceAction},
            sidebar::layout_tests::{fixture_window, full_draw},
        };
        use gpui::ClipboardItem;

        const LABELS: [&str; 4] = ["Cut", "Copy", "Paste", "Select All"];
        let (view, cx) = cx.add_window_view(|window, cx| {
            crate::bind_keys(cx);
            fixture_window(window, cx)
        });
        let available = |cx: &mut gpui::VisualTestContext| {
            cx.update(|window, cx| {
                full_draw(window, cx).clear(cx);
                LABELS
                    .into_iter()
                    .filter(|label| window.is_action_available(edit_action(label).as_ref(), cx))
                    .collect::<Vec<_>>()
            })
        };
        let choose = |label: &str, cx: &mut gpui::VisualTestContext| {
            cx.update(|window, cx| window.dispatch_action(edit_action(label), cx));
            cx.run_until_parked();
        };

        // A released selection is already copied; the terminal only pastes.
        cx.update(|window, cx| view.read(cx).focus.clone().focus(window, cx));
        assert_eq!(available(cx), ["Paste"]);

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.live.status = crate::state::ConnectionStatus::Connected;
                view.open_workspace_menu("w3", Default::default(), window, cx);
                view.menu.page = Some(Page::Dialog(WorkspaceAction::Rename));
                view.menu.input = Some(DialogInput::new("draft".into()));
            });
            cx.write_to_clipboard(ClipboardItem::new_string("renamed".into()));
        });
        assert_eq!(available(cx), LABELS);
        choose("Select All", cx);
        choose("Paste", cx);
        let draft = |cx: &mut gpui::VisualTestContext| {
            view.read_with(cx, |view, _| view.menu.input.as_ref().unwrap().text.clone())
        };
        assert_eq!(draft(cx), "renamed");
        cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("other".into())));
        choose("Select All", cx);
        choose("Copy", cx);
        assert_eq!(draft(cx), "renamed");
        let clipboard = |cx: &mut gpui::VisualTestContext| {
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
        };
        assert_eq!(clipboard(cx).as_deref(), Some("renamed"));
        cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("other".into())));
        choose("Cut", cx);
        assert_eq!(draft(cx), "");
        assert_eq!(clipboard(cx).as_deref(), Some("renamed"));

        // A search field takes the action before the overlay around it.
        let search = cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.dismiss_menu(window, cx);
                view.open_theme_picker(window, cx);
            });
            cx.write_to_clipboard(ClipboardItem::new_string("nord".into()));
            view.read(cx).menu.themes.as_ref().unwrap().search.clone()
        });
        assert_eq!(available(cx), LABELS);
        choose("Paste", cx);
        assert_eq!(
            search.read_with(cx, |search, _| search.text().to_owned()),
            "nord"
        );
        view.read_with(cx, |view, _| {
            assert!(view.menu.input.is_none());
            assert_eq!(view.menu.page, Some(Page::Themes));
        });
    }

    /// The font size and sidebar items are the only way to reach these
    /// commands from the macOS menu bar, and each must dispatch the catalog
    /// command rather than an action of its own.
    #[test]
    fn view_menu_carries_the_font_size_and_sidebar_commands() {
        let menus = menus();
        let view = menus
            .iter()
            .find(|menu| menu.name.as_ref() == "View")
            .unwrap();
        let actions: Vec<_> = view
            .items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Action { name, action, .. } => Some((name.as_ref(), action)),
                _ => None,
            })
            .collect();
        let expected = [
            ("Increase Font Size", Command::IncreaseFontSize),
            ("Decrease Font Size", Command::DecreaseFontSize),
            ("Reset Font Size", Command::ResetFontSize),
            ("Collapse or Expand Sidebar", Command::CollapseSidebar),
            ("Show or Hide Sidebar", Command::ToggleSidebar),
        ];
        assert_eq!(actions.len(), expected.len());
        for ((name, action), (label, command)) in actions.iter().zip(expected) {
            assert_eq!(*name, label);
            assert!(action.partial_eq(&RunCommand { command }), "{label}");
        }
        // Reset is a different kind of act from stepping, so it sits apart.
        assert!(matches!(view.items[2], MenuItem::Separator));
        assert!(matches!(view.items[4], MenuItem::Separator));
    }
}
