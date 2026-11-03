use super::*;
use core::prelude::v1::test;

impl Availability {
    /// Completed source query, for deterministic picker tests without scripts.
    /// A finished check made while the pane looked like `seen`. Its callers,
    /// the picker's move tests, run only where moves are offered.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(in crate::pane_menu) fn ready_seen(result: carry::Result<()>, seen: PaneState) -> Self {
        let mut ready = Self::ready(result);
        ready.seen = seen;
        ready
    }

    pub(in crate::pane_menu) fn ready(result: carry::Result<()>) -> Self {
        let (_, receiver) = mpsc::sync_channel(1);
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            receiver,
            result: Some(result),
            seen: PaneState::default(),
        }
    }
}

#[test]
fn a_stopped_availability_worker_reports_a_reason() {
    let mut availability = Availability::ready(Ok(()));
    availability.result = None;
    assert!(availability.poll());
    assert!(availability.muted());
    assert!(
        availability
            .reason()
            .is_some_and(|reason| reason.contains("stopped"))
    );
    assert!(!availability.poll());
}
