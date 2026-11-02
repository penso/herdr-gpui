use super::*;
use crate::worktree_identity::Error;

#[test]
fn every_worktree_identity_variant_has_a_message() {
    assert_localized(&[Error::InvalidTint("teal".into())]);
}
