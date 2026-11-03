#![allow(clippy::unwrap_used)]

use super::*;

#[test]
fn a_fence_opens_within_three_spaces_and_keeps_its_info() {
    let fence = Fence::open("   ```Mermaid \n").unwrap();
    assert_eq!(fence.info, "Mermaid");
    assert_eq!(Fence::open("~~~~").unwrap().info, "");
    for line in ["    ```", "\t```", "``", "``` a`b", "text"] {
        assert_eq!(Fence::open(line), None, "{line:?}");
    }
    assert_eq!(Fence::open("~~~ a`b").unwrap().info, "a`b");
}

#[test]
fn only_a_long_enough_run_of_the_same_marker_closes_a_fence() {
    let fence = Fence::open("````rust").unwrap();
    assert!(fence.closes("````\n"));
    assert!(fence.closes("   `````  \r\n"));
    assert!(!fence.closes("```"));
    assert!(!fence.closes("    ````"));
    assert!(!fence.closes("~~~~"));
    assert!(!fence.closes("```` x"));
}
