#![allow(clippy::unwrap_used)]
use super::*;
use crate::browser::{Scope, TabId};

fn zoom(percent: u16) -> Zoom {
    Zoom::try_from(percent).unwrap()
}

fn changed(from: u16, change: ZoomChange) -> u16 {
    zoom(from).changed(change).unwrap().percent()
}

#[test]
fn steps_climb_the_ladder_and_stop_at_its_ends() {
    assert_eq!(STEPS.first(), Some(&MIN_PERCENT));
    assert_eq!(STEPS.last(), Some(&MAX_PERCENT));
    assert!(STEPS.contains(&ACTUAL));
    assert!(STEPS.windows(2).all(|pair| pair[0] < pair[1]));
    // Every step in lands on the next one, and every step out comes back.
    for pair in STEPS.windows(2) {
        assert_eq!(changed(pair[0], ZoomChange::In), pair[1]);
        assert_eq!(changed(pair[1], ZoomChange::Out), pair[0]);
    }
    assert_eq!(changed(MAX_PERCENT, ZoomChange::In), MAX_PERCENT);
    assert_eq!(changed(MIN_PERCENT, ZoomChange::Out), MIN_PERCENT);
}

#[test]
fn a_zoom_between_steps_moves_to_the_nearer_one_its_way() {
    assert_eq!(changed(133, ZoomChange::In), 150);
    assert_eq!(changed(133, ZoomChange::Out), 125);
    assert_eq!(changed(133, ZoomChange::Reset), ACTUAL);
    assert_eq!(changed(133, ZoomChange::Percent { percent: 80 }), 80);
}

#[test]
fn a_zoom_of_its_own_must_be_in_range() {
    assert_eq!(Zoom::default().percent(), ACTUAL);
    assert!(Zoom::default().is_actual());
    for percent in [MIN_PERCENT, 133, MAX_PERCENT] {
        assert_eq!(zoom(percent).percent(), percent);
    }
    for percent in [0, MIN_PERCENT - 1, MAX_PERCENT + 1, u16::MAX] {
        assert_eq!(Zoom::try_from(percent), Err(OutOfRange(percent)));
        assert_eq!(
            Zoom::default().changed(ZoomChange::Percent { percent }),
            Err(OutOfRange(percent))
        );
    }
    assert_eq!(
        OutOfRange(400).to_string(),
        "Page zoom must be between 50% and 300%, not 400%"
    );
}

#[test]
fn a_tab_saves_its_zoom_only_once_zoomed() {
    let mut store = Store::default();
    let id = store
        .open(Scope::endpoint("local"), "w_1", None, None)
        .unwrap();
    let saved = |store: &Store| serde_json::to_value(store.get(id).unwrap()).unwrap();
    // Tabs at 100% are saved as before zoom existed.
    assert!(saved(&store).get("zoom").is_none());
    assert!(store.set_zoom(id, zoom(150)));
    assert!(!store.set_zoom(id, zoom(150)), "nothing changed");
    assert!(!store.set_zoom(TabId::test(999), zoom(150)));
    assert_eq!(saved(&store)["zoom"], 150);
    let tab: Tab = serde_json::from_value(saved(&store)).unwrap();
    assert_eq!(tab.zoom, zoom(150));
}

#[test]
fn saved_zooms_are_validated_when_read() {
    let tab = |zoom: &str| {
        serde_json::from_str::<Tab>(&format!(
            r#"{{"id":0,"scope":"local","workspace_id":"w_1","location":null,"title":"New Tab"{zoom}}}"#
        ))
    };
    assert_eq!(tab("").unwrap().zoom, Zoom::default());
    assert_eq!(tab(r#","zoom":75"#).unwrap().zoom, zoom(75));
    for invalid in [
        r#","zoom":10"#,
        r#","zoom":301"#,
        r#","zoom":-1"#,
        r#","zoom":1.5"#,
    ] {
        assert!(tab(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn a_snapshot_of_a_zoomed_page_asks_for_its_rect_in_view_points() {
    use crate::browser::annotate::Rect;
    let rect = Rect {
        x: 10.,
        y: 20.,
        width: 100.,
        height: 50.,
    };
    assert_eq!(Zoom::default().view_rect(rect), rect);
    assert_eq!(
        zoom(150).view_rect(rect),
        Rect {
            x: 15.,
            y: 30.,
            width: 150.,
            height: 75.,
        }
    );
    assert_eq!(
        zoom(50).view_rect(rect),
        Rect {
            x: 5.,
            y: 10.,
            width: 50.,
            height: 25.,
        }
    );
}
