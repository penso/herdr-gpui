use super::*;
use PaneSurfaceSplitDirection::{Horizontal, Vertical};

/// A root split with its second side divided `below` more times along one
/// chain of `direction` splits: one pane beside `below + 1`.
fn beside_chain(direction: PaneSurfaceSplitDirection, below: usize) -> Vec<PaneSurfaceSplit> {
    (0..=below)
        .map(|depth| match depth {
            0 => split(Horizontal, vec![]),
            _ => split(direction, vec![true; depth]),
        })
        .collect()
}

#[test]
fn each_split_gives_its_first_side_its_share_of_the_panes() {
    let cases = [
        (
            "2x2",
            vec![
                split(Horizontal, vec![]),
                split(Vertical, vec![false]),
                split(Vertical, vec![true]),
            ],
            vec![(vec![], 0.5), (vec![false], 0.5), (vec![true], 0.5)],
        ),
        (
            "one pane beside two stacked",
            beside_chain(Vertical, 1),
            vec![(vec![], 1.0f32 / 3.0), (vec![true], 0.5)],
        ),
        (
            "three columns",
            beside_chain(Horizontal, 1),
            vec![(vec![], 1.0f32 / 3.0), (vec![true], 0.5)],
        ),
        (
            "one pane beside a stack of eight",
            beside_chain(Vertical, 7),
            vec![(vec![], 1.0f32 / 9.0), (vec![true; 7], 0.5)],
        ),
        (
            "twelve same-direction splits stop at Herdr's floor",
            beside_chain(Horizontal, 11),
            vec![(vec![], 0.1), (vec![true; 11], 0.5)],
        ),
        (
            "a split whose parent is missing",
            vec![split(Vertical, vec![true])],
            vec![(vec![true], 0.5)],
        ),
    ];
    for (name, splits, want) in cases {
        let got = even_targets(&splits);
        assert_eq!(got.len(), splits.len(), "{name}: one target per split");
        for (path, ratio) in want {
            let target = got.iter().find(|target| target.path == path).unwrap();
            assert_eq!(target.ratio, ratio, "{name}: {path:?}");
        }
    }
}
