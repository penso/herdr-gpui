//! Shutdown ends the daemon even while a phone's stream stays open.

use super::*;

#[tokio::test]
async fn a_stop_ends_serving_after_the_grace_even_with_open_streams() {
    let (stop, stopped) = watch::channel(false);
    // A server whose open connection never closes.
    let serving = tokio::spawn(until_stopped(
        std::future::pending(),
        stopped,
        Duration::from_millis(50),
    ));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!serving.is_finished());
    stop.send_replace(true);
    let ended = tokio::time::timeout(Duration::from_secs(5), serving).await;
    assert!(ended.unwrap().unwrap().is_ok());
}

#[tokio::test]
async fn a_server_that_ends_first_reports_its_own_result() {
    let (_stop, stopped) = watch::channel(false);
    let failed = until_stopped(
        async { Err(std::io::Error::from(std::io::ErrorKind::AddrInUse)) },
        stopped,
        Duration::from_secs(2),
    )
    .await;
    assert_eq!(failed.unwrap_err().kind(), std::io::ErrorKind::AddrInUse);
}
