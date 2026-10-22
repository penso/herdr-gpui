//! A click's file the host does not have: the question that offers its path
//! to edit, and what its answers open.

use super::*;
use crate::menu::Page;

const PATH: &str = "/home/me/gone.md";
const CWD: &str = "/home/me/src";
const PAGE: &str = "/srv/www/gone.html";

fn posix(path: &str) -> RemotePath {
    RemotePath {
        target: "me@devbox".into(),
        shell: RemoteShell::Posix,
        path: path.into(),
        line: None,
    }
}

/// A host that is reached but cuts the transfer short.
fn cut_short() -> herdr_client::Result<&'static [u8]> {
    Err(herdr_client::Error::RemoteFile(
        herdr_client::RemoteFileError::Incomplete,
    ))
}

/// A window whose host has none of the files asked for.
fn on_empty_host<'a>(
    host: &FakeHost,
    cx: &'a mut TestAppContext,
) -> (Entity<HerdrWindow>, &'a mut VisualTestContext) {
    let (view, cx) = window(host, cx);
    view.update(cx, |view, _| host.serve(&mut view.remote_files, missing));
    (view, cx)
}

/// A click on `remote`, its fetch finished, and the poll that opens a
/// waiting question.
fn click(
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
    remote: RemotePath,
    unjoined: Option<RemotePath>,
    cwd: Option<&str>,
) {
    let cwd = cwd.map(str::to_owned);
    view.update(cx, |view, cx| {
        view.open_remote_file(remote, None, unjoined, cwd, cx)
    });
    poll(view, cx);
}

fn poll(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    cx.run_until_parked();
    view.update_in(cx, |view, window, cx| view.poll_remote_files(window, cx));
    cx.run_until_parked();
}

fn page(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Option<Page> {
    view.read_with(cx, |view, _| view.menu.page)
}

fn field(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |view, cx| view.remote_missing_text(cx))
}

/// Types `text` over the field's selected path, and presses Enter.
fn open(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, text: &str) {
    cx.simulate_input(text);
    cx.simulate_keystrokes("enter");
    poll(view, cx);
}

#[gpui::test]
fn a_missing_file_asks_for_a_path_holding_the_clicked_one(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = on_empty_host(&host, cx);
    click(&view, cx, posix(PATH), None, Some(CWD));
    assert_eq!(page(&view, cx), Some(Page::RemoteMissing));
    assert_eq!(field(&view, cx).as_deref(), Some(PATH));
    assert_eq!(host.asked(), vec![posix(PATH)]);
    assert!(cx.opened_url().is_none());
    view.read_with(cx, |view, _| {
        let (flash, _) = view.flash.as_ref().unwrap();
        assert_eq!(
            flash.text.as_ref(),
            "Fetching gone.md from me@devbox…",
            "no failure flash"
        );
    });
}

#[gpui::test]
fn a_missing_joined_path_offers_the_first_row_s_part(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = on_empty_host(&host, cx);
    let joined = posix("/home/me/gone.mdnext-row.txt");
    click(&view, cx, joined.clone(), Some(posix(PATH)), Some(CWD));
    assert_eq!(host.asked(), vec![joined]);
    assert_eq!(page(&view, cx), Some(Page::RemoteMissing));
    assert_eq!(field(&view, cx).as_deref(), Some(PATH));
}

#[gpui::test]
fn open_fetches_the_edited_path_under_the_click_s_directory(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = on_empty_host(&host, cx);
    click(&view, cx, posix(PATH), None, Some(CWD));
    open(&view, cx, "  docs/other.md ");
    assert_eq!(host.asked()[1], posix("/home/me/src/docs/other.md"));
    // Not there either: asked again, offering the path as the host reads it.
    assert_eq!(page(&view, cx), Some(Page::RemoteMissing));
    assert_eq!(
        field(&view, cx).as_deref(),
        Some("/home/me/src/docs/other.md")
    );
}

#[gpui::test]
fn a_path_opened_from_the_question_that_is_missing_asks_again(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = on_empty_host(&host, cx);
    click(&view, cx, posix(PATH), None, Some(CWD));
    open(&view, cx, "/srv/typed.md");
    assert_eq!(host.asked().len(), 2);
    assert_eq!(page(&view, cx), Some(Page::RemoteMissing));
    assert_eq!(field(&view, cx).as_deref(), Some("/srv/typed.md"));
}

#[gpui::test]
fn escape_closes_the_question_without_a_fetch(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = on_empty_host(&host, cx);
    click(&view, cx, posix(PATH), None, Some(CWD));
    cx.simulate_keystrokes("escape");
    poll(&view, cx);
    assert_eq!(page(&view, cx), None);
    assert_eq!(field(&view, cx), None);
    assert_eq!(host.asked().len(), 1);
}

#[gpui::test]
fn open_with_an_empty_field_does_nothing(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = on_empty_host(&host, cx);
    click(&view, cx, posix(PATH), None, Some(CWD));
    open(&view, cx, "   ");
    assert_eq!(host.asked().len(), 1);
    assert_eq!(page(&view, cx), Some(Page::RemoteMissing));
}

#[gpui::test]
fn open_of_a_relative_path_with_no_directory_does_nothing(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = on_empty_host(&host, cx);
    click(&view, cx, posix(PATH), None, None);
    open(&view, cx, "other.md");
    assert_eq!(host.asked().len(), 1);
    assert_eq!(page(&view, cx), Some(Page::RemoteMissing));
    assert_eq!(field(&view, cx).as_deref(), Some("other.md"));
}

#[gpui::test]
fn any_other_fetch_failure_says_why_and_asks_nothing(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = window(&host, cx);
    view.update(cx, |view, _| host.serve(&mut view.remote_files, cut_short));
    click(&view, cx, posix(PATH), None, Some(CWD));
    assert_eq!(page(&view, cx), None);
    view.read_with(cx, |view, _| {
        let (flash, _) = view.flash.as_ref().unwrap();
        assert!(
            flash.text.starts_with("Could not fetch the file: "),
            "{}",
            flash.text
        );
    });
}

/// A window whose host has no page until the question is up, recording the
/// addresses the default browser is asked to open.
fn page_window<'a>(
    host: &FakeHost,
    cx: &'a mut TestAppContext,
) -> (
    Entity<HerdrWindow>,
    &'a mut VisualTestContext,
    Arc<Mutex<Vec<String>>>,
) {
    let (view, cx) = on_empty_host(host, cx);
    let opened = Arc::new(Mutex::new(Vec::new()));
    let record = opened.clone();
    view.update(cx, |view, cx| {
        view.default_browser = Arc::new(move |url: &url::Url| {
            record.lock().unwrap().push(url.to_string());
            Ok(())
        });
        let clicked = RemotePath {
            line: Some(3),
            ..posix(PAGE)
        };
        view.open_remote_file(clicked, Some("?a=1#top".into()), None, Some(CWD.into()), cx)
    });
    poll(&view, cx);
    assert_eq!(page(&view, cx), Some(Page::RemoteMissing));
    host.change_file(NOTES);
    (view, cx, opened)
}

#[gpui::test]
fn open_of_the_clicked_path_keeps_its_query_and_line(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx, opened) = page_window(&host, cx);
    cx.simulate_keystrokes("enter");
    poll(&view, cx);
    assert_eq!(host.asked()[1].line, Some(3));
    let opened = opened.lock().unwrap().clone();
    assert_eq!(opened.len(), 1);
    assert!(opened[0].ends_with("gone.html?a=1#top"), "{opened:?}");
}

#[gpui::test]
fn open_of_an_edited_path_drops_the_click_s_query_and_line(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx, opened) = page_window(&host, cx);
    open(&view, cx, "/srv/www/other.html");
    assert_eq!(host.asked()[1], posix("/srv/www/other.html"));
    // With no query, the copy opens in the system's application.
    assert!(opened.lock().unwrap().is_empty());
    let url = cx.opened_url().unwrap();
    assert!(url.ends_with("/other.html"), "{url}");
}
