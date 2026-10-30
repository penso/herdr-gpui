//! Files attached to a prompt: dropped or picked files show as chips in
//! the composer, can be taken back, and go out with the prompt.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use gpui::{Modifiers, TestAppContext};
use herdr_chat_protocol::{Attachment, Command, MAX_ATTACHMENTS};
use herdr_client::ConnectTarget;
use std::path::PathBuf;

const PANE: &str = "p_1";

fn idle() -> ServerMessage {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Idle;
    transcript.entries = vec![agent(1, "Hi.")];
    ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(transcript),
    }
}

fn next_command(stream: &mut Peer) -> Command {
    match read(stream) {
        Some(ClientMessage::Command { command, .. }) => command,
        other => panic!("a command, got {other:?}"),
    }
}

fn pane_id(view: &gpui::Entity<crate::HerdrWindow>, cx: &mut gpui::VisualTestContext) -> String {
    view.read_with(cx, |view, _| {
        view.chats
            .panes()
            .next()
            .map(|pane| pane.target.pane_id.clone())
            .unwrap_or_else(|| PANE.into())
    })
}

#[gpui::test]
fn files_show_as_chips_and_go_with_the_prompt(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, idle(), cx);
    let pane = pane_id(&view, cx);
    let files = vec![
        PathBuf::from("/work/notes.md"),
        PathBuf::from("/work/shot.png"),
    ];
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.attach_chat_files(&pane, files.clone(), window, cx);
            // The same file twice is one attachment.
            view.attach_chat_files(&pane, vec![files[0].clone()], window, cx);
        });
    });
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-attachment-0").is_some());
    assert!(cx.debug_bounds("chat-attachment-1").is_some());
    assert!(cx.debug_bounds("chat-attachment-2").is_none());

    // Taking the first back leaves the picture.
    let detach = cx.debug_bounds("chat-detach-0").unwrap();
    cx.simulate_click(detach.center(), Modifiers::default());
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-attachment-1").is_none());

    // Files alone make a prompt; sending clears them.
    let send = cx.debug_bounds("chat-send").unwrap();
    cx.simulate_click(send.center(), Modifiers::default());
    assert_eq!(
        next_command(&mut stream),
        Command::Prompt {
            text: String::new(),
            attachments: vec![Attachment {
                path: "/work/shot.png".into()
            }],
        }
    );
    poll(&view, &program, cx);
    assert!(cx.debug_bounds("chat-attachments").is_none());
}

#[gpui::test]
fn a_prompt_takes_a_bounded_number_of_files(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, idle(), cx);
    let pane = pane_id(&view, cx);
    let files: Vec<_> = (0..MAX_ATTACHMENTS + 3)
        .map(|n| PathBuf::from(format!("/work/{n}.txt")))
        .collect();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.attach_chat_files(&pane, files, window, cx)
        });
    });
    let (count, error) = view.read_with(cx, |view, _| {
        let pane = view.chats.panes().next().unwrap();
        (pane.attachments.len(), pane.error.clone())
    });
    assert_eq!(count, MAX_ATTACHMENTS);
    assert!(error.is_some());
}

#[gpui::test]
fn an_agent_with_its_own_bypass_mode_has_no_shield(cx: &mut TestAppContext) {
    use herdr_chat_protocol::{Choice, ConfigOption, OptionCategory, OptionKind};
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Idle;
    transcript.options = vec![ConfigOption {
        category: OptionCategory::Mode,
        description: None,
        id: "mode".into(),
        kind: OptionKind::Select {
            choices: ["default", "acceptEdits", "plan", "bypassPermissions"]
                .into_iter()
                .map(|id| Choice {
                    description: None,
                    group: None,
                    id: id.into(),
                    name: id.into(),
                })
                .collect(),
            current: "default".into(),
        },
        name: "Mode".into(),
    }];
    let snapshot = ServerMessage::Snapshot {
        cursor: cursor(1, 0),
        transcript: Box::new(transcript),
    };
    let _stream = attach_with(&view, &program, snapshot, cx);
    assert!(cx.debug_bounds("chat-setting-mode").is_some());
    assert!(cx.debug_bounds("chat-auto-accept").is_none());
}

#[gpui::test]
fn files_dropped_on_the_chat_become_attachments(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, idle(), cx);
    // The chat covers the terminal's own drop handler, so the drop has to
    // reach the chat through real event dispatch, not a direct call.
    let at = cx.debug_bounds("chat-view").unwrap().center();
    let shot = PathBuf::from("/work/shot.png");
    cx.simulate_event(gpui::FileDropEvent::Entered {
        position: at,
        paths: gpui::ExternalPaths(std::iter::once(shot.clone()).collect()),
    });
    cx.simulate_event(gpui::FileDropEvent::Submit { position: at });
    poll(&view, &program, cx);
    let attachments = view.read_with(cx, |view, _| {
        view.chats.panes().next().unwrap().attachments.clone()
    });
    assert_eq!(attachments, vec![shot]);
    assert!(cx.debug_bounds("chat-attachment-0").is_some());
}

fn paste_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "cmd-v"
    } else {
        "ctrl-shift-v"
    }
}

fn attachments(
    view: &gpui::Entity<crate::HerdrWindow>,
    cx: &mut gpui::VisualTestContext,
) -> Vec<PathBuf> {
    view.read_with(cx, |view, _| {
        view.chats.panes().next().unwrap().attachments.clone()
    })
}

#[gpui::test]
fn copied_files_paste_as_attachments_not_their_names(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let mut stream = attach_with(&view, &program, idle(), cx);
    let shot = PathBuf::from("/work/shot.png");
    cx.write_to_clipboard(gpui::ClipboardItem {
        entries: vec![
            gpui::ClipboardEntry::ExternalPaths(gpui::ExternalPaths(
                std::iter::once(shot.clone()).collect(),
            )),
            gpui::ClipboardEntry::String(gpui::ClipboardString::new("shot.png".into())),
        ],
    });
    cx.simulate_keystrokes(paste_key());
    poll(&view, &program, cx);
    assert_eq!(attachments(&view, cx), vec![shot.clone()]);
    let send = cx.debug_bounds("chat-send").unwrap();
    cx.simulate_click(send.center(), Modifiers::default());
    assert_eq!(
        next_command(&mut stream),
        Command::Prompt {
            text: String::new(),
            attachments: vec![Attachment {
                path: "/work/shot.png".into()
            }],
        }
    );
}

#[gpui::test]
fn a_copied_screenshot_pastes_as_an_attached_file(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let _stream = attach_with(&view, &program, idle(), cx);
    // PNG bytes within the upload limit pass through undecoded; unique bytes
    // keep this test's file apart from any other run's.
    let bytes = format!("screenshot {:?}", std::time::SystemTime::now()).into_bytes();
    cx.write_to_clipboard(gpui::ClipboardItem::new_image(&gpui::Image::from_bytes(
        gpui::ImageFormat::Png,
        bytes.clone(),
    )));
    cx.simulate_keystrokes(paste_key());
    cx.run_until_parked();
    poll(&view, &program, cx);
    let attached = attachments(&view, cx);
    let [file] = attached.as_slice() else {
        panic!("one attachment, got {attached:?}");
    };
    assert_eq!(file.extension().unwrap(), "png");
    assert_eq!(std::fs::read(file).unwrap(), bytes);
    assert!(cx.debug_bounds("chat-attachment-0").is_some());
    std::fs::remove_file(file).unwrap();
}
