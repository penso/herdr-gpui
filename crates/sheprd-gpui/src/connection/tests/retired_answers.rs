#![allow(clippy::unwrap_used)]
use super::*;

fn answered(id: &str) -> Option<(String, Option<crate::state::DialogResponse>)> {
    Some((id.into(), Some(Ok(serde_json::json!({"result": {}})))))
}

/// An answer read while the event reader held the mailbox is cleared at
/// the next update instead of being copied into every one after it.
#[test]
fn an_answer_read_during_contention_is_cleared_at_the_next_update() {
    let bridge = bridge();
    {
        let mut inbox = bridge.inbox.lock().unwrap();
        inbox.landing_response = answered("create");
        inbox.script_response = answered("list");
        inbox.dirty = true;
    }
    let held = bridge.inbox.lock().unwrap();
    bridge.retire_answer(Answer::Landing, "create");
    bridge.retire_answer(Answer::Script, "list");
    drop(held);
    // A newer script request took the slot before the update.
    bridge.inbox.lock().unwrap().script_response = Some(("tab".into(), None));

    let update = bridge.take_update().unwrap();
    assert!(update.landing_response.is_none());
    assert!(matches!(&update.script_response, Some((id, None)) if id == "tab"));
    assert!(bridge.retired.lock().unwrap().is_empty());
}

#[test]
fn an_uncontended_answer_is_cleared_at_once() {
    let bridge = bridge();
    bridge.inbox.lock().unwrap().editor_response = answered("split");
    bridge.retire_answer(Answer::Editor, "split");
    assert!(bridge.inbox.lock().unwrap().editor_response.is_none());
    assert!(bridge.retired.lock().unwrap().is_empty());
    // A slot that now belongs to a newer request is left alone.
    bridge.inbox.lock().unwrap().cleanup_response = answered("remove-2");
    bridge.retire_answer(Answer::Cleanup, "remove-1");
    assert!(bridge.inbox.lock().unwrap().cleanup_response.is_some());
}
