use super::*;

/// The first row of `text` as an explicit hyperlink to `target`.
fn linked(text: &str, target: &str) -> PaneSurfaceFrame {
    let mut s = surface(text);
    s.frame.hyperlinks = vec![target.into()];
    for cell in &mut s.frame.cells[..text.chars().count()] {
        cell.hyperlink = Some(0);
    }
    s
}

fn target(surface: &PaneSurfaceFrame) -> Option<RowTarget> {
    pane_link_at(surface, 11., 1., 10., 20.).map(|link| link.link.target)
}

#[test]
fn an_external_address_has_a_scheme_other_than_the_webs_and_file() {
    for valid in [
        "mailto:name@example.com",
        "ssh://example.com",
        "vscode://file/tmp",
        "tel:+15550100",
    ] {
        assert!(ExternalUrl::try_from(valid).is_ok(), "{valid}");
    }
    let long = format!("mailto:{}@example.com", "a".repeat(8192));
    for invalid in [
        "http://example.com/",
        "https://example.com/",
        "file:///etc/passwd",
        "/etc/passwd",
        "~/notes.md",
        "../notes.md",
        "not a url",
        "mailto:name@example.com\n",
        "mailto:name @example.com",
        "",
        long.as_str(),
    ] {
        assert!(
            matches!(
                ExternalUrl::try_from(invalid),
                Err(crate::Error::InvalidExternalUrl)
            ),
            "{invalid}"
        );
    }
}

#[test]
fn only_mail_and_editor_addresses_open_without_asking() {
    for (url, trusted) in [
        ("mailto:name@example.com", true),
        ("vscode://file/tmp", true),
        ("ssh://example.com", false),
        ("tel:+15550100", false),
        ("javascript:alert(1)", false),
        ("vscode-insiders://file/tmp", false),
    ] {
        assert_eq!(
            ExternalUrl::try_from(url).unwrap().trusted(),
            trusted,
            "{url}"
        );
    }
}

#[test]
fn an_explicit_hyperlink_may_name_an_external_address() {
    let mail = linked("write to us", "mailto:name@example.com");
    assert_eq!(
        target(&mail),
        Some(RowTarget::External(
            ExternalUrl::try_from("mailto:name@example.com").unwrap()
        ))
    );
    // It is not a web address, which is all a popup or a plain click reads.
    assert_eq!(link_at(&mail, 11., 1., 10., 20.), None);
    // A web address this client would not open is no external one either.
    let long = format!("https://example.com/{}", "a".repeat(8192));
    assert_eq!(
        target(&linked("write to us", &long)),
        Some(RowTarget::Malformed)
    );
}

#[test]
fn plain_text_reads_only_the_defined_schemes() {
    let external = |url: &str| RowTarget::External(ExternalUrl::try_from(url).unwrap());
    for (text, expected) in [
        (
            "mailto:name@example.com",
            Some(external("mailto:name@example.com")),
        ),
        (
            "vscode://file/tmp/a.rs",
            Some(external("vscode://file/tmp/a.rs")),
        ),
        // Prose punctuation around an address is not part of it.
        (
            "(mailto:name@example.com).",
            Some(external("mailto:name@example.com")),
        ),
        // A scheme this client does not define is plain text there.
        ("ssh://example.com", None),
        ("tel:+15550100", None),
        ("vscode-insiders://file/tmp/a.rs", None),
        // So is a defined one that sits inside a word.
        ("xmailto:name@example.com", None),
    ] {
        assert_eq!(target(&surface(text)), expected, "{text}");
    }
    // A scheme with nothing after it names no address.
    assert!(!matches!(
        target(&surface("mailto:")),
        Some(RowTarget::External(_))
    ));
}

// Windows file URLs name drive paths, which these Unix fixtures do not.
#[cfg(unix)]
#[test]
fn a_plain_file_address_is_a_path_to_look_for() {
    assert_eq!(
        target(&surface("file:///tmp/a%20b.rs")),
        Some(RowTarget::Path {
            path: "/tmp/a b.rs".into(),
            line: None,
            query: None,
            explicit: false,
        })
    );
    // Another host's file is nothing this machine can look for.
    assert_eq!(target(&surface("file://elsewhere/tmp/a.rs")), None);
}
