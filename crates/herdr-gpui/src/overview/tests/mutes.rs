use super::*;
use crate::{
    endpoint::Endpoint,
    notification_mutes::{Daemon, Mutes, Scope},
    overview::{Marks, Sections, model},
    state::ConnectionStatus,
};
use std::sync::Arc;

#[test]
fn muted_rows_stay_listed_but_leave_the_bars_count() {
    let mut marks = Marks::default();
    marks.toggle_pin(&key("pinned"));
    let mut quiet = row("quiet", State::Blocked);
    quiet.muted = true;
    quiet.since = Some(at(900));
    let mut loud = row("loud", State::Failed);
    loud.since = Some(at(100));
    let mut pinned = row("pinned", State::Blocked);
    pinned.muted = true;
    let sections = Sections::of(vec![quiet, loud, pinned], Some(&marks), at(1000));
    assert_eq!(sections.needs.len(), 2, "a muted row is still listed");
    assert_eq!(sections.pinned.len(), 1);
    assert_eq!(sections.needing(), 1);
    assert_eq!(sections.needing_since(), Some(at(100)));
}

#[test]
fn agents_of_a_muted_workspace_or_tab_are_marked() {
    let mut endpoint = Endpoint::new(
        "local".into(),
        "Local".into(),
        herdr_client::ConnectTarget::Local,
        true,
    );
    // Agent p0 runs in workspace w0, tab t0; p1 in workspace w1.
    let snapshot = Arc::new(crate::sidebar::layout_tests::snapshot(2));
    endpoint.live.snapshot = Some(snapshot.clone());
    endpoint.live.status = ConnectionStatus::Connected;
    let daemon = Daemon::of(&endpoint);
    let muted = |mutes: Option<&Mutes>| {
        model::agent_rows(
            &endpoint,
            &endpoint.live,
            &Default::default(),
            |_, _| None,
            mutes,
        )
        .into_iter()
        .map(|row| row.muted)
        .collect::<Vec<_>>()
    };
    assert_eq!(muted(None), [false, false]);
    let mut mutes = Mutes::default();
    assert_eq!(muted(Some(&mutes)), [false, false]);
    mutes.set(
        &daemon,
        &snapshot,
        Scope::Tab,
        "t0",
        true,
        SystemTime::now(),
    );
    assert_eq!(muted(Some(&mutes)), [true, false]);
    mutes.set(
        &daemon,
        &snapshot,
        Scope::Workspace,
        "w1",
        true,
        SystemTime::now(),
    );
    assert_eq!(muted(Some(&mutes)), [true, true]);
    mutes.set(
        &daemon,
        &snapshot,
        Scope::Tab,
        "t0",
        false,
        SystemTime::now(),
    );
    assert_eq!(muted(Some(&mutes)), [false, true]);
}

#[test]
fn a_muted_rows_mark_is_named_for_screen_readers() {
    use gpui::{
        Element, IntoElement,
        accesskit::{Node, Role},
    };
    let element =
        crate::overview::view::muted_mark("local-p0".into(), &Default::default()).into_element();
    let mut node = Node::new(element.a11y_role().unwrap());
    element.write_a11y_info(&mut node);
    assert_eq!(node.role(), Role::Image);
    assert_eq!(node.label(), Some("Notifications muted"));
}
