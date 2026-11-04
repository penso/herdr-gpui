use super::*;

#[test]
fn a_chat_pane_reads_as_one_line() {
    let mut chat = crate::control::protocol::chat::ChatInfo {
        pane_id: "w1:p1".into(),
        agent: "Claude".into(),
        status: "working".into(),
        attached: true,
        shown: true,
        permissions: 1,
        queued: 2,
        model: None,
        context: None,
        input_mode: crate::control::protocol::chat::InputMode::Chat,
        session_id: None,
    };
    assert_eq!(
        chat::describe(&chat),
        "w1:p1  Claude  working  chat view  1 waiting for permission  2 queued"
    );
    chat.attached = false;
    chat.shown = false;
    chat.permissions = 0;
    chat.queued = 0;
    assert_eq!(
        chat::describe(&chat),
        "w1:p1  Claude  working (reconnecting)  terminal"
    );
}
