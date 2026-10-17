#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn a_zoomed_tab_says_its_zoom() {
    let output = zoomed(Response::Tab {
        reply: Reply::Zoom {
            id: 4,
            percent: 150,
        },
    })
    .unwrap();
    assert_eq!(output.text, "Tab 4 is zoomed to 150%");
    assert_eq!(output.json, serde_json::json!({ "id": 4, "zoom": 150 }));
    assert!(
        zoomed(Response::Tab {
            reply: Reply::Text { text: "x".into() }
        })
        .is_err()
    );
}
