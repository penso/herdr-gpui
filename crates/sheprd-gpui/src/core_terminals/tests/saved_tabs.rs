use crate::core_terminals::panes::Panes;
use sheprd_backend::TerminalId;

fn id(name: &str) -> TerminalId {
    TerminalId::try_from(name.to_owned()).unwrap()
}

#[test]
fn a_tab_saved_before_splits_reads_back_as_one_pane() {
    use crate::browser::Location;
    let saved = r#"{"kind":"core_terminal","terminal":"t4"}"#;
    let location: Location = serde_json::from_str(saved).unwrap();
    assert_eq!(
        location,
        Location::CoreTerminal {
            panes: Panes::single(id("t4")),
            observe: false,
        }
    );
    let again = serde_json::to_string(&location).unwrap();
    assert_eq!(serde_json::from_str::<Location>(&again).unwrap(), location);
}

#[test]
fn every_tab_layout_draws_split_panes() {
    use crate::{config::TabLayoutMode, tab_layout::layout_for};
    for &mode in TabLayoutMode::ALL {
        let chrome = layout_for(mode).panes();
        assert!(chrome.gutter() >= 1., "{mode:?} needs a gutter to drag");
        assert!(chrome.veil() > 0, "{mode:?} must mark the panes not in use");
        let (top, bottom) = chrome.corners();
        assert!(top >= 0. && bottom >= 0.);
    }
}

#[test]
fn a_mirror_tab_saves_that_it_only_watches() {
    use crate::browser::Location;
    let mirror = Location::CoreTerminal {
        panes: Panes::single(id("t4")),
        observe: true,
    };
    let saved = serde_json::to_string(&mirror).unwrap();
    assert!(saved.contains("\"observe\":true"), "{saved}");
    assert_eq!(serde_json::from_str::<Location>(&saved).unwrap(), mirror);
    // A tab that types saves no flag at all.
    let typing = Location::CoreTerminal {
        panes: Panes::single(id("t4")),
        observe: false,
    };
    assert!(!serde_json::to_string(&typing).unwrap().contains("observe"));
}

/// The phone reads what the app saves through `sheprd_session::SavedTabs`, so
/// a change to the app's format that it cannot follow fails here.
#[test]
fn the_phone_reads_the_tabs_the_app_saves() {
    use crate::browser::{Location, Scope, Tab, TabId};
    let mut panes = Panes::single(id("t1"));
    panes
        .split(&id("t1"), crate::core_terminals::panes::Axis::Row, id("t2"))
        .unwrap();
    let tab = |number, observe| Tab {
        id: TabId::test(number),
        location: Some(Location::CoreTerminal {
            observe,
            panes: panes.clone(),
        }),
        origin: None,
        place: Default::default(),
        scope: Scope::endpoint("mac"),
        title: "build".into(),
        workspace_id: "w1".into(),
        zoom: Default::default(),
    };
    let saved = serde_json::json!({"tabs": [tab(4, false), tab(5, true)]});
    let read = sheprd_session::SavedTabs::parse(&serde_json::to_vec(&saved).unwrap()).unwrap();
    let scope = Scope::endpoint("mac");
    let tabs: Vec<_> = read.core_tabs(scope.as_str()).collect();
    assert_eq!(tabs.len(), 1, "a mirror lists nothing of its own");
    assert_eq!(tabs[0].id, 4);
    assert_eq!(tabs[0].title, "build");
    assert_eq!(tabs[0].workspace_id, "w1");
    assert_eq!(tabs[0].panes.terminals(), [&id("t1"), &id("t2")]);
}
