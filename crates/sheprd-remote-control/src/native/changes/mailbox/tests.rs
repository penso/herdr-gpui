use super::*;
use crate::control::changes::{CheckLog, FileList, MAX_EXCERPT, Scope};
use std::{sync::Arc, thread};

const NOW_WAIT: Duration = Duration::ZERO;

fn wanted() -> Wanted {
    Wanted::Files {
        scope: Scope::Uncommitted,
    }
}

fn open() -> (Mailbox, Instant) {
    let mailbox = Mailbox::default();
    let now = Instant::now();
    mailbox.seen_at(now);
    (mailbox, now)
}

fn ask<'a>(mailbox: &'a Mailbox, now: Instant) -> Result<Pending<'a>, Error> {
    mailbox.ask("b".into(), "w".into(), wanted(), now)
}

#[test]
fn a_phone_does_not_wait_for_a_sheprd_never_heard_from() {
    let mailbox = Mailbox::default();
    assert!(matches!(
        ask(&mailbox, Instant::now()),
        Err(Error::NoDesktop)
    ));
}

#[test]
fn a_phone_does_not_wait_for_a_sheprd_gone_quiet() {
    let (mailbox, now) = open();
    assert!(ask(&mailbox, now + OPEN_FOR - Duration::from_millis(1)).is_ok());
    assert!(matches!(
        ask(&mailbox, now + OPEN_FOR),
        Err(Error::NoDesktop)
    ));
}

#[test]
fn questions_are_bounded_and_a_withdrawn_one_frees_its_place() {
    let (mailbox, now) = open();
    let mut held: Vec<_> = (0..MAX_WAITING)
        .map(|_| ask(&mailbox, now).unwrap())
        .collect();
    assert!(matches!(ask(&mailbox, now), Err(Error::Busy)));
    held.pop();
    assert!(ask(&mailbox, now).is_ok());
}

#[test]
fn a_question_withdrawn_before_it_was_taken_is_never_asked() {
    let (mailbox, now) = open();
    drop(ask(&mailbox, now).unwrap());
    assert!(mailbox.take(NOW_WAIT).is_empty());
}

#[tokio::test]
async fn an_answer_reaches_the_phone_that_asked() {
    let (mailbox, now) = open();
    let mut first = ask(&mailbox, now).unwrap();
    let mut second = ask(&mailbox, now).unwrap();
    let asks = mailbox.take(NOW_WAIT);
    assert_eq!(asks.len(), 2);
    assert_ne!(asks[0].id, asks[1].id);
    let list = FileList::bounded(None, []);
    mailbox.answer(asks[1].id, Answer::Files(list.clone()));
    mailbox.answer(asks[0].id, Answer::Failed(Failure::NoBase));
    assert_eq!(second.answer().await, Some(Answer::Files(list)));
    assert_eq!(first.answer().await, Some(Answer::Failed(Failure::NoBase)));
}

#[tokio::test]
async fn an_answer_past_the_bounds_reads_as_a_failure() {
    let (mailbox, now) = open();
    let mut pending = ask(&mailbox, now).unwrap();
    let id = mailbox.take(NOW_WAIT)[0].id;
    let excerpt = "x".repeat(MAX_EXCERPT + 1);
    mailbox.answer(id, Answer::CheckLog(CheckLog { excerpt }));
    assert_eq!(
        pending.answer().await,
        Some(Answer::Failed(Failure::Unreadable))
    );
}

#[test]
fn an_answer_nobody_waits_for_is_dropped() {
    let (mailbox, now) = open();
    drop(ask(&mailbox, now).unwrap());
    mailbox.answer(AskId(0), Answer::Failed(Failure::NoBase));
    mailbox.answer(AskId(99), Answer::Failed(Failure::NoBase));
    assert!(ask(&mailbox, now).is_ok());
}

#[test]
fn taking_returns_at_once_when_a_question_arrives_during_the_wait() {
    let (mailbox, now) = open();
    let mailbox = Arc::new(mailbox);
    let taker = {
        let mailbox = mailbox.clone();
        thread::spawn(move || mailbox.take(Duration::from_secs(30)))
    };
    let pending = ask(&mailbox, now).unwrap();
    assert_eq!(taker.join().unwrap().len(), 1);
    drop(pending);
}

#[test]
fn taking_counts_as_being_heard_from() {
    let mailbox = Mailbox::default();
    assert!(mailbox.take(NOW_WAIT).is_empty());
    assert!(ask(&mailbox, Instant::now()).is_ok());
}
