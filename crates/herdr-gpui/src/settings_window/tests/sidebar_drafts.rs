//! The Sidebar page's live drafts: previews reach every main window at once,
//! commits write through one serial save, and close waits for that save.
use super::*;
use crate::config::preferences::Preference;
use crate::config::{SelectMode, SidebarOverrides, SidebarStyle};

type Edits = Arc<Mutex<Vec<Preference>>>;

fn settings_with_source(
    cx: &mut TestAppContext,
) -> (
    WindowHandle<HerdrWindow>,
    Entity<SettingsWindow>,
    &mut VisualTestContext,
    Edits,
) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let edits = Arc::new(Mutex::new(Vec::new()));
    let captured = edits.clone();
    let weak = cx.update(|cx| source.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let (view, cx) = cx.add_window_view(move |window, cx| {
        let mut view = SettingsWindow::new(weak, cx);
        view.controls.preference_io = Some(controls::preferences::PreferenceIo::recording(
            move |edit| captured.lock().unwrap().push(edit),
        ));
        view.initialize_controls(cx);
        view.section = Section::Sidebar;
        window.focus(&view.focus, cx);
        view
    });
    (source, view, cx, edits)
}

fn intent(overrides: SidebarOverrides, select: SelectMode) -> SidebarIntent {
    SidebarIntent {
        style: SidebarStyle {
            // As the layout fixture has it, so only the keys a test sets differ.
            activity_bar: false,
            overrides,
            select,
            ..Default::default()
        },
        touched: Default::default(),
    }
}

#[gpui::test]
fn slider_preview_reaches_windows_before_any_write_and_release_writes_once(
    cx: &mut TestAppContext,
) {
    let (source, view, cx, edits) = settings_with_source(cx);
    cx.simulate_resize(size(px(960.), px(1400.)));
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let slider = cx.debug_bounds("sidebar-indent-control-slider").unwrap();
    let start = point(slider.left() + px(1.), slider.center().y);
    let end = point(slider.right() - px(1.), slider.center().y);
    cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
    // The first move past the threshold only starts the drag; the next one moves it.
    cx.simulate_mouse_move(
        point(start.x + px(20.), start.y),
        MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(end, MouseButton::Left, Default::default());
    cx.run_until_parked();
    let indent = cx.update(|_, cx| {
        source
            .read(cx)
            .unwrap()
            .config
            .sidebar_style
            .overrides
            .indent
    });
    assert_eq!(indent, Some(48.));
    assert!(edits.lock().unwrap().is_empty(), "no write while dragging");
    assert!(view.read_with(cx, |view, _| view.sidebar_dirty()));
    cx.simulate_mouse_up(end, MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert_eq!(
        *edits.lock().unwrap(),
        vec![Preference::SidebarIndent(Some(48.))]
    );
    assert!(!view.read_with(cx, |view, _| view.sidebar_dirty()));
}

#[gpui::test]
fn newer_draft_wins_over_a_reload_started_earlier(cx: &mut TestAppContext) {
    let (_, view, cx, _) = settings_with_source(cx);
    let revision = cx.update(|_, cx| sidebar_load_revision(cx));
    view.update(cx, |view, cx| {
        view.preview_sidebar(
            intent(
                SidebarOverrides {
                    indent: Some(20.),
                    ..Default::default()
                },
                SelectMode::Group,
            ),
            cx,
        )
    });
    let mut stale = Config::default();
    cx.update(|_, cx| apply_loaded_sidebar(&mut stale, revision, cx));
    assert_eq!(stale.sidebar_style.overrides.indent, Some(20.));
    assert_eq!(stale.sidebar_style.select, SelectMode::Group);
}

#[gpui::test]
fn closing_with_a_live_draft_saves_first_and_a_failed_save_keeps_the_window(
    cx: &mut TestAppContext,
) {
    let (_, view, cx, edits) = settings_with_source(cx);
    let gap = SidebarOverrides {
        gap: Some(9.),
        ..Default::default()
    };
    view.update(cx, |view, cx| {
        view.controls.preference_io = Some(controls::preferences::PreferenceIo::failing());
        view.preview_sidebar(intent(gap, SelectMode::Row), cx);
    });
    let closed = cx.update(|window, cx| view.update(cx, |view, cx| view.should_close(window, cx)));
    assert!(!closed, "close waits for the save");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.error.is_some(), "footer shows the error");
        assert!(view.closing.is_none(), "window stays open");
        assert!(view.sidebar_dirty(), "draft kept for retry");
        assert_eq!(view.config.sidebar_style.overrides.gap, Some(9.));
    });
    let window = cx.windows().len();
    let captured = edits.clone();
    view.update(cx, |view, _| {
        view.controls.preference_io = Some(controls::preferences::PreferenceIo::recording(
            move |edit| captured.lock().unwrap().push(edit),
        ));
    });
    let closed = cx.update(|window, cx| view.update(cx, |view, cx| view.should_close(window, cx)));
    assert!(!closed, "close waits for the retried save");
    cx.run_until_parked();
    assert_eq!(
        *edits.lock().unwrap(),
        vec![Preference::SidebarRowGap(Some(9.))]
    );
    assert_eq!(
        cx.windows().len(),
        window - 1,
        "a saved draft lets close finish"
    );
}

#[gpui::test]
fn one_commit_writes_every_changed_key_and_removed_hosts(cx: &mut TestAppContext) {
    let (_, view, cx, edits) = settings_with_source(cx);
    view.update(cx, |view, cx| {
        for config in [&mut view.config, &mut view.loaded_config] {
            config.sidebar_style.hosts.insert("Gone".into(), 0x445566);
        }
        let mut next = intent(
            SidebarOverrides {
                host_gap: Some(4.),
                ..Default::default()
            },
            SelectMode::GroupDim,
        );
        next.style.hosts.insert("New".into(), 0x112233);
        view.commit_sidebar(next, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        *edits.lock().unwrap(),
        vec![
            Preference::SidebarHostGap(Some(4.)),
            Preference::SidebarSelect(SelectMode::GroupDim),
            Preference::HostColor {
                host: "Gone".into(),
                color: None
            },
            Preference::HostColor {
                host: "New".into(),
                color: Some(0x112233)
            },
        ]
    );
}

#[gpui::test]
fn host_rows_merge_configured_and_connected_hosts(cx: &mut TestAppContext) {
    let (source, view, cx, _) = settings_with_source(cx);
    let label = cx.update(|_, cx| {
        source
            .update(cx, |window, _, _| {
                let endpoint = &mut window.endpoints[0];
                endpoint.live.status = crate::state::ConnectionStatus::Connected;
                endpoint.label.clone()
            })
            .unwrap()
    });
    assert_eq!(
        cx.update(|_, cx| source.read(cx).unwrap().endpoints[0].id.clone()),
        crate::endpoint::LOCAL
    );
    view.update(cx, |view, _| {
        view.config
            .sidebar_style
            .hosts
            .insert("Archive".into(), 0x112233);
    });
    let rows = view.update(cx, |view, cx| view.host_rows(cx));
    assert_eq!(
        rows,
        vec![
            (label, None, "settings-sidebar-host-uncoloured"),
            (
                "Archive".into(),
                Some(0x112233),
                "settings-sidebar-host-offline"
            ),
        ]
    );
    for (.., hint) in rows {
        assert!(crate::i18n::takes(hint).is_some(), "{hint}");
    }
}

#[gpui::test]
fn host_colours_say_how_to_get_a_row_when_there_is_none(cx: &mut TestAppContext) {
    let (_, view, cx, _) = settings_with_source(cx);
    cx.simulate_resize(size(px(960.), px(1400.)));
    // The fixture's endpoint is not connected and no colour is configured.
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let heading = cx.debug_bounds("sidebar-host-colours").unwrap();
    let empty = cx.debug_bounds("sidebar-host-colours-empty").unwrap();
    assert!(empty.top() >= heading.bottom());
    assert!(cx.debug_bounds("host-colour-0").is_none());
    view.update(cx, |view, cx| {
        view.config
            .sidebar_style
            .hosts
            .insert("Archive".into(), 0x112233);
        cx.notify();
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("sidebar-host-colours-empty").is_none());
    assert!(cx.debug_bounds("host-colour-0").is_some());
}

#[gpui::test]
fn reset_glyph_shows_only_for_an_override_and_commits_none(cx: &mut TestAppContext) {
    let (_, view, cx, edits) = settings_with_source(cx);
    cx.simulate_resize(size(px(960.), px(1400.)));
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("sidebar-indent-reset").is_none());
    view.update(cx, |view, cx| {
        view.config.sidebar_style.overrides.indent = Some(30.);
        view.loaded_config.sidebar_style.overrides.indent = Some(30.);
        cx.notify();
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let reset = cx.debug_bounds("sidebar-indent-reset").unwrap();
    cx.simulate_click(reset.center(), Default::default());
    cx.run_until_parked();
    assert_eq!(
        *edits.lock().unwrap(),
        vec![Preference::SidebarIndent(None)]
    );
}

#[gpui::test]
fn typed_hex_commits_on_enter_and_invalid_text_writes_nothing(cx: &mut TestAppContext) {
    let (_, view, cx, edits) = settings_with_source(cx);
    view.update(cx, |view, _| {
        view.config
            .sidebar_style
            .hosts
            .insert("Archive".into(), 0x112233);
        view.loaded_config.sidebar_style.hosts = view.config.sidebar_style.hosts.clone();
    });
    // The fixture's endpoint is not connected and has no colour, so it has
    // no row; Archive, configured only, is row 0.
    cx.update(|window, cx| view.update(cx, |view, cx| view.begin_hex_edit("Archive", window, cx)));
    let input = view.read_with(cx, |view, _| view.hex_edit.as_ref().unwrap().input.clone());
    assert_eq!(
        cx.update(|_, cx| input.read(cx).text().to_owned()),
        "112233"
    );
    cx.update(|_, cx| input.update(cx, |input, cx| input.set_text_selected("zz", cx)));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.hex_edit.as_ref().unwrap().invalid, "field marked");
        assert!(!view.sidebar_dirty());
    });
    assert!(edits.lock().unwrap().is_empty());
    cx.update(|_, cx| input.update(cx, |input, cx| input.set_text_selected("#ABC", cx)));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.hex_edit.is_none()));
    assert_eq!(
        *edits.lock().unwrap(),
        vec![Preference::HostColor {
            host: "Archive".into(),
            color: Some(0xaabbcc)
        }]
    );
}

/// A writer whose reload reflects what it wrote, so a follow-up save diffs
/// against the first one's result.
fn disk_io(edits: Edits) -> controls::preferences::PreferenceIo {
    let mut start = Config::default();
    // As the layout fixture has it, so a draft's untouched bar is not a change.
    start.sidebar_style.activity_bar = false;
    disk_io_from(edits, start)
}

fn disk_io_from(edits: Edits, start: Config) -> controls::preferences::PreferenceIo {
    let disk = Arc::new(Mutex::new(start));
    let written = disk.clone();
    controls::preferences::PreferenceIo::with_load(
        move |edit| {
            let mut config = written.lock().unwrap();
            let overrides = &mut config.sidebar_style.overrides;
            match &edit {
                Preference::SidebarIndent(value) => overrides.indent = *value,
                Preference::SidebarRowPadding(value) => overrides.row_padding = *value,
                Preference::SidebarRowGap(value) => overrides.gap = *value,
                Preference::SidebarHostGap(value) => overrides.host_gap = *value,
                other => panic!("unexpected write {other:?}"),
            }
            edits.lock().unwrap().push(edit);
        },
        move || {
            let mut loaded = fixture();
            loaded.config = disk.lock().unwrap().clone();
            Ok(loaded)
        },
    )
}

fn spacing(indent: Option<f32>, row_padding: Option<f32>) -> SidebarIntent {
    intent(
        SidebarOverrides {
            indent,
            row_padding,
            ..Default::default()
        },
        SelectMode::Row,
    )
}

#[gpui::test]
fn a_commit_during_a_write_follows_with_only_the_newer_change(cx: &mut TestAppContext) {
    let (_, view, cx, edits) = settings_with_source(cx);
    view.update(cx, |view, cx| {
        view.controls.preference_io = Some(disk_io(edits.clone()));
        view.commit_sidebar(spacing(Some(10.), None), cx);
        assert!(view.busy(), "first write in flight");
        view.commit_sidebar(spacing(Some(10.), Some(2.)), cx);
    });
    cx.run_until_parked();
    assert_eq!(
        *edits.lock().unwrap(),
        vec![
            Preference::SidebarIndent(Some(10.)),
            Preference::SidebarRowPadding(Some(2.)),
        ]
    );
    view.read_with(cx, |view, _| {
        assert!(!view.sidebar_dirty());
        assert!(!view.busy());
        assert_eq!(view.config.sidebar_style.overrides.row_padding, Some(2.));
    });
}

#[gpui::test]
fn a_preview_during_a_write_is_not_written_until_release(cx: &mut TestAppContext) {
    let (_, view, cx, edits) = settings_with_source(cx);
    view.update(cx, |view, cx| {
        view.controls.preference_io = Some(disk_io(edits.clone()));
        view.commit_sidebar(spacing(Some(10.), None), cx);
        assert!(view.busy(), "first write in flight");
        view.preview_sidebar(spacing(Some(10.), Some(2.)), cx);
    });
    cx.run_until_parked();
    assert_eq!(
        *edits.lock().unwrap(),
        vec![Preference::SidebarIndent(Some(10.))]
    );
    view.read_with(cx, |view, _| {
        assert!(view.sidebar_dirty(), "the drag is still live");
        assert_eq!(view.config.sidebar_style.overrides.row_padding, Some(2.));
    });
    view.update(cx, |view, cx| {
        view.commit_sidebar(spacing(Some(10.), Some(3.)), cx)
    });
    cx.run_until_parked();
    assert_eq!(
        *edits.lock().unwrap(),
        vec![
            Preference::SidebarIndent(Some(10.)),
            Preference::SidebarRowPadding(Some(3.)),
        ]
    );
    assert!(!view.read_with(cx, |view, _| view.sidebar_dirty()));
}

#[gpui::test]
fn blur_keeps_an_invalid_hex_open_and_escape_discards_it(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    // Focus events reach only the active window, from a focused root.
    cx.update(|window, cx| {
        window.activate_window();
        window.focus(&view.read(cx).focus.clone(), cx);
    });
    cx.run_until_parked();
    let edits: Edits = Arc::new(Mutex::new(Vec::new()));
    let captured = edits.clone();
    view.update(cx, |view, _| {
        view.controls.preference_io = Some(controls::preferences::PreferenceIo::recording(
            move |edit| captured.lock().unwrap().push(edit),
        ));
        view.section = Section::Sidebar;
        view.config
            .sidebar_style
            .hosts
            .insert("Archive".into(), 0x112233);
        view.loaded_config.sidebar_style.hosts = view.config.sidebar_style.hosts.clone();
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.begin_hex_edit("Archive", window, cx)));
    let input = view.read_with(cx, |view, _| view.hex_edit.as_ref().unwrap().input.clone());
    cx.simulate_input("nope");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.focus(&view.read(cx).focus.clone(), cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.hex_edit.is_some(), "blur keeps the field");
        let edit = view.hex_edit.as_ref().unwrap();
        assert!(edit.invalid, "blur marks it like Enter");
    });
    // With the hex field still open, Escape in a number field closes that
    // number field, not the hex one.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let field = inputs::NumberField::Spacing(pages::Spacing::Indent);
            view.begin_number_edit(field, 10., window, cx)
        })
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(
            view.number_edit.is_none(),
            "escape closes the focused field"
        );
        assert!(view.hex_edit.is_some(), "the unfocused hex field stays");
    });
    cx.update(|window, cx| window.focus(&input.read(cx).focus.clone(), cx));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.hex_edit.is_none()));
    assert!(edits.lock().unwrap().is_empty());
}

#[gpui::test]
fn a_reload_during_a_drag_keeps_an_external_edit_to_an_untouched_key(cx: &mut TestAppContext) {
    let (_, view, cx, edits) = settings_with_source(cx);
    let mut edited = Config::default();
    edited.sidebar_style.overrides.gap = Some(8.);
    view.update(cx, |view, _| {
        view.controls.preference_io = Some(disk_io_from(edits.clone(), edited.clone()));
    });
    cx.simulate_resize(size(px(960.), px(1400.)));
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let slider = cx.debug_bounds("sidebar-indent-control-slider").unwrap();
    let start = point(slider.left() + px(1.), slider.center().y);
    let end = point(slider.right() - px(1.), slider.center().y);
    cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
        point(start.x + px(20.), start.y),
        MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(end, MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.sidebar_dirty()));
    // Someone edits `gap` in the file while Indent is still held.
    view.update(cx, |view, cx| {
        let mut loaded = fixture();
        loaded.config = edited;
        view.apply_loaded(Ok(loaded), cx);
        assert_eq!(view.config.sidebar_style.overrides.gap, Some(8.));
        assert_eq!(view.config.sidebar_style.overrides.indent, Some(48.));
    });
    cx.simulate_mouse_up(end, MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert_eq!(
        *edits.lock().unwrap(),
        vec![Preference::SidebarIndent(Some(48.))]
    );
    view.read_with(cx, |view, _| {
        assert!(!view.sidebar_dirty());
        assert_eq!(view.config.sidebar_style.overrides.gap, Some(8.));
        assert_eq!(view.config.sidebar_style.overrides.indent, Some(48.));
    });
}

#[gpui::test]
fn a_draft_fenced_main_window_load_takes_only_touched_keys(cx: &mut TestAppContext) {
    let (_, view, cx, _) = settings_with_source(cx);
    let revision = cx.update(|_, cx| sidebar_load_revision(cx));
    view.update(cx, |view, cx| {
        let mut next = SidebarIntent::of(&view.config);
        next.style.overrides.indent = Some(20.);
        view.preview_sidebar(next, cx)
    });
    let mut read = Config::default();
    read.sidebar_style.overrides.gap = Some(8.);
    cx.update(|_, cx| apply_loaded_sidebar(&mut read, revision, cx));
    assert_eq!(read.sidebar_style.overrides.indent, Some(20.));
    assert_eq!(
        read.sidebar_style.overrides.gap,
        Some(8.),
        "untouched key kept"
    );
}

#[gpui::test]
fn a_hex_edit_lands_on_its_host_when_rows_reorder(cx: &mut TestAppContext) {
    let (_, view, cx, edits) = settings_with_source(cx);
    view.update(cx, |view, _| {
        for config in [&mut view.config, &mut view.loaded_config] {
            config.sidebar_style.hosts.insert("Work".into(), 0x112233);
        }
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.begin_hex_edit("Work", window, cx)));
    let input = view.read_with(cx, |view, _| view.hex_edit.as_ref().unwrap().input.clone());
    // A host that sorts first arrives, so Work is no longer row 0.
    view.update(cx, |view, cx| {
        for config in [&mut view.config, &mut view.loaded_config] {
            config.sidebar_style.hosts.insert("Alpha".into(), 0x445566);
        }
        let rows = view.host_rows(cx);
        assert_eq!(rows[0].0, "Alpha");
        assert_eq!(rows[1].0, "Work");
        cx.notify();
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    cx.update(|_, cx| input.update(cx, |input, cx| input.set_text_selected("#abcdef", cx)));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.hex_edit.is_none()));
    assert_eq!(
        *edits.lock().unwrap(),
        vec![Preference::HostColor {
            host: "Work".into(),
            color: Some(0xabcdef)
        }]
    );
}
