use super::*;
use std::time::Duration;

/// Windows share one sound worker but reload their config separately, so a
/// window still holding an older volume must not keep writing it back over
/// the volume a newer load set.
#[gpui::test]
fn window_ticks_leave_a_newer_volume_alone(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    let (sound, volumes) = crate::sound::Service::recording_volume();
    let sound = std::rc::Rc::new(sound);
    view.update(cx, |view, _| {
        view.config.sound.volume = 30;
        view.use_sound(sound.clone());
    });
    sound.preview();
    assert_eq!(volumes.recv_timeout(Duration::from_secs(3)).unwrap(), 30);

    // Another window's load lands while this one's reloads are held back.
    sound.set_volume(80);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            for _ in 0..3 {
                view.tick(window, cx);
            }
        });
    });
    sound.preview();
    assert_eq!(volumes.recv_timeout(Duration::from_secs(3)).unwrap(), 80);
}
