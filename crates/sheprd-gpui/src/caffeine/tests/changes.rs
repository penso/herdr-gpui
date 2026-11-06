use super::*;

#[gpui::test]
fn every_settled_hold_and_release_counts_as_a_change(cx: &mut gpui::TestAppContext) {
    let (log, reports) = (Log::default(), Reports::default());
    assert_eq!(cx.update(|cx| changes(cx)), 0);
    cx.update(|cx| toggle_with(cx, holder(&log), reports.sink()));
    // Pending is not a change yet.
    assert_eq!(cx.update(|cx| changes(cx)), 0);
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| changes(cx)), 1);
    cx.update(|cx| toggle_with(cx, holder(&log), reports.sink()));
    cx.run_until_parked();
    assert_eq!((cup_of(cx), cx.update(|cx| changes(cx))), (Cup::Off, 2));

    // A failed hold settles too.
    let failing = |_| -> keepawake::Result<Held> {
        Err(keepawake::BuilderError::ValidationError("no power".into()).into())
    };
    cx.update(|cx| {
        cx.set_global(Caffeine::default());
        toggle_with(cx, failing, reports.sink());
    });
    cx.run_until_parked();
    assert_eq!((cup_of(cx), cx.update(|cx| changes(cx))), (Cup::Off, 1));
    assert_eq!(reports.take().len(), 1);
}
