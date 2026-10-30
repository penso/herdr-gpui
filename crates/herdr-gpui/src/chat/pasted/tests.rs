use super::*;

#[test]
#[allow(clippy::unwrap_used)]
fn the_same_picture_is_saved_once_with_its_extension() {
    let image = PreparedImage {
        extension: "png",
        bytes: b"not decoded here".to_vec(),
        resized: false,
    };
    let root = tempfile::tempdir().unwrap();
    let first = write(root.path(), &image).unwrap();
    let again = write(root.path(), &image).unwrap();
    assert_eq!(first, again);
    assert_eq!(first.extension().unwrap(), "png");
    assert_eq!(std::fs::read(&first).unwrap(), image.bytes);
}
