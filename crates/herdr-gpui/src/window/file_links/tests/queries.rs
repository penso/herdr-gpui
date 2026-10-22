//! Paths a pane printed with a `?query` or `#fragment`: found on disk by
//! their path, and a page among them opened in the default browser with the
//! rest. Every other file opens as it would without it.
use super::*;
use std::sync::Mutex;

/// The addresses a fake default browser was started on.
type Asked = Arc<Mutex<Vec<String>>>;

/// Has `view` start a fake default browser, which records each address it
/// is started on and starts when `starts`.
fn fake_browser(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, starts: bool) -> Asked {
    let asked = Asked::default();
    let record = asked.clone();
    view.update(cx, |view, _| {
        view.default_browser = Arc::new(move |url: &url::Url| {
            record.lock().unwrap().push(url.to_string());
            if starts {
                Ok(())
            } else {
                Err(crate::default_browser::Error::UnparsableCommand {
                    command: "fake".into(),
                })
            }
        });
    });
    asked
}

/// A folder holding `files`, each a path under it, which the pane runs in.
fn folder(files: &[&str]) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    for file in files {
        let path = root.path().join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "x").unwrap();
    }
    root
}

/// The address the system opens for `path` under `root`, symlinks resolved.
fn address(root: &tempfile::TempDir, path: &str) -> String {
    let real = real_path(&root.path().join(path)).unwrap();
    url::Url::from_file_path(real).unwrap().to_string()
}

/// Opens in the system's default application rather than the editor.
fn system() -> Modifiers {
    Modifiers {
        alt: true,
        ..Modifiers::secondary_key()
    }
}

#[gpui::test]
fn a_printed_page_with_a_query_opens_in_the_default_browser_with_it(cx: &mut gpui::TestAppContext) {
    let root = folder(&["m/index.html"]);
    let (view, cx) = window(
        "open m/index.html?sheet=1 or m/missing.html?x=1 now",
        root.path(),
        cx,
    );
    let page = at(&view, cx, 6);
    let missing = at(&view, cx, 32);

    // Found by its path, the page is underlined with its query.
    cx.simulate_mouse_move(page, None, Modifiers::secondary_key());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.hovered_local_link().unwrap().rows, vec![(0, 5..25)]);
    });
    // A page that is not there stays plain text, query or not.
    cx.simulate_mouse_move(missing, None, Modifiers::secondary_key());
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(view.hovered_local_link().is_none()));

    let bare = address(&root, "m/index.html");
    let expected = format!("{bare}?sheet=1");
    let asked = fake_browser(&view, cx, true);
    modifier_click(cx, page, system());
    assert_eq!(*asked.lock().unwrap(), vec![expected.clone()]);
    assert!(cx.opened_url().is_none());

    // A browser that cannot be started leaves the page to the system's
    // opener, without the query it would drop anyway.
    let asked = fake_browser(&view, cx, false);
    modifier_click(cx, page, system());
    assert_eq!(*asked.lock().unwrap(), vec![expected]);
    assert_eq!(cx.opened_url(), Some(bare));
}

#[gpui::test]
fn a_file_without_an_extension_and_a_pdf_page_open_in_the_browser(cx: &mut gpui::TestAppContext) {
    let root = folder(&["srv/query", "m/guide.pdf"]);
    let (view, cx) = window(
        "see srv/query?x=1 and m/guide.pdf#page=3 ok",
        root.path(),
        cx,
    );
    let asked = fake_browser(&view, cx, true);
    // Even where a file would open in the editor, a page with a query
    // opens in the browser.
    let position = at(&view, cx, 6);
    modifier_click(cx, position, Modifiers::secondary_key());
    let position = at(&view, cx, 24);
    modifier_click(cx, position, Modifiers::secondary_key());
    assert_eq!(
        *asked.lock().unwrap(),
        vec![
            format!("{}?x=1", address(&root, "srv/query")),
            format!("{}#page=3", address(&root, "m/guide.pdf")),
        ]
    );
    assert!(cx.opened_url().is_none());
}

#[gpui::test]
fn a_file_that_is_no_page_opens_as_it_would_without_the_query(cx: &mut gpui::TestAppContext) {
    let root = folder(&["m/tool.py"]);
    let (view, cx) = window("run m/tool.py?x=1 now", root.path(), cx);
    let asked = fake_browser(&view, cx, true);
    // A script opens the folder holding it, which takes no query.
    let position = at(&view, cx, 6);
    modifier_click(cx, position, system());
    assert!(asked.lock().unwrap().is_empty());
    let opened = cx.opened_url().unwrap();
    assert_eq!(opened, address(&root, "m"));
    assert!(!opened.contains('?'), "{opened}");
}

#[gpui::test]
fn a_line_fragment_names_the_line_and_never_reaches_the_browser(cx: &mut gpui::TestAppContext) {
    let root = folder(&["docs/notes.txt", "src/main.rs"]);
    let (view, cx) = window(
        "see docs/notes.txt#L3 or src/main.rs#L10 ok",
        root.path(),
        cx,
    );
    let notes = at(&view, cx, 6);
    let main = at(&view, cx, 28);
    view.read_with(cx, |view, _| {
        let link = view.file_link_at(notes).unwrap();
        assert_eq!(link.path, "docs/notes.txt");
        assert_eq!(link.line, Some(3));
        assert_eq!(link.query.as_deref(), Some("#L3"));
        let link = view.file_link_at(main).unwrap();
        assert_eq!(link.path, "src/main.rs");
        assert_eq!(link.line, Some(10));
    });

    // In the editor or the system's application, these files open as they
    // would without the fragment, never in the browser.
    let asked = fake_browser(&view, cx, true);
    for (position, path) in [(notes, "docs/notes.txt"), (main, "src/main.rs")] {
        modifier_click(cx, position, Modifiers::secondary_key());
        modifier_click(cx, position, system());
        assert!(asked.lock().unwrap().is_empty(), "{path}");
        assert_eq!(cx.opened_url(), Some(address(&root, path)), "{path}");
    }
}

/// The browser never runs a page, but the system opener may: an executable
/// one it is left falls back to the folder holding it, as a click on it
/// without a query does.
#[cfg(unix)]
#[gpui::test]
fn an_executable_page_the_browser_cannot_open_opens_its_folder(cx: &mut gpui::TestAppContext) {
    use std::os::unix::fs::PermissionsExt;
    let root = folder(&["m/tool"]);
    let tool = root.path().join("m/tool");
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (view, cx) = window("run m/tool?x=1 now", root.path(), cx);
    let asked = fake_browser(&view, cx, false);
    let position = at(&view, cx, 6);
    modifier_click(cx, position, system());
    assert_eq!(
        *asked.lock().unwrap(),
        vec![format!("{}?x=1", address(&root, "m/tool"))]
    );
    assert_eq!(cx.opened_url(), Some(address(&root, "m")));
}
