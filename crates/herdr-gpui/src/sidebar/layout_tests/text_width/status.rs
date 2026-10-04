//! Install modal, app update panel, and status bar checks.
use super::super::*;

pub(super) fn check_install_modal(
    view: &Entity<HerdrWindow>,
    cx: &mut gpui::VisualTestContext,
) -> Option<Arc<ClientShellSnapshot>> {
    let before_install = cx.update(|_, cx| view.read(cx).live.snapshot.clone());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.show_install_modal(window, cx));
    });
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        let view = view.read(cx);
        assert!(view.menu.page == Some(crate::menu::Page::Install));
        assert!(!view.live.missing_installation);
        assert_eq!(view.live.snapshot, before_install);
    });
    assert!(cx.debug_bounds("menu-install").is_some());
    assert!(cx.debug_bounds("menu-dismiss").is_some());
    cx.simulate_keystrokes("escape");
    cx.update(|_, cx| assert!(view.read(cx).menu.page.is_none()));
    before_install
}

pub(super) fn check_app_update(
    view: &Entity<HerdrWindow>,
    before_install: &Option<Arc<ClientShellSnapshot>>,
    cx: &mut gpui::VisualTestContext,
) -> Result<()> {
    // Fixtures have no updater worker, and unavailable updates use the shared panel.
    let updater_before = cx.update(|_, cx| view.read(cx).updater.state().clone());
    assert!(matches!(updater_before, crate::updater::State::Disabled(_)));
    cx.update(|window, cx| window.dispatch_action(Box::new(crate::CheckForUpdates), cx));
    assert!(cx.pending_prompt().is_none());
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        assert!(view.read(cx).menu.page == Some(crate::menu::Page::AppUpdate));
        assert_eq!(view.read(cx).live.snapshot, *before_install);
    });
    assert!(cx.debug_bounds("app-update-action").is_none());
    let releases = cx
        .debug_bounds("app-update-releases")
        .context("update releases bounds")?;
    cx.simulate_click(releases.center(), Default::default());
    assert_eq!(
        cx.opened_url().as_deref(),
        Some("https://github.com/penso/herdr-gpui/releases")
    );
    let close = cx
        .debug_bounds("app-update-close")
        .context("update close bounds")?;
    cx.simulate_click(close.center(), Default::default());
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert!(view.menu.page.is_none());
        assert!(view.focus.is_focused(window));
        assert_eq!(view.updater.state(), &updater_before);
    });
    for (width, height) in [(320., 360.), (320., 600.), (480., 600.), (800., 600.)] {
        cx.simulate_resize(size(px(width), px(height)));
        cx.update(|window, cx| window.dispatch_action(Box::new(crate::ShowUpdatePreview), cx));
        for ready in [false, true] {
            cx.update(|window, cx| {
                full_draw(window, cx).clear(cx);
                let view = view.read(cx);
                assert_eq!(view.updater.state(), &updater_before);
                assert_eq!(view.live.snapshot, *before_install);
                assert_eq!(
                    view.update_preview,
                    Some(if ready {
                        crate::updater::State::Ready {
                            version: "9999.0.0".into(),
                        }
                    } else {
                        crate::updater::State::Available {
                            version: "9999.0.0".into(),
                        }
                    })
                );
            });
            let panel = cx
                .debug_bounds("app-update-panel")
                .context("update panel bounds")?;
            let action = cx
                .debug_bounds("app-update-action")
                .context("update action bounds")?;
            let header = cx
                .debug_bounds("app-update-header")
                .context("update header bounds")?;
            let close = cx
                .debug_bounds("app-update-close")
                .context("update close bounds")?;
            assert_eq!(close.right(), header.right() - px(16.));
            assert!(close.left() > header.center().x);
            assert!(close.top() >= header.top() && close.bottom() <= header.bottom());
            assert!(header.bottom() < action.top());
            let body = cx
                .debug_bounds("app-update-body")
                .context("update body bounds")?;
            let footer = cx
                .debug_bounds("app-update-footer")
                .context("update footer bounds")?;
            let current = cx
                .debug_bounds("app-update-current-version")
                .context("current version bounds")?;
            let latest = cx
                .debug_bounds("app-update-latest-version")
                .context("latest version bounds")?;
            assert_eq!(current.left(), latest.left());
            assert_eq!(current.right(), latest.right());
            assert!(current.bottom() < latest.top());
            assert_eq!(header.left(), panel.left());
            assert_eq!(header.right(), panel.right());
            assert!(body.top() >= header.bottom());
            assert!((footer.top() - body.bottom()).abs() <= px(1.));
            assert!(panel.top() >= px(0.) && panel.bottom() <= px(height));
            assert!(action.top() >= footer.top() && action.bottom() <= footer.bottom());
            assert!(panel.left() >= px(0.) && panel.right() <= px(width));
            assert!(action.left() >= panel.left() && action.right() <= panel.right());
            assert!(action.top() >= panel.top() && action.bottom() <= panel.bottom());
            cx.simulate_click(action.center(), Default::default());
        }
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert!(view.menu.page.is_none());
            assert!(view.update_preview.is_none());
            assert_eq!(view.updater.state(), &updater_before);
        });
        assert!(cx.pending_prompt().is_none());
    }
    // The same panel is reachable without native menus, including on Linux.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.open_menu(window, cx));
        full_draw(window, cx).clear(cx);
    });
    let updates = cx
        .debug_bounds("menu-app updates")
        .context("app updates menu bounds")?;
    assert!(cx.debug_bounds("menu-preview app update").is_some());
    cx.simulate_click(updates.center(), Default::default());
    cx.update(|_, cx| {
        assert!(view.read(cx).menu.page == Some(crate::menu::Page::AppUpdate));
        assert!(view.read(cx).update_preview.is_none());
    });
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| window.dispatch_action(Box::new(crate::ShowUpdatePreview), cx));
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert!(view.menu.page.is_none());
        assert!(view.update_preview.is_none());
        assert!(view.focus.is_focused(window));
        assert_eq!(view.updater.state(), &updater_before);
    });
    Ok(())
}

pub(super) fn check_status_bar(
    view: &Entity<HerdrWindow>,
    cx: &mut gpui::VisualTestContext,
) -> Result<()> {
    // Exercise the real status bar without starting a daemon connection.
    view.update(cx, |view, cx| {
        view.marked = "composition ".repeat(100);
        view.local_error = Some("long connection error ".repeat(100));
        cx.notify();
    });
    for width in [480., 800.] {
        cx.simulate_resize(size(px(width), px(600.)));
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        let status = cx.debug_bounds("connection-status").unwrap();
        let report = cx.debug_bounds("report-issue").unwrap();
        assert!(report.size.width >= px(33.));
        assert!(report.left() >= status.left());
        assert!(report.right() <= status.right());
        assert!(report.top() >= status.top());
        assert!(report.bottom() <= status.bottom());
        let version = cx
            .debug_bounds("status-version")
            .context("status version bounds")?;
        assert!(version.size.width > px(0.));
        assert!(version.left() >= report.right());
        assert!(version.right() <= status.right());
        assert!(version.top() >= status.top());
        assert!(version.bottom() <= status.bottom());
        let theme = cx.debug_bounds("status-theme").unwrap();
        let keybinds = cx.debug_bounds("status-keybinds").unwrap();
        assert!(theme.left() >= status.left());
        assert!(theme.right() <= keybinds.left());
        assert!(keybinds.right() <= report.left());
        for button in [theme, keybinds] {
            assert!(button.size.width > px(0.));
            assert!(button.top() >= status.top());
            assert!(button.bottom() <= status.bottom());
        }
        cx.simulate_click(report.center(), Default::default());
        assert_eq!(
            cx.opened_url().as_deref(),
            Some(
                format!(
                    "https://github.com/penso/herdr-gpui/issues/new?template=bug_report.yml&version={}",
                    crate::APP_VERSION.replace('+', "%2B"),
                )
                .as_str()
            )
        );
    }
    Ok(())
}
