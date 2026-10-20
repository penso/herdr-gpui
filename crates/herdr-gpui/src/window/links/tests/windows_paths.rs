//! Whether a pane's printed relative path is read with Windows separators,
//! as its working directory says its host writes them.
use super::*;

/// Printed alone, so it ends inside the fixture's 20-column row.
const PRINTED: &str = r"docs\index.html?x=1";

/// The path and query `file_link_at` reads at the start of `PRINTED` in a
/// pane whose foreground directory is `cwd`.
fn read_in(cwd: &str, cx: &mut gpui::TestAppContext) -> Option<(String, Option<String>)> {
    let peer = MockPeer::new();
    let (view, cx) = window(&peer, false, cx);
    view.update(cx, |view, _| {
        let mut snapshot = snapshot();
        let pane = snapshot
            .panes
            .iter_mut()
            .find(|pane| pane.pane_id == "w1:p1")
            .unwrap();
        pane.foreground_cwd = Some(cwd.into());
        view.live.snapshot = Some(Arc::new(snapshot));
        let mut surface = (*wrapped(2)).clone();
        let mut symbols = PRINTED.chars().chain(std::iter::repeat(' '));
        for cell in &mut surface.frame.cells {
            cell.symbol = symbols.next().unwrap().to_string();
        }
        view.live.surface = Some(Arc::new(surface));
    });
    let position = at(&view, cx, 1, 0);
    view.read_with(cx, |view, _| {
        view.file_link_at(position)
            .map(|link| (link.path, link.query))
    })
}

#[gpui::test]
fn a_pane_in_a_windows_folder_splits_a_relative_path_s_query(cx: &mut gpui::TestAppContext) {
    assert_eq!(
        read_in(r"C:\dev", cx),
        Some((r"docs\index.html".into(), Some("?x=1".into())))
    );
}

#[gpui::test]
fn a_pane_in_a_posix_folder_keeps_the_backslash_an_escape(cx: &mut gpui::TestAppContext) {
    assert_eq!(
        read_in("/srv/app", cx),
        Some((r"docs\index.html?x=1".into(), None))
    );
}
