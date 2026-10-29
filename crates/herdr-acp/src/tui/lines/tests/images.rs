use super::*;
use herdr_chat_protocol::Image;

fn png() -> Image {
    Image::new("image/png", "iVBORw0KGgo=".to_owned()).unwrap()
}

#[test]
fn the_terminal_names_a_picture_instead_of_drawing_it() {
    let item = Entry {
        id: EntryId(1),
        kind: EntryKind::Image {
            author: Author::Agent,
            image: png(),
        },
    };
    assert_eq!(plain(&entry(&item, 40))[0], "  [image]");
    let item = Entry {
        id: EntryId(2),
        kind: EntryKind::Image {
            author: Author::User,
            image: png(),
        },
    };
    assert_eq!(plain(&entry(&item, 40))[0], "› [image]");
    let item = Entry {
        id: EntryId(3),
        kind: EntryKind::Tool(Tool {
            call_id: "c".to_owned(),
            content: vec![ToolContent::Image { image: png() }],
            kind: ToolKind::Read,
            locations: Vec::new(),
            parent: None,
            status: ToolStatus::Completed,
            title: "Read shot.png".to_owned(),
        }),
    };
    assert_eq!(plain(&entry(&item, 40))[1], "  │ [image]");
}
