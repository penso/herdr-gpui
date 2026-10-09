//! `e` opens the file at the top of the diff in the editor, at its line.
use super::{line, window};
use crate::review::{
    diff::{Diff, RowId},
    view::Loaded,
};

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
    let spot = |view: &mut crate::HerdrWindow, row: RowId| {
        let review = view.reviews.values_mut().next().unwrap();
        review.scroll_to_row(row);
        review
            .editor_spot()
            .map(|(path, line)| (path.to_owned(), line))
    };
    view.update(cx, |view, _| {
        assert_eq!(
            spot(view, RowId::Header(0)),
            Some(("src/a.rs".into(), None))
        );
        // A hunk header and a removed line go to the next line still there.
        for row in [line(0), line(1), line(2)] {
            assert_eq!(
                spot(view, row),
                Some(("src/a.rs".into(), Some(3))),
                "{row:?}"
            );
        }
        assert_eq!(spot(view, line(3)), Some(("src/a.rs".into(), Some(40))));
    });
}
