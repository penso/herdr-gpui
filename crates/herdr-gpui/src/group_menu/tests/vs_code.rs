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
