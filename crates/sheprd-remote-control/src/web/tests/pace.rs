use super::*;
use std::time::Duration;
use tokio::time::timeout;

/// Shorter than the regular interval, so only an echo read fits in it.
const SOONER: Duration = Duration::from_millis(200);

#[tokio::test]
async fn typing_into_the_pane_reads_its_screen_at_once() {
    let typed = Typed::default();
    let mut pace = typed.pace("p1");
    typed.typed("p1");
    timeout(SOONER, pace.wait())
        .await
        .expect("an echo read is due at once");
}

#[tokio::test]
async fn typing_keeps_reading_for_a_slower_redraw() {
    let typed = Typed::default();
    let mut pace = typed.pace("p1");
    typed.typed("p1");
    pace.wait().await;
    // The second and third echo reads, both before the regular one.
    timeout(SOONER, pace.wait()).await.expect("the 50 ms read");
    timeout(SOONER, pace.wait()).await.expect("the 150 ms read");
}

#[tokio::test]
async fn typing_into_another_pane_leaves_the_timer_alone() {
    let typed = Typed::default();
    let mut pace = typed.pace("p1");
    typed.typed("p2");
    assert!(timeout(SOONER, pace.wait()).await.is_err());
}

#[tokio::test]
async fn a_follower_without_a_signal_still_reads_on_the_timer() {
    let typed = Typed::default();
    let mut pace = typed.pace("p1");
    drop(typed);
    timeout(Duration::from_secs(2), pace.wait())
        .await
        .expect("the regular read");
}
