use super::*;
use crate::config::LayoutMode;

/// The long row's name after `$prefix`, as the `'static` selectors want it.
macro_rules! long {
    ($prefix:literal) => {
        concat!(
            $prefix,
            "sidebar-child-with-a-name-far-too-long-for-any-narrow-sidebar"
        )
    };
}

/// A minimal row keeps its pull request's number whole however long its name
/// is, the name ellipsizing before it, and still shows no uncommitted work.
#[gpui::test]
fn a_long_name_gives_way_to_the_pull_request_number(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        let mut snapshot = snapshot(6);
        snapshot.workspaces[4].label = long!("").into();
        // A nested row reads its branch unless the user named it.
        snapshot.workspaces[4].custom_label = true;
        view.live.snapshot = Some(Arc::new(snapshot));
        let input = crate::pull_request::Input {
            checkout: None,
            repo_key: Some(REPO_KEY.into()),
            branch: "worktree/sidebar-child".into(),
        };
        let now = std::time::Instant::now();
        let mut pr = crate::pull_request::fixture().unwrap();
        pr.number = 1234;
        view.seed_pr(input.clone(), pr, now);
        view.git.seed_probe(input, true, now);
        view.config.layout.mode = LayoutMode::Minimal;
        view.sidebar_width = Some(200.);
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));

    let row = cx.debug_bounds(long!("row-")).unwrap();
    let name = cx.debug_bounds(long!("name-")).unwrap();
    let number = cx.debug_bounds(long!("pr-")).unwrap();
    assert!(name.right() <= number.left(), "{name:?} {number:?}");
    assert!(number.right() <= row.right(), "{number:?} {row:?}");
    // "#1234" at its natural width, not squeezed into what the name left.
    let glyph = cx.update(|_, cx| crate::sidebar::glyph_width(&view.read(cx).config.sidebar));
    assert!(number.size.width >= px(glyph * 5.), "{number:?}");
    assert!(cx.debug_bounds(long!("dirty-")).is_none());
}
