use super::*;
use sheprd_remote_control::control::changes::{Answer, Ask, AskId, Failure, Wanted};
use std::sync::Mutex;

static NO_PHONE_WAITS: AtomicUsize = AtomicUsize::new(0);

fn daemon_without_phones(_: &Path, request: &Request) -> Result<Response, control::Error> {
    match request {
        Request::Status => Ok(Response::Status(status(0))),
        Request::ChangeAsks => {
            NO_PHONE_WAITS.fetch_add(1, Ordering::SeqCst);
            Ok(Response::ChangeAsks { asks: Vec::new() })
        }
        _ => Ok(Response::Done),
    }
}

#[gpui::test]
fn with_no_phone_connected_nothing_waits_on_the_daemon(cx: &mut TestAppContext) {
    cx.update(|cx| Supervisor::fixture(cx, on(), daemon_without_phones));
    for _ in 0..2 {
        cx.update(Supervisor::tick);
        cx.run_until_parked();
    }
    assert_eq!(NO_PHONE_WAITS.load(Ordering::SeqCst), 0);
}

static OLD_DAEMON_WAITS: AtomicUsize = AtomicUsize::new(0);

fn old_daemon(_: &Path, request: &Request) -> Result<Response, control::Error> {
    match request {
        Request::Status => Ok(Response::Status(status(1))),
        Request::ChangeAsks => {
            OLD_DAEMON_WAITS.fetch_add(1, Ordering::SeqCst);
            Err(control::Error::NoAnswer)
        }
        _ => Ok(Response::Done),
    }
}

#[gpui::test]
fn a_daemon_that_refuses_is_not_asked_again_at_once(cx: &mut TestAppContext) {
    cx.update(|cx| Supervisor::fixture(cx, on(), old_daemon));
    // The first tick learns a phone is connected; the next two would wait.
    for _ in 0..3 {
        cx.update(Supervisor::tick);
        cx.run_until_parked();
    }
    assert_eq!(OLD_DAEMON_WAITS.load(Ordering::SeqCst), 1);
}

static ANSWERED: Mutex<Vec<(AskId, Answer)>> = Mutex::new(Vec::new());
static HANDED_OVER: AtomicUsize = AtomicUsize::new(0);

fn daemon_with_a_question(_: &Path, request: &Request) -> Result<Response, control::Error> {
    match request {
        Request::Status => Ok(Response::Status(status(1))),
        Request::ChangeAsks if HANDED_OVER.fetch_add(1, Ordering::SeqCst) == 0 => {
            Ok(Response::ChangeAsks {
                asks: vec![Ask {
                    id: AskId(5),
                    boot_id: "another session".into(),
                    workspace_id: "w".into(),
                    wanted: Wanted::Checks,
                }],
            })
        }
        Request::ChangeAsks => Err(control::Error::NoAnswer),
        Request::ChangeAnswer { id, answer } => {
            ANSWERED.lock().unwrap().push((*id, answer.clone()));
            Ok(Response::Done)
        }
        _ => Ok(Response::Done),
    }
}

#[gpui::test]
fn a_question_taken_from_the_daemon_is_answered_by_its_id(cx: &mut TestAppContext) {
    cx.update(|cx| Supervisor::fixture(cx, on(), daemon_with_a_question));
    for _ in 0..2 {
        cx.update(Supervisor::tick);
        cx.run_until_parked();
    }
    assert_eq!(
        *ANSWERED.lock().unwrap(),
        [(AskId(5), Answer::Failed(Failure::Gone))]
    );
}
