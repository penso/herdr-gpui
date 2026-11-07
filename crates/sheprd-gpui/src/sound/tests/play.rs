use super::*;

#[test]
fn play_sends_the_named_sound_whatever_the_notification_settings() {
    let (service, played) = Service::recording();
    for sound in [Sound::Request, Sound::Done] {
        assert!(service.play(sound));
        assert_eq!(played.recv_timeout(Duration::from_secs(3)).unwrap(), sound);
    }
}

#[test]
fn play_reports_a_service_that_cannot_take_it() {
    assert!(!Service::default().play(Sound::Done));
}
