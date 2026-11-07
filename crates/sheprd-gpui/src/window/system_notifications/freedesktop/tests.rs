use super::super::{Posted, REGISTRY_LIMIT, SheprdWindow};
use super::*;
use gpui::{WindowHandle, WindowId};
use std::{sync::Weak, time::Instant};

pub(super) fn posted(tag: &str) -> Posted {
    Posted {
        tag: tag.to_owned().into(),
        window: WindowHandle::<SheprdWindow>::new(WindowId::from(1)),
        endpoint: "local".into(),
        generation: 0,
        inbox: Weak::new(),
        id: 0,
        at: Instant::now(),
        bus_id: None,
    }
}

fn signal(body: &(impl serde::Serialize + zbus::zvariant::DynamicType)) -> Message {
    Message::signal(PATH, NAME, "ActionInvoked")
        .and_then(|builder| builder.build(body))
        .unwrap_or_else(|error| panic!("signal: {error}"))
}

#[test]
fn a_click_resolves_to_the_tag_its_reply_named() {
    let mut registry = Registry::default();
    registry.record(posted("a"));
    registry.record(posted("b"));
    registry.set_bus_id("a", 7);
    registry.set_bus_id("b", 8);
    assert_eq!(registry.find_bus_id(7).map(|p| p.tag.as_ref()), Some("a"));
    assert_eq!(registry.find_bus_id(8).map(|p| p.tag.as_ref()), Some("b"));
    // Another app's notification.
    assert!(registry.find_bus_id(9).is_none());
}

#[test]
fn a_repost_keeps_the_shown_notification_id_until_its_reply() {
    let mut registry = Registry::default();
    registry.record(posted("a"));
    registry.set_bus_id("a", 7);
    // What the next post for the tag passes as `replaces`.
    assert_eq!(registry.find("a").and_then(|p| p.bus_id), Some(7));
    registry.record(posted("a"));
    assert_eq!(registry.find_bus_id(7).map(|p| p.tag.as_ref()), Some("a"));
    registry.set_bus_id("a", 8);
    assert!(registry.find_bus_id(7).is_none());
    assert_eq!(registry.find_bus_id(8).map(|p| p.tag.as_ref()), Some("a"));
}

#[test]
fn evicted_posts_route_no_clicks() {
    let mut registry = Registry::default();
    registry.record(posted("first"));
    registry.set_bus_id("first", 1);
    for index in 0..REGISTRY_LIMIT {
        registry.record(posted(&index.to_string()));
    }
    assert_eq!(registry.posted.len(), REGISTRY_LIMIT);
    assert!(registry.find_bus_id(1).is_none());
}

#[test]
fn only_a_body_click_counts() {
    assert_eq!(clicked(&signal(&(7_u32, DEFAULT_ACTION))), Some(7));
    assert_eq!(clicked(&signal(&(7_u32, "reply"))), None);
    assert_eq!(clicked(&signal(&(7_u32,))), None);
}

#[test]
fn markup_servers_get_the_body_as_plain_text() {
    let body = "<img src=\"file:///etc/passwd\"> & <a href=\"x\">y</a>";
    assert_eq!(
        plain(body, true),
        "&lt;img src=\"file:///etc/passwd\"&gt; &amp; &lt;a href=\"x\"&gt;y&lt;/a&gt;"
    );
    assert_eq!(plain("&lt;", true), "&amp;lt;");
    assert_eq!(plain(body, false), body);
    assert!(matches!(plain("plain text", true), Cow::Borrowed(_)));
}
