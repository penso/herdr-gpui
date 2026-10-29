use super::*;

/// `e` and a combining acute accent; a family emoji joined by zero-width
/// joiners; a flag from two regional indicators; and a Hangul syllable
/// spelled as its three conjoining jamo.
const CLUSTERS: [&str; 4] = [
    "e\u{301}",
    "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}",
    "\u{1f1ef}\u{1f1f5}",
    "\u{1112}\u{1161}\u{11ab}",
];

#[test]
fn cuts_keep_every_cluster_whole() {
    let text = CLUSTERS.concat();
    for limit in 0..CLUSTERS.len() {
        let kept = cut(&text, limit).unwrap_or_default();
        assert_eq!(kept, CLUSTERS[..limit].concat(), "limit {limit}");
        assert_eq!(
            ellipsize(&text, limit),
            format!("{}{ELLIPSIS}", CLUSTERS[..limit].concat())
        );
    }
    assert_eq!(cut(&text, CLUSTERS.len()), None);
    assert!(matches!(
        ellipsize(&text, CLUSTERS.len()),
        Cow::Borrowed(same) if same == text
    ));
}

#[test]
fn cjk_text_counts_one_cluster_per_character() {
    assert_eq!(ellipsize("你好世界", 2), "你好\u{2026}");
    assert_eq!(ellipsize("こんにちは", 5), "こんにちは");
}

#[test]
fn width_cuts_count_wide_characters_twice() {
    assert_eq!(cut_width("abcdef", 4), Some("abcd"));
    assert_eq!(cut_width("你好世界", 5), Some("你好"));
    assert_eq!(cut_width("你好世界", 8), None);
    // A combining accent adds no width and stays with its letter.
    assert_eq!(
        cut_width("e\u{301}e\u{301}e\u{301}", 2),
        Some("e\u{301}e\u{301}")
    );
    // A wide character that would overrun is left out whole.
    assert_eq!(cut_width("ab你", 3), Some("ab"));
}

#[test]
fn a_character_budget_bounds_clusters_of_combining_marks() {
    let heavy = format!("a{}b", "\u{301}".repeat(1000));
    // The accented `a` alone is over budget, so nothing of it is kept.
    assert_eq!(cut_chars(&heavy, 300), Some(""));
    assert_eq!(cut_chars("e\u{301}x", 2), Some("e\u{301}"));
    assert_eq!(cut_chars("e\u{301}x", 3), None);
}

#[test]
fn one_line_never_ends_in_part_of_a_cluster() {
    // Seven characters each, so the 800-character safety cut falls inside
    // the 115th family; it is left out whole and the cut is marked.
    let family = CLUSTERS[1].to_owned() + "\u{200d}\u{1f466}";
    assert_eq!(family.chars().count(), 7);
    let line = one_line(&family.repeat(115), 200);
    assert_eq!(line, format!("{}{ELLIPSIS}", family.repeat(114)));
}

#[test]
fn one_line_keeps_words_apart_and_bounds_mark_runs() {
    assert_eq!(
        one_line("one\u{2028}two\tthree\n  four", 100),
        "one two three four"
    );
    let heavy = format!("a{}", "\u{301}".repeat(5000));
    let line = one_line(&heavy, 300);
    assert!(line.chars().count() <= 1201, "{}", line.chars().count());
    assert!(line.ends_with(ELLIPSIS));
}

#[test]
fn a_long_blank_line_stays_empty() {
    assert_eq!(one_line(&" ".repeat(8001), 2000), "");
    assert_eq!(one_line(&"\n\t ".repeat(400), 100), "");
}
