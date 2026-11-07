use super::*;

#[test]
fn tags_are_tidied_within_bounds() {
    let tags = ComputerTags::new(["  home   lab ", "", "Work", "work", "a\u{7}b"], 5);
    assert_eq!(tags.tags, ["home lab", "Work"]);
    assert_eq!(tags.updated_ms, 5);

    let many = (0..MAX_TAGS + 3).map(|index| format!("tag{index}"));
    assert_eq!(ComputerTags::new(many, 1).tags.len(), MAX_TAGS);

    let long = "é".repeat(MAX_TAG_CHARS + 5);
    assert_eq!(
        normalized(&long).unwrap().chars().count(),
        MAX_TAG_CHARS,
        "the limit counts characters, not bytes"
    );
}

#[test]
fn a_tag_cut_at_a_space_keeps_no_trailing_space() {
    let text = format!("{} tail", "a".repeat(MAX_TAG_CHARS - 1));
    assert_eq!(normalized(&text).unwrap(), "a".repeat(MAX_TAG_CHARS - 1));
}

#[test]
fn the_latest_edit_wins_whole() {
    let stored = ComputerTags::new(["work", "lab"], 100);
    let newer = ComputerTags::new(["personal"], 200);
    assert_eq!(stored.clone().merged(newer.clone(), 1_000), newer);

    let older = ComputerTags::new(["personal"], 50);
    assert_eq!(stored.clone().merged(older, 1_000), stored);

    let tie = ComputerTags::new(["personal"], 100);
    assert_eq!(stored.clone().merged(tie, 1_000), stored);

    let cleared = ComputerTags::new(Vec::<String>::new(), 300);
    assert_eq!(stored.merged(cleared.clone(), 1_000), cleared);
}

#[test]
fn a_fast_clock_cannot_outlast_later_edits() {
    let ahead = ComputerTags::new(["work"], 9_000);
    let stored = ComputerTags::default().merged(ahead, 1_000);
    assert_eq!(stored.updated_ms, 1_000);

    let later = ComputerTags::new(["personal"], 2_000);
    assert_eq!(stored.merged(later.clone(), 2_000), later);
}

#[test]
fn an_offer_is_tidied_before_it_stands() {
    let offer = ComputerTags {
        tags: vec!["  work ".into(), "WORK".into()],
        updated_ms: 10,
    };
    assert_eq!(
        ComputerTags::default().merged(offer, 100).tags,
        ["work".to_owned()]
    );
}
