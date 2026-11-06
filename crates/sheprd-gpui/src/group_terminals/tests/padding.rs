use super::*;
use crate::{
    config::{Padding, Sides},
    terminal::viewport,
};
use gpui::px;

/// A second tab group touches no sidebar, and its terminal is drawn through
/// a parked connection. It takes the same padding, and that connection is
/// sized to what is left.
#[gpui::test]
fn a_parked_group_takes_the_terminal_padding_too(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.terminal_padding = Padding {
                x: Sides::new(16., 8.).unwrap(),
                y: Sides::new(4., 12.).unwrap(),
            };
            view.command(Command::SplitEditor, window, cx)
        })
    });
    draw(cx);
    let [left, right] = view.read_with(cx, |view, _| {
        let slots = view.group_slots();
        [slots[0].id, slots[1].id]
    });
    let peer = MockPeer::new();
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            terminals_side_by_side(view, left, right);
            let mut on_t1 = snapshot(40);
            on_t1.focused_workspace_id = Some("w0".into());
            on_t1.focused_tab_id = Some("t1".into());
            let key = view.browser_key().unwrap();
            let target = view.endpoints[0].connection.target.clone();
            let parked = parked(left, key, target, &peer, on_t1);
            view.browser.terminals.parked.push(parked);
        })
    });
    draw(cx);

    let inset = |outer: Bounds<Pixels>| {
        Bounds::new(
            outer.origin + point(px(16.), px(4.)),
            size(
                outer.size.width - px(16. + 8.),
                outer.size.height - px(4. + 12.),
            ),
        )
    };
    // The window's own terminal, in the group away from the sidebar.
    let terminal = cx.debug_bounds("terminal").unwrap();
    let outer = cx.debug_bounds("parked-terminal").unwrap();
    assert!(outer.right() <= terminal.left());
    view.read_with(cx, |view, _| {
        assert_eq!(view.bounds, inset(terminal));
        let parked = &view.browser.terminals.parked[0];
        assert_eq!(parked.bounds, inset(outer));
        assert_eq!(
            parked.options.surface_size,
            viewport(
                f32::from(outer.size.width) - 24.,
                f32::from(outer.size.height) - 16.,
                view.cell_width,
                view.config.terminal.line_height(),
            )
        );
    });
}
