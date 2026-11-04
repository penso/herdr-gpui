use super::{Arc, AtomicBool, Cancel, Ordering, Unsynced};

#[test]
fn a_closed_dialogs_search_cannot_replace_its_successor() {
    let old = Arc::new(AtomicBool::new(false));
    let mut unsynced = Unsynced::Checking {
        _cancel: Cancel(old.clone()),
    };
    assert!(unsynced.is_search(&old));
    let new = Arc::new(AtomicBool::new(false));
    unsynced = Unsynced::Checking {
        _cancel: Cancel(new.clone()),
    };
    assert!(old.load(Ordering::Acquire));
    assert!(!unsynced.is_search(&old));
    assert!(unsynced.is_search(&new));
    assert!(!new.load(Ordering::Acquire));
    unsynced = Unsynced::Clean;
    assert!(!unsynced.is_search(&new));
    assert!(new.load(Ordering::Acquire));
}
