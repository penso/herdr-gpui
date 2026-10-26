use super::*;
use crate::terminal::{RowTarget, file_target};
use gpui::{Modifiers, TestAppContext, point, px};
use herdr_client::ConnectTarget;

fn document_link(label: &str, path: &std::path::Path) -> String {
    // Raw Windows paths may contain Markdown escapes: `\.` in a hidden
    // directory loses its separator, even inside angle brackets.
    let destination = url::Url::from_file_path(path).unwrap();
    format!("[{label}](<{destination}>)")
}

#[cfg(windows)]
#[test]
fn windows_file_links_accept_absolute_drives_and_reject_network_paths() {
    for (source, expected, line) in [
        (r"C:\project\notes.md", r"C:\project\notes.md", None),
        ("C:/project/notes.md:12:4", "C:/project/notes.md", Some(12)),
        (
            r"C:\my project\notes.md:7",
            r"C:\my project\notes.md",
            Some(7),
        ),
    ] {
        assert_eq!(
            file_target(source),
            Some(RowTarget::Path {
                path: expected.into(),
                line,
                explicit: true,
            })
        );
    }
    for source in [
        r"C:relative\notes.md",
        r"\\other-host\share\notes.md",
        "//other-host/share/notes.md",
    ] {
        assert!(file_target(source).is_none(), "{source}");
    }
}

#[test]
fn explicit_document_targets_reuse_path_and_line_rules_without_unsafe_schemes() {
    for (source, expected, line) in [
        ("/tmp/notes.md", "/tmp/notes.md", None),
        ("docs/notes.md:12:4", "docs/notes.md", Some(12)),
        ("~/notes.md:7", "~/notes.md", Some(7)),
    ] {
        assert_eq!(
            file_target(source),
            Some(RowTarget::Path {
                path: expected.into(),
                line,
                explicit: true,
            })
        );
    }
    for source in [
        "javascript:alert(1)",
        "data:text/html,test",
        "file://other-host/tmp/notes.md",
        "//other-host/share/notes.md",
        r"\\other-host\share\notes.md",
        "#section",
        "/tmp/bad\nname.md",
        "",
    ] {
        assert!(file_target(source).is_none(), "{source}");
    }
    assert!(file_target(&format!("/tmp/{}", "x".repeat(4096))).is_none());
}

#[gpui::test]
fn local_rich_document_link_opens_only_on_click_and_keeps_original_copy(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notes.md");
    std::fs::write(&path, "# Synthetic notes\n").unwrap();
    let source = document_link("Changes and verification details", &path);
    let (view, _peer, cx) = overlay::open(cx, ConnectTarget::Local);
    view.update(cx, |view, _| {
        view.config.open_files_in = crate::config::FileTarget::System;
    });
    let program = Program::new();
    let _stream = overlay::attach_with(
        &view,
        &program,
        snapshot(cursor(1, 0), vec![agent(1, &source)]),
        cx,
    );
    cx.run_until_parked();
    assert!(cx.opened_url().is_none(), "rendering must not open files");
    let prose = cx.debug_bounds("chat-prose").unwrap();
    cx.simulate_click(
        point(prose.origin.x + px(8.), prose.center().y),
        Modifiers::default(),
    );
    wait_until(|| {
        cx.run_until_parked();
        cx.opened_url().is_some()
    });
    let opened = url::Url::parse(&cx.opened_url().unwrap())
        .unwrap()
        .to_file_path()
        .unwrap();
    // Opening resolves symlinks without expanding Windows short names;
    // canonicalize both sides to compare the file rather than its spelling.
    assert_eq!(
        std::fs::canonicalize(opened).unwrap(),
        std::fs::canonicalize(path).unwrap()
    );
    let copy = cx.debug_bounds("chat-copy-1").unwrap();
    cx.simulate_mouse_move(copy.center(), None, Modifiers::default());
    overlay::poll(&view, &program, cx);
    cx.simulate_click(copy.center(), Modifiers::default());
    assert_eq!(
        cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap()),
        source
    );
}

#[gpui::test]
fn remote_rich_document_link_never_opens_an_identically_named_local_file(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notes.md");
    std::fs::write(&path, "# Laptop-only notes\n").unwrap();
    let source = document_link("Remote notes", &path);
    let (view, _peer, cx) = overlay::open(
        cx,
        ConnectTarget::Ssh {
            target: "synthetic-host".into(),
            session: "default".into(),
        },
    );
    let program = Program::new();
    let _stream = overlay::attach_with(
        &view,
        &program,
        snapshot(cursor(1, 0), vec![agent(1, &source)]),
        cx,
    );
    let prose = cx.debug_bounds("chat-prose").unwrap();
    cx.simulate_click(
        point(prose.origin.x + px(8.), prose.center().y),
        Modifiers::default(),
    );
    cx.run_until_parked();
    assert!(cx.opened_url().is_none());
}

mod markdown;
