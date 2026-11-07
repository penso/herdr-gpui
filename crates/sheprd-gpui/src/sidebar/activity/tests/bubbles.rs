//! A count bubble stays inside the bar however wide its count, and shows in
//! the accent only while it holds something changed since its section was
//! last open.
use super::*;
use crate::security::{Alert, Alerts, Repo, Reports, Severity};
use crate::sidebar::activity::counts::Counts;
use crate::sidebar::layout_tests::{SidebarFixture, fixture_window, full_draw};
use chrono::{DateTime, Duration, Utc};
use gpui::{AppContext as _, Entity, VisualTestContext, px, size};
use std::collections::{HashMap, HashSet};

fn fixture(cx: &mut gpui::TestAppContext) -> (Entity<SheprdWindow>, &mut VisualTestContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            view.config.sidebar_style.activity_bar = true;
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    cx.simulate_resize(size(px(1000.), px(700.)));
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    (view, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        full_draw(window, cx).clear(cx);
    });
}

fn alerts(count: u64, created_at: DateTime<Utc>) -> Repo {
    let alert = |number| Alert {
        number,
        severity: Severity::Critical,
        package: "rustls-webpki".into(),
        ecosystem: "rust".into(),
        vulnerable: "< 0.102.8".into(),
        patched: Some("0.102.8".into()),
        manifest: "Cargo.lock".into(),
        advisory: "GHSA-xpw8-rcwv-8f8p".into(),
        summary: "Name constraints were ignored".into(),
        created_at: Some(created_at),
    };
    Repo {
        name: "sheprd-labs/sheprd".into(),
        host: "Local".into(),
        alerts: Ok(Alerts::Open {
            alerts: (1..=count).map(alert).collect(),
            total: count,
        }),
        reports: Ok(Reports::Off),
        account: None,
        checkouts: Vec::new(),
    }
}

#[gpui::test]
fn a_wide_count_stays_inside_the_bar(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    for count in [3, 42, 150] {
        view.update(cx, |view, cx| {
            view.security.read_for_test(vec![alerts(count, Utc::now())]);
            cx.notify();
        });
        draw(cx);
        let bar = cx.debug_bounds("activity-bar").unwrap();
        let bubble = cx.debug_bounds("activity-security-bubble").unwrap();
        let glyph = cx.debug_bounds("activity-security-glyph").unwrap();
        // Inside the bar's 1px right border, whatever the count's width.
        assert!(
            bubble.right() <= bar.right() - px(1.),
            "{count}: {bubble:?}"
        );
        assert!(bubble.left() > bar.left(), "{count}: {bubble:?}");
        // Still on the icon's corner.
        assert!(bubble.left() < glyph.right(), "{count}: {bubble:?}");
        assert!(bubble.top() < glyph.top(), "{count}: {bubble:?}");
    }
}

#[test]
fn a_count_is_unseen_until_its_section_opens_then_only_for_newer_items() {
    let mut activity = super::super::Activity::default();
    let earlier = Utc::now() - Duration::hours(1);
    let later = Utc::now() + Duration::hours(1);
    let panel = ActivityPanel::PullRequests;
    // Never opened: whatever it counts is new.
    assert!(activity.unseen(panel, None));
    assert!(activity.unseen(panel, Some(earlier)));
    // Shown: the user is looking at it.
    assert!(activity.pick(panel));
    assert!(!activity.unseen(panel, Some(later)));
    // Left: what changed while it was open was seen, what changes after is not.
    assert!(activity.pick(ActivityPanel::Workspaces));
    assert!(!activity.unseen(panel, Some(earlier)));
    assert!(!activity.unseen(panel, None));
    assert!(activity.unseen(panel, Some(later)));
    // Other sections keep their own stamps.
    assert!(activity.unseen(ActivityPanel::Issues, Some(earlier)));
}

#[test]
fn stored_stamps_merge_without_moving_a_later_visit_back() {
    let mut activity = super::super::Activity::default();
    activity.pick(ActivityPanel::Issues);
    activity.pick(ActivityPanel::Workspaces);
    let visited = activity.last_opened()[&ActivityPanel::Issues];
    let stale = visited - Duration::days(1);
    let stored: HashMap<_, _> = [
        (ActivityPanel::Issues, stale),
        (ActivityPanel::Security, stale),
    ]
    .into();
    // This run's visits are newer than the stored ones, so the merge differs
    // from the file and must be saved.
    assert!(activity.restore(None, &stored));
    assert_eq!(activity.last_opened()[&ActivityPanel::Issues], visited);
    assert_eq!(activity.last_opened()[&ActivityPanel::Security], stale);
    assert!(!activity.unseen(ActivityPanel::Security, Some(stale)));
    assert!(activity.unseen(ActivityPanel::Security, Some(stale + Duration::seconds(1))));
}

#[test]
fn counts_keep_their_newest_item_time() {
    let at = |hour: i64| DateTime::<Utc>::from_timestamp(hour * 3600, 0).unwrap();
    let mut updated = item("1", "Updated");
    updated.created_at = Some(at(1));
    updated.updated_at = Some(at(5));
    let mut created = item("2", "Only created");
    created.created_at = Some(at(3));
    let mut closed = item("3", "Closed later");
    closed.state = "closed".into();
    closed.updated_at = Some(at(9));
    let mut pull = item("pr/4", "Pull request");
    pull.pull_request = Some(pull_request(4));
    let mut counts = Counts::default();
    counts.add(&mut HashSet::new(), &[updated, created, closed, pull]);
    // A closed item is not counted, so its change does not light the bubble.
    assert_eq!(counts.latest(ActivityPanel::Issues), Some(at(5)));
    // An item without times counts but cannot be told new.
    assert_eq!(counts.of(ActivityPanel::PullRequests), Some(1));
    assert_eq!(counts.latest(ActivityPanel::PullRequests), None);
    assert_eq!(counts.latest(ActivityPanel::Usage), None);
}

#[test]
fn a_newer_copy_in_a_later_view_still_lights_the_count() {
    let at = |hour: i64| DateTime::<Utc>::from_timestamp(hour * 3600, 0).unwrap();
    let mut stale = item("1", "Synced earlier");
    stale.updated_at = Some(at(1));
    let mut fresh = stale.clone();
    fresh.updated_at = Some(at(4));
    let mut seen = HashSet::new();
    let mut counts = Counts::default();
    counts.add(&mut seen, &[stale]);
    counts.add(&mut seen, &[fresh]);
    assert_eq!(counts.of(ActivityPanel::Issues), Some(1));
    assert_eq!(counts.latest(ActivityPanel::Issues), Some(at(4)));
}

#[gpui::test]
fn security_counts_its_newest_alert(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx);
    let created = Utc::now() - Duration::minutes(5);
    view.update(cx, |view, _| {
        view.security.read_for_test(vec![alerts(2, created)]);
    });
    cx.update(|_, cx| {
        let view = view.read(cx);
        let counts = view.activity_counts(cx);
        assert_eq!(counts.of(ActivityPanel::Security), Some(2));
        assert_eq!(counts.latest(ActivityPanel::Security), Some(created));
    });
}
