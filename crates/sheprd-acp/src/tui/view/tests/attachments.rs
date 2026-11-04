use super::*;
use sheprd_chat_protocol::Attachment;

#[test]
fn a_dragged_file_is_attached_and_goes_with_the_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("shot.png");
    std::fs::write(&file, "x").unwrap();
    let mut view = View::default();
    view.paste(&format!("{} ", file.display()));
    assert!(view.composer.text.is_empty(), "the path is not typed");
    assert_eq!(view.attachments.len(), 1);
    // Backspace from an empty composer takes it back; paste it again.
    press(&mut view, KeyCode::Backspace);
    assert!(view.attachments.is_empty());
    view.paste(&file.display().to_string());
    typed(&mut view, "look");
    assert_eq!(
        press(&mut view, KeyCode::Enter),
        Some(Out::Command(Command::Prompt {
            text: "look".to_owned(),
            attachments: vec![Attachment {
                path: file.display().to_string()
            }],
        }))
    );
    assert!(view.attachments.is_empty());
}

#[test]
fn ordinary_text_still_pastes() {
    let mut view = View::default();
    view.paste("/not/a/real/file.txt");
    assert_eq!(view.composer.text, "/not/a/real/file.txt");
    assert!(view.attachments.is_empty());
}
