//! A pasted picture is pending until it is saved, so a prompt sent
//! meanwhile waits for it, and it joins only the conversation it was
//! pasted into.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use gpui::TestAppContext;
use herdr_client::ConnectTarget;

fn idle() -> ServerMessage {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Idle;
    transcript.entries = vec![agent(1, "Hi.")];
    ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(transcript),
    }
}

fn picture() -> gpui::Image {
    let bytes = format!("pasted {:?}", std::time::SystemTime::now()).into_bytes();
    gpui::Image::from_bytes(gpui::ImageFormat::Png, bytes)
}

fn pane_state(
    view: &gpui::Entity<crate::HerdrWindow>,
    cx: &mut gpui::VisualTestContext,
) -> (usize, Vec<PathBuf>) {
    view.read_with(cx, |view, _| {
        let pane = view.chats.panes().next().unwrap();
        (pane.uploading, pane.attachments.clone())
    })
}

#[gpui::test]
fn a_picture_is_pending_until_it_is_saved(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, idle(), cx);
    let pane = view.read_with(cx, |view, _| {
        view.chats.panes().next().unwrap().target.pane_id.clone()
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.attach_chat_image(&pane, picture(), window, cx)
        });
    });
    // Before the save lands, a prompt would wait for it.
    let (pending, attached) = pane_state(&view, cx);
    assert_eq!(pending, 1);
    assert!(attached.is_empty());
    cx.run_until_parked();
    poll(&view, &program, cx);
    let (pending, attached) = pane_state(&view, cx);
    assert_eq!(pending, 0);
    let [file] = attached.as_slice() else {
        panic!("one attachment, got {attached:?}");
    };
    std::fs::remove_file(file).unwrap();
}

#[gpui::test]
fn a_picture_never_joins_a_conversation_that_replaced_its_own(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, idle(), cx);
    let pane = view.read_with(cx, |view, _| {
        view.chats.panes().next().unwrap().target.pane_id.clone()
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.attach_chat_image(&pane, picture(), window, cx)
        });
    });
    // Another program takes the pane while the picture is saved.
    view.update(cx, |view, _| {
        let pane = view.chats.pane_mut(&pane).unwrap();
        pane.target.endpoint = EndpointId::random();
    });
    cx.run_until_parked();
    let (_, attached) = pane_state(&view, cx);
    assert!(attached.is_empty(), "{attached:?}");
}
