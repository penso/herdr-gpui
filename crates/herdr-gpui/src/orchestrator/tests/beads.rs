use super::*;
use crate::orchestrator::beads::{flags_from_help, parse, valid_id};

#[test]
fn beads_records_become_items_with_parents_and_sorted_blockers() {
    let list = br#"[
      {"id":"hg-a3f2","title":"Epic","status":"open","priority":1,"created_by":"penso",
       "labels":["epic"],"created_at":"2026-10-07T10:00:00Z","updated_at":"2026-10-08T10:00:00Z"},
      {"id":"hg-a3f2.3","title":"Child","description":"Body","status":"blocked","priority":2,
       "dependencies":[{"depends_on_id":"hg-a3f2","type":"parent-child"},{"depends_on_id":"x","type":"blocks"}]}
    ]"#;
    let blocked = br#"{"issues":[{"id":"hg-a3f2.3","blocked_by":["hg-a3f2.1",{"depends_on_id":"hg-a3f2.0"},"hg-a3f2.1"]}]}"#;
    let items = parse(&beads(), list, blocked).unwrap();
    assert_eq!(items.len(), 2);
    let epic = &items[0];
    assert_eq!(epic.identifier, "hg-a3f2");
    assert_eq!(
        epic.key.canonical(),
        "beads:local:/Users/me/src/herdr-gpui:hg-a3f2"
    );
    assert_eq!(
        (epic.priority, epic.author.as_deref()),
        (Some(1), Some("penso"))
    );
    assert_eq!(epic.updated_at, Some(at("2026-10-08T10:00:00Z")));
    let child = &items[1];
    assert_eq!(child.parent_id.as_deref(), Some("hg-a3f2"));
    assert_eq!(child.blocked_by, ["hg-a3f2.0", "hg-a3f2.1"]);
    assert_eq!(child.state, "blocked");
    assert!(child.url.is_none() && child.activity.is_none());
}

#[test]
fn malformed_beads_output_is_a_typed_error() {
    assert!(matches!(
        parse(&beads(), b"not json", b"[]"),
        Err(Error::OutputJson {
            operation: "Reading Beads",
            ..
        })
    ));
}

#[test]
fn safety_flags_come_from_bd_help() {
    let help = "Flags:\n  --readonly   Read only\n  --sandbox    No sync\n  --db string";
    assert_eq!(flags_from_help(help), ["--readonly", "--sandbox"]);
    assert!(flags_from_help("Flags:\n  --db string").is_empty());
    // A mention inside a word is not the flag.
    assert!(flags_from_help("use --readonly-ish").is_empty());
}

#[test]
fn only_plain_bead_ids_can_be_deleted() {
    for id in ["hg-a3f2", "moltis-l04w", "a.b_c-1"] {
        assert!(valid_id(id), "{id}");
    }
    for id in ["", "-rf", "../x", "a b", "a;rm", &"a".repeat(256)] {
        assert!(!valid_id(id), "{id}");
    }
}
