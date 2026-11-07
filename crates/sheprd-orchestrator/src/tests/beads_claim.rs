//! What `bd` says when it refuses a claim, as `bd` 1.3 words it.
use crate::beads::claim::holder;

#[test]
fn a_refused_claim_names_who_holds_the_bead() {
    let stderr = "Error updating tt-1: issue already claimed by Ada Lovelace\n\
                  {\"error\":\"1 of 1 issues failed to update\"}\n";
    assert_eq!(holder(stderr), Some(Some("Ada Lovelace".to_owned())));
}

#[test]
fn an_assigned_bead_names_its_assignee() {
    let stderr = "Error updating tt-1: issue already claimed: already assigned to \"carol\" \
                  — coordinate with the holder\n";
    assert_eq!(holder(stderr), Some(Some("carol".to_owned())));
}

#[test]
fn a_claim_refused_without_a_name_is_still_held() {
    assert_eq!(holder("issue already claimed\n"), Some(None));
}

#[test]
fn other_failures_are_not_a_conflict() {
    assert_eq!(holder("issue not claimable: status closed\n"), None);
    assert_eq!(holder(""), None);
}
