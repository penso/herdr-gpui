use super::*;
use crate::config::Theme;
use core::prelude::v1::test;
use gpui::{Modifiers, px, size};
use herdr_client::protocol::SurfaceRect;

#[gpui::test]
fn structured_message_copy_stays_below_the_last_table_cell_and_copies_exact_text(
    cx: &mut TestAppContext,
) {
    let text = "A **formatted** result.\n\n```text\nvalue = 4\n```\n\n| Input | Result |\n|:--|--:|\n| 2 | 4 |";
    for theme in ["Default", "Catppuccin Latte"] {
        for from_user in [false, true] {
            let (view, _peer, cx) = open(cx, ConnectTarget::Local);
            view.update(cx, |view, cx| {
                view.config.chat.layout = ChatLayoutMode::IMessage;
                view.theme = Theme::builtin(theme).unwrap();
                view.sidebar_width = Some(150.);
                view.config.sidebar_style.activity_bar = false;
                cx.notify();
            });
            cx.simulate_resize(size(px(650.), px(860.)));
            cx.run_until_parked();
            let program = Program::new();
            let entry = if from_user {
                Entry {
                    id: EntryId(1),
                    kind: EntryKind::User {
                        text: text.into(),
                        files: 0,
                    },
                }
            } else {
                agent(1, text)
            };
            let _stream = attach_with(&view, &program, snapshot(cursor(1, 0), vec![entry]), cx);
            // Acknowledge the narrow viewport in this mock daemon's grid.
            view.update(cx, |view, _| {
                let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
                surface.frame.width = view.options.surface_size.cols;
                surface.frame.height = view.options.surface_size.rows;
                let rect = SurfaceRect {
                    x: 0,
                    y: 0,
                    width: surface.frame.width,
                    height: surface.frame.height,
                };
                surface.panes[0].rect = rect;
                surface.panes[0].inner_rect = rect;
            });
            poll(&view, &program, cx);
            let table = cx.debug_bounds("chat-rich-table").unwrap();
            cx.simulate_mouse_move(table.center(), None, Modifiers::default());
            poll(&view, &program, cx);
            let copy = cx.debug_bounds("chat-copy-1").unwrap();
            let cell = cx.debug_bounds("chat-table-cell-1-1").unwrap();
            let bubble = cx.debug_bounds("chat-imessage-1").unwrap();
            assert!(
                copy.top() >= table.bottom() && copy.top() >= cell.bottom(),
                "{theme} user={from_user}: copy {copy:?} covers table {table:?} / final cell {cell:?}"
            );
            assert!(
                copy.left() >= bubble.left()
                    && copy.right() <= bubble.right()
                    && copy.bottom() <= bubble.bottom(),
                "{theme} user={from_user}: copy remains reachable inside its bubble"
            );
            cx.simulate_click(copy.center(), Modifiers::default());
            assert_eq!(
                cx.update(|_, cx| cx.read_from_clipboard().unwrap().text().unwrap()),
                text,
                "{theme} user={from_user}: copy preserves the original Markdown"
            );
        }
    }
}
