//! Stopping the agent: presses, limits, and how the loop ends.

use super::*;

/// Runs [`stop`] against a shell that returns after `after` questions, and
/// says what it pressed and how often it waited.
fn stopped_after(after: u32) -> (Result<(), Error>, u32, u32) {
    let asked = Cell::new(0);
    let pressed = Cell::new(0);
    let waited = Cell::new(0);
    let result = stop(
        Agent::Claude,
        || {
            asked.set(asked.get() + 1);
            Ok(asked.get() > after)
        },
        || {
            pressed.set(pressed.get() + 1);
            Ok(())
        },
        || waited.set(waited.get() + 1),
        &AtomicBool::new(false),
    );
    (result, pressed.get(), waited.get())
}

#[test]
fn an_agent_that_already_left_is_not_pressed() {
    let (result, pressed, waited) = stopped_after(0);
    assert!(result.is_ok());
    assert_eq!((pressed, waited), (0, 0));
}

#[test]
fn presses_stop_as_soon_as_the_shell_is_back() {
    let (result, pressed, _) = stopped_after(2);
    assert!(result.is_ok());
    assert_eq!(pressed, 2);
}

#[test]
fn a_slow_agent_gets_its_presses_and_then_only_time() {
    let (result, pressed, waited) = stopped_after(INTERRUPTS + 20);
    assert!(result.is_ok());
    assert_eq!(pressed, INTERRUPTS);
    assert_eq!(waited, INTERRUPTS + 20);
}

#[test]
fn an_agent_that_never_leaves_is_reported_still_running() {
    let (result, pressed, waited) = stopped_after(u32::MAX);
    assert!(matches!(result, Err(Error::StillRunning(Agent::Claude))));
    assert_eq!(pressed, INTERRUPTS);
    assert_eq!(waited, ROUNDS);
}

#[test]
fn a_failed_question_ends_the_stop_without_a_press() {
    let pressed = Cell::new(0);
    let result = stop(
        Agent::Codex,
        || Err(Error::Moved),
        || {
            pressed.set(pressed.get() + 1);
            Ok(())
        },
        || {},
        &AtomicBool::new(false),
    );
    assert!(matches!(result, Err(Error::Moved)));
    assert_eq!(pressed.get(), 0);
}

#[test]
fn a_question_that_fails_after_a_press_is_asked_again() {
    let asked = Cell::new(0);
    let result = stop(
        Agent::Codex,
        || {
            asked.set(asked.get() + 1);
            match asked.get() {
                1 => Ok(false),
                2 => Err(Error::Moved),
                _ => Ok(true),
            }
        },
        || Ok(()),
        || {},
        &AtomicBool::new(false),
    );
    assert!(result.is_ok());
    assert_eq!(asked.get(), 3);
}

#[test]
fn a_cancelled_stop_presses_nothing_more() {
    let cancelled = AtomicBool::new(false);
    let events = RefCell::new(Vec::new());
    let result = stop(
        Agent::Codex,
        || Ok(false),
        || {
            events.borrow_mut().push("press");
            cancelled.store(true, Ordering::Release);
            Ok(())
        },
        || {},
        &cancelled,
    );
    assert!(matches!(result, Err(Error::Stopped)));
    assert_eq!(*events.borrow(), ["press"]);
}

#[test]
fn only_the_shell_itself_counts_as_back() {
    let in_front = |group: Option<u32>, shell: Option<u32>| {
        shell_in_front(&ProcessInfo {
            foreground_process_group_id: group,
            shell_pid: shell,
            foreground_processes: Vec::new(),
        })
    };
    assert!(in_front(Some(10), Some(10)));
    assert!(!in_front(Some(20), Some(10)));
    // A pane Herdr cannot describe has not been shown to have stopped.
    assert!(!in_front(None, None));
}
