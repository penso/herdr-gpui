use super::*;
use crate::control::protocol::cloud::Phase;

#[test]
fn a_cloud_batch_moving_on_is_an_event() {
    let mut log = Log::default();
    let before = Seen {
        batches: [(1, Phase::Running), (2, Phase::Confirming)].into(),
        ..Seen::default()
    };
    let now = Seen {
        batches: [
            (1, Phase::Running),
            (2, Phase::Done),
            (3, Phase::Confirming),
        ]
        .into(),
        ..Seen::default()
    };
    compare(&mut log, &before, &now);
    let events: Vec<_> = log
        .after(0, &[EventKind::CloudBatch])
        .into_iter()
        .map(|event| (event.subject, event.detail))
        .collect();
    assert_eq!(
        events,
        [
            ("2".to_owned(), Some("done".to_owned())),
            ("3".to_owned(), Some("confirming".to_owned())),
        ]
    );
    assert_eq!("cloud-batch".parse(), Ok(EventKind::CloudBatch));
}
