use super::*;

const PNG: &str = "iVBORw0KGgo=";

#[test]
fn only_raster_types_clients_decode_are_images() {
    assert_eq!(ImageType::from_mime("image/png"), Some(ImageType::Png));
    assert_eq!(ImageType::from_mime("IMAGE/JPEG"), Some(ImageType::Jpeg));
    assert_eq!(ImageType::from_mime("image/jpg"), Some(ImageType::Jpeg));
    assert_eq!(
        ImageType::from_mime("image/webp; charset=binary"),
        Some(ImageType::Webp)
    );
    assert_eq!(ImageType::from_mime("image/gif"), Some(ImageType::Gif));
    for refused in ["image/svg+xml", "image/tiff", "text/plain", ""] {
        assert_eq!(ImageType::from_mime(refused), None, "{refused}");
    }
}

#[test]
fn an_image_is_bounded_base64() {
    assert!(Image::new("image/png", PNG.to_owned()).is_some());
    assert!(Image::new("image/svg+xml", PNG.to_owned()).is_none());
    assert!(Image::new("image/png", String::new()).is_none());
    assert!(Image::new("image/png", "not base64!".to_owned()).is_none());
    assert!(Image::new("image/png", "A".repeat(MAX_IMAGE_DATA)).is_some());
    assert!(Image::new("image/png", "A".repeat(MAX_IMAGE_DATA + 1)).is_none());
}

#[test]
fn images_cross_the_wire_by_name() {
    let image = Image::new("image/png", PNG.to_owned()).unwrap();
    let entry = Entry {
        id: EntryId(1),
        kind: EntryKind::Image {
            author: Author::Agent,
            image: image.clone(),
        },
    };
    let value = serde_json::to_value(&entry).unwrap();
    assert_eq!(
        value,
        serde_json::json!({"id": 1, "kind": {"type": "image", "author": "agent",
            "image": {"data": PNG, "mime_type": "image/png"}}})
    );
    assert_eq!(serde_json::from_value::<Entry>(value).unwrap(), entry);
    let content = ToolContent::Image { image };
    let value = serde_json::to_value(&content).unwrap();
    assert_eq!(value["type"], "image");
    assert_eq!(
        serde_json::from_value::<ToolContent>(value).unwrap(),
        content
    );
    assert!(
        EntryKind::Image {
            author: Author::User,
            image: Image::new("image/gif", PNG.to_owned()).unwrap(),
        }
        .text()
        .is_none()
    );
}

#[test]
fn text_cannot_be_appended_to_an_image() {
    let mut transcript = Transcript::new("Claude");
    transcript
        .apply(&Change::Push {
            entry: Entry {
                id: EntryId(1),
                kind: EntryKind::Image {
                    author: Author::Agent,
                    image: Image::new("image/png", PNG.to_owned()).unwrap(),
                },
            },
        })
        .unwrap();
    let appended = transcript.apply(&Change::Append {
        id: EntryId(1),
        text: "x".to_owned(),
    });
    assert!(matches!(appended, Err(Error::NotText(EntryId(1)))));
}
