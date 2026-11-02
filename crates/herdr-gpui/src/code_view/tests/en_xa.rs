//! The `en-XA` leak test (`crate::i18n::leak_tests`) for a code tab: its
//! header, outline heading, and definition kinds are messages; the file's
//! path, lines, and definition names are data.
use super::*;
use crate::i18n::leak_tests::{assert_translated, draw, under_en_xa};
use gpui::{px, size};

#[test]
fn a_code_tab_shows_only_translated_text_under_en_xa() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lib.rs");
    let source = "pub struct Pane;\nimpl Pane {\n    fn run(&self) {}\n}\n";
    std::fs::write(&path, source).unwrap();
    under_en_xa("code tab en-XA leak", |cx, recorder| {
        let (view, cx) = window(cx);
        cx.simulate_resize(size(px(1600.), px(900.)));
        draw(cx);
        recorder.take();
        let target = EditorTarget {
            path: path.clone(),
            line: Some(1),
        };
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.open_code_view(&target, window, cx))
        });
        cx.run_until_parked();
        draw(cx);
        draw(cx);
        let mut data: Vec<String> = source.lines().map(|line| line.trim().to_owned()).collect();
        view.read_with(cx, |view, _| {
            let text = view.code_views.values().next().unwrap().text().unwrap();
            data.extend(text.outline.iter().map(|entry| entry.name.clone()));
        });
        data.push("lib.rs".into());
        // The header truncates the path to its width, so it may show only
        // a piece of it, such as the drive letter of a Windows temp path.
        let shown_path = path.display().to_string();
        let shown: Vec<String> = recorder
            .take()
            .into_iter()
            .filter(|line| {
                let piece = line.trim().trim_end_matches('\u{2026}');
                piece.is_empty() || !shown_path.contains(piece)
            })
            .collect();
        assert_translated("code tab", &shown, &data);
    });
}
