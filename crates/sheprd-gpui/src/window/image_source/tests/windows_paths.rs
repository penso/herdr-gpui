use super::*;

#[test]
fn windows_paths_keep_their_backslashes() {
    let expected = Path::new(r"C:\Users\Alice\My Pictures\shot.png");
    for text in [
        r"C:\Users\Alice\My Pictures\shot.png",
        // Explorer's "Copy as path" and Windows Terminal drops quote it.
        r#""C:\Users\Alice\My Pictures\shot.png""#,
        concat!(r"'C:\Users\Alice\My Pictures\shot.png'", "\r\n"),
        concat!(
            "\x1b[200~",
            r#""C:\Users\Alice\My Pictures\shot.png""#,
            "\x1b[201~"
        ),
    ] {
        assert!(
            matches!(from_paste(text), Some(Source::File { path, extension: "png" })
            if path == expected),
            "{text:?}"
        );
    }
    // Forward slashes and UNC shares are absolute too.
    for text in ["C:/Users/Alice/shot.png", r"\\server\share\shot.png"] {
        assert!(
            matches!(from_paste(text), Some(Source::File { path, .. })
            if path == Path::new(text)),
            "{text:?}"
        );
    }
    // A drive-relative or WSL-only path is not a file this app can read.
    for text in [
        r"C:shot.png",
        r"Users\Alice\shot.png",
        "/home/alice/shot.png",
    ] {
        assert!(from_paste(text).is_none(), "{text:?}");
    }
}
