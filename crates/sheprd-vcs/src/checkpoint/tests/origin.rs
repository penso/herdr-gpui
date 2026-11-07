//! A chat finding the checkpoint from the start of one of its prompts.

use super::*;

fn origin() -> Origin {
    Origin {
        chat: "p0".into(),
        prompt: "1-abc".into(),
    }
}

#[test]
fn a_turn_start_carries_its_prompt_and_a_turn_end_does_not() {
    use AgentStatus::*;
    let mut checkpoints = Checkpoints::default();
    let known = |pane: &str| (pane == "p0").then(origin);
    checkpoints.observe("local", &Host::Local, &with([Idle, Idle]), known);
    checkpoints.observe("local", &Host::Local, &with([Working, Idle]), known);
    assert_eq!(
        queued(&checkpoints),
        [(
            feature(),
            Request::Capture {
                label: "Before Claude Code's turn".into(),
                origin: Some(origin()),
            }
        )]
    );
    checkpoints.observe("local", &Host::Local, &with([Idle, Idle]), known);
    assert_eq!(
        queued(&checkpoints),
        [(
            feature(),
            Request::Capture {
                label: "Claude Code finished a turn".into(),
                origin: None,
            }
        )],
        "still waiting, it will not snapshot the turn's start"
    );
}

#[test]
fn a_search_goes_ahead_of_waiting_checkpoints_and_answers_once() {
    let mut checkpoints = Checkpoints::default();
    checkpoints.capture(feature(), "turn".into(), None);
    checkpoints.find(feature(), origin());
    checkpoints.find(feature(), origin());
    let queue = queued(&checkpoints);
    assert_eq!(queue.len(), 2);
    assert_eq!(queue[0].1, Request::Find { origin: origin() });
    let found = checkpoint("1", "feature");
    answer(
        &mut checkpoints,
        Request::Find { origin: origin() },
        Answer::Found(Ok(Some(found.clone()))),
    );
    let mut answers = checkpoints.take_found();
    assert_eq!(answers.len(), 1);
    let answer = answers.remove(0);
    assert_eq!(answer.origin, origin());
    assert_eq!(answer.result.unwrap(), Some(found));
    assert!(checkpoints.take_found().is_empty());
}

#[test]
fn a_restore_by_id_is_reported_like_the_dialogs() {
    let mut checkpoints = Checkpoints::default();
    checkpoints.restore_id(feature(), "7".into()).unwrap();
    assert_eq!(
        queued(&checkpoints),
        [(feature(), Request::Restore { id: "7".into() })]
    );
    let restored = answer(
        &mut checkpoints,
        Request::Restore { id: "7".into() },
        Answer::Restored(Ok(())),
    );
    assert_eq!(restored.len(), 1);
    assert!(restored[0].result.is_ok());
}

#[test]
fn too_many_hosts_is_an_answer_and_not_silence() {
    let mut checkpoints = Checkpoints::default();
    for index in 0..HOST_LIMIT {
        let host = Host::Ssh(format!("box{index}"));
        let checkout = Checkout::new(host, "/r/.git", Some("main")).unwrap();
        checkpoints.capture(checkout, "turn".into(), None);
    }
    checkpoints.find(feature(), origin());
    let found = checkpoints.take_found();
    assert!(matches!(
        found[0].result,
        Err(crate::Error::Checkpoint(Error::Hosts))
    ));
    assert!(matches!(
        checkpoints.restore_id(feature(), "7".into()),
        Err(crate::Error::Checkpoint(Error::Hosts))
    ));
}
