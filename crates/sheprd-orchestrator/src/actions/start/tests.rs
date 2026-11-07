use super::*;
use std::{cell::Cell, error::Error as _, os::unix::process::ExitStatusExt};

fn rejection(stderr: &str) -> Error {
    Error::Script {
        operation: STARTING,
        source: sheprd_client::Error::ScriptExit {
            status: std::process::ExitStatus::from_raw(256),
            stderr: stderr.into(),
        },
    }
}

fn busy() -> Error {
    rejection(
        r#"{"id":"cli:agent:start","error":{"code":"agent_pane_busy","message":"not an available shell"}}"#,
    )
}

#[test]
fn busy_shell_initialization_retries_until_one_launch_succeeds() {
    let attempts = Cell::new(0);
    let checks = Cell::new(0);
    let time = Cell::new(Duration::ZERO);
    retry_busy(
        &AtomicBool::new(false),
        || {
            attempts.set(attempts.get() + 1);
            if attempts.get() < 4 {
                Err(busy())
            } else {
                Ok(())
            }
        },
        |_| {
            checks.set(checks.get() + 1);
            Ok(true)
        },
        || time.get(),
        |delay| time.set(time.get() + delay),
    )
    .unwrap();
    assert_eq!(attempts.get(), 4);
    assert_eq!(checks.get(), 3);
    assert_eq!(time.get(), POLL_INTERVAL * 3);
}

#[test]
fn a_permanently_busy_pane_stops_at_the_deadline_with_its_original_cause() {
    let attempts = Cell::new(0);
    let time = Cell::new(Duration::ZERO);
    let error = retry_busy(
        &AtomicBool::new(false),
        || {
            attempts.set(attempts.get() + 1);
            Err(busy())
        },
        |_| Ok(true),
        || time.get(),
        |delay| time.set(time.get() + delay),
    )
    .unwrap_err();
    assert_eq!(time.get(), SHELL_START_TIMEOUT);
    assert_eq!(attempts.get(), 100);
    assert!(is_busy_rejection(&error));
    assert!(error.source().unwrap().is::<sheprd_client::Error>());
}

#[test]
fn cancellation_during_the_wait_prevents_probes_and_launches() {
    let cancelled = AtomicBool::new(false);
    let error = retry_busy(
        &cancelled,
        || Err(busy()),
        |_| panic!("cancelled dispatch must not query the pane"),
        || Duration::ZERO,
        |_| cancelled.store(true, Ordering::Release),
    )
    .unwrap_err();
    assert!(matches!(error, Error::Cancelled));
    assert!(matches!(
        retry_busy(
            &cancelled,
            || panic!("cancelled dispatch must not launch"),
            |_| panic!("cancelled dispatch must not query"),
            || Duration::ZERO,
            |_| panic!("cancelled dispatch must not wait"),
        ),
        Err(Error::Cancelled)
    ));
}

#[test]
fn a_replaced_terminal_or_expired_identity_check_prevents_retry() {
    for replaced in [true, false] {
        let attempts = Cell::new(0);
        let time = Cell::new(Duration::ZERO);
        let error = retry_busy(
            &AtomicBool::new(false),
            || {
                attempts.set(attempts.get() + 1);
                Err(busy())
            },
            |_| {
                if !replaced {
                    time.set(SHELL_START_TIMEOUT);
                }
                Ok(!replaced)
            },
            || time.get(),
            |delay| time.set(time.get() + delay),
        )
        .unwrap_err();
        assert!(is_busy_rejection(&error));
        assert_eq!(attempts.get(), 1);
    }
}

#[test]
fn ambiguous_failures_and_other_rejections_are_never_replayed() {
    for error in [
        rejection("agent_pane_busy"),
        rejection(r#"{"id":"cli:agent:start","error":{"code":"agent_start_timeout"}}"#),
        rejection(r#"{"id":"cli:agent:start","error":{"code":"agent_pane_not_found"}}"#),
        rejection(r#"{"id":"cli:agent:prompt","error":{"code":"agent_pane_busy"}}"#),
        rejection(r#"{"id":"cli:agent:start","error":{"code":"agent_pane_busy"}"#),
        Error::Script {
            operation: STARTING,
            source: sheprd_client::Error::ScriptCancelled,
        },
    ] {
        let mut error = Some(error);
        assert!(
            retry_busy(
                &AtomicBool::new(false),
                || Err(error.take().unwrap()),
                |_| panic!("must not query after an ambiguous failure"),
                || Duration::ZERO,
                |_| panic!("must not retry"),
            )
            .is_err()
        );
    }
}

#[test]
fn the_identity_check_is_bounded_by_the_remaining_retry_window() {
    let time = Cell::new(Duration::ZERO);
    let budgets = std::cell::RefCell::new(Vec::new());
    retry_busy(
        &AtomicBool::new(false),
        || Err(busy()),
        |budget| {
            budgets.borrow_mut().push(budget);
            // A stalled query consumes its whole budget.
            time.set(time.get() + budget);
            Ok(true)
        },
        || time.get(),
        |delay| time.set(time.get() + delay),
    )
    .unwrap_err();
    assert_eq!(*budgets.borrow(), [SHELL_START_TIMEOUT - POLL_INTERVAL]);
    assert_eq!(time.get(), SHELL_START_TIMEOUT);
}
