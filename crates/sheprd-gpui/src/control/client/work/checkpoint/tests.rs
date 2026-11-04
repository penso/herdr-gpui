use super::*;

#[test]
fn listings_print_one_line_per_checkpoint_newest_first() {
    let checkpoint = |id: &str, created, files| CheckpointInfo {
        id: id.into(),
        created,
        label: format!("label {id}"),
        branch: Some("main".into()),
        files,
        additions: 3,
        deletions: 1,
    };
    let now = 1_700_007_200;
    assert_eq!(
        describe(
            &[
                checkpoint("0001700007100000", 1_700_007_100, 1),
                checkpoint("0001700000000000", 1_700_000_000, 2),
            ],
            "main",
            now
        ),
        "0001700007100000  1 min ago     label 0001700007100000  (1 file, +3 -1)\n\
         0001700000000000  2 h ago       label 0001700000000000  (2 files, +3 -1)"
    );
    assert_eq!(describe(&[], "main", now), "main has no checkpoints");
    let json = serde_json::to_value(checkpoint("1", 5, 0)).unwrap_or_default();
    assert_eq!(
        json,
        serde_json::json!({
            "id": "1", "created": 5, "label": "label 1", "branch": "main",
            "files": 0, "additions": 3, "deletions": 1
        })
    );
}
