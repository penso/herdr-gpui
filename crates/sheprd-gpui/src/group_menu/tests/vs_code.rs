use super::*;
use crate::browser::WebUrl;

fn vs_code(actions: &[Action]) -> Option<&Action> {
    actions
        .iter()
        .find(|action| matches!(action, Action::VsCode))
}

#[gpui::test]
fn the_menu_opens_vs_code_in_its_group_once_there_is_a_server(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    let group = groups(&view, cx)[0];
    assert!(
        vs_code(&actions(&view, cx, group)).is_none(),
        "no server set"
    );
    view.update(cx, |view, _| {
        view.config.code.url = Some(WebUrl::try_from("http://127.0.0.1:8000/").unwrap());
    });
    let listed = actions(&view, cx, group);
    if !crate::browser::EMBEDDED {
        assert!(vs_code(&listed).is_none(), "this build shows no pages");
        return;
    }
    // Among the tabs to open, after a new browser tab.
    assert_eq!(listed[0], Action::NewBrowserTab);
    let row = vs_code(&listed).unwrap().clone();
    assert_eq!(row.label(), "VS Code");
    assert_eq!(row.icon(), Some("icons/vscode.svg"));
    assert_eq!(row.section(), Action::NewBrowserTab.section());

    run(&view, cx, group, row);
    let id = cx.update(|_, cx| view.read(cx).code_tab_id(cx)).unwrap();
    assert!(tabs(&view, cx, group).contains(&Pick::Page(id)));
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).group_shown(group, cx),
            crate::browser::Shown::Page(id)
        );
    });
}

#[gpui::test]
fn the_title_bar_has_no_vs_code_button(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    view.update(cx, |view, _| {
        view.config.code.url = Some(WebUrl::try_from("http://127.0.0.1:8000/").unwrap());
    });
    draw(cx);
    assert!(cx.debug_bounds("toggle-code").is_none());
}

/// A split with a VS Code server set, and the menu row that opens VS Code.
/// `None` in a build that shows no pages.
fn split_with_vs_code(
    cx: &mut TestAppContext,
) -> Option<(
    Entity<SheprdWindow>,
    &mut VisualTestContext,
    GroupId,
    GroupId,
)> {
    if !crate::browser::EMBEDDED {
        return None;
    }
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.code.url = Some(WebUrl::try_from("http://127.0.0.1:8000/").unwrap());
            view.command(Command::SplitEditor, window, cx);
        })
    });
    let [left, right] = groups(&view, cx)[..] else {
        panic!("two groups")
    };
    Some((view, cx, left, right))
}

#[gpui::test]
fn asking_for_vs_code_in_another_group_moves_its_one_tab_there(cx: &mut TestAppContext) {
    let Some((view, cx, left, right)) = split_with_vs_code(cx) else {
        return;
    };
    // The right group has a page of its own beside VS Code.
    open_pages(&view, cx, 1);
    let page = cx.update(|_, cx| view.read(cx).browser_tab_ids(cx))[0];
    move_page(&view, cx, right, page);
    run(&view, cx, right, Action::VsCode);
    let code = cx.update(|_, cx| view.read(cx).code_tab_id(cx)).unwrap();
    assert_eq!(tabs(&view, cx, right), [Pick::Page(page), Pick::Page(code)]);

    // Asked for on the left, the same tab moves there: the space still has
    // one, and the right group goes to the tab beside it.
    run(&view, cx, left, Action::VsCode);
    assert_eq!(cx.update(|_, cx| view.read(cx).code_tab_id(cx)), Some(code));
    assert!(tabs(&view, cx, left).contains(&Pick::Page(code)));
    assert_eq!(tabs(&view, cx, right), [Pick::Page(page)]);
    view.read_with(cx, |view, _| {
        assert_eq!(view.group_pick(left), Some(Pick::Page(code)));
        assert_eq!(view.group_pick(right), Some(Pick::Page(page)));
        assert_eq!(view.active_group(), Some(left));
    });
}

#[gpui::test]
fn a_group_whose_only_tab_was_vs_code_closes_when_it_moves_away(cx: &mut TestAppContext) {
    let Some((view, cx, left, right)) = split_with_vs_code(cx) else {
        return;
    };
    run(&view, cx, right, Action::VsCode);
    let code = cx.update(|_, cx| view.read(cx).code_tab_id(cx)).unwrap();
    assert_eq!(tabs(&view, cx, right), [Pick::Page(code)]);
    run(&view, cx, left, Action::VsCode);
    assert_eq!(groups(&view, cx), [left]);
    assert!(tabs(&view, cx, left).contains(&Pick::Page(code)));
    // Asked for where it already is, it is only shown.
    run(&view, cx, left, Action::VsCode);
    assert_eq!(groups(&view, cx), [left]);
    assert_eq!(
        view.read_with(cx, |view, _| view.group_pick(left)),
        Some(Pick::Page(code))
    );
}
