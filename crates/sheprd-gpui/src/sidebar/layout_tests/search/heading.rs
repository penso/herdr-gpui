use super::*;
use crate::{
    sidebar::layout_tests::REPO_KEY,
    worktree_notes::{Checkout, Notes},
};

fn placeholder(view: &Entity<SheprdWindow>, cx: &mut gpui::VisualTestContext) -> String {
    view.read_with(cx, |view, cx| {
        view.sidebar_search.input.read(cx).placeholder().to_owned()
    })
}

#[gpui::test]
fn the_heading_is_the_field_and_spans_a_rows_highlight(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(connected);
    cx.simulate_resize(size(px(800.), px(700.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let field = cx.debug_bounds("sidebar-search").unwrap();
    let highlight = cx.debug_bounds("highlight-herdr").unwrap();
    assert_eq!(field.left(), highlight.left());
    assert_eq!(field.right(), highlight.right());
    let heading = cx.debug_bounds("header-spaces").unwrap();
    let list = cx.debug_bounds("spaces-scroll").unwrap();
    assert_eq!(list.top(), heading.bottom(), "no line besides the heading");
    // At rest it reads as the heading.
    let label = placeholder(&view, cx);
    assert!(label.eq_ignore_ascii_case("spaces"), "{label}");
    assert!(cx.debug_bounds("sidebar-search-clear").is_none());

    cx.update(|window, cx| {
        let focus = view.read(cx).sidebar_search.input.read(cx).focus.clone();
        window.focus(&focus, cx);
        full_draw(window, cx).clear(cx);
    });
    assert_eq!(
        placeholder(&view, cx),
        "Devices, worktrees, branches, notes"
    );

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(view.read(cx).focus.is_focused(window));
        full_draw(window, cx).clear(cx);
    });
    assert_eq!(placeholder(&view, cx), label);
}

#[gpui::test]
fn a_query_finds_a_note_and_enter_opens_its_workspace(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(connected);
    cx.simulate_resize(size(px(800.), px(700.)));
    cx.update(|_, cx| {
        let endpoint = view.read(cx).endpoints[0].id.clone();
        Notes::update(cx, |notes| {
            notes.set(
                Checkout {
                    endpoint,
                    repo_key: REPO_KEY.into(),
                    branch: "worktree/sidebar-child".into(),
                },
                "waiting on the design review",
            )
        });
    });
    type_query(&view, "design review", cx);
    assert!(cx.debug_bounds("sidebar-search-hit-0").is_some());
    assert!(cx.debug_bounds("sidebar-search-hit-1").is_none());
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.pending_navigation,
            Some(crate::NavigationTarget::Workspace("w4".into()))
        );
    });
}

/// The sidebar is a cached view, so focus moving into or out of the empty
/// field has to invalidate it: an ordinary draw, without the forced refresh
/// `full_draw` does, must still swap the heading for the hint and back.
#[gpui::test]
fn focus_and_blur_redraw_the_cached_heading(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(connected);
    cx.simulate_resize(size(px(800.), px(700.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let label = placeholder(&view, cx);
    let renders = |cx: &mut gpui::VisualTestContext| {
        cx.update(|_, cx| view.read(cx).sidebar_view.read(cx).renders)
    };
    let before = renders(cx);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(renders(cx), before, "an idle draw reuses the cache");

    let field = cx.debug_bounds("sidebar-search").unwrap();
    cx.simulate_click(field.center(), Modifiers::none());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(renders(cx) > before, "focusing re-renders the sidebar");
    assert_eq!(
        placeholder(&view, cx),
        "Devices, worktrees, branches, notes"
    );

    let before = renders(cx);
    cx.update(|window, cx| {
        let focus = view.read(cx).focus.clone();
        window.focus(&focus, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(renders(cx) > before, "blurring re-renders the sidebar");
    assert_eq!(placeholder(&view, cx), label);
}

/// The clear button grows with the field's text, but the heading's padding
/// does not, so in the tighter densities the button stops at the field's
/// border: its hover fill never reaches past the field, whatever the layout
/// or `[sidebar] size`. With room to spare it keeps growing with the text.
#[gpui::test]
fn the_clear_button_stays_inside_the_field_at_large_sizes(cx: &mut gpui::TestAppContext) {
    for mode in crate::config::LayoutMode::ALL
        .into_iter()
        .filter(|mode| mode.lists_workspaces())
    {
        for font_size in [12., 24., 48.] {
            let (view, cx) = cx.add_window_view(|window, cx| {
                let mut view = connected(window, cx);
                view.config.layout.mode = mode;
                view.config.sidebar.size = font_size;
                view
            });
            cx.simulate_resize(size(px(800.), px(700.)));
            type_query(&view, "sidebar", cx);
            let field = cx.debug_bounds("sidebar-search").unwrap();
            let clear = cx.debug_bounds("sidebar-search-clear").unwrap();
            let inside = field.inset(px(1.));
            assert!(
                clear.top() >= inside.top() && clear.bottom() <= inside.bottom(),
                "{mode} at {font_size}: {clear:?} outside {inside:?}"
            );
            assert!(clear.right() <= inside.right(), "{mode} at {font_size}");
            let font = cx.update(|_, cx| view.read(cx).config.sidebar.clone());
            let grown = px(crate::sidebar::scaled(&font, 20.));
            let room = inside.size.height;
            let expected = if grown < room { grown } else { room };
            assert_eq!(clear.size.height, expected, "{mode} at {font_size}");
        }
    }
}
