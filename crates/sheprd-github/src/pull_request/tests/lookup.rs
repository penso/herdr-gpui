use super::*;
use crate::pull_request::{
    batch::Batch,
    lookup::{Finished, Prefetch},
};

#[test]
fn worker_discards_stale_batches_and_runs_only_requested_jobs() {
    let (requests, incoming) = mpsc::sync_channel(1);
    let (outgoing, results) = mpsc::sync_channel(1);
    let mut prefetch = Prefetch::default();
    prefetch.worker = Some(Worker { requests, results });
    let input = Input {
        checkout: Some("/fixture".into()),
        repo_key: Some("/fixture/.git".into()),
        branch: "feature".into(),
    };
    let answer = |input: &Input| Batch {
        found: vec![(input.clone(), Ok(Some(fixture().unwrap())))],
        ..Batch::default()
    };
    prefetch.request(
        vec![input.clone()],
        Origin::Local,
        Arc::new("fixture".into()),
        Vec::new(),
    );
    assert!(prefetch.poll().is_none());
    let (old, _, _, _, _) = incoming.try_recv().unwrap();
    prefetch.clear();
    prefetch.request(
        vec![input.clone()],
        Origin::Local,
        Arc::new("fixture".into()),
        Vec::new(),
    );
    assert!(prefetch.poll().is_none());
    assert!(incoming.try_recv().is_err(), "single in-flight batch");
    outgoing.send((old, answer(&input))).unwrap();
    assert!(prefetch.poll().is_none(), "a superseded batch is discarded");
    let (current, inputs, _, _, _) = incoming.try_recv().unwrap();
    assert_eq!(inputs, std::slice::from_ref(&input));
    outgoing.send((current, answer(&input))).unwrap();
    let Some(Finished::Answered(batch)) = prefetch.poll() else {
        panic!("the current batch is answered");
    };
    assert_eq!(batch.found.len(), 1);
    assert!(prefetch.poll().is_none());
    assert!(incoming.try_recv().is_err(), "no automatic polling");
}
