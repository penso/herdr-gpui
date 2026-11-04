use super::*;

#[gpui::test]
fn editors_share_group_placement_and_keep_failures_provider_scoped(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    let opened = cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            [Editor::VsCode, Editor::Px0]
                .map(|editor| view.open_embedded_editor(editor, None, None, window, cx))
        })
    });
    if !crate::browser::EMBEDDED {
        assert_eq!(opened, [None, None]);
        return;
    }
    let [code, px0] = opened.map(Option::unwrap);
    assert_ne!(code, px0);
    view.update(cx, |view, cx| {
        view.browser.failed.insert(code, "code failed".into());
        view.browser.failed.insert(px0, "px0 failed".into());
        assert!(view.editor_page_settled(code));
        assert!(view.editor_page_settled(px0));
        view.close_editor_pages(Editor::Px0, cx);
        assert!(!view.editor_page_settled(px0));
        assert!(view.editor_page_settled(code));
        assert_eq!(view.editor_tab_id(Editor::VsCode, cx), Some(code));
        assert_eq!(view.editor_tab_id(Editor::Px0, cx), Some(px0));
    });
    for (editor, id) in [(Editor::VsCode, code), (Editor::Px0, px0)] {
        let opened = cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_embedded_editor(editor, None, None, window, cx)
            })
        });
        assert_eq!(opened, Some(id));
        assert!(cx.update(|_, cx| view.read(cx).group_shows_page(id, cx)));
    }
}
