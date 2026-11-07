use super::*;
use crate::window::tab_strip::agent_title::title;
use sheprd_client::protocol::{ClientShellAgent, ClientShellSnapshot};

const TITLE: &str = "fix the login redirect";

/// The fixture's one unnamed tab, numbered, with its one agent titled and
/// no plugin agent view.
fn snapshot() -> ClientShellSnapshot {
    let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../../../sheprd-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap();
    snapshot.agent_view_label = None;
    snapshot.tabs[0].label = "1".into();
    snapshot.agents[0].terminal_title_stripped = Some(TITLE.into());
    snapshot
}

/// A second agent in the fixture's tab, after the first in the snapshot.
fn second_agent(snapshot: &mut ClientShellSnapshot, title: &str) {
    let agent = ClientShellAgent {
        pane_id: "w1:p2".into(),
        terminal_title_stripped: Some(title.into()),
        ..snapshot.agents[0].clone()
    };
    snapshot.agents.push(agent);
}

fn shown(snapshot: &ClientShellSnapshot) -> Option<String> {
    title(snapshot, &snapshot.tabs[0])
}

#[test]
fn an_unnamed_tab_takes_its_agents_title() {
    assert_eq!(shown(&snapshot()).as_deref(), Some(TITLE));
}

#[test]
fn a_named_tab_keeps_its_name() {
    let mut snapshot = snapshot();
    snapshot.tabs[0].custom_label = true;
    assert_eq!(shown(&snapshot), None);
}

#[test]
fn a_tab_without_an_agent_keeps_its_number() {
    let mut snapshot = snapshot();
    snapshot.agents[0].tab_id = "w1:t2".into();
    assert_eq!(shown(&snapshot), None);
}

#[test]
fn an_absent_or_blank_title_keeps_the_number() {
    for blank in [None, Some(""), Some("   "), Some("\u{1b}\n\u{202e}")] {
        let mut snapshot = snapshot();
        snapshot.agents[0].terminal_title_stripped = blank.map(Into::into);
        assert_eq!(shown(&snapshot), None, "{blank:?}");
    }
}

#[test]
fn the_first_agent_in_layout_order_names_the_tab() {
    let mut snapshot = snapshot();
    second_agent(&mut snapshot, "second");
    snapshot.agent_order = vec!["w1:p2".into(), "w1:p1".into()];
    assert_eq!(shown(&snapshot).as_deref(), Some("second"));
    // Its title is the tab's, so a blank one is not passed over.
    snapshot.agents[1].terminal_title_stripped = None;
    assert_eq!(shown(&snapshot), None);
}

#[test]
fn an_agent_the_order_leaves_out_still_counts_without_a_view() {
    let mut snapshot = snapshot();
    snapshot.agent_order.clear();
    assert_eq!(shown(&snapshot).as_deref(), Some(TITLE));
}

#[test]
fn a_plugin_view_decides_which_agents_count() {
    let mut snapshot = snapshot();
    second_agent(&mut snapshot, "second");
    snapshot.agent_view_label = Some("review".into());
    snapshot.agent_order = vec!["w1:p2".into()];
    assert_eq!(shown(&snapshot).as_deref(), Some("second"));
    snapshot.agent_order.clear();
    assert_eq!(shown(&snapshot), None);
}

#[test]
fn a_long_title_is_cut_like_a_page_title() {
    let mut snapshot = snapshot();
    let long = "a session title much longer than any tab should be";
    snapshot.agents[0].terminal_title_stripped = Some(long.into());
    assert_eq!(
        shown(&snapshot).as_deref(),
        Some("a session title much longer…")
    );
    snapshot.agents[0].terminal_title_stripped = Some("é".repeat(40));
    assert_eq!(shown(&snapshot).unwrap().chars().count(), 29);
}

#[test]
fn a_title_loses_what_untrusted_text_may_not_carry() {
    let mut snapshot = snapshot();
    snapshot.agents[0].terminal_title_stripped = Some("one\n\u{202e}two\u{1b}".into());
    assert_eq!(shown(&snapshot).as_deref(), Some("one two"));
}

/// Draws the fixture's tab and returns its width and the label the strip
/// gave it, with the setting as `on` says.
fn drawn(cx: &mut TestAppContext, on: bool) -> (Pixels, SharedString) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        view.config.tab_labels.agent_title = on;
        view.live.snapshot = Some(Arc::new(snapshot()));
        view
    });
    cx.simulate_resize(size(px(1600.), px(600.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    let width = cx.debug_bounds("tab-w1:t1").unwrap().size.width;
    let label = view.read_with(cx, |view, _| {
        let snapshot = view.live.snapshot.as_ref().unwrap();
        view.strip_label(snapshot, &snapshot.tabs[0])
    });
    (width, label)
}

#[gpui::test]
fn the_strip_keeps_the_number_until_the_setting_is_on(cx: &mut TestAppContext) {
    let (narrow, number) = drawn(cx, false);
    let (wide, named) = drawn(cx, true);
    assert_eq!(number, "1");
    assert_eq!(named, TITLE);
    assert!(wide > narrow, "{wide:?} wider than {narrow:?}");
}
