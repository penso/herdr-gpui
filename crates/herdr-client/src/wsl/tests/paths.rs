#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use std::path::PathBuf;

fn paths(texts: &[&str]) -> Vec<PathBuf> {
    texts.iter().map(PathBuf::from).collect()
}

#[test]
fn only_absolute_windows_paths_are_sent() {
    let input = input(&paths(&[
        r"C:\Users\me\it's here.txt",
        "D:/data",
        r"\\wsl.localhost\Ubuntu\home\me",
        r"\\wsl$\Ubuntu\etc",
    ]))
    .unwrap();
    assert_eq!(
        input,
        "C:\\Users\\me\\it's here.txt\nD:/data\n\\\\wsl.localhost\\Ubuntu\\home\\me\n\\\\wsl$\\Ubuntu\\etc\n"
    );
    for bad in [
        "relative\\x",
        "/home/me",
        r"\x",
        r"\\\x",
        "C:",
        "1:\\x",
        "C:\\a\nD:\\b",
        "C:\\a\u{1b}",
        "",
    ] {
        assert!(
            matches!(input_of(bad), Err(crate::Error::Wsl(Error::PathInvalid(path))) if path == Path::new(bad)),
            "{bad:?}"
        );
    }
    let long = format!("C:\\{}", "a".repeat(PATH_BYTES));
    assert!(matches!(
        input_of(&long),
        Err(crate::Error::Wsl(Error::PathInvalid(_)))
    ));
}

fn input_of(path: &str) -> Result<String> {
    input(&[PathBuf::from(path)])
}

#[test]
fn a_drop_is_bounded_before_anything_starts() {
    let many = vec![PathBuf::from(r"C:\x"); PATH_LIMIT + 1];
    assert!(matches!(
        input(&many),
        Err(crate::Error::Wsl(Error::PathLimit))
    ));
    assert!(matches!(
        wsl_paths("Ubuntu", &many, &AtomicBool::new(false)),
        Err(crate::Error::Wsl(Error::PathLimit))
    ));
    assert!(matches!(
        wsl_paths("a;b", &paths(&[r"C:\x"]), &AtomicBool::new(false)),
        Err(crate::Error::Wsl(Error::InvalidDistro))
    ));
}

#[test]
fn every_path_needs_its_own_answer() {
    let asked = paths(&[r"C:\a b", r"\\wsl$\Ubuntu\etc"]);
    assert_eq!(
        parse(b"/mnt/c/a b\n/etc\n", &asked).unwrap(),
        ["/mnt/c/a b", "/etc"]
    );
    assert!(
        matches!(parse(b"/mnt/c/a b\n-\n", &asked), Err(crate::Error::Wsl(Error::PathUnmapped(path))) if path == asked[1])
    );
    for wrong in [
        &b"/mnt/c/a b\n"[..],
        b"/mnt/c/a b\n/etc\n/extra\n",
        b"/mnt/c/a b\nrelative\n",
        b"/mnt/c/a b\n/e\x1bc\n",
        b"/mnt/c/a b\n/\xff\n",
    ] {
        assert!(
            matches!(
                parse(wrong, &asked),
                Err(crate::Error::Wsl(Error::PathResponse))
            ),
            "{wrong:?}"
        );
    }
}

#[test]
fn an_unreachable_path_names_itself() {
    let error = crate::Error::Wsl(Error::PathUnmapped(PathBuf::from(r"\\server\share\x")));
    assert_eq!(
        error.to_string(),
        r"\\server\share\x is not reachable from this WSL distribution; only its own files and mounted drives are"
    );
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
}

#[cfg(not(windows))]
#[test]
fn mapping_needs_a_windows_client() {
    assert!(matches!(
        wsl_paths("Ubuntu", &paths(&[r"C:\x"]), &AtomicBool::new(false)),
        Err(crate::Error::Wsl(Error::Unsupported))
    ));
}

/// Maps real paths in a distribution:
/// `HERDR_TEST_WSL_DISTRO=Ubuntu-22.04 cargo test -p herdr-client
/// live_wsl_paths -- --ignored --nocapture`.
#[cfg(windows)]
#[test]
#[ignore = "starts a real WSL distribution"]
fn live_wsl_paths() {
    let distro = std::env::var("HERDR_TEST_WSL_DISTRO").expect("HERDR_TEST_WSL_DISTRO");
    let own = format!(r"\\wsl.localhost\{distro}\etc\hostname");
    let legacy = format!(r"\\wsl$\{distro}\etc");
    let found = wsl_paths(
        &distro,
        &paths(&[r"C:\Users\it's here.txt", &own, &legacy]),
        &AtomicBool::new(false),
    )
    .unwrap();
    println!("{found:?}");
    assert!(found[0].ends_with("/c/Users/it's here.txt"), "{found:?}");
    assert_eq!(found[1..], ["/etc/hostname", "/etc"]);
    for unreachable in [r"\\server\share\x", r"\\wsl.localhost\NoSuchDistro\x"] {
        let error =
            wsl_paths(&distro, &paths(&[unreachable]), &AtomicBool::new(false)).unwrap_err();
        assert!(
            matches!(&error, crate::Error::Wsl(Error::PathUnmapped(path)) if path == Path::new(unreachable)),
            "{error:?}"
        );
    }
}
