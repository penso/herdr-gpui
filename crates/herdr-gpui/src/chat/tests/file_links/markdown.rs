use super::*;
use crate::chat::{
    rich::{self, Block, Inline},
    visual::Visuals,
};
use pulldown_cmark::{Event, Parser, Tag};

fn destination(source: &str) -> String {
    Parser::new(source)
        .find_map(|event| match event {
            Event::Start(Tag::Link { dest_url, .. }) => Some(dest_url.into_string()),
            _ => None,
        })
        .unwrap()
}

fn rich_destination(source: &str) -> String {
    rich::parse(source, &mut Visuals::default())
        .unwrap()
        .into_iter()
        .find_map(|block| match block {
            Block::Paragraph { content, .. } => {
                content.into_iter().find_map(|inline| match inline {
                    Inline::Text(_, style) => style.link,
                    _ => None,
                })
            }
            _ => None,
        })
        .unwrap()
}

#[test]
fn windows_backslashes_are_markdown_escapes_even_inside_angle_brackets() {
    let path = r"C:\Users\runner\Temp\.tmp123\notes.md";
    assert_eq!(
        destination(&format!("[notes](<{path}>)")),
        r"C:\Users\runner\Temp.tmp123\notes.md"
    );
    let escaped = path.replace('\\', r"\\");
    assert_eq!(destination(&format!("[notes](<{escaped}>)")), path);
    #[cfg(windows)]
    {
        assert_eq!(
            rich_destination(&format!("[notes](<{path}>)")),
            r"C:\Users\runner\Temp.tmp123\notes.md"
        );
        assert_eq!(rich_destination(&format!("[notes](<{escaped}>)")), path);
    }
}

#[test]
fn document_link_fixture_round_trips_hidden_directories_and_filename_punctuation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(".hidden/notes #1 50% & café (v1).md");
    let source = document_link("Notes", &path);
    let target = rich_destination(&source);
    assert_eq!(target, url::Url::from_file_path(&path).unwrap().as_str());
    let Some(RowTarget::Path {
        path: actual,
        line: None,
        explicit: true,
    }) = file_target(&target)
    else {
        panic!("file URL was not a local document target: {target}");
    };
    assert_eq!(std::path::Path::new(&actual), path);
}
