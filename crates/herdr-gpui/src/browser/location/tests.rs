#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn only_preview_frames_count_as_local() {
    for local in [
        "herdr-preview://localhost/index.html",
        "http://herdr-preview.localhost/docs/a.html",
        "https://herdr-preview.localhost/",
    ] {
        assert_eq!(PostedFrom::of(local), PostedFrom::Preview, "{local}");
    }
    for other in [
        "https://evil.test/",
        "https://herdr-preview.localhost.evil.test/",
        "http://evil.test/?u=herdr-preview://localhost/",
        "about:srcdoc",
        "about:blank",
        "",
    ] {
        assert_eq!(PostedFrom::of(other), PostedFrom::Elsewhere, "{other}");
    }
}

#[cfg(unix)]
fn file(path: &str) -> crate::Result<LocalFile> {
    LocalFile::new(Path::new(path), Some(Path::new("/Users/me")))
}

// Windows paths need a drive to be absolute.
#[cfg(unix)]
#[test]
fn local_pages_are_served_from_a_private_folder() {
    let page = file("/Users/me/project/design/mockup v2.html").unwrap();
    assert_eq!(page.root(), Path::new("/Users/me/project/design"));
    let location = Location::Local { file: page };
    assert_eq!(
        location.page_url(),
        "herdr-preview://localhost/mockup%20v2.html"
    );
    assert_eq!(location.default_title(), "mockup v2.html");
    assert_eq!(
        location.display(),
        "/Users/me/project/design/mockup v2.html"
    );
    for refused in [
        "relative/page.html",
        "/Users/me/page.html",
        "/Users/page.html",
        "/page.html",
        "/Users/me/.secret/page.html",
        "/Users/me/project/.env",
        "/Users/me/project/../page.html",
    ] {
        assert!(file(refused).is_err(), "{refused}");
    }
}

#[test]
fn requests_stay_inside_the_folder_and_skip_hidden_names() {
    let root = Path::new("/srv/site");
    assert_eq!(requested_file(root, "/").unwrap(), root.join("index.html"));
    assert_eq!(
        requested_file(root, "/assets/app%20x.css").unwrap(),
        root.join("assets").join("app x.css")
    );
    for refused in [
        "/../etc/passwd",
        "/%2e%2e/x",
        "/.git/config",
        "/a/../../b",
        "/a%2fb",
        "/a//b",
    ] {
        assert!(requested_file(root, refused).is_none(), "{refused}");
    }
}

// Windows paths need a drive to be absolute.
#[cfg(unix)]
#[test]
fn navigation_within_a_local_folder_keeps_its_root() {
    let page = Location::Local {
        file: file("/Users/me/site/index.html").unwrap(),
    };
    for origin in PREVIEW_ORIGINS {
        let next = page.visited(&format!("{origin}docs/b.html#top")).unwrap();
        assert_eq!(next.display(), "/Users/me/site/docs/b.html");
    }
    assert!(
        page.visited("herdr-preview://localhost/.git/config")
            .is_none()
    );
    let web = page.visited("https://a.test/").unwrap();
    assert!(matches!(web, Location::Web { .. }));
    // A web page cannot claim a local file.
    let web = Location::Web {
        url: WebUrl::try_from("https://a.test/").unwrap(),
    };
    assert!(web.visited("herdr-preview://localhost/x.html").is_none());
}

// Windows paths need a drive to be absolute.
#[cfg(unix)]
#[test]
fn saved_local_files_are_checked_again() {
    let saved = r#"{"kind":"local","file":{"root":"/","entry":"etc"}}"#;
    assert!(serde_json::from_str::<Location>(saved).is_err());
    let saved = r#"{"kind":"local","file":{"root":"/srv/site","entry":"index.html"}}"#;
    assert!(serde_json::from_str::<Location>(saved).is_ok());
}
