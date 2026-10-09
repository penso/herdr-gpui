#![allow(clippy::unwrap_used)]
// Not a glob: the parent's `gpui::*` would shadow the `#[test]` that
// `gpui::test` expands to.
use super::{CodeFile, State, Text};
use crate::{
    HerdrWindow,
    browser::{Location, Store, scope},
    code_index::Kind,
    editor::EditorTarget,
};
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::sync::Arc;

#[test]
fn saved_code_tabs_are_checked_when_read() {
    let absolute = std::env::temp_dir().join("main.rs");
    let saved = |path: &str| {
        serde_json::json!({"kind": "code", "file": {"path": path, "line": 4}}).to_string()
    };
    let location: Location = serde_json::from_str(&saved(absolute.to_str().unwrap())).unwrap();
    assert!(!location.is_page());
    assert_eq!(location.default_title(), "main.rs");
    let Location::Code { file } = location else {
        panic!("not a code tab");
    };
    assert_eq!(file.line, Some(4));
    for bad in ["", "relative/main.rs", "/a\nb.rs"] {
        assert!(
            serde_json::from_str::<Location>(&saved(bad)).is_err(),
            "{bad:?}"
        );
    }
}

#[test]
fn a_file_is_read_into_lines_and_an_outline() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lib.rs");
    std::fs::write(
        &path,
        "pub struct Pane;\r\n\r\nimpl Pane {\n    fn a_much_longer_line(&self) {}\n}",
    )
    .unwrap();
    let text = Text::read(&path).unwrap();
    assert_eq!(text.lines.len(), 5);
    assert_eq!(text.line(0), "pub struct Pane;");
    assert_eq!(text.line(1), "");
    assert_eq!(text.line(4), "}");
    assert_eq!(text.line(9), "");
    assert_eq!(text.widest, 3);
    let outline: Vec<_> = text
        .outline
        .iter()
        .map(|entry| (entry.name.as_str(), entry.kind, entry.line))
        .collect();
    assert_eq!(
        outline,
        [
            ("Pane", Kind::Type, 1),
            ("impl Pane", Kind::Implementation, 3),
            ("a_much_longer_line", Kind::Method, 4),
        ]
    );
    // Tabs widen and a direction override never reaches the screen.
    std::fs::write(&path, "\tlet a = \"\u{202e}x\";\n").unwrap();
    let text = Text::read(&path).unwrap();
    assert_eq!(text.line(0), "    let a = \"x\";");
    // A missing or binary file says so.
    assert!(Text::read(&dir.path().join("gone.rs")).is_err());
    std::fs::write(dir.path().join("blob.bin"), b"\0\x01").unwrap();
    assert!(matches!(
        Text::read(&dir.path().join("blob.bin")),
        Err(crate::Error::CodeFileUnreadable)
    ));
}

fn window(cx: &mut TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    cx.add_window_view(|window, cx| {
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        let mut shown = serde_json::to_value(crate::sidebar::layout_tests::snapshot(1)).unwrap();
        shown["focused_workspace_id"] = "w0".into();
        view.live.snapshot = Some(Arc::new(serde_json::from_value(shown).unwrap()));
        view
    })
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

#[gpui::test]
fn a_code_tab_shows_its_file_at_a_line_and_keeps_one_tab_per_file(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("main.rs");
    let source: String = (1..=80).map(|i| format!("fn f{i}() {{}}\n")).collect();
    std::fs::write(&path, source).unwrap();
    let (view, cx) = window(cx);
    let open = |line: u32, view: &Entity<HerdrWindow>, cx: &mut VisualTestContext| {
        let target = EditorTarget {
            path: path.clone(),
            line: Some(line),
        };
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.open_code_view(&target, window, cx))
        });
        cx.run_until_parked();
    };
    open(40, &view, cx);
    draw(cx);
    view.read_with(cx, |view, _| {
        let code = view.code_views.values().next().unwrap();
        assert_eq!(code.shown(), (Some(40), Some(80)));
        assert_eq!(code.spans.len(), 80, "coloured as Rust");
    });
    assert!(cx.debug_bounds("code-view").is_some());
    assert!(cx.debug_bounds("code-line-39").is_some());
    assert!(
        cx.debug_bounds("code-line-0").is_none(),
        "scrolled to the line"
    );

    // The same file again reuses its tab and moves to the new line.
    open(2, &view, cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.code_views.len(), 1);
        let tabs = cx.global::<Store>();
        let (scope, workspace) = view.browser_key().unwrap();
        assert_eq!(tabs.in_workspace(&scope, &workspace).count(), 1);
        assert_eq!(view.code_views.values().next().unwrap().shown().0, Some(2));
    });

    // `o` hides the outline, `e` opens the editor at the marked line, which
    // this fixture's missing connection refuses.
    let id = view.read_with(cx, |view, _| *view.code_views.keys().next().unwrap());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let focus = view.code_views[&id].focus.clone();
            window.focus(&focus, cx);
        })
    });
    draw(cx);
    cx.simulate_keystrokes("o");
    view.read_with(cx, |view, _| assert!(!view.code_views[&id].outline));
    cx.simulate_keystrokes("e");
    let refusal = if crate::editor::SUPPORTED {
        crate::Error::NotConnected
    } else {
        crate::Error::EditorUnsupported
    };
    view.read_with(cx, |view, _| {
        let (flash, _) = view.flash.as_ref().unwrap();
        assert_eq!(flash.text.as_ref(), refusal.to_string());
    });
}

#[gpui::test]
fn a_restored_code_tab_reads_its_file_and_a_closed_one_is_forgotten(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    let id = cx.update(|_, cx| {
        let tab_scope = scope(&view.read(cx).endpoints[0]);
        let file = CodeFile {
            path: std::env::temp_dir()
                .join("herdr-gpui-missing-code-tab.rs")
                .to_str()
                .unwrap()
                .into(),
            line: None,
        };
        Store::update(cx, |store| {
            store.open(tab_scope, "w0", Some(Location::Code { file }), None)
        })
        .unwrap()
    });
    cx.update(|_, cx| view.update(cx, |view, cx| view.poll_code_views(cx)));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(matches!(view.code_views[&id].state, State::Failed(_)));
    });
    cx.update(|_, cx| {
        Store::update(cx, |store| store.close(id));
        view.update(cx, |view, cx| {
            view.poll_code_views(cx);
            assert!(view.code_views.is_empty());
        })
    });
}

#[gpui::test]
fn reopening_a_code_tab_reads_its_file_again(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("main.rs");
    std::fs::write(&path, "fn old() {}\n").unwrap();
    let (view, cx) = window(cx);
    let open = |line: u32, view: &Entity<HerdrWindow>, cx: &mut VisualTestContext| {
        let target = EditorTarget {
            path: path.clone(),
            line: Some(line),
        };
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.open_code_view(&target, window, cx))
        });
        cx.run_until_parked();
    };
    open(1, &view, cx);
    // An agent edits the file, and a newer index points into the new text.
    std::fs::write(&path, "fn old() {}\nfn new() {}\nfn newer() {}\n").unwrap();
    open(3, &view, cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.code_views.len(), 1);
        let code = view.code_views.values().next().unwrap();
        assert_eq!(code.shown(), (Some(3), Some(3)));
        assert_eq!(code.text().unwrap().line(2), "fn newer() {}");
    });
}
