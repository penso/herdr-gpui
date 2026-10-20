//! A host's page fetched by a click on its path printed with a query: the
//! copy opens in the default browser with it, however the click finds it.

use super::*;

const PAGE: &str = "/srv/www/page.html";
const EDITED: &[u8] = b"<p>edited</p>\n";

fn key() -> Key {
    ("me@devbox".to_owned(), PAGE.to_owned())
}

/// A click on `PAGE` printed with `?a=1#top`, and its work finished.
fn click(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| {
        view.open_remote_file(remote(PAGE), Some("?a=1#top".into()), cx)
    });
    cx.run_until_parked();
}

#[gpui::test]
fn a_fetched_page_opens_in_the_default_browser_with_its_query(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = window(&host, cx);
    let asked = Arc::new(Mutex::new(Vec::new()));
    let record = asked.clone();
    view.update(cx, |view, _| {
        view.default_browser = Arc::new(move |url: &url::Url| {
            record.lock().unwrap().push(url.to_string());
            Ok(())
        });
    });
    click(&view, cx);

    // The host is asked for the path alone, and the copy keeps it as its key.
    assert_eq!(host.asked(), vec![remote(PAGE)]);
    let copy = view.read_with(cx, |view, _| view.remote_files.copies[&key()].local.clone());
    let page = crate::window::opened(&copy).unwrap();
    let expected = format!("{}?a=1#top", url::Url::from_file_path(page).unwrap());
    assert_eq!(*asked.lock().unwrap(), vec![expected.clone()]);

    // Saved before the next click's fetch lands, the copy keeps its edits,
    // and that copy opens with the query.
    std::fs::write(&copy, EDITED).unwrap();
    click(&view, cx);
    assert_eq!(host.asked().len(), 2);
    assert_eq!(std::fs::read(&copy).unwrap(), EDITED);
    assert_eq!(asked.lock().unwrap().len(), 2);
    assert_eq!(asked.lock().unwrap()[1], expected);

    // A copy with edits the host lacks is not fetched again; a click opens
    // the copy it has, with the query.
    view.update(cx, |view, _| {
        view.remote_files.copies.get_mut(&key()).unwrap().state = SyncState::Edited {
            sha256: Sha256::digest(EDITED).into(),
        };
    });
    click(&view, cx);
    assert_eq!(host.asked().len(), 2, "not fetched again");
    assert_eq!(*asked.lock().unwrap(), vec![expected; 3]);
    assert!(cx.opened_url().is_none());
}
