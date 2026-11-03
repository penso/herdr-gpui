use super::*;
use herdr_chat_protocol::ImageType;

const PNG: &str = "iVBORw0KGgo=";

fn image_chunk(kind: &str, mime: &str) -> Value {
    json!({"sessionUpdate": kind,
        "content": {"type": "image", "data": PNG, "mimeType": mime}})
}

#[test]
fn an_image_in_a_message_is_an_entry_between_its_text() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::default();
    for update in [
        chunk("agent_message_chunk", "before", None),
        image_chunk("agent_message_chunk", "image/png"),
        chunk("agent_message_chunk", "after", None),
        image_chunk("user_message_chunk", "image/jpeg"),
    ] {
        apply(&mut projector, &mut transcript, update);
    }
    let kinds: Vec<_> = transcript.entries.iter().map(|entry| &entry.kind).collect();
    assert!(matches!(kinds[0], EntryKind::Agent { text } if text == "before"));
    assert!(
        matches!(kinds[1], EntryKind::Image { author: Author::Agent, image }
        if image.kind == ImageType::Png && image.data == PNG)
    );
    assert!(matches!(kinds[2], EntryKind::Agent { text } if text == "after"));
    assert!(
        matches!(kinds[3], EntryKind::Image { author: Author::User, image }
        if image.kind == ImageType::Jpeg)
    );
    assert_eq!(kinds.len(), 4);
}

#[test]
fn images_clients_cannot_draw_stay_named() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::default();
    apply(
        &mut projector,
        &mut transcript,
        image_chunk("agent_message_chunk", "image/svg+xml"),
    );
    apply(
        &mut projector,
        &mut transcript,
        image_chunk("agent_thought_chunk", "image/png"),
    );
    let texts: Vec<_> = transcript
        .entries
        .iter()
        .map(|entry| entry.kind.text().unwrap())
        .collect();
    assert_eq!(texts, ["[image]", "[image]"]);
}

#[test]
fn a_tool_call_keeps_the_images_it_returned() {
    let mut projector = Projector::default();
    let mut transcript = Transcript::default();
    apply(
        &mut projector,
        &mut transcript,
        json!({"sessionUpdate": "tool_call", "toolCallId": "read", "title": "Read shot.png",
        "content": [
            {"type": "content", "content": {"type": "image", "data": PNG, "mimeType": "image/png"}},
            {"type": "content", "content": {"type": "image", "data": PNG, "mimeType": "image/bmp"}},
        ]}),
    );
    let EntryKind::Tool(tool) = &transcript.entries[0].kind else {
        panic!("not a tool call");
    };
    assert!(matches!(&tool.content[0], ToolContent::Image { image } if image.data == PNG));
    assert!(matches!(&tool.content[1], ToolContent::Text { text } if text == "[image]"));
}
