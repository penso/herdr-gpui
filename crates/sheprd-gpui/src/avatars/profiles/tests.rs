#![allow(clippy::unwrap_used)]

use super::*;

const URL: &str = "https://avatars.githubusercontent.com/u/1";

#[test]
fn a_picture_is_loaded_once_and_kept_only_while_its_account_is_shown() {
    let mut avatars = ProfileAvatars::default();
    let loads = std::cell::Cell::new(0);
    let senders = std::cell::RefCell::new(Vec::new());
    let load = |_| {
        loads.set(loads.get() + 1);
        let (tx, rx) = mpsc::sync_channel(2);
        senders.borrow_mut().push(tx);
        rx
    };
    assert!(!avatars.poll_with([URL], load));
    assert!(avatars.get(URL).is_none());
    // A cached copy, then a fresher one, both land.
    let cached = Arc::new(Image::empty());
    senders.borrow()[0].send(cached.clone()).unwrap();
    assert!(avatars.poll_with([URL], load));
    assert!(Arc::ptr_eq(&avatars.get(URL).unwrap(), &cached));
    let fresh = Arc::new(Image::empty());
    senders.borrow()[0].send(fresh.clone()).unwrap();
    senders.borrow_mut().clear();
    assert!(avatars.poll_with([URL], load));
    assert!(Arc::ptr_eq(&avatars.get(URL).unwrap(), &fresh));
    // A finished load is not started again.
    assert!(!avatars.poll_with([URL], load));
    assert_eq!(loads.get(), 1);
    // Signing out drops the picture; signing in again loads it anew.
    assert!(avatars.poll_with([], load));
    assert!(avatars.get(URL).is_none());
    assert!(!avatars.poll_with([URL], load));
    assert_eq!(loads.get(), 2);
}

#[test]
fn a_picture_that_never_arrives_is_not_asked_for_again() {
    let mut avatars = ProfileAvatars::default();
    let loads = std::cell::Cell::new(0);
    let load = |_| {
        loads.set(loads.get() + 1);
        mpsc::sync_channel(1).1
    };
    for _ in 0..3 {
        assert!(!avatars.poll_with([URL, ""], load));
    }
    assert_eq!(loads.get(), 1);
    assert!(avatars.get(URL).is_none());
}

#[test]
fn a_late_picture_for_a_signed_out_account_is_dropped() {
    let mut avatars = ProfileAvatars::default();
    let (tx, rx) = mpsc::sync_channel(1);
    let mut rx = Some(rx);
    avatars.poll_with([URL], |_| rx.take().unwrap());
    avatars.poll_with([], |_| unreachable!());
    assert!(tx.send(Arc::new(Image::empty())).is_err());
    assert!(avatars.get(URL).is_none());
}
