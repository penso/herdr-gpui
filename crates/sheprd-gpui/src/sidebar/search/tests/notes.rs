use super::*;
use crate::{
    sidebar::layout_tests::REPO_KEY,
    worktree_notes::{Checkout, Notes},
};

fn noted(text: &str) -> Notes {
    let mut notes = Notes::default();
    assert!(notes.set(
        Checkout {
            endpoint: "local".into(),
            repo_key: REPO_KEY.into(),
            branch: "worktree/sidebar-child".into(),
        },
        text,
    ));
    notes
}

#[test]
fn notes_match_after_branches_and_open_their_workspace() {
    let snapshot = snapshot(6);
    let notes = noted("waiting on the sidebar review");
    let [mut local, _] = devices(&snapshot);
    local.notes = Some(&notes);
    let hits = search("sidebar", &[local]);
    let last = hits.last().unwrap();
    assert_eq!(last.kind, Kind::Note);
    assert_eq!(last.text, "waiting on the sidebar review");
    assert_eq!(last.range, 15..22);
    assert_eq!(last.context, "sidebar-child");
    assert_eq!(
        last.target,
        Target::Workspace {
            endpoint: "local".into(),
            workspace: "w4".into(),
        }
    );
    assert!(
        hits[..hits.len() - 1]
            .iter()
            .all(|hit| hit.kind != Kind::Note)
    );

    // A note on another endpoint's checkout is not this device's.
    let [mut studio, _] = devices(&snapshot);
    studio.id = "studio";
    studio.notes = Some(&notes);
    assert!(search("review", &[studio]).is_empty());
}

#[test]
fn a_deep_match_shows_from_shortly_before_it() {
    let snapshot = snapshot(6);
    let notes = noted("a long reminder about many unrelated things, then the needle");
    let [mut local, _] = devices(&snapshot);
    local.notes = Some(&notes);
    let hits = search("needle", &[local]);
    assert_eq!(hits.len(), 1);
    let hit = &hits[0];
    assert_eq!(hit.text, "…s, then the needle");
    assert_eq!(&hit.text[hit.range.clone()], "needle");
}

#[test]
fn excerpts_cut_only_a_far_match_and_keep_characters_whole() {
    assert_eq!(excerpt("short note", 6..10), ("short note".into(), 6..10));
    // As far in as a row shows: nothing is cut.
    let text = "abcdefghijklmnopqrstuvw match";
    assert_eq!(excerpt(text, 24..29), (text.into(), 24..29));
    // One character further, the text keeps the lead behind an ellipsis.
    let (text, range) = excerpt("xabcdefghijklmnopqrstuvw match", 25..30);
    assert_eq!(text, "…mnopqrstuvw match");
    assert_eq!(&text[range], "match");
    let note = format!("{} match", "é".repeat(30));
    let (text, range) = excerpt(&note, 61..66);
    assert_eq!(text, format!("…{} match", "é".repeat(11)));
    assert_eq!(&text[range], "match");
}
