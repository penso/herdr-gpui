#![allow(clippy::unwrap_used)]
// Not a glob: the parent's `gpui::*` would shadow the `#[test]` that
// `gpui::test` expands to.
use super::Choice;
use crate::sidebar::layout_tests::fixture_window;

mod en_xa;

/// A new workspace runs the setup script by default; attaching an existing
/// one, which may be set up already, does not.
#[gpui::test]
fn the_setup_script_defaults_on_for_new_workspaces_only(cx: &mut gpui::TestAppContext) {
    if crate::cloud::unavailable().is_some() {
        return;
    }
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_coder_fixture(window, cx);
            let wizard = view.menu.coder.as_ref().unwrap();
            assert_eq!(wizard.choice, Some(Choice::Template("t1".into())));
            assert!(wizard.setup);
            let workspace = wizard.account.as_ref().unwrap().workspaces[0].clone();
            view.choose_coder_workspace(&workspace, cx);
            assert!(!view.menu.coder.as_ref().unwrap().setup);
        })
    });
}
