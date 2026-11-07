//! `e` and the file header's button open a file in the editor, at its line.
use super::{line, the, window};
use crate::review::{
    diff::{Diff, RowId},
    view::{
        Loaded,
        rows::Numbers,
        selection::{Caret, Selection},
    },
};
use std::path::PathBuf;

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

/// A file whose first hunk replaces line 3 and whose second adds line 40.
fn changes() -> Loaded {
    Loaded::of(Diff::parse(
        "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n\
         @@ -3,1 +3,1 @@\n-old\n+new\n@@ -39,0 +40,1 @@\n+added\n",
    ))
}

#[gpui::test]
fn the_editor_opens_at_the_line_the_changed_file_has(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, None);
    cx.update(|window, cx| view.update(cx, |view, cx| view.seed_review(changes(), window, cx)));
    let spot = |view: &mut crate::SheprdWindow, row: RowId| {
        let review = view.reviews.values_mut().next().unwrap();
        review.scroll_to_row(row);
        review.editor_row().and_then(|row| review.editor_spot(row))
    };
    let at = |line| Some((PathBuf::from("/work/repo/src/a.rs"), Some(line)));
    view.update(cx, |view, _| {
        // A file's header goes to its first hunk.
        assert_eq!(spot(view, RowId::Header(0)), at(3));
        // A hunk header and a removed line go to the next line still there.
        for row in [line(0), line(1), line(2)] {
            assert_eq!(spot(view, row), at(3), "{row:?}");
        }
        assert_eq!(spot(view, line(3)), at(40));
    });
}

/// A file taller than the view, its added lines numbered from 1.
fn tall() -> Loaded {
    let added: String = (0..200).map(|line| format!("+line {line}\n")).collect();
    Loaded::of(Diff::parse(&format!(
        "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -0,0 +1,200 @@\n{added}"
    )))
}

#[gpui::test]
fn a_clicked_line_opens_while_it_is_in_view(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, None);
    cx.update(|window, cx| view.update(cx, |view, cx| view.seed_review(tall(), window, cx)));
    draw(cx);
    let opens = |view: &crate::SheprdWindow| {
        let review = view.reviews.values().next().unwrap();
        review.editor_spot(review.editor_row().unwrap()).unwrap().1
    };
    let caret = Caret { line: 5, offset: 0 };
    view.update(cx, |view, _| {
        view.reviews.values_mut().next().unwrap().selection = Some(Selection {
            file: 0,
            side: Numbers::Both,
            anchor: caret,
            head: caret,
        });
    });
    draw(cx);
    view.read_with(cx, |view, _| assert_eq!(opens(view), Some(5)));
    // Scrolled away, the click no longer points at anything on screen.
    view.update(cx, |view, _| {
        let review = view.reviews.values_mut().next().unwrap();
        review.scroll_to_row(line(150));
    });
    draw(cx);
    view.read_with(cx, |view, _| assert_eq!(opens(view), Some(150)));
}

#[gpui::test]
fn e_on_a_clicked_file_in_the_list_opens_that_file(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, None);
    cx.simulate_resize(gpui::size(gpui::px(1600.), gpui::px(900.)));
    let files = super::files::many_files();
    cx.update(|window, cx| view.update(cx, |view, cx| view.seed_review(files, window, cx)));
    draw(cx);
    draw(cx);
    let file = cx.debug_bounds("review-file-1").unwrap();
    cx.simulate_click(file.center(), gpui::Modifiers::default());
    draw(cx);
    cx.simulate_keystrokes("e");
    cx.run_until_parked();
    // The fixture's checkout is not on disk, so the file reports itself.
    view.read_with(cx, |view, _| {
        let flash = view.flash.as_ref().map(|(flash, _)| flash.text.to_string());
        assert_eq!(
            flash.as_deref(),
            Some("\u{2068}/work/repo/src/a.rs\u{2069} is no longer in the checkout")
        );
    });
}

#[gpui::test]
fn the_header_button_opens_its_file_but_a_deleted_file_has_none(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, None);
    cx.simulate_resize(gpui::size(gpui::px(1600.), gpui::px(900.)));
    let diff = Diff::parse(
        "diff --git a/kept.rs b/kept.rs\n--- a/kept.rs\n+++ b/kept.rs\n@@ -1,1 +1,1 @@\n-a\n+b\n\
         diff --git a/gone.rs b/gone.rs\ndeleted file mode 100644\n--- a/gone.rs\n+++ /dev/null\n\
         @@ -1,1 +0,0 @@\n-a\n",
    );
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.seed_review(Loaded::of(diff), window, cx)
        })
    });
    draw(cx);
    draw(cx);
    assert!(cx.debug_bounds("review-open-editor-1").is_none());
    let button = cx.debug_bounds("review-open-editor-0").unwrap();
    cx.simulate_click(button.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let review = &view.reviews[&the(view)];
        assert!(review.draft.is_none(), "the click took no note");
        let flash = view.flash.as_ref().map(|(flash, _)| flash.text.to_string());
        assert_eq!(
            flash.as_deref(),
            Some("\u{2068}/work/repo/kept.rs\u{2069} is no longer in the checkout")
        );
    });
}

/// The window's snapshot with a second pane in the same terminal tab.
fn with_editor_pane(view: &mut crate::SheprdWindow) {
    let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
    let mut editor = snapshot.panes[0].clone();
    editor.pane_id = "w0:editor".into();
    snapshot.panes.push(editor);
}

#[gpui::test]
fn quitting_the_editor_brings_the_review_back(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx, None);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.seed_review(changes(), window, cx);
            view.live.status = crate::state::ConnectionStatus::Connected;
            with_editor_pane(view);
            let (endpoint, boot) = (
                view.endpoints[view.selected_endpoint].id.clone(),
                view.live.snapshot.as_ref().unwrap().boot_id.clone(),
            );
            let page = Some(the(view));
            view.note_editor_return(&endpoint, &boot, "w0:editor", page);
            // The editor holds the keyboard, away from the diff.
            window.blur(cx);
            view.poll_editor_returns(window, cx);
            assert_eq!(view.editor_returns.len(), 1, "the pane is still open");
            let review = view.reviews.values().next().unwrap();
            assert!(!review.focus.is_focused(window));

            let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.panes.retain(|pane| pane.pane_id != "w0:editor");
            view.poll_editor_returns(window, cx);
            assert!(view.editor_returns.is_empty());
            let review = view.reviews.values().next().unwrap();
            assert!(review.focus.is_focused(window));
        })
    });
}
