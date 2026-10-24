use super::*;
use crate::web::render::home::urlencode;

#[test]
fn raw_html_in_agent_text_is_shown_as_text() {
    let html = markdown("hello <script>alert(1)</script> <b>x</b>").into_string();
    assert!(!html.contains("<script>"));
    assert!(html.contains("&lt;script&gt;"));
    assert!(!html.contains("<b>"));
}

#[test]
fn only_safe_link_schemes_keep_their_target() {
    let html = markdown("[a](javascript:alert(1)) [b](https://example.com) [c](data:text/html,x)")
        .into_string();
    assert!(!html.contains("javascript:"));
    assert!(!html.contains("data:text"));
    assert!(html.contains(r#"href="https://example.com""#));
    assert!(html.contains("noopener"));
}

#[test]
fn ids_are_percent_encoded_for_urls() {
    assert_eq!(urlencode("w1:p 2/x"), "w1%3Ap%202%2Fx");
    assert_eq!(urlencode("plain-id_1.2~"), "plain-id_1.2~");
}

#[test]
fn images_in_agent_text_become_links_that_load_nothing() {
    let html = markdown("![pixel](https://tracker.example/p.gif?chat=1)").into_string();
    assert!(!html.contains("<img"));
    assert!(html.contains(r#"href="https://tracker.example/p.gif?chat=1""#));
    assert!(html.contains("pixel"));
}
