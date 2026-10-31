//! Run with --nocapture in release mode for the three preparation timings.
//! Invariants are deterministic; machine-dependent timings have no threshold.

use super::*;
use std::rc::Rc;

#[test]
fn retained_history_preparation_reuses_bodies_and_bounds_visual_work() {
    let mut transcript = Transcript::new("Synthetic preparation measurement");
    transcript.entries = (1..=2_000)
        .map(|id| {
            let mut text = format!(
                "## Message {id}\n\n**Synthetic** café text with $x_{{{id}}}$ and a [link](https://example.org/).\n\n```rust\nlet value_{id} = {id};\n```\n\n"
            );
            text.push_str(&"Ordinary retained prose. ".repeat(40));
            text.truncate(text.floor_char_boundary(1_024));
            agent(id, &text)
        })
        .collect();
    let bytes: usize = transcript
        .entries
        .iter()
        .map(|entry| entry.kind.text().unwrap().len())
        .sum();
    assert!((1_900_000..=2 * 1024 * 1024).contains(&bytes));
    let mut keys: Vec<_> = transcript
        .entries
        .iter()
        .map(|entry| (entry.id, 1))
        .collect();
    let mut prepared = Prepared::default();
    let started = Instant::now();
    prepared.update(&transcript, &keys);
    let cold = started.elapsed();
    let original: Vec<_> = keys
        .iter()
        .map(|(id, _)| prepared.body(*id).unwrap())
        .collect();
    let started = Instant::now();
    prepared.update(&transcript, &keys);
    let unchanged = started.elapsed();
    for ((id, _), body) in keys.iter().zip(&original) {
        assert!(Rc::ptr_eq(body, &prepared.body(*id).unwrap()));
    }
    let last = transcript.entries.last_mut().unwrap();
    let EntryKind::Agent { text } = &mut last.kind else {
        panic!("agent fixture");
    };
    text.push_str("\n\nOne new tail revision.");
    keys.last_mut().unwrap().1 += 1;
    let started = Instant::now();
    prepared.update(&transcript, &keys);
    let tail = started.elapsed();
    for ((id, _), body) in keys.iter().zip(&original).take(keys.len() - 1) {
        assert!(Rc::ptr_eq(body, &prepared.body(*id).unwrap()));
    }
    assert!(!Rc::ptr_eq(
        original.last().unwrap(),
        &prepared.body(keys.last().unwrap().0).unwrap()
    ));
    assert!(prepared.visuals.rich_budget().0 <= crate::chat::visual::MAX_RICH_VISUALS);
    assert_eq!(
        prepared.visuals.rich_budget().1,
        0,
        "preparation does not rasterize"
    );
    let mut selection = crate::chat::selection::Selection::default();
    selection.select_all(prepared.text());
    let copied = selection.text(prepared.text()).unwrap();
    assert!(copied.contains("Message 1"));
    assert!(copied.contains("One new tail revision."));
    eprintln!(
        "rich-preparation entries={} bytes={bytes} cold_us={} unchanged_us={} tail_revision_us={}",
        keys.len(),
        cold.as_micros(),
        unchanged.as_micros(),
        tail.as_micros()
    );
}
