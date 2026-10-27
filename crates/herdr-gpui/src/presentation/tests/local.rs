use super::*;

#[test]
fn local_navigation_releases_earlier_screens() {
    let mut presentation = Presentation::default();
    let mut a = connected("boot", 1);
    Arc::make_mut(a.snapshot.as_mut().unwrap()).focused_tab_id = Some("t0".into());
    let weak = Arc::downgrade(a.surface.as_ref().unwrap());
    presentation.remember(&a, false);
    drop(a);
    assert!(weak.upgrade().is_some(), "the current picture still paints");

    let mut b = connected("boot", 2);
    Arc::make_mut(b.snapshot.as_mut().unwrap()).focused_tab_id = Some("t1".into());
    presentation.remember(&b, false);
    assert!(weak.upgrade().is_none(), "no visited screen is cached");
    let current = b.surface.take().unwrap();
    let picture = presentation
        .navigation_picture(&b, Some(NavigationTarget::Tab("t0")), (0, 0))
        .unwrap();
    assert!(Arc::ptr_eq(&picture.frame, &current));
}
