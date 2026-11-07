use super::*;

#[test]
fn outside_home_and_traversal_are_refused() {
    for cwd in [
        "/outside",
        "/home/user-other",
        "/home/user/../other",
        "/home/user/notes/../../other",
    ] {
        let error = relative_cwd("/home/user", cwd).unwrap_err();
        assert!(matches!(error, Error::OutsideHome { .. }));
        assert!(error.to_string().contains("outside"));
    }
    assert_eq!(
        relative_cwd("/home/user/", "/home/user/notes").unwrap(),
        "notes"
    );
}
