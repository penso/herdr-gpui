use super::*;
use herdr_chat_protocol::{Attachment, MAX_ATTACHMENTS};

fn attach(path: &std::path::Path) -> Attachment {
    Attachment {
        path: path.display().to_string(),
    }
}

#[test]
fn files_go_with_the_prompt_and_show_in_the_transcript() {
    let dir = tempfile::tempdir().unwrap();
    let notes = dir.path().join("notes.md");
    std::fs::write(&notes, "# Notes\n").unwrap();
    let shot = dir.path().join("shot.png");
    std::fs::write(&shot, b"\x89PNG fake").unwrap();
    let mut harness = ready();
    harness.session.accepts = crate::files::Accepts {
        embedded: true,
        image: true,
    };
    harness.command(
        0,
        Command::Prompt {
            text: "look".to_owned(),
            attachments: vec![attach(&notes), attach(&shot)],
        },
    );
    let prompt = &harness.last_request("session/prompt")["params"]["prompt"];
    let kinds: Vec<_> = prompt
        .as_array()
        .unwrap()
        .iter()
        .map(|block| block["type"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(kinds, ["text", "resource", "image"]);
    let entries = &harness.transcript().entries;
    assert!(matches!(&entries[0].kind,
        EntryKind::User { text, files: 2 } if text == "look\n\u{1f4ce} notes.md\n\u{1f4ce} shot.png"));
    assert!(matches!(&entries[1].kind, EntryKind::Image { .. }));
}

#[test]
fn files_alone_make_a_prompt_and_go_as_links_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("spec.pdf");
    std::fs::write(&file, b"%PDF").unwrap();
    let mut harness = ready();
    harness.command(
        0,
        Command::Prompt {
            text: String::new(),
            attachments: vec![attach(&file)],
        },
    );
    let prompt = &harness.last_request("session/prompt")["params"]["prompt"];
    assert_eq!(prompt[0]["type"], "resource_link");
    assert_eq!(prompt[0]["name"], "spec.pdf");
}

#[test]
fn missing_or_too_many_files_are_refused() {
    let mut harness = ready();
    harness.command(
        0,
        Command::Prompt {
            text: "x".to_owned(),
            attachments: vec![Attachment {
                path: "/no/such/file".into(),
            }],
        },
    );
    assert_eq!(
        harness.session.handled.back().unwrap().1,
        Some(Rejection::UnknownFile)
    );
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.txt");
    std::fs::write(&file, "a").unwrap();
    harness.command(
        0,
        Command::Prompt {
            text: "x".to_owned(),
            attachments: vec![attach(&file); MAX_ATTACHMENTS + 1],
        },
    );
    assert_eq!(
        harness.session.handled.back().unwrap().1,
        Some(Rejection::TooManyAttachments)
    );
}
