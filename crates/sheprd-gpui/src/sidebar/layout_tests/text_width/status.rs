//! Status bar checks.
use super::*;

pub(super) fn check_status_bar(
    view: &Entity<SheprdWindow>,
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
        // The platform is gathered in the background before the report opens.
        cx.run_until_parked();
        let opened = cx.opened_url().unwrap_or_default();
        assert!(opened.starts_with(
            "https://github.com/sheprd-labs/sheprd/issues/new?template=bug_report.yml&version="
        ));
        assert!(opened.contains("&os="));
    }
    Ok(())
}
