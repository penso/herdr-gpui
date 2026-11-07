use super::*;

#[gpui::test]
fn each_hold_keeps_what_the_scope_said_when_it_was_asked_for(cx: &mut gpui::TestAppContext) {
    let (seen, reports) = (Arc::new(Mutex::new(Vec::new())), Reports::default());
    let start = {
        let seen = seen.clone();
        move |scope| {
            seen.lock().unwrap().push(scope);
            Ok(Held(Log::default()))
        }
    };
    cx.update(|cx| {
        set_scope(AwakeScope::System, cx);
        toggle_with(cx, start, reports.sink());
    });
    cx.run_until_parked();
    cx.update(|cx| {
        toggle_with(
            cx,
            |_| -> keepawake::Result<Held> { unreachable!() },
            reports.sink(),
        )
    });
    cx.run_until_parked();
    assert_eq!(cup_of(cx), Cup::Off);

    // The worker outlives a release, and its next hold follows the new scope.
    cx.update(|cx| {
        set_scope(AwakeScope::Display, cx);
        toggle_with(
            cx,
            |_| -> keepawake::Result<Held> { unreachable!() },
            reports.sink(),
        );
    });
    cx.run_until_parked();
    assert_eq!(cup_of(cx), Cup::On(Mode::Indefinite));
    assert_eq!(
        *seen.lock().unwrap(),
        [AwakeScope::System, AwakeScope::Display]
    );
    assert!(reports.take().is_empty());
}

#[test]
fn display_is_the_default_scope() {
    assert_eq!(Caffeine::default().scope, AwakeScope::Display);
}

#[gpui::test]
fn an_active_hold_reports_the_scope_it_started_with(cx: &mut gpui::TestAppContext) {
    let (log, reports) = (Log::default(), Reports::default());
    cx.update(|cx| {
        set_scope(AwakeScope::System, cx);
        toggle_with(cx, holder(&log), reports.sink());
    });
    cx.run_until_parked();

    // A config change mid-hold waits for the next hold, tooltip included.
    cx.update(|cx| {
        set_scope(AwakeScope::Display, cx);
        assert_eq!(scope(cx), AwakeScope::System);
    });
    cx.update(|cx| toggle_with(cx, holder(&log), reports.sink()));
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| scope(cx)), AwakeScope::Display);
}

#[gpui::test]
fn install_takes_the_scope_from_the_loaded_config(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let mut appearance = crate::app::InitialAppearance::default();
        appearance.config.status_bar.keep_awake_scope = AwakeScope::System;
        cx.set_global(appearance);
        install(cx);
        assert_eq!(scope(cx), AwakeScope::System);
    });
}
