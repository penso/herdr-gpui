use super::*;

#[test]
fn sounds_start_at_the_default_volume() {
    let (service, volumes) = Service::recording_volume();
    service.preview();
    assert_eq!(
        volumes.recv_timeout(Duration::from_secs(3)).unwrap(),
        crate::config::SoundConfig::default().volume
    );
}

#[test]
fn each_sound_plays_at_the_volume_set_before_it() {
    let (service, volumes) = Service::recording_volume();
    for percent in [0, 30, 100] {
        service.set_volume(percent);
        service.preview();
        assert_eq!(
            volumes.recv_timeout(Duration::from_secs(3)).unwrap(),
            percent
        );
    }
}
