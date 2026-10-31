//! An existing agent on this machine is observed by the `herdr-acp` the
//! app located, not a path guessed beside the executable.

use super::*;
use crate::chat::{Program, StartError, chats, existing::Binding};
use herdr_chat_protocol::Agent;

fn observed() -> Target {
    Target {
        endpoint: "0123456789abcdef0123456789abcdef".parse().unwrap(),
        existing: Some(Binding {
            pane_id: "w1:p1".into(),
            agent: Agent::Codex,
            session_id: "12345678-1234-4123-8123-123456789abc".into(),
            herdr_session: None,
            pid: 21,
            shell_pid: 20,
            started_ticks: 123,
        }),
        pane_id: "w1:p1".into(),
        route: Route::Local,
    }
}

#[test]
fn an_unlocated_observer_program_is_retried_not_permanent() {
    for (program, expected) in [
        (Program::Unknown, StartError::ProgramUnknown),
        (Program::Looking, StartError::ProgramUnknown),
        (Program::Missing, StartError::ProgramMissing),
    ] {
        let Err(error) = chats::connect(&observed(), &program, None, Vec::new()) else {
            panic!("no program to observe with");
        };
        assert!(
            matches!(&error, Error::Start(found) if std::mem::discriminant(found) == std::mem::discriminant(&expected)),
            "{error:?}"
        );
        assert!(!error.permanent());
    }
}

#[test]
fn pending_helper_lookup_does_not_back_off_or_count_as_attachment_failure() {
    let mut chats = Chats::default();
    let now = Instant::now();
    for program in [Program::Unknown, Program::Looking] {
        chats.reconcile(vec![observed()], now, &mut |target, since, resend| {
            chats::connect(target, &program, since, resend)
        });
        let pane = chats.pane("w1:p1").unwrap();
        assert!(pane.loading());
        assert!(!pane.available());
        assert!(!chats.shown("w1:p1"));
    }
    let fixture = super::Program::new();
    let mut connected = 0;
    chats.reconcile(vec![observed()], now, &mut |_, since, resend| {
        connected += 1;
        crate::chat::link::Link::start(fixture.path.clone(), since, resend)
    });
    assert_eq!(
        connected, 1,
        "resolved helper connects on the next poll without elapsed time"
    );
    assert!(chats.pane("w1:p1").unwrap().has_link());
    let (_stream, _) = fixture.accept();
}

#[test]
fn a_missing_helper_still_exposes_the_original_terminal() {
    let mut chats = Chats::default();
    let now = Instant::now();
    chats.reconcile(vec![observed()], now, &mut |target, since, resend| {
        chats::connect(target, &Program::Unknown, since, resend)
    });
    assert!(chats.pane("w1:p1").unwrap().loading());
    assert!(
        chats.reconcile(vec![observed()], now, &mut |target, since, resend| {
            chats::connect(target, &Program::Missing, since, resend)
        }),
        "the loading-to-terminal transition requests a redraw"
    );
    assert!(!chats.pane("w1:p1").unwrap().loading());
    assert!(!chats.shown("w1:p1"));
}
