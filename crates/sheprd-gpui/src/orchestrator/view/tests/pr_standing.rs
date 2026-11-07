use super::super::{
    look::Look,
    pr_cells::{row_tone, standing_word},
};
use super::*;
use crate::{
    i18n::fl,
    orchestrator::PrStatus,
    pull_request::{MergeState, Tone},
};

fn row(state: &str) -> Item {
    let mut item = item(Provider::Github, "pr/1", "fixture", "2026-10-01T00:00:00Z");
    state.clone_into(&mut item.state);
    item
}

fn merging(merge: MergeState) -> PrStatus {
    PrStatus {
        merge,
        ..PrStatus::default()
    }
}

#[test]
fn a_row_with_no_status_is_never_called_ready() {
    let open = row("open");
    assert_eq!(row_tone(&open, None), Tone::Muted);
    assert_eq!(standing_word(Tone::Muted, &open, None), None);
}

#[test]
fn the_lifecycle_outranks_a_status_kept_from_when_it_was_open() {
    let clean = merging(MergeState::Clean);
    assert_eq!(row_tone(&row("open"), Some(&clean)), Tone::Ready);
    assert_eq!(row_tone(&row("draft"), Some(&clean)), Tone::Muted);
    assert_eq!(row_tone(&row("merged"), Some(&clean)), Tone::Merged);
    assert_eq!(row_tone(&row("closed"), Some(&clean)), Tone::Attention);
}

#[test]
fn the_word_names_what_keeps_a_pull_request_from_merging() {
    let word = |item: &Item, merge| {
        let status = merging(merge);
        standing_word(row_tone(item, Some(&status)), item, Some(&status))
    };
    let open = row("open");
    assert_eq!(
        word(&open, MergeState::Clean),
        Some(fl!("orchestrator-standing-ready"))
    );
    assert_eq!(
        word(&open, MergeState::Dirty),
        Some(fl!("orchestrator-standing-conflicts"))
    );
    assert_eq!(
        word(&open, MergeState::Behind),
        Some(fl!("orchestrator-standing-behind"))
    );
    assert_eq!(
        word(&open, MergeState::Blocked),
        Some(fl!("orchestrator-badge-blocked"))
    );
    assert_eq!(
        word(&open, MergeState::Unknown),
        Some(fl!("orchestrator-standing-waiting"))
    );
    assert_eq!(
        word(&row("closed"), MergeState::Dirty),
        Some(fl!("orchestrator-standing-closed"))
    );
    assert_eq!(
        word(&row("merged"), MergeState::Clean),
        Some(fl!("orchestrator-standing-merged"))
    );
}

#[test]
fn the_status_columns_grow_with_the_text_and_never_shrink_below_their_width() {
    let config = crate::config::Config::default();
    let mut look = Look {
        theme: crate::config::Theme::default(),
        ui: config.ui.clone(),
        mono: config.terminal.clone(),
    };
    look.ui.size = 14.;
    assert_eq!(look.column(70.), 70.);
    look.ui.size = 10.;
    assert_eq!(look.column(70.), 70.);
    look.ui.size = 28.;
    assert_eq!(look.column(70.), 140.);
}
