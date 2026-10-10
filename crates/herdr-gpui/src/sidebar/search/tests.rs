#![allow(clippy::unwrap_used)]

use super::*;
use crate::sidebar::layout_tests::snapshot;

mod labels;

#[test]
fn find_ignores_case_and_measures_the_original_bytes() {
    assert_eq!(find("Herdr-GPUI", "gpui"), Some(6..10));
    assert_eq!(find("studio.lan", "  LAN "), Some(7..10));
    // Lowercasing changes neither the match nor where it lands.
    assert_eq!(find("Ünïcode box", "üNÏ"), Some(0..5));
    assert_eq!(find("café", "É"), Some(3..5));
    // `İ` lowercases to `i` and a combining dot: a query ending inside that
    // expansion still matches the whole character.
    assert_eq!(find("İstanbul", "i"), Some(0..2));
    assert_eq!(find("İstanbul", "i\u{307}s"), Some(0..3));
    assert_eq!(find("İstanbul", "is"), None);
    assert_eq!(find("main", "mains"), None);
    assert_eq!(find("main", ""), None);
    assert_eq!(find("main", "   "), None);
}

fn devices(snapshot: &ClientShellSnapshot) -> [Device<'_>; 2] {
    [
        Device {
            id: "local",
            label: "MacBook Pro",
            workspaces: &snapshot.workspaces,
        },
        Device {
            id: "studio",
            label: "studio.lan",
            workspaces: &[],
        },
    ]
}

use herdr_client::protocol::ClientShellSnapshot;

#[test]
fn results_group_devices_then_worktrees_then_branches_in_sidebar_order() {
    let snapshot = snapshot(6);
    let hits = search("sidebar-child", &devices(&snapshot));
    let shown: Vec<_> = hits
        .iter()
        .map(|hit| (hit.kind, hit.text.as_str(), hit.context.as_str()))
        .collect();
    assert_eq!(
        shown,
        [
            // Linked worktrees match by the name their rows show.
            (
                Kind::Worktree,
                "sidebar-child",
                "MacBook Pro · worktree/sidebar-child"
            ),
            (
                Kind::Worktree,
                "sidebar-child-with-a-long-readable-branch-name",
                "MacBook Pro · worktree/sidebar-child-with-a-long-readable-branch-name"
            ),
            (
                Kind::Branch,
                "worktree/sidebar-child",
                "MacBook Pro · sidebar-child"
            ),
            (
                Kind::Branch,
                "worktree/sidebar-child-with-a-long-readable-branch-name",
                "MacBook Pro · sidebar-child-with-a-long-readable-branch-name"
            ),
        ]
    );
    assert_eq!(
        hits[0].target,
        Target::Workspace {
            endpoint: "local".into(),
            workspace: "w4".into()
        }
    );
    assert_eq!(hits[2].range, 9..22);

    let hits = search("STUDIO", &devices(&snapshot));
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, Kind::Device);
    assert_eq!(hits[0].context, "0 spaces");
    assert_eq!(
        hits[0].target,
        Target::Device {
            endpoint: "studio".into()
        }
    );
}

#[test]
fn a_single_device_leaves_its_name_out_of_the_context() {
    let snapshot = snapshot(6);
    let [local, _] = devices(&snapshot);
    let hits = search("develop", &[local]);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, Kind::Branch);
    assert_eq!(hits[0].context, "agent-launcher");
    assert_eq!(hits[0].status, Some(AgentStatus::Working));
}

#[test]
fn results_are_bounded() {
    let snapshot = snapshot(RESULT_LIMIT + 50);
    let [local, _] = devices(&snapshot);
    // "another workspace" names most of them, and their branch matches "o" too.
    assert_eq!(search("o", &[local]).len(), RESULT_LIMIT);
}

#[test]
fn each_section_is_bounded_before_the_sections_join() {
    let snapshot = snapshot(RESULT_LIMIT + 50);
    let [local, _] = devices(&snapshot);
    // "fix/sidebar-…" names most branches but no workspace label, so the
    // branches section alone fills the results, in sidebar order.
    let hits = search("fix/", &[local]);
    assert_eq!(hits.len(), RESULT_LIMIT);
    assert!(hits.iter().all(|hit| hit.kind == Kind::Branch));
}
