#![allow(clippy::unwrap_used)]
use super::*;

/// `rest` under this platform's root, so the path is absolute here.
fn absolute(rest: &str) -> String {
    if cfg!(windows) {
        format!(r"C:\{rest}")
    } else {
        format!("/{rest}")
    }
}

#[test]
fn files_are_absolute_printable_paths_at_lines_from_one() {
    let main = absolute("src/main.rs");
    let target = editor_target(&main, Some(42)).unwrap();
    assert_eq!(target.path, Path::new(&main));
    assert_eq!(target.line, Some(42));
    assert_eq!(editor_target(&main, None).unwrap().line, None);
    for (path, line) in [
        (String::new(), None),
        ("src/main.rs".into(), None),
        (absolute("src/ma\u{1b}in.rs"), None),
        (main.clone(), Some(0)),
    ] {
        assert!(
            matches!(
                editor_target(&path, line),
                Err(Response::Error {
                    code: ErrorCode::InvalidRequest,
                    ..
                })
            ),
            "{path:?} {line:?}"
        );
    }
    let long = absolute(&"a".repeat(MAX_PATH));
    assert!(editor_target(&long, None).is_err());
}
