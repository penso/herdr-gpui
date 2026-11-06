use super::*;

#[test]
fn markdown_files_are_served_rendered() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("plan.md"), "# Plan\n\n<script>x</script>\n").unwrap();
    std::fs::write(dir.path().join("NOTES.MARKDOWN"), b"# caf\xff\n").unwrap();
    std::fs::write(dir.path().join("notes.txt"), "# raw").unwrap();

    let (bytes, kind) = read(dir.path(), "/plan.md").unwrap();
    let page = String::from_utf8(bytes).unwrap();
    assert_eq!(kind, "text/html; charset=utf-8");
    assert!(page.contains("<h1 id=\"user-content-plan\">Plan</h1>"));
    assert!(!page.contains("<script"));

    let (bytes, kind) = read(dir.path(), "/NOTES.MARKDOWN").unwrap();
    assert_eq!(kind, "text/html; charset=utf-8");
    assert!(
        String::from_utf8(bytes)
            .unwrap()
            .contains("<title>caf\u{fffd}</title>")
    );

    assert_eq!(
        read(dir.path(), "/notes.txt").unwrap(),
        (b"# raw".to_vec(), "text/plain; charset=utf-8")
    );
}
