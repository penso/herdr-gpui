use super::*;

#[test]
fn only_the_stable_channel_updates_a_homebrew_installation_through_homebrew() {
    assert!(through_homebrew(UpdateChannel::Stable, || Some(())));
    assert!(!through_homebrew(UpdateChannel::Stable, || None::<()>));
    // A beta never reaches the tap, so the beta channel updates in place.
    assert!(!through_homebrew(UpdateChannel::Beta, || Some(())));
    assert!(!through_homebrew(UpdateChannel::Beta, || None::<()>));
}
