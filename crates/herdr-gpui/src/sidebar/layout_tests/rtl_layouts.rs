//! Every row layout mirrors under `en-XB` with the guides it shares, so a
//! worktree's tree line still meets the row it hangs from.
use super::*;
use herdr_i18n::Pseudo;

#[gpui::test]
fn guides_stay_beside_their_rows_right_to_left(cx: &mut gpui::TestAppContext) {
    use crate::config::LayoutMode;
    crate::i18n::show(&Pseudo::Bidi.locale());
    for mode in [LayoutMode::Minimal, LayoutMode::Orca, LayoutMode::Superset] {
        // A fresh window per mode, so no bounds linger from another layout.
        let (_, cx) = cx.add_window_view(|window, cx| {
            let mut view = fixture_window(window, cx);
            view.live.snapshot = Some(Arc::new(snapshot(6)));
            view.config.layout.mode = mode;
            view
        });
        cx.simulate_resize(size(px(800.), px(900.)));
        cx.run_until_parked();
        cx.update(|window, cx| {
            full_draw(window, cx).clear(cx);
        });
        let sidebar = cx.debug_bounds("sidebar").unwrap();
        let row = cx.debug_bounds("row-sidebar-child").unwrap();
        let name = cx.debug_bounds("name-sidebar-child").unwrap();
        let guide = cx
            .debug_bounds("tree-sidebar-child")
            .unwrap_or_else(|| panic!("{mode}: no guide"));
        // The guide lies in the indent on the row's right, its start, past
        // the name the row reads toward the left.
        assert!(guide.left() >= name.right(), "{mode}: {guide:?} {name:?}");
        assert!(guide.right() <= sidebar.right(), "{mode}: {guide:?}");
        assert!(name.right() <= row.right(), "{mode}: {name:?} {row:?}");
        assert!(
            row.right() - name.right() > name.left() - row.left(),
            "{mode}: {name:?} leans left in {row:?}"
        );
    }
    crate::i18n::show(&herdr_i18n::Locale::FALLBACK);
}
