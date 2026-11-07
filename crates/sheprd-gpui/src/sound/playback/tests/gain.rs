use super::*;

#[test]
fn gain_follows_loudness_rather_than_amplitude() {
    assert_eq!(gain(0), 0.);
    assert_eq!(gain(50), 0.25);
    assert_eq!(gain(100), 1.);
    // Each step up is louder than the last.
    for percent in 1..=100 {
        assert!(gain(percent) > gain(percent - 1));
    }
}

#[test]
fn gain_never_amplifies_past_the_file_level() {
    assert_eq!(gain(101), 1.);
    assert_eq!(gain(u8::MAX), 1.);
}

#[test]
fn silent_volume_plays_nothing_without_opening_a_device() {
    let flag = AtomicBool::new(false);
    // A cancelled job would fail; volume 0 returns before any check.
    let cancelled = AtomicBool::new(true);
    assert!(play(Sound::Done, None, 0, &[&cancelled], &flag).is_ok());
}
