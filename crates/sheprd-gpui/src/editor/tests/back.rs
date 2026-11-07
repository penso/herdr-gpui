//! Which editor panes closing brings a review back.
use super::super::back::{Return, settle};
use crate::browser::TabId;
use sheprd_client::protocol::ClientShellSnapshot;

/// `panes` as (pane, terminal tab), the first focused.
fn shown(boot: &str, panes: &[(&str, &str)]) -> ClientShellSnapshot {
    let mut value = serde_json::to_value(crate::sidebar::layout_tests::snapshot(1)).unwrap();
    value["boot_id"] = boot.into();
    value["focused_pane_id"] = panes
        .first()
        .map_or(serde_json::Value::Null, |p| p.0.into());
    value["panes"] = panes
        .iter()
        .map(|(pane, tab)| {
            serde_json::json!({
                "pane_id": pane, "workspace_id": "w0", "tab_id": tab, "label": null,
                "cwd": null, "foreground_cwd": null, "focused": false,
                "right_click_passthrough": false
            })
        })
        .collect();
    serde_json::from_value(value).unwrap()
}

fn page() -> TabId {
    TabId::test(1)
}

fn entry(pane: &str, page: TabId) -> Return {
    Return {
        endpoint_id: "local".into(),
        boot: "boot".into(),
        pane_id: pane.into(),
        page,
        seen_in: None,
        in_use: false,
    }
}

#[test]
fn a_pane_not_listed_yet_has_not_closed() {
    let mut returns = vec![entry("editor", page())];
    assert_eq!(
        settle(&mut returns, "local", &shown("boot", &[("agent", "t")])),
        None
    );
    assert_eq!(returns.len(), 1);
}

#[test]
fn a_seen_pane_closing_on_the_tab_in_use_brings_the_page_back() {
    let mut returns = vec![entry("editor", page())];
    let open = shown("boot", &[("agent", "t"), ("editor", "t")]);
    assert_eq!(settle(&mut returns, "local", &open), None);
    let closed = shown("boot", &[("agent", "t")]);
    assert_eq!(settle(&mut returns, "local", &closed), Some(page()));
    assert!(returns.is_empty());
}

#[test]
fn a_pane_closing_after_the_user_moved_on_pulls_nobody_back() {
    let mut returns = vec![entry("editor", page())];
    let open = shown("boot", &[("agent", "t"), ("editor", "t")]);
    settle(&mut returns, "local", &open);
    let elsewhere = shown("boot", &[("other", "u"), ("agent", "t")]);
    assert_eq!(settle(&mut returns, "local", &elsewhere), None);
    assert!(returns.is_empty(), "it does not fire later either");
}

#[test]
fn quitting_the_last_pane_of_the_tab_in_use_brings_the_page_back() {
    let mut returns = vec![entry("editor", page())];
    settle(
        &mut returns,
        "local",
        &shown("boot", &[("editor", "t"), ("other", "u")]),
    );
    let emptied = shown("boot", &[("other", "u")]);
    assert_eq!(settle(&mut returns, "local", &emptied), Some(page()));
    assert!(returns.is_empty());
}

#[test]
fn a_last_pane_closed_after_the_user_left_its_tab_pulls_nobody_back() {
    let mut returns = vec![entry("editor", page())];
    settle(
        &mut returns,
        "local",
        &shown("boot", &[("other", "u"), ("editor", "t")]),
    );
    let emptied = shown("boot", &[("other", "u")]);
    assert_eq!(settle(&mut returns, "local", &emptied), None);
    assert!(returns.is_empty());
}

#[test]
fn a_new_boot_forgets_and_another_endpoint_keeps() {
    let mut returns = vec![entry("editor", page())];
    assert_eq!(settle(&mut returns, "other", &shown("new", &[])), None);
    assert_eq!(returns.len(), 1);
    assert_eq!(settle(&mut returns, "local", &shown("new", &[])), None);
    assert!(returns.is_empty());
}
