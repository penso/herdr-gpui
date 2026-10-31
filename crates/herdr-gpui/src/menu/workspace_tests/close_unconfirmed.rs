use super::*;
use crate::menu::workspace_close::{CloseCheck, Report};

/// Opens the close dialog for `w3` with a check whose worker is gone, so the
/// next poll reports unverifiable status, as for a space outside a worktree.
fn open_close(
    view: &mut HerdrWindow,
    confirm: bool,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<HerdrWindow>,
) {
    view.config.confirm_close_workspace = confirm;
    view.live.status = crate::state::ConnectionStatus::Connected;
    let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
    snapshot.workspaces = sidebar::layout_tests::snapshot(7).workspaces;
    view.open_workspace_menu("w3", Default::default(), window, cx);
    view.menu.page = Some(crate::menu::Page::Dialog(WorkspaceAction::Close));
    let snapshot = view.live.snapshot.as_ref().unwrap();
    let target = view.menu.target.as_ref().unwrap();
    view.menu.close_check = Some(CloseCheck::fixture(snapshot, target, None));
}

#[test]
fn only_unpublished_work_holds_an_unconfirmed_close() {
    let snapshot = sidebar::layout_tests::snapshot(7);
    let target = WorkspaceTarget::new(&snapshot, &snapshot.workspaces[3]);
    let mut check = CloseCheck::fixture(&snapshot, &target, None);
    // Still checking: nothing may close yet, asked or not.
    assert!(!check.allows("", false));
    for (report, skips) in [
        (Report::default(), true),
        (
            Report {
                unknown: true,
                ..Report::default()
            },
            true,
        ),
        (
            Report {
                dirty: true,
                ..Report::default()
            },
            false,
        ),
        (
            Report {
                unpushed: true,
                ..Report::default()
            },
            false,
        ),
    ] {
        check.report = Some(report);
        assert_eq!(check.skippable(false), skips);
        assert_eq!(check.allows("", false), skips);
        assert!(!check.skippable(true));
        assert!(check.allows("close", false));
    }
}

#[gpui::test]
fn unconfirmed_close_goes_out_once_the_check_finishes(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            open_close(view, true, window, cx);
            view.update_workspace_dialog(window, cx);
            // Asking: unverifiable status waits for typed consent.
            assert!(view.menu.error.is_none());
            assert!(view.menu.input.is_some());
            view.dismiss_menu(window, cx);

            open_close(view, false, window, cx);
            view.update_workspace_dialog(window, cx);
            // Not asking: the close is submitted without consent, and only
            // the fixture's absent connection refuses it.
            assert_eq!(
                view.menu.error.as_deref(),
                Some(crate::Error::NotConnected.to_string().as_str())
            );
            assert!(view.menu.input.is_none());
        })
    });
}

#[gpui::test]
fn unconfirmed_close_still_asks_about_unpublished_work(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            open_close(view, false, window, cx);
            view.menu.close_check.as_mut().unwrap().report = Some(Report {
                dirty: true,
                ..Report::default()
            });
            view.menu.input = Some(DialogInput::new(String::new()));
            view.submit_workspace_dialog(window, cx);
            assert!(view.menu.error.is_none());
            assert!(view.menu.page.is_some());
        })
    });
}

#[test]
fn status_asks_for_typed_consent_only_when_the_close_needs_it() {
    let snapshot = sidebar::layout_tests::snapshot(7);
    let target = WorkspaceTarget::new(&snapshot, &snapshot.workspaces[3]);
    let mut check = CloseCheck::fixture(&snapshot, &target, None);
    assert!(check.status(false).starts_with("Checking"));
    let consent = "Type \u{2068}close\u{2069}";
    check.report = Some(Report {
        unknown: true,
        ..Report::default()
    });
    assert!(check.status(true).contains(consent));
    // A failed automatic close keeps the dialog without an input, so it must
    // not ask for typing there.
    assert!(!check.status(false).contains(consent));
    assert!(check.status(false).contains("could not be verified"));
    check.report = Some(Report {
        dirty: true,
        ..Report::default()
    });
    assert!(check.status(false).contains(consent));
    check.report = Some(Report::default());
    assert!(check.status(true).starts_with("No uncommitted files"));
}
