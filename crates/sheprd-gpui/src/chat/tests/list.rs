use super::*;
use crate::chat::list::{Splice, splices};

fn keys(pairs: &[(u64, u64)]) -> Vec<(EntryId, u64)> {
    pairs
        .iter()
        .map(|&(id, revision)| (EntryId(id), revision))
        .collect()
}

#[test]
fn a_new_entry_appends_a_row() {
    assert_eq!(
        splices(&keys(&[(1, 0)]), &keys(&[(1, 0), (2, 3)])),
        Some(vec![Splice {
            range: 1..1,
            count: 1
        }])
    );
    assert_eq!(
        splices(&[], &keys(&[(1, 0), (2, 0)])),
        Some(vec![Splice {
            range: 0..0,
            count: 2
        }])
    );
}

#[test]
fn a_streamed_chunk_remeasures_its_row() {
    assert_eq!(
        splices(&keys(&[(1, 0), (2, 1)]), &keys(&[(1, 0), (2, 2)])),
        Some(vec![Splice {
            range: 1..2,
            count: 1
        }])
    );
    assert_eq!(
        splices(&keys(&[(1, 0)]), &keys(&[(1, 0)])),
        Some(Vec::new())
    );
}

#[test]
fn a_truncation_drops_rows_from_the_front() {
    assert_eq!(
        splices(&keys(&[(1, 0), (2, 0), (3, 0)]), &keys(&[(3, 0), (4, 1)])),
        Some(vec![
            Splice {
                range: 0..2,
                count: 0
            },
            Splice {
                range: 1..1,
                count: 1
            }
        ])
    );
}

#[test]
fn anything_else_starts_over() {
    assert_eq!(
        splices(&keys(&[(1, 0), (2, 0)]), &keys(&[(1, 0), (3, 0)])),
        None
    );
    assert_eq!(splices(&keys(&[(1, 0), (2, 0)]), &keys(&[(1, 0)])), None);
    assert_eq!(splices(&keys(&[(1, 0)]), &[]), None);
    assert_eq!(splices(&[], &[]), Some(Vec::new()));
}
