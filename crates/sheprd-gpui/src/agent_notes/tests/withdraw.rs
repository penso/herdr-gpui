#![allow(clippy::unwrap_used)]
use super::*;

/// A batch for `pane` queued in this window, as `deliver_notes` queues it.
fn delivery(pane: &str, pending: Option<Arc<()>>) -> Delivery {
    Delivery {
        target: key(pane),
        origin: Origin {
            endpoint: "local".into(),
            boot_id: "boot".into(),
        },
        text: format!("notes for {pane}"),
        until: Instant::now() + HOLD,
        pending,
        copies: None,
    }
}

fn texts(deliveries: &Deliveries) -> Vec<&str> {
    deliveries
        .queue
        .iter()
        .map(|delivery| delivery.text.as_str())
        .collect()
}

/// Starts a send, as Send does, returning its name and the batch's marker.
fn send(sending: &mut PendingSend) -> (SendId, Option<Arc<()>>) {
    let (id, pending) = sending.start().unwrap();
    (id, Some(pending))
}

#[test]
fn withdrawing_one_batch_keeps_the_others_queued_in_order() {
    let (mut first, mut second, mut third) = (
        PendingSend::default(),
        PendingSend::default(),
        PendingSend::default(),
    );
    let mut deliveries = Deliveries::default();
    deliveries.queue.push(delivery("p1", send(&mut first).1));
    let (withdrawn, pending) = send(&mut second);
    deliveries.queue.push(delivery("p2", pending));
    // A terminal note's batch has no tab to withdraw it.
    deliveries.queue.push(delivery("p3", None));
    deliveries.queue.push(delivery("p4", send(&mut third).1));
    assert_eq!(second.queued(&deliveries, None), Some(Queued::UntilIdle));

    assert_eq!(second.withdraw(&mut deliveries, None), Some(withdrawn));
    assert_eq!(
        texts(&deliveries),
        ["notes for p1", "notes for p3", "notes for p4"]
    );
    assert_eq!(second.queued(&deliveries, None), None);
    assert_eq!(first.queued(&deliveries, None), Some(Queued::UntilIdle));
    assert_eq!(third.queued(&deliveries, None), Some(Queued::UntilIdle));
    let (resent, _) = send(&mut second);
    assert_ne!(resent, withdrawn, "each send has its own name");
    assert!(first.start().is_none(), "the others still pend");
}

#[test]
fn a_batch_cannot_be_withdrawn_twice_or_after_it_left() {
    let mut sending = PendingSend::default();
    let mut deliveries = Deliveries::default();
    assert_eq!(sending.withdraw(&mut deliveries, None), None);
    // A batch without a tab is not mistaken for a tab that never sent.
    deliveries.queue.push(delivery("p1", None));
    assert_eq!(sending.withdraw(&mut deliveries, None), None);
    assert_eq!(deliveries.len(), 1);

    let (id, pending) = send(&mut sending);
    deliveries.queue.push(delivery("p2", pending));
    assert_eq!(sending.withdraw(&mut deliveries, None), Some(id));
    assert_eq!(sending.withdraw(&mut deliveries, None), None);

    // Pasted: `poll_deliveries` took it out of the queue.
    deliveries.queue.push(delivery("p3", send(&mut sending).1));
    let pasted = deliveries.queue.pop();
    assert_eq!(sending.queued(&deliveries, None), None);
    assert_eq!(sending.withdraw(&mut deliveries, None), None);
    assert!(sending.is_pending(), "still on its way");
    drop(pasted);
    assert!(!sending.is_pending());
}

#[test]
fn a_batch_kept_for_feedback_is_withdrawn_from_there_only() {
    let (mut kept, mut other) = (PendingSend::default(), PendingSend::default());
    let mut deliveries = Deliveries::default();
    let mut feedback = Feedback::default();
    feedback.keep(
        crate::browser::Batch {
            target: key("p1"),
            text: "older".into(),
        },
        send(&mut other).1,
    );
    let (mine, pending) = send(&mut kept);
    feedback.keep(
        crate::browser::Batch {
            target: key("p1"),
            text: "mine".into(),
        },
        pending,
    );
    deliveries.queue.push(delivery("p2", None));
    assert_eq!(
        kept.queued(&deliveries, Some(&feedback)),
        Some(Queued::ForFeedback)
    );
    assert_eq!(
        kept.withdraw(&mut deliveries, Some(&mut feedback)),
        Some(mine)
    );
    assert_eq!(kept.queued(&deliveries, Some(&feedback)), None);
    assert_eq!(deliveries.len(), 1, "the window queue is left alone");
    assert_eq!(feedback.take(&key("p1")).as_deref(), Some("older"));
    assert_eq!(
        other.withdraw(&mut deliveries, Some(&mut feedback)),
        None,
        "fetched by the agent"
    );
}
