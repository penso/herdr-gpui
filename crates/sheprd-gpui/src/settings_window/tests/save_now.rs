//! The footer's "Save now" commits every pending draft in one call.
use super::*;
use crate::config::LayoutMode;

struct Fixture {
    view: Entity<SettingsWindow>,
    themes: Arc<Mutex<Vec<String>>>,
    layouts: Arc<Mutex<Vec<LayoutMode>>>,
}

fn fixture_view(cx: &mut TestAppContext) -> (Fixture, &mut VisualTestContext) {
    let main = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| main.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let themes = Arc::new(Mutex::new(Vec::new()));
    let layouts = Arc::new(Mutex::new(Vec::new()));
    let (view, vcx) = cx.add_window_view({
        let (themes, layouts) = (themes.clone(), layouts.clone());
        move |window, cx| {
            let mut view = SettingsWindow::new(weak, cx);
            view.theme_io = Some(recording_themes(themes, false));
            view.layout_io = Some(recording_layouts(layouts, false));
            window.focus(&view.focus, cx);
            view
        }
    });
    vcx.simulate_resize(size(px(960.), px(780.)));
    (
        Fixture {
            view,
            themes,
            layouts,
        },
        vcx,
    )
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

fn assert_saved(fixture: &Fixture, cx: &mut VisualTestContext) {
    fixture.view.read_with(cx, |view, _| {
        assert!(!view.theme_dirty());
        assert!(view.layout_intent.is_none());
        assert!(!view.flushing);
        assert!(!view.busy());
        assert_eq!(view.footer_text(), "All changes saved.");
    });
    draw(cx);
    assert!(cx.debug_bounds("settings-footer-status").is_some());
    assert!(cx.debug_bounds("settings-footer-save").is_none());
}

#[gpui::test]
fn save_now_commits_a_theme_only_draft(cx: &mut TestAppContext) {
    let (fixture, cx) = fixture_view(cx);
    fixture.view.update(cx, |view, cx| choose(view, "Nord", cx));
    fixture
        .view
        .update(cx, |view, cx| view.save_pending_drafts(cx));
    cx.run_until_parked();
    assert_eq!(*fixture.themes.lock().unwrap(), ["Nord"]);
    assert!(fixture.layouts.lock().unwrap().is_empty());
    assert_saved(&fixture, cx);
}

#[gpui::test]
fn save_now_commits_a_layout_only_draft(cx: &mut TestAppContext) {
    let (fixture, cx) = fixture_view(cx);
    fixture.view.update(cx, |view, cx| {
        view.accept_layout_choice(LayoutMode::Orca, cx)
    });
    fixture
        .view
        .update(cx, |view, cx| view.save_pending_drafts(cx));
    cx.run_until_parked();
    assert_eq!(*fixture.layouts.lock().unwrap(), [LayoutMode::Orca]);
    assert!(fixture.themes.lock().unwrap().is_empty());
    assert_saved(&fixture, cx);
}

#[gpui::test]
fn save_now_commits_theme_then_layout_in_one_call(cx: &mut TestAppContext) {
    let (fixture, cx) = fixture_view(cx);
    fixture.view.update(cx, |view, cx| {
        choose(view, "Nord", cx);
        view.accept_layout_choice(LayoutMode::Orca, cx);
    });
    fixture
        .view
        .update(cx, |view, cx| view.save_pending_drafts(cx));
    cx.run_until_parked();
    assert_eq!(*fixture.themes.lock().unwrap(), ["Nord"]);
    assert_eq!(*fixture.layouts.lock().unwrap(), [LayoutMode::Orca]);
    assert_saved(&fixture, cx);
}

#[gpui::test]
fn footer_shows_pending_drafts_until_they_are_saved(cx: &mut TestAppContext) {
    let (fixture, cx) = fixture_view(cx);
    fixture.view.update(cx, |view, cx| {
        view.accept_layout_choice(LayoutMode::Orca, cx)
    });
    draw(cx);
    fixture.view.read_with(cx, |view, _| {
        assert_eq!(
            view.footer_text(),
            // The window isolates the list, as every placeable.
            "Unsaved: \u{2068}layout\u{2069} \u{2014} saved when this window closes"
        );
    });
    let save = cx.debug_bounds("settings-footer-save").unwrap();
    cx.simulate_click(save.center(), Default::default());
    cx.run_until_parked();
    assert_eq!(*fixture.layouts.lock().unwrap(), [LayoutMode::Orca]);
    assert_saved(&fixture, cx);
}

#[gpui::test]
fn a_failed_theme_load_ends_the_flush_and_later_choices_stay_drafts(cx: &mut TestAppContext) {
    let (fixture, cx) = fixture_view(cx);
    fixture.view.update(cx, |view, _| {
        let mut io = recording_themes(fixture.themes.clone(), false);
        io.resolve = Some(Arc::new(|_| Err(crate::Error::MissingHome)));
        view.theme_io = Some(io);
    });
    fixture.view.update(cx, |view, cx| {
        choose(view, "missing-definition", cx);
        view.accept_layout_choice(LayoutMode::Orca, cx);
    });
    cx.run_until_parked();
    fixture
        .view
        .update(cx, |view, cx| view.save_pending_drafts(cx));
    cx.run_until_parked();
    fixture.view.read_with(cx, |view, _| {
        assert!(!view.flushing);
        assert!(view.error.is_some());
        assert_eq!(view.footer_text(), view.error.clone().unwrap());
    });
    draw(cx);
    assert!(cx.debug_bounds("settings-footer-status").is_some());
    assert!(fixture.themes.lock().unwrap().is_empty());
    fixture
        .view
        .update(cx, |view, cx| choose(view, "other-missing", cx));
    cx.run_until_parked();
    assert!(fixture.themes.lock().unwrap().is_empty());
    assert!(fixture.layouts.lock().unwrap().is_empty());
    fixture
        .view
        .read_with(cx, |view, _| assert!(!view.flushing));
}
