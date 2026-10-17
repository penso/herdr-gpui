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

#[test]
fn withdrawing_one_batch_keeps_the_others_queued_in_order() {
    let (mut first, mut second, mut third) = (
        PendingSend::default(),
        PendingSend::default(),
        PendingSend::default(),
    );
    let mut deliveries = Deliveries::default();
    deliveries
        .queue
        .push(delivery("p1", first.start(vec!["first"])));
    deliveries
        .queue
        .push(delivery("p2", second.start(vec!["second", "again"])));
    // A terminal note's batch has no tab to withdraw it.
    deliveries.queue.push(delivery("p3", None));
    deliveries
        .queue
        .push(delivery("p4", third.start(vec!["third"])));
    assert_eq!(second.queued(&deliveries, None), Some(Queued::UntilIdle));

    assert_eq!(
        second.withdraw(&mut deliveries, None),
        Some(vec!["second", "again"])
    );
    assert_eq!(
        texts(&deliveries),
        ["notes for p1", "notes for p3", "notes for p4"]
    );
    assert_eq!(second.queued(&deliveries, None), None);
    assert_eq!(first.queued(&deliveries, None), Some(Queued::UntilIdle));
    assert_eq!(third.queued(&deliveries, None), Some(Queued::UntilIdle));
    assert!(
        second.start(vec!["edited"]).is_some(),
        "a withdrawn batch frees the tab to send again"
    );
    assert!(first.start(Vec::new()).is_none(), "the others still pend");
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

    deliveries
        .queue
        .push(delivery("p2", sending.start(vec![1])));
    assert_eq!(sending.withdraw(&mut deliveries, None), Some(vec![1]));
    assert_eq!(sending.withdraw(&mut deliveries, None), None);

    // Pasted: `poll_deliveries` took it out of the queue.
    deliveries
        .queue
        .push(delivery("p3", sending.start(vec![2])));
    let pasted = deliveries.queue.pop();
    assert_eq!(sending.queued(&deliveries, None), None);
    assert_eq!(sending.withdraw(&mut deliveries, None), None);
    assert!(sending.start(Vec::new()).is_none(), "still on its way");
    drop(pasted);
    assert!(sending.start(Vec::new()).is_some());
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
        other.start(vec!["older"]),
    );
    feedback.keep(
        crate::browser::Batch {
            target: key("p1"),
            text: "mine".into(),
        },
        kept.start(vec!["mine"]),
    );
    deliveries.queue.push(delivery("p2", None));
    assert_eq!(
        kept.queued(&deliveries, Some(&feedback)),
        Some(Queued::ForFeedback)
    );
    assert_eq!(
        kept.withdraw(&mut deliveries, Some(&mut feedback)),
        Some(vec!["mine"])
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
