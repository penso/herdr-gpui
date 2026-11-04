use super::*;
use crate::config::ChatLayoutMode;

#[gpui::test]
fn user_and_agent_messages_render_at_the_configured_terminal_size(cx: &mut TestAppContext) {
    for kind in [
        EntryKind::User {
            text: "A short message.".into(),
            files: 0,
        },
        EntryKind::Agent {
            text: "A short message.".into(),
        },
    ] {
        let (view, _peer, cx) = open(cx, ConnectTarget::Local);
        view.update(cx, |view, _| {
            view.config.chat.layout = ChatLayoutMode::Terminal
        });
        let program = Program::new();
        let _stream = attach_with(
            &view,
            &program,
            snapshot(
                cursor(1, 0),
                vec![Entry {
                    id: EntryId(1),
                    kind,
                }],
            ),
            cx,
        );
        for font_size in [12., 14., 18.] {
            view.update(cx, |view, _| view.config.terminal.size = font_size);
            poll(&view, &program, cx);
            let prose = cx.debug_bounds("chat-prose").unwrap();
            assert!(
                (prose.size.height - px(font_size * 1.6)).abs() < px(1.),
                "a single message line should use {font_size}px text: {prose:?}"
            );
        }
    }
}
