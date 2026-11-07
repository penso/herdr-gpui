#![allow(clippy::unwrap_used)]

use super::*;

#[test]
fn a_click_opens_the_notification_link_and_text_stays_text() {
    let notification = SystemNotification {
        tag: "herdr:local:boot:w2:p1".into(),
        title: "<b>agent</b> & \"friends\"".into(),
        body: "Local · background".into(),
        actions: Vec::new(),
    };
    let xml = document(&notification)
        .unwrap()
        .GetXml()
        .unwrap()
        .to_string();
    assert!(xml.starts_with(
        r#"<toast launch="sheprd://notification?tag=herdr%3Alocal%3Aboot%3Aw2%3Ap1" activationType="protocol">"#
    ), "{xml}");
    assert!(
        xml.contains("&lt;b&gt;agent&lt;/b&gt; &amp; \"friends\""),
        "{xml}"
    );
    assert!(!xml.contains("<b>"), "{xml}");
    assert!(xml.contains(r#"<audio silent="true"/>"#), "{xml}");
    // What the shell runs on a click is a link the app accepts.
    let launch = xml.split('"').nth(1).unwrap();
    assert_eq!(
        Link::parse(launch).unwrap(),
        Link::Notification {
            tag: "herdr:local:boot:w2:p1".into()
        }
    );
}

#[test]
fn tags_fit_windows_limit_and_stay_stable() {
    let long = "herdr:".repeat(100);
    assert_eq!(short_tag(&long).len(), 16);
    assert_eq!(short_tag("herdr:a"), short_tag("herdr:a"));
    assert_ne!(short_tag("herdr:a"), short_tag("herdr:b"));
}
