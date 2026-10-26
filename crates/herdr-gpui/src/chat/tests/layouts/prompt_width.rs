use super::*;
use gpui::{px, size};
use herdr_client::protocol::SurfaceRect;

#[gpui::test]
fn rich_user_prompts_use_the_narrow_column_without_inflating_short_bubbles(
    cx: &mut TestAppContext,
) {
    let caption = "Show the result with equations, clear formatting, and controls I can explore.";
    for text in [caption, "Hi."] {
        let (view, _peer, cx) = open(cx, ConnectTarget::Local);
        view.update(cx, |view, cx| {
            view.sidebar_width = Some(150.);
            view.config.sidebar_style.activity_bar = false;
            cx.notify();
        });
        cx.simulate_resize(size(px(650.), px(860.)));
        cx.run_until_parked();
        let program = Program::new();
        let _stream = attach_with(
            &view,
            &program,
            snapshot(
                cursor(1, 0),
                vec![Entry {
                    id: EntryId(1),
                    kind: EntryKind::User {
                        text: text.into(),
                        files: 0,
                    },
                }],
            ),
            cx,
        );
        // The mock daemon has no resize loop; acknowledge this actual narrow
        // viewport so the chat cannot silently lay out in a wider old frame.
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
        for &mode in ChatLayoutMode::ALL {
            switch(&view, mode, cx);
            poll(&view, &program, cx);
            let chat = cx.debug_bounds("chat-view").unwrap();
            let prose = cx.debug_bounds("chat-prose").unwrap();
            assert!(
                chat.size.width >= px(440.) && chat.size.width <= px(500.),
                "{mode:?}: {chat:?}"
            );
            assert!(
                prose.left() >= chat.left() && prose.right() <= chat.right(),
                "{mode:?}: {prose:?}"
            );
            if text == caption {
                // iMessage reserves a quarter of the column beside each bubble.
                let minimum_width = if mode == ChatLayoutMode::IMessage {
                    260.
                } else {
                    300.
                };
                assert!(
                    prose.size.width >= px(minimum_width),
                    "{mode:?}: caption collapsed to {prose:?}"
                );
                assert!(
                    prose.size.height <= px(90.),
                    "{mode:?}: caption wraps one word per line: {prose:?}"
                );
            } else if matches!(mode, ChatLayoutMode::Bubbles | ChatLayoutMode::IMessage) {
                let mark = if mode == ChatLayoutMode::IMessage {
                    "chat-imessage-1"
                } else {
                    "chat-bubble-1"
                };
                let bubble = cx.debug_bounds(mark).unwrap();
                assert!(
                    bubble.size.width < px(160.),
                    "short prompt should stay compact: {bubble:?}"
                );
                assert!(prose.size.height < px(40.));
            }
        }
    }
}
