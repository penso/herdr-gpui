use super::*;

#[test]
fn blur_uses_dwm_only_where_it_has_system_backdrops() {
    assert_eq!(route(Appearance::Blurred, true), Route::Dwm);
    assert_eq!(route(Appearance::Blurred, false), Route::Accent);
    assert_eq!(route(Appearance::MicaBackdrop, true), Route::Dwm);
    assert_eq!(route(Appearance::Transparent, true), Route::Accent);
    assert_eq!(route(Appearance::Opaque, true), Route::None);
}

#[test]
fn mica_falls_back_to_the_older_blur_before_system_backdrops() {
    assert_eq!(
        available(Appearance::MicaBackdrop, false),
        Appearance::Blurred
    );
    assert_eq!(route(Appearance::MicaBackdrop, false), Route::Accent);
    // Switching between the two materials there changes nothing.
    assert_eq!(
        route(Appearance::Blurred, false),
        route(Appearance::MicaBackdrop, false)
    );
    assert_eq!(
        available(Appearance::MicaBackdrop, true),
        Appearance::MicaBackdrop
    );
    assert_eq!(
        available(Appearance::Transparent, false),
        Appearance::Transparent
    );
}
