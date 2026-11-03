#![allow(clippy::unwrap_used)]

use super::*;
use herdr_chat_protocol::ToolKind;

/// The first bytes of a PNG, which is all [`sniff`] reads.
pub(crate) const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

#[test]
fn links_name_local_files_once_and_skip_the_web() {
    let cwd = Path::new("/work");
    let text = "See ![chart](out/chart.png) and ![same](out/chart.png).\n\
        ![abs](/tmp/a.png \"title\") ![web](https://example.com/b.png)\n\
        ![data](data:image/png;base64,AAAA) ![spaced](<docs/my shot.png>)\n\
        ![file](file:///tmp/c.png) [not an image](d.png)";
    assert_eq!(
        linked(text, cwd),
        [
            PathBuf::from("/work/out/chart.png"),
            PathBuf::from("/tmp/a.png"),
            PathBuf::from("/work/docs/my shot.png"),
        ]
        .into_iter()
        .chain(cfg!(unix).then(|| PathBuf::from("/tmp/c.png")))
        .take(MAX_PICTURES)
        .collect::<Vec<_>>()
    );
    let many: String = (0..10).map(|n| format!("![{n}](p{n}.png) ")).collect();
    assert_eq!(linked(&many, cwd).len(), MAX_PICTURES);
    assert!(linked("![broken](", cwd).is_empty());
}

#[test]
fn a_picture_is_read_only_when_its_bytes_match_its_name() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real.png");
    std::fs::write(&real, PNG).unwrap();
    let image = read(&real).unwrap();
    assert_eq!(image.kind, ImageType::Png);
    let named = dir.path().join("secret.png");
    std::fs::write(&named, b"not a picture at all").unwrap();
    assert!(read(&named).is_none());
    let wrong = dir.path().join("photo.jpg");
    std::fs::write(&wrong, PNG).unwrap();
    assert!(read(&wrong).is_none(), "a PNG named .jpg");
    let text = dir.path().join("notes.txt");
    std::fs::write(&text, PNG).unwrap();
    assert!(read(&text).is_none(), "not an image name");
    assert!(read(&dir.path().join("missing.png")).is_none());
}

#[test]
fn a_finished_tool_call_shows_the_pictures_it_names() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("shot.png"), PNG).unwrap();
    let mut tool = Tool {
        call_id: "read".into(),
        content: vec![ToolContent::Diff {
            new: String::new(),
            old: None,
            path: "shot.png".into(),
        }],
        kind: ToolKind::Read,
        locations: vec!["shot.png:3".into(), "src/main.rs".into()],
        parent: None,
        status: ToolStatus::InProgress,
        title: "Read shot.png".into(),
    };
    assert!(of_tool(&tool, dir.path()).is_empty(), "not finished yet");
    tool.status = ToolStatus::Completed;
    let images = of_tool(&tool, dir.path());
    assert_eq!(images.len(), 1, "named twice, shown once");
    tool.content.extend(images);
    assert!(of_tool(&tool, dir.path()).is_empty(), "it has its own");
}

#[test]
fn links_never_name_a_network_share() {
    let cwd = Path::new("/work");
    for target in [
        r"\\server\share\a.png",
        "//server/share/a.png",
        "file://server/share/a.png",
        r"\\?\UNC\server\share\a.png",
    ] {
        assert!(
            linked(&format!("![x]({target})"), cwd).is_empty(),
            "{target}"
        );
    }
    assert_eq!(
        linked("![x](file://localhost/tmp/a.png)", cwd).len(),
        usize::from(cfg!(unix))
    );
}
