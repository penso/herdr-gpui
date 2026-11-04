//! Waiting for a new chat to be openable before a phone is sent to it.

use super::*;
use crate::hub::layout::project;
use std::sync::atomic::{AtomicBool, Ordering};

const ENDPOINT: &str = "0123456789abcdef0123456789abcdef";

fn listing_p1() -> Arc<Layout> {
    Arc::new(project(&snapshot(json!([agent(
        "p1",
        "t1",
        json!([["acp", ENDPOINT]])
    )]))))
}

#[tokio::test]
async fn a_new_chat_is_ready_once_listed_and_linked() {
    let (sender, mut layout) = watch::channel(Arc::new(Layout::default()));
    let linked = Arc::new(AtomicBool::new(false));
    let waiting = {
        let linked = linked.clone();
        tokio::spawn(async move {
            chat_listed_and_linked(&mut layout, "p1", || linked.load(Ordering::SeqCst)).await;
        })
    };
    tokio::task::yield_now().await;
    assert!(!waiting.is_finished(), "nothing is listed yet");
    sender.send(listing_p1()).unwrap();
    tokio::time::sleep(LINK_CHECK * 2).await;
    assert!(!waiting.is_finished(), "listed but not linked");
    // The link comes up without another layout change.
    linked.store(true, Ordering::SeqCst);
    tokio::time::timeout(Duration::from_secs(5), waiting)
        .await
        .expect("ready once linked")
        .unwrap();
}

#[tokio::test]
async fn another_chat_appearing_does_not_count() {
    let (sender, mut layout) = watch::channel(listing_p1());
    let wait = chat_listed_and_linked(&mut layout, "p9", || true);
    assert!(
        tokio::time::timeout(LINK_CHECK * 4, wait).await.is_err(),
        "only the new chat's pane counts"
    );
    drop(sender);
}

#[tokio::test]
async fn a_hub_whose_chat_never_appears_answers_not_ready() {
    let hub = Hub::offline(Layout::default());
    assert!(!hub.chat_ready("p1", LINK_CHECK).await);
}
