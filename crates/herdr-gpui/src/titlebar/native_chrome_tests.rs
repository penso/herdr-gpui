#![allow(clippy::unwrap_used)]
use gpui::{Bounds, TestAppContext, point, px, size};

#[gpui::test]
fn header_bounds_above_body_in_windowed_and_fullscreen(cx: &mut TestAppContext) {
    let (_, cx) = cx.add_window_view(super::tests::header_window);
    for fullscreen in [false, true, false] {
        cx.update(|window, _| {
            if window.is_fullscreen() != fullscreen {
                window.toggle_fullscreen();
            }
            assert_eq!(window.is_fullscreen(), fullscreen);
        });
        for (width, height) in [(1200., 780.), (640., 400.), (360., 400.)] {
            cx.simulate_resize(size(px(width), px(height)));
            cx.run_until_parked();
            cx.update(|window, cx| {
                window.refresh();
                let _ = window.draw(cx);
            });
            assert_eq!(
                cx.debug_bounds("titlebar").unwrap(),
                Bounds::new(point(px(0.), px(0.)), size(px(width), px(34.)))
            );
            assert_eq!(
                cx.debug_bounds("titlebar-account-slot").unwrap(),
                Bounds::new(point(px(width - 40.), px(0.)), size(px(40.), px(34.)))
            );
            assert_eq!(
                cx.debug_bounds("titlebar-avatar").unwrap(),
                Bounds::new(point(px(width - 34.), px(3.)), size(px(28.), px(28.)))
            );
            assert_eq!(
                cx.debug_bounds("titlebar-avatar-circle").unwrap(),
                Bounds::new(point(px(width - 30.), px(7.)), size(px(20.), px(20.)))
            );
            // Signing in swaps the placeholder for the avatar, which sits
            // inside the same hit target rather than filling it.
            let view = cx.update(|window, _| window.root::<crate::HerdrWindow>().unwrap().unwrap());
            cx.update(|_, cx| {
                view.update(cx, |view, cx| {
                    view.menu.github = crate::github::Auth::connected_fixture();
                    if let Some(profile) = view.menu.github.profile.as_mut() {
                        profile.avatar_url = super::tests::AVATAR_URL.into();
                        view.menu.github_avatars.show(
                            super::tests::AVATAR_URL,
                            std::sync::Arc::new(gpui::Image::from_bytes(
                                gpui::ImageFormat::Svg,
                                include_bytes!("../../../../assets/icons/user.svg").to_vec(),
                            )),
                        );
                    }
                    cx.notify();
                })
            });
            cx.update(|window, cx| {
                window.refresh();
                let _ = window.draw(cx);
            });
            let hit = cx.debug_bounds("titlebar-avatar").unwrap();
            let circle = cx.debug_bounds("titlebar-avatar-circle").unwrap();
            assert_eq!(circle.size, size(px(super::AVATAR), px(super::AVATAR)));
            assert_eq!(circle.center(), hit.center());
            assert!(circle.size.width < hit.size.width);
            cx.update(|_, cx| {
                view.update(cx, |view, cx| {
                    view.menu.github = Default::default();
                    cx.notify();
                })
            });
            cx.update(|window, cx| {
                window.refresh();
                let _ = window.draw(cx);
            });
            // The draggable center starts after Back and Forward and
            // the room they keep.
            let leading = cx.debug_bounds("titlebar-navigation").unwrap().right()
                + px(super::navigation::NAVIGATION_MARGIN);
            assert_eq!(
                cx.debug_bounds("titlebar-center").unwrap(),
                Bounds::new(
                    point(leading, px(0.)),
                    size(px(width - 40.) - leading, px(34.))
                )
            );
            let body = cx.debug_bounds("window-body").unwrap();
            // A worktree build's banner sits at the window's foot.
            assert_eq!(body.top(), px(34.));
            assert_eq!(body.size.width, px(width));
            assert!(body.bottom() <= px(height));
        }
    }
}
