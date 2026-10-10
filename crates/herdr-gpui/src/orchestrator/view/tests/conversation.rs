use crate::orchestrator::view::conversation::{Entry, Summary, ago, entries, summary};
use crate::pr_actions::{Comment, CommentKind, Review};

fn comment(author: &str, kind: CommentKind, at: &str) -> Comment {
    Comment {
        author: author.into(),
        kind,
        body: String::new(),
        created_at: format!("2026-09-20T12:00:{at}Z"),
        source: String::new(),
        avatar: None,
        bot: false,
    }
}

fn thread(author: &str, path: &str, resolved: bool, at: &str) -> Comment {
    let kind = CommentKind::Thread {
        path: path.into(),
        resolved,
        replies: 0,
    };
    comment(author, kind, at)
}

fn shape<'a>(entries: &'a [Entry<'a>]) -> Vec<(&'a str, Option<&'a str>, Vec<&'a str>)> {
    entries
        .iter()
        .map(|entry| match entry {
            Entry::Comment(comment) => (comment.author.as_str(), None, Vec::new()),
            Entry::Review {
                author,
                review,
                threads,
            } => (
                *author,
                Some(review.map_or("gathered", |r| r.created_at.as_str())),
                threads.iter().map(|t| t.created_at.as_str()).collect(),
            ),
        })
        .collect()
}

#[test]
fn threads_join_the_review_their_author_submitted_after_them() {
    let comments = [
        comment("alice", CommentKind::Conversation, "01"),
        thread("bob", "a.rs", false, "02"),
        thread("bob", "b.rs", true, "03"),
        comment("bob", CommentKind::Review(Review::ChangesRequested), "04"),
        thread("bob", "c.rs", false, "05"),
        comment("bob", CommentKind::Review(Review::Approved), "06"),
        // A thread with no later review gathers under its author.
        thread("carol", "d.rs", false, "07"),
        thread("carol", "e.rs", false, "08"),
    ];
    let entries = entries(&comments);
    let at = |s: &str| format!("2026-09-20T12:00:{s}Z");
    let (four, six) = (at("04"), at("06"));
    let (two, three, five, seven, eight) = (at("02"), at("03"), at("05"), at("07"), at("08"));
    assert_eq!(
        shape(&entries),
        vec![
            ("alice", None, vec![]),
            (
                "bob",
                Some(four.as_str()),
                vec![two.as_str(), three.as_str()]
            ),
            ("bob", Some(six.as_str()), vec![five.as_str()]),
            (
                "carol",
                Some("gathered"),
                vec![seven.as_str(), eight.as_str()]
            ),
        ]
    );
}

#[test]
fn the_summary_counts_each_reviewers_newest_verdict() {
    let comments = [
        comment("bob", CommentKind::Review(Review::ChangesRequested), "01"),
        comment("bob", CommentKind::Review(Review::Approved), "02"),
        comment("carol", CommentKind::Review(Review::ChangesRequested), "03"),
        comment("dave", CommentKind::Review(Review::Approved), "04"),
        comment("dave", CommentKind::Review(Review::Dismissed), "05"),
        comment("bob", CommentKind::Review(Review::Commented), "06"),
        thread("carol", "a.rs", false, "07"),
        thread("carol", "b.rs", true, "08"),
    ];
    assert_eq!(
        summary(&comments),
        Summary {
            approved: 1,
            changes_requested: 1,
            open_threads: 1,
            resolved_threads: 1,
            comments: 8,
            people: 3,
        }
    );
}

#[test]
fn times_read_as_how_long_ago() {
    let now = chrono::DateTime::parse_from_rfc3339("2026-09-20T14:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    assert_eq!(ago("2026-09-20T12:00:00Z", now), "2h ago");
    assert_eq!(ago("2026-09-20T13:59:30Z", now), "now");
    assert_eq!(ago("yesterday", now), "");
}
