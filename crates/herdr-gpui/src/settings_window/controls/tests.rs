use super::*;
use crate::config::{FontFace, LayoutMode};
use crate::settings_window::inputs::NumberField;
use core::prelude::v1::test;

mod activity_bar;
mod agents_placement;
mod automations;
mod backdrop;
mod change_counts;
mod chat_width;
mod device_icons;
mod features;
mod focus_follows_mouse;
mod indicators;
mod repository_icons;
mod session_restore;
mod sidebar_search;
mod sound_volume;
mod terminal_padding;
mod update_channel;

use crate::agent_skill::{AgentSkill, Choice};
use crate::settings_window::tests::{skill_fixture, skill_load};

#[gpui::test]
fn notification_rows_stack_inside_the_body_and_descriptions_wrap(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    for font_size in [12., 24., 48.] {
        for width in [680., 960.] {
            cx.simulate_resize(size(px(width), px(3200.)));
            view.update(cx, |view, cx| {
                view.section = Section::Notifications;
                view.shared = Some(crate::herdr_settings::Settings::parse_text("").unwrap());
                view.config.ui.size = font_size;
                cx.notify();
            });
            cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
            let body = cx.debug_bounds("settings-body").unwrap();
            let mut previous_top = body.top();
            // Rows that hold a dropdown or number field share their id with it,
            // so the titles are what stack.
            for title_id in [
                "notif-delivery-title",
                "notif-finished-title",
                "notif-show-title",
                "notif-delay-title",
                "notif-position-title",
                "notif-sound-title",
                "notif-volume-title",
            ] {
                let title = cx.debug_bounds(title_id).unwrap();
                assert!(
                    title.left() >= body.left() && title.right() <= body.right(),
                    "{title_id}"
                );
                assert!(
                    title.top() > previous_top,
                    "{title_id} overlaps the row above"
                );
                previous_top = title.top();
            }
            if width == 680. {
                // The delay row is local, so it keeps its description on
                // every platform. The shared delivery row shows it only where
                // Herdr's file is writable; elsewhere its reason replaces it.
                let mut wrapping = vec!["notif-delay-description"];
                if cfg!(unix) {
                    wrapping.push("notif-delivery-description");
                } else {
                    assert!(cx.debug_bounds("notif-delivery-description").is_none());
                    assert!(cx.debug_bounds("notif-delivery-reason").is_some());
                }
                for id in wrapping {
                    let note = cx.debug_bounds(id).unwrap_or_else(|| panic!("{id}"));
                    assert!(note.size.height > px(18.), "narrow {id} must wrap");
                }
            }
        }
    }
}

#[gpui::test]
fn show_agents_is_in_appearance_and_disabled_during_saves(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    cx.simulate_resize(size(px(960.), px(2200.)));
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("sidebar-show-agents-toggle").is_none());
    for show in [false, true] {
        view.update(cx, |view, cx| {
            view.section = Section::Sidebar;
            view.config.show_agents = show;
            view.saving = true;
            cx.notify();
        });
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        let button = cx.debug_bounds("sidebar-show-agents-toggle").unwrap();
        cx.simulate_click(button.center(), Default::default());
        view.update(cx, |view, cx| {
            assert_eq!(view.config.show_agents, show);
            assert!(view.save_completion.is_none());
            view.saving = false;
            view.accept_layout_choice(LayoutMode::Orca, cx);
            // Exercise reconciliation without touching personal configuration.
            view.save_with(
                || Ok(()),
                move || {
                    let mut loaded = skill_load()?;
                    loaded.config.show_agents = !show;
                    Ok(loaded)
                },
                false,
                cx,
            );
        });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert_eq!(view.config.show_agents, !show);
            assert_eq!(view.layout_intent, Some(LayoutMode::Orca));
            assert_eq!(view.config.layout.mode, LayoutMode::Orca);
            assert!(!view.busy());
        });
    }
}

#[gpui::test]
fn sidebar_rows_stack_inside_the_body_and_layout_picks_are_drafts(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    view.update(cx, |view, cx| {
        view.section = Section::Sidebar;
        cx.notify();
    });
    for width in [680., 960.] {
        cx.simulate_resize(size(px(width), px(2200.)));
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        let body = cx.debug_bounds("settings-body").unwrap();
        let mut previous_top = body.top();
        for title_id in [
            "sidebar-layout-title",
            "sidebar-select-title",
            "sidebar-show-agents-title",
            "sidebar-show-search-title",
            "sidebar-repository-icons-title",
            "sidebar-device-icons-title",
            "sidebar-indent-title",
            "sidebar-row-padding-title",
            "sidebar-row-gap-title",
            "sidebar-host-gap-title",
        ] {
            let title = cx.debug_bounds(title_id).unwrap();
            assert!(
                title.left() >= body.left() && title.right() <= body.right(),
                "{title_id}"
            );
            assert!(
                title.top() > previous_top,
                "{title_id} overlaps the row above"
            );
            previous_top = title.top();
        }
        for control in ["sidebar-select-control-2", "sidebar-host-gap-control-plus"] {
            let bounds = cx.debug_bounds(control).unwrap();
            assert!(bounds.right() <= body.right(), "{control} at {width}");
        }
    }
    view.update(cx, |view, cx| {
        view.saving = true;
        view.error = Some("Existing error".into());
        cx.notify();
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let opener = cx.debug_bounds("sidebar-layout-control").unwrap();
    cx.simulate_click(opener.center(), Default::default());
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    // Option 7 of `LayoutMode::ALL` is Orca.
    let choice = cx.debug_bounds("sidebar-layout-control-option-7").unwrap();
    cx.simulate_click(choice.center(), Default::default());
    view.update(cx, |view, _| {
        assert_eq!(view.layout_intent, Some(LayoutMode::Orca));
        assert!(view.save_completion.is_none());
        assert_eq!(view.error.as_deref(), Some("Existing error"));
        view.saving = false;
    });
}

#[gpui::test]
fn sidebar_scroll_reaches_host_colours(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    for width in [680., 960.] {
        cx.simulate_resize(size(px(width), px(560.)));
        view.update(cx, |view, cx| {
            view.section = Section::Sidebar;
            view.body_scroll.set_offset(Point::default());
            cx.notify();
        });
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        let heading = cx.debug_bounds("sidebar-host-colours").unwrap();
        let body = cx.debug_bounds("settings-body").unwrap();
        assert!(
            heading.bottom() > body.bottom(),
            "host colours need scrolling"
        );
        view.update(cx, |view, cx| {
            view.body_scroll
                .set_offset(point(px(0.), body.top() - heading.top()));
            cx.notify();
        });
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        let heading = cx.debug_bounds("sidebar-host-colours").unwrap();
        assert!(heading.top() >= body.top() && heading.bottom() <= body.bottom());
        view.read_with(cx, |view, _| {
            assert!(!view.busy());
            assert!(view.save_completion.is_none());
        });
    }
}

#[gpui::test]
fn sidebar_layout_draft_survives_other_save_failure(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    view.update(cx, |view, cx| {
        view.accept_layout_choice(LayoutMode::Orca, cx);
        view.save_with(|| Err(crate::Error::MissingHome), skill_load, false, cx);
        assert_eq!(view.layout_intent, Some(LayoutMode::Orca));
        assert!(view.busy());
        view.sync_controls(cx);
        assert_eq!(view.layout_intent, Some(LayoutMode::Orca));
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(!view.busy());
        assert!(view.error.is_some());
        assert_eq!(view.layout_intent, Some(LayoutMode::Orca));
        assert_eq!(view.config.layout.mode, LayoutMode::Orca);
    });
}

#[gpui::test]
fn browser_skill_general_card_tracks_choice_without_a_source(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(AgentSkill::unasked()));
    let (view, cx) = cx.add_window_view(skill_fixture);
    cx.simulate_resize(size(px(960.), px(1200.)));
    view.update(cx, |view, _| view.section = Section::General);
    for choice in [None, Some(Choice::Installed), Some(Choice::Declined)] {
        cx.update(|window, cx| {
            if let Some(choice) = choice {
                AgentSkill::choose(choice, cx);
            }
            view.update(cx, |view, cx| {
                assert!(view.source.upgrade().is_none());
                cx.notify();
            });
            window.draw(cx).clear(cx);
        });
        assert_eq!(
            cx.debug_bounds("general-browser-skill-install").is_some(),
            choice != Some(Choice::Installed)
        );
        assert_eq!(
            cx.debug_bounds("general-browser-skill-remove").is_some(),
            choice == Some(Choice::Installed)
        );
    }
    // Busy buttons must not invoke the real installer even when clicked.
    view.update(cx, |view, cx| {
        view.saving = true;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let button = cx.debug_bounds("general-browser-skill-install").unwrap();
    cx.simulate_click(button.center(), Modifiers::default());
    view.update(cx, |view, cx| {
        assert_eq!(AgentSkill::choice(cx), Some(Choice::Declined));
        assert!(view.save_completion.is_none());
        view.saving = false;
    });
}

#[gpui::test]
fn browser_skill_choice_changes_only_after_success_and_serializes(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(AgentSkill::unasked()));
    let (view, cx) = cx.add_window_view(skill_fixture);
    for (choice, succeeds, expected) in [
        (Choice::Installed, false, None),
        (Choice::Installed, true, Some(Choice::Installed)),
        (Choice::Declined, false, Some(Choice::Installed)),
        (Choice::Declined, true, Some(Choice::Declined)),
    ] {
        view.update(cx, |view, cx| {
            let before = AgentSkill::choice(cx);
            view.save_skill_with(
                choice,
                move || {
                    if succeeds {
                        Ok(())
                    } else {
                        Err(crate::Error::MissingHome)
                    }
                },
                skill_load,
                cx,
            );
            assert!(view.busy());
            assert_eq!(AgentSkill::choice(cx), before);
            view.save_skill_with(
                choice,
                || panic!("overlapping skill operation"),
                skill_load,
                cx,
            );
        });
        cx.run_until_parked();
        view.read_with(cx, |view, cx| {
            assert!(!view.busy());
            assert_eq!(AgentSkill::choice(cx), expected);
            assert_eq!(view.error.is_some(), !succeeds);
            assert_eq!(
                view.status.as_deref(),
                (!succeeds).then_some("Save failed; reloaded current preferences")
            );
        });
    }
    // A successful file operation remains successful if config reload fails.
    view.update(cx, |view, cx| {
        view.save_skill_with(
            Choice::Installed,
            || Ok(()),
            || Err(crate::Error::MissingHome),
            cx,
        );
    });
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        assert_eq!(AgentSkill::choice(cx), Some(Choice::Installed));
        assert!(view.error.is_some());
    });
}

#[test]
fn font_filter_keeps_all_four_hundred_families_and_matches_case_insensitively() {
    let names: Vec<_> = (0..400).map(|i| format!("Family {i:03}")).collect();
    assert_eq!(filter_fonts(&names, "").len(), 400);
    assert_eq!(
        filter_fonts(&names, " FAMILY 39 "),
        (390..400).collect::<Vec<_>>()
    );
    assert_eq!(filter_fonts(&names, "Family 399"), vec![399]);
    assert!(filter_fonts(&names, "missing").is_empty());
}

#[test]
fn rapid_size_intents_coalesce_without_erasing_other_faces() {
    let mut pending = Vec::new();
    queue_size(&mut pending, FontFace::Sidebar, 15.);
    queue_size(&mut pending, FontFace::Terminal, 18.);
    queue_size(&mut pending, FontFace::Sidebar, 16.);
    assert_eq!(
        pending,
        vec![(FontFace::Sidebar, 16.), (FontFace::Terminal, 18.)]
    );
}

#[gpui::test]
fn busy_size_clicks_survive_config_refresh_without_starting_a_write(cx: &mut TestAppContext) {
    let (source, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    let settings = cx.new(|cx| SettingsWindow::new(source.downgrade(), cx));
    settings.update(cx, |settings, cx| {
        settings.saving = true;
        settings.controls.saving_sizes = vec![(FontFace::Sidebar, 20.)];
        for size in 21..=40 {
            settings.accept_control_size(FontFace::Sidebar, size as f32, cx);
        }
        let terminal = settings.controls.size(FontFace::Terminal, &settings.config);
        settings.accept_control_size(FontFace::Terminal, terminal + 1., cx);
        settings.config.sidebar.size = 12.;
        settings.sync_controls(cx);
        assert_eq!(
            settings.controls.size(FontFace::Sidebar, &settings.config),
            40.
        );
        assert_eq!(settings.controls.pending_sizes.len(), 2);
        assert_eq!(
            settings.controls.saving_sizes,
            vec![(FontFace::Sidebar, 20.)]
        );
        // Keep the test entirely in memory: completion and persistence are
        // exercised by the root save tests, not the user's configuration.
        settings.controls.pending_sizes.clear();
        settings.saving = false;
        settings.sync_controls(cx);
        assert_eq!(
            settings.controls.size(FontFace::Sidebar, &settings.config),
            12.
        );
    });
}

#[test]
fn numeric_size_accepts_only_bounded_ascii_integers() {
    for (text, expected) in [
        ("8", Some(8.)),
        (" 48 ", Some(48.)),
        ("24", Some(24.)),
        ("7", None),
        ("49", None),
        ("12.5", None),
        ("-12", None),
        ("", None),
        ("２４", None),
        ("NaN", None),
        ("999999", None),
    ] {
        assert_eq!(
            crate::settings_window::ui::parse_number(text, &FONT_SIZE_RANGE),
            expected,
            "{text:?}"
        );
    }
}

fn numeric_fixture(window: &mut Window, cx: &mut Context<SettingsWindow>) -> SettingsWindow {
    let source = cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx));
    let mut view = SettingsWindow::new(source.downgrade(), cx);
    view.section = Section::Fonts;
    // Accepted intents remain in memory while assertions inspect the queue.
    view.saving = true;
    view.controls.discovering = false;
    window.focus(&view.focus, cx);
    view
}

#[gpui::test]
fn numeric_enter_blur_and_escape_use_the_same_size_queue(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(numeric_fixture);
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.begin_number_edit(NumberField::FontSize(FontFace::Terminal), 13., window, cx);
        })
    });
    cx.simulate_input("23");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert!(view.number_edit.is_none());
        assert_eq!(view.controls.pending_sizes, vec![(FontFace::Terminal, 23.)]);
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.begin_number_edit(NumberField::FontSize(FontFace::Terminal), 13., window, cx);
        })
    });
    cx.simulate_input("41");
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| {
        assert_eq!(view.controls.pending_sizes, vec![(FontFace::Terminal, 23.)])
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.begin_number_edit(NumberField::FontSize(FontFace::Sidebar), 12., window, cx);
        })
    });
    cx.simulate_input("18");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.focus(&view.read(cx).focus.clone(), cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert!(view.number_edit.is_none());
        assert_eq!(
            view.take_pending_control_sizes(),
            vec![(FontFace::Terminal, 23.), (FontFace::Sidebar, 18.)]
        );
    });
}

#[gpui::test]
fn numeric_invalid_and_composition_do_not_enqueue_writes(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(numeric_fixture);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.begin_number_edit(NumberField::FontSize(FontFace::Ui), 13., window, cx);
        })
    });
    cx.simulate_input("99");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert!(view.number_edit.as_ref().unwrap().invalid);
        assert!(view.controls.pending_sizes.is_empty());
    });
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.begin_number_edit(NumberField::FontSize(FontFace::Ui), 13., window, cx);
            let input = view.number_edit.as_ref().unwrap().input.clone();
            input.update(cx, |input, cx| {
                input.replace_and_mark_text_in_range(None, "二十四", Some(3..3), window, cx)
            });
            for key in ["enter", "escape"] {
                view.settings_key(
                    &KeyDownEvent {
                        keystroke: Keystroke::parse(key).unwrap(),
                        is_held: false,
                        prefer_character_input: false,
                    },
                    window,
                    cx,
                );
                assert!(view.number_edit.is_some());
                assert!(view.controls.pending_sizes.is_empty());
            }
            input.update(cx, |input, cx| input.unmark_text(window, cx));
            assert!(!view.finish_number_edit(true, cx));
            assert!(view.finish_number_edit(false, cx));
            assert!(view.controls.pending_sizes.is_empty());
        })
    });
}
