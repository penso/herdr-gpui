use super::*;
use crate::cloud::CloudProvider;
use std::time::Instant;

#[test]
fn a_new_machine_is_a_choice_even_with_one_host() {
    let now = Instant::now();
    let mut picker = Picker::new("laptop", now);
    picker.update(vec![current(host("laptop", 8, 1.))], now);
    assert!(!picker.offers_choice());
    picker.offer_sandboxes(vec![CloudProvider::Container]);
    assert!(picker.offers_choice());
    assert_eq!(picker.destination(), Destination::Here);
}

#[test]
fn choosing_a_new_machine_replaces_the_host_until_a_host_is_chosen() {
    let now = Instant::now();
    let mut picker = Picker::new("laptop", now);
    picker.update(
        vec![current(host("laptop", 8, 1.)), host("beelink", 16, 1.)],
        now,
    );
    picker.offer_sandboxes(vec![CloudProvider::Container]);
    assert!(picker.choose("beelink"));
    assert!(picker.choose_sandbox(CloudProvider::Container));
    assert!(
        !picker.choose_sandbox(CloudProvider::Container),
        "already chosen"
    );
    assert_eq!(
        picker.destination(),
        Destination::Sandbox(CloudProvider::Container)
    );
    assert!(picker.dispatched().is_none());
    assert!(picker.picked().is_none(), "no host tile shows as chosen");
    // Choosing the host that was chosen before still takes it back.
    assert!(picker.choose("beelink"));
    assert!(
        matches!(picker.destination(), Destination::Host(host) if host.endpoint_id == "beelink")
    );
}

#[test]
fn a_machine_no_longer_offered_is_no_longer_chosen() {
    let now = Instant::now();
    let mut picker = Picker::new("laptop", now);
    picker.update(vec![current(host("laptop", 8, 1.))], now);
    picker.offer_sandboxes(vec![CloudProvider::Container]);
    picker.choose_sandbox(CloudProvider::Container);
    picker.offer_sandboxes(Vec::new());
    assert_eq!(picker.destination(), Destination::Here);
}
