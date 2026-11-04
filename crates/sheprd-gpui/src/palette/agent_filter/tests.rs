use super::super::Action;
use super::*;
use crate::NavigationTarget;

#[test]
fn alt_and_a_navigator_letter_choose_a_filter() {
    let parse = |text| AgentFilter::from_keystroke(&Keystroke::parse(text).unwrap());
    for (text, expected) in [
        ("alt-a", AgentFilter::All),
        ("alt-b", AgentFilter::Blocked),
        ("alt-w", AgentFilter::Working),
        ("alt-i", AgentFilter::Idle),
        ("alt-d", AgentFilter::Done),
    ] {
        assert_eq!(parse(text), Some(expected), "{text}");
    }
    for text in [
        "b",
        "shift-b",
        "ctrl-b",
        "cmd-b",
        "alt-shift-b",
        "ctrl-alt-b",
        "alt-x",
        "alt-1",
    ] {
        assert_eq!(parse(text), None, "{text}");
    }
}

#[test]
fn a_status_filter_keeps_only_agents_reporting_that_status() {
    let statuses = [
        None,
        Some(AgentStatus::Idle),
        Some(AgentStatus::Working),
        Some(AgentStatus::Blocked),
        Some(AgentStatus::Done),
        Some(AgentStatus::Unknown),
    ];
    for filter in AgentFilter::ALL {
        let kept: Vec<_> = statuses
            .into_iter()
            .filter(|status| filter.accepts(*status))
            .collect();
        let expected = match filter {
            AgentFilter::All => statuses.to_vec(),
            AgentFilter::Blocked => vec![Some(AgentStatus::Blocked)],
            AgentFilter::Working => vec![Some(AgentStatus::Working)],
            AgentFilter::Idle => vec![Some(AgentStatus::Idle)],
            AgentFilter::Done => vec![Some(AgentStatus::Done)],
        };
        assert_eq!(kept, expected, "{filter:?}");
    }
}

#[test]
fn headings_return_above_their_agents_in_the_order_of_each_workspaces_best_agent() {
    // w1 > a, and w2 > tab > b, c.
    let rows = [
        ("w1", None),
        ("a", Some(0)),
        ("w2", None),
        ("tab", Some(2)),
        ("b", Some(3)),
        ("c", Some(3)),
    ];
    let entries: Vec<_> = rows
        .iter()
        .map(|(label, parent)| {
            Entry::new(
                (*label).to_owned(),
                String::new(),
                "",
                Action::Go {
                    endpoint: String::new(),
                    boot: String::new(),
                    target: NavigationTarget::Pane((*label).to_owned()),
                },
                *parent,
            )
        })
        .collect();
    let hit = |index| search::Hit {
        index,
        depth: 0,
        highlights: search::Highlights::default(),
    };
    // The query ranked c first, then a, then b.
    let shown: Vec<_> = headed(&entries, vec![hit(5), hit(1), hit(4)])
        .into_iter()
        .map(|hit| (rows[hit.index].0, hit.depth))
        .collect();
    assert_eq!(
        shown,
        [
            ("w2", 0),
            ("tab", 1),
            ("b", 2),
            ("c", 2),
            ("w1", 0),
            ("a", 1)
        ]
    );
    assert!(headed(&entries, Vec::new()).is_empty());
}
