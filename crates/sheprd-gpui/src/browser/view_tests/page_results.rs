use super::*;
use crate::browser::{LocalFile, PostedFrom, TabId};

const SENT: &str = r#"{"kind":"send","summary":"Picked B","data":{"variant":"B"}}"#;

fn opened(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, location: Location) -> TabId {
    let tab_scope = view.read_with(cx, |view, _| scope(&view.endpoints[0]));
    cx.update(|_, cx| {
        Store::update(cx, |store| {
            store.open(tab_scope, "w0", Some(location), Some("w0:p1".into()))
        })
        .unwrap()
    })
}

fn post_from(
    view: &Entity<SheprdWindow>,
    cx: &mut VisualTestContext,
    id: TabId,
    body: &str,
    from: PostedFrom,
) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.page_posted(id, body, from, window, cx))
    });
}

fn post(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, id: TabId, body: &str) {
    post_from(view, cx, id, body, PostedFrom::Preview);
}

fn listed(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, id: TabId) -> usize {
    view.read_with(cx, |view, _| view.browser.annotations.listed(id).len())
}

#[gpui::test]
fn a_local_pages_result_waits_in_its_notes_without_annotating(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    // An absolute path on every platform: `/srv/...` is not one on Windows.
    let path = std::env::temp_dir().join("mockups").join("index.html");
    let file = LocalFile::new(&path, None).unwrap();
    let local = opened(&view, cx, Location::Local { file });
    // A web frame inside the local page, or a web page it is navigating to
    // before the tab's location catches up, cannot send.
    post_from(&view, cx, local, SENT, PostedFrom::Elsewhere);
    assert_eq!(listed(&view, cx, local), 0);
    post(&view, cx, local, SENT);
    // Queued as a note the user still sends, with the panel open.
    assert_eq!(listed(&view, cx, local), 1);
    view.read_with(cx, |view, _| {
        assert_eq!(view.browser.annotations.queued(local), 1);
        assert!(view.browser.annotations.open(local));
    });
    // Picking again changes the unsent answer instead of adding one.
    post(
        &view,
        cx,
        local,
        r#"{"kind":"send","summary":"Picked C","data":{"variant":"C"}}"#,
    );
    assert_eq!(listed(&view, cx, local), 1);
    // A web page cannot send results, however it posts.
    let web = opened(&view, cx, url("http://localhost:3000/"));
    post(&view, cx, web, SENT);
    assert_eq!(listed(&view, cx, web), 0);
}

fn comment_on(
    view: &Entity<SheprdWindow>,
    cx: &mut VisualTestContext,
    id: TabId,
    text: &str,
    save: bool,
) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.edit_note(id, 0, window, cx);
            let input = view.browser.annotations.input(id, cx);
            input.update(cx, |input, cx| input.set_text_selected(text, cx));
            if save {
                view.add_note(id, window, cx);
            }
        })
    });
}

#[gpui::test]
fn a_newer_result_keeps_the_users_comment(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    let path = std::env::temp_dir().join("mockups").join("index.html");
    let file = LocalFile::new(&path, None).unwrap();
    let local = opened(&view, cx, Location::Local { file });
    post(&view, cx, local, SENT);
    comment_on(&view, cx, local, "But keep the old icon", true);
    post(
        &view,
        cx,
        local,
        r#"{"kind":"send","summary":"Picked C","data":{"variant":"C"}}"#,
    );
    let listed = |view: &Entity<SheprdWindow>, cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| view.browser.annotations.listed(local))
    };
    assert_eq!(listed(&view, cx), [("But keep the old icon".into(), false)]);
    // While the user edits that comment, a newer result is added beside it
    // rather than taking the comment being written.
    comment_on(&view, cx, local, "Unsaved", false);
    post(
        &view,
        cx,
        local,
        r#"{"kind":"send","summary":"Picked D","data":{"variant":"D"}}"#,
    );
    assert_eq!(
        listed(&view, cx),
        [
            ("But keep the old icon".into(), false),
            (String::new(), false)
        ]
    );
}
