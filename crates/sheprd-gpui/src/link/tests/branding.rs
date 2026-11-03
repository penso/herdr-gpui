use super::*;

#[test]
fn current_and_installed_links_share_the_same_strict_parser() {
    for scheme in ["sheprd", "herdr-gpui", "SHEPRD", "HERDR-GPUI"] {
        let text = format!("{scheme}://device?id=local");
        assert!(is_link(std::ffi::OsStr::new(&text)));
        let link = Link::parse(&text).unwrap();
        assert_eq!(link, Link::Device { id: "local".into() });
        assert_eq!(link.to_url(), "sheprd://device?id=local");
        assert_eq!(
            Link::parse(&format!("{scheme}://device?id=local&id=other")),
            Err(Error::DuplicateParameter("id"))
        );
        assert_eq!(
            Link::parse(&format!("{scheme}://notification?tag=herdr:local:pane")),
            Ok(Link::Notification {
                tag: "herdr:local:pane".into()
            })
        );
    }
    for scheme in ["sheprdx", "herdr-gpuix"] {
        assert!(!is_link(std::ffi::OsStr::new(&format!("{scheme}:"))));
        assert_eq!(
            Link::parse(&format!("{scheme}://device?id=local")),
            Err(Error::Scheme)
        );
    }
}
