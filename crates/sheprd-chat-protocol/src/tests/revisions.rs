use serde_json::json;

use super::*;

#[test]
fn a_greeting_from_before_revisions_reads_as_revision_zero() {
    let hello: ClientMessage = serde_json::from_value(json!({
        "type": "hello", "client": "gui", "since": null, "version": VERSION,
    }))
    .unwrap();
    assert_eq!(
        hello,
        ClientMessage::Hello {
            client: ClientKind::Gui,
            revision: 0,
            since: None,
            version: VERSION,
        }
    );
}
