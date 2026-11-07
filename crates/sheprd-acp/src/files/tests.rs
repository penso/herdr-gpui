#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

fn attachment(path: &Path) -> Attachment {
    Attachment {
        path: path.display().to_string(),
    }
}

const ALL: Accepts = Accepts {
    embedded: true,
    image: true,
};

#[test]
fn a_picture_goes_as_an_image_when_the_agent_takes_images() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shot.PNG");
    std::fs::write(&path, b"\x89PNG\r\n\x1a\nfake").unwrap();
    let prepared = prepare(&attachment(&path), ALL);
    assert!(matches!(prepared.block, wire::ContentBlock::Image(_)));
    assert_eq!(prepared.image.unwrap().kind, ImageType::Png);
    // Without image support it is a link.
    let linked = prepare(&attachment(&path), Accepts::default());
    assert!(matches!(linked.block, wire::ContentBlock::ResourceLink(_)));
    assert!(linked.image.is_none());
}

#[test]
fn text_is_embedded_and_binary_or_large_files_are_linked() {
    let dir = tempfile::tempdir().unwrap();
    let text = dir.path().join("notes.md");
    std::fs::write(&text, "# Notes\n").unwrap();
    let wire::ContentBlock::Resource(resource) = prepare(&attachment(&text), ALL).block else {
        panic!("not embedded");
    };
    let wire::EmbeddedResourceResource::TextResourceContents(contents) = resource.resource else {
        panic!("not text");
    };
    assert_eq!(contents.text, "# Notes\n");
    assert!(contents.uri.starts_with("file://"));

    let binary = dir.path().join("blob.bin");
    std::fs::write(&binary, b"a\0b").unwrap();
    assert!(matches!(
        prepare(&attachment(&binary), ALL).block,
        wire::ContentBlock::ResourceLink(_)
    ));
    let large = dir.path().join("big.txt");
    std::fs::write(&large, "x".repeat(MAX_TEXT as usize + 1)).unwrap();
    let wire::ContentBlock::ResourceLink(link) = prepare(&attachment(&large), ALL).block else {
        panic!("not linked");
    };
    assert_eq!(link.name, "big.txt");
}

#[test]
fn only_absolute_paths_to_files_are_readable() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.txt");
    std::fs::write(&file, "a").unwrap();
    assert!(readable(&attachment(&file)));
    assert!(!readable(&attachment(dir.path())));
    assert!(!readable(&Attachment {
        path: "relative.txt".into()
    }));
    assert!(!readable(&attachment(&dir.path().join("missing"))));
}
