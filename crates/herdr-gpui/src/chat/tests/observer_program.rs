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
