//! Picking the branch a branch review is compared with from its header.
use super::{State, changes, the, window};
use crate::review::{
    diff::{BaseChoice, Scope},
    view::base_menu::BaseList,
};

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

fn choice(reference: &str, behind: u64) -> BaseChoice {
    BaseChoice {
        reference: reference.into(),
        behind: Some(behind),
    }
}

#[gpui::test]
fn the_header_s_base_lists_branches_and_a_pick_reloads(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, None);
    let mut branch = changes();
    branch.scope = Scope::Branch;
    branch.base = Some("origin/main".into());
    cx.update(|window, cx| view.update(cx, |view, cx| view.seed_review(branch, window, cx)));
    draw(cx);
    assert!(cx.debug_bounds("review-base-menu").is_none());
    let base = cx.debug_bounds("review-base").unwrap();
    cx.simulate_click(base.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("review-base-menu").is_some());
    // The fixture's checkout is not a repository, so its branches cannot
    // be read; the list says so and still offers the automatic base.
    view.read_with(cx, |view, _| {
        let menu = view.reviews.values().next().unwrap().base_menu.as_ref();
        assert!(matches!(menu.unwrap().list, BaseList::Failed(_)));
    });
    assert!(cx.debug_bounds("review-base-option-0").is_some());

    // Listed, the closest branch first; the keyboard picks it.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let id = the(view);
            let review = view.reviews.get_mut(&id).unwrap();
            review.base_menu.as_mut().unwrap().list = BaseList::Loading;
            view.review_bases_listed(
                id,
                Ok(vec![
                    choice("refs/heads/rc", 1),
                    choice("refs/remotes/origin/main", 2),
                ]),
            );
        })
    });
    draw(cx);
    assert!(cx.debug_bounds("review-base-option-2").is_some());
    let request = view.read_with(cx, |view, _| view.reviews[&the(view)].request);
    cx.simulate_keystrokes("down enter");
    view.read_with(cx, |view, _| {
        let review = &view.reviews[&the(view)];
        assert_eq!(review.base.as_deref(), Some("refs/heads/rc"));
        assert!(review.base_menu.is_none());
        assert!(review.request > request, "the review is read again");
        assert_eq!(review.base_label(), "rc");
    });

    // Picking it again reads nothing; Automatic drops the pick.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let id = the(view);
            let request = view.reviews[&id].request;
            view.set_review_base(id, Some("refs/heads/rc".into()), window, cx);
            assert_eq!(view.reviews[&id].request, request);
            view.set_review_base(id, None, window, cx);
            let review = &view.reviews[&id];
            assert_eq!(review.base, None);
            assert!(review.request > request);
            assert!(matches!(review.state, State::Loading));
        })
    });
}

#[gpui::test]
fn a_reopened_list_selects_the_current_pick(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, None);
    let mut branch = changes();
    branch.scope = Scope::Branch;
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let id = view.seed_review(branch, window, cx);
            view.reviews.get_mut(&id).unwrap().base = Some("refs/remotes/origin/main".into());
            view.open_review_bases(id, window, cx);
            view.review_bases_listed(
                id,
                Ok(vec![
                    choice("refs/heads/rc", 1),
                    choice("refs/remotes/origin/main", 2),
                ]),
            );
            let menu = view.reviews[&id].base_menu.as_ref().unwrap();
            assert_eq!(menu.selected, 2);
            view.close_review_bases(id, window, cx);
            assert!(view.reviews[&id].base_menu.is_none());
        })
    });
}

#[gpui::test]
fn only_a_branch_review_names_a_base(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, None);
    cx.update(|window, cx| view.update(cx, |view, cx| view.seed_review(changes(), window, cx)));
    draw(cx);
    assert!(cx.debug_bounds("review-against").is_some());
    assert!(cx.debug_bounds("review-base").is_none());
}

#[gpui::test]
fn a_long_base_truncates_inside_the_header(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, None);
    cx.simulate_resize(gpui::size(gpui::px(1300.), gpui::px(700.)));
    let mut branch = changes();
    branch.scope = Scope::Branch;
    branch.base = Some(format!(
        "origin/{}",
        "a-very-long-release-branch-".repeat(8)
    ));
    cx.update(|window, cx| view.update(cx, |view, cx| view.seed_review(branch, window, cx)));
    draw(cx);
    let against = cx.debug_bounds("review-against").unwrap();
    let base = cx.debug_bounds("review-base").unwrap();
    assert!(base.size.width <= gpui::px(240.), "{base:?}");
    assert!(base.right() <= against.right(), "{base:?} in {against:?}");
    // The controls after it keep their place.
    let search = cx.debug_bounds("review-search-open").unwrap();
    assert!(base.right() <= search.left(), "{base:?} before {search:?}");
}
