//! A picture in the conversation opens large on a click, and a click
//! anywhere or Escape closes it again.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use crate::chat::visual::{Shown, VisualId};
use base64::Engine as _;
use gpui::{Modifiers, TestAppContext, point, px};
use herdr_chat_protocol::{Author, Image};
use herdr_client::ConnectTarget;
use std::io::Cursor;

fn png(width: u32, height: u32) -> Image {
    let mut bytes = Vec::new();
    image::RgbaImage::from_pixel(width, height, image::Rgba([30, 120, 200, 255]))
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .unwrap();
    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
    Image::new("image/png", data).unwrap()
}

fn with_picture(image: &Image) -> ServerMessage {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Idle;
    transcript.entries = vec![
        agent(1, "Here is the screenshot."),
        Entry {
            id: EntryId(2),
            kind: EntryKind::Image {
                author: Author::Agent,
                image: image.clone(),
            },
        },
    ];
    ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(transcript),
    }
}

#[gpui::test]
fn a_clicked_picture_opens_large_and_closes(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let image = png(40, 30);
    let _stream = attach_with(&view, &program, with_picture(&image), cx);
    let id = VisualId::of(&image.data);
    wait_until(|| {
        poll(&view, &program, cx);
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            view.chats
                .panes()
                .next()
                .is_some_and(|pane| matches!(pane.prepared.visuals.shown(id), Shown::Ready(_)))
        })
    });
    poll(&view, &program, cx);
    let small = cx.debug_bounds("chat-visual").unwrap();
    assert!(cx.debug_bounds("chat-viewer").is_none());

    cx.simulate_click(small.center(), Modifiers::default());
    poll(&view, &program, cx);
    let chat = cx.debug_bounds("chat-view").unwrap();
    assert_eq!(cx.debug_bounds("chat-viewer").unwrap(), chat);
    // A small picture grows, up to twice its size, to fill the chat.
    let large = cx.debug_bounds("chat-viewer-picture").unwrap();
    assert!(
        large.size.width > small.size.width,
        "{large:?} vs {small:?}"
    );
    assert!(large.size.width <= px(80.) + px(1.), "{large:?}");

    // A click anywhere closes it, after it has faded out.
    cx.simulate_click(chat.origin + point(px(4.), px(4.)), Modifiers::default());
    cx.executor().advance_clock(crate::motion::LEAVE * 2);
    cx.run_until_parked();
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-viewer").is_none());

    // While it shows, typing never reaches the composer hidden behind it.
    cx.simulate_click(small.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-viewer").is_some());
    cx.simulate_input("hidden");
    cx.simulate_keystrokes("enter");
    poll(&view, &program, cx);
    assert_eq!(composer_text(&view, cx), "");
    assert!(cx.debug_bounds("chat-viewer").is_some());

    // Escape closes it, and the keyboard is back on the composer.
    cx.simulate_keystrokes("escape");
    cx.executor().advance_clock(crate::motion::LEAVE * 2);
    cx.run_until_parked();
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-viewer").is_none());
    cx.simulate_input("back");
    assert_eq!(composer_text(&view, cx), "back");
}

fn composer_text(
    view: &gpui::Entity<crate::HerdrWindow>,
    cx: &mut gpui::VisualTestContext,
) -> String {
    view.read_with(cx, |view, cx| {
        let pane = view.chats.panes().next().unwrap();
        let draft = pane.draft.join("\n");
        let line = pane
            .composer
            .as_ref()
            .map(|input| input.read(cx).text().to_owned())
            .unwrap_or_default();
        format!("{draft}{line}")
    })
}

#[gpui::test]
fn a_picture_in_another_pane_makes_its_pane_active(cx: &mut TestAppContext) {
    let (view, mut peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let image = png(40, 30);
    let _stream = attach_with(&view, &program, with_picture(&image), cx);
    // Input waits for the daemon's frame to match the window.
    selection::acknowledge_size(&view, cx);
    let id = VisualId::of(&image.data);
    wait_until(|| {
        poll(&view, &program, cx);
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            view.chats
                .panes()
                .next()
                .is_some_and(|pane| matches!(pane.prepared.visuals.shown(id), Shown::Ready(_)))
        })
    });
    poll(&view, &program, cx);
    // Another pane is the active one when the picture is clicked.
    let chat_pane = view.update(cx, |view, _| {
        let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        snapshot.focused_pane_id = Some("elsewhere".into());
        view.chats.panes().next().unwrap().target.pane_id.clone()
    });
    let small = cx.debug_bounds("chat-visual").unwrap();
    cx.simulate_click(small.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-viewer").is_some());
    // The clicked chat's pane becomes the active one.
    let focus = std::iter::repeat_with(|| peer.request())
        .take(8)
        .find(|request| request["method"] == herdr_client::Method::PaneFocus.as_str())
        .expect("a pane focus request");
    assert_eq!(focus["params"]["pane_id"], chat_pane.as_str());
}
