use super::*;
use crate::menu::devices::discover::complete::{Query, matching};
use form::{fields, open, report, settle};
use gpui::{Entity, VisualTestContext};
use std::sync::mpsc::SyncSender;

fn list(entries: &[(Source, &str, &str)]) -> Vec<Suggestion> {
    let mut list = Vec::new();
    for (source, name, target) in entries {
        let host = target.strip_prefix("ssh://").unwrap_or(target);
        merge(&mut list, candidate(*source, name, target, &[host]));
    }
    list
}

fn targets(rows: &[&Suggestion]) -> Vec<String> {
    rows.iter().map(|row| row.target.clone()).collect()
}

#[test]
fn an_empty_field_or_a_bare_user_lists_every_row() {
    let list = list(&[
        (Source::Tailscale, "prox1", "prox1.tail1.ts.net"),
        (Source::SshConfig, "orb", "orb"),
    ]);
    for text in ["", "  ", "root@"] {
        let query = Query::new(text);
        assert!(!query.filters(), "{text:?}");
        assert_eq!(matching(&list, &query).len(), 2, "{text:?}");
        assert_eq!(query.ghost("orb"), None);
    }
}

#[test]
fn prefix_matches_come_before_contained_ones() {
    let list = list(&[
        (Source::Tailscale, "samba", "samba.tail1.ts.net"),
        (
            Source::Tailscale,
            "tailscale-lisbon",
            "tailscale-lisbon.tail1.ts.net",
        ),
        (Source::Tailscale, "prox1", "prox1.tail1.ts.net"),
    ]);
    // Every target contains `tail`, but only one starts with it.
    let rows = matching(&list, &Query::new("TAIL"));
    assert_eq!(
        targets(&rows),
        [
            "tailscale-lisbon.tail1.ts.net",
            "prox1.tail1.ts.net",
            "samba.tail1.ts.net"
        ]
    );
    assert!(matching(&list, &Query::new("nothing")).is_empty());
}

#[test]
fn names_and_url_targets_match_too() {
    let list = list(&[
        (Source::Bonjour, "Studio", "ssh://studio.local:2222"),
        (Source::SshConfig, "work", "build-01"),
    ]);
    assert_eq!(
        targets(&matching(&list, &Query::new("stu"))),
        ["ssh://studio.local:2222"]
    );
    assert_eq!(targets(&matching(&list, &Query::new("wo"))), ["build-01"]);
}

#[test]
fn a_typed_user_is_kept_in_the_fill_and_ghost() {
    let list = list(&[
        (Source::Tailscale, "samba", "samba.tail1.ts.net"),
        (Source::Bonjour, "nas", "ssh://nas.local:2222"),
        (Source::SshConfig, "box", "admin@box"),
    ]);
    let query = Query::new("root@sa");
    let rows = matching(&list, &query);
    assert_eq!(targets(&rows), ["samba.tail1.ts.net"]);
    let fill = query.fill(rows[0]);
    assert_eq!(fill, "root@samba.tail1.ts.net");
    assert_eq!(query.ghost(&fill).as_deref(), Some("mba.tail1.ts.net"));

    let query = Query::new("root@na");
    assert_eq!(
        query.fill(matching(&list, &query)[0]),
        "ssh://root@nas.local:2222"
    );
    // A target with its own user keeps it.
    let query = Query::new("root@bo");
    assert_eq!(query.fill(matching(&list, &query)[0]), "admin@box");
}

#[test]
fn the_ghost_only_extends_a_typed_prefix() {
    let query = Query::new("SAM");
    assert_eq!(
        query.ghost("samba.tail1.ts.net").as_deref(),
        Some("ba.tail1.ts.net")
    );
    assert_eq!(Query::new("mba").ghost("samba.tail1.ts.net"), None);
    assert_eq!(Query::new("samba").ghost("samba"), None);
    // A multibyte name never splits inside a character.
    assert_eq!(Query::new("é").ghost("e-host"), None);
}

fn type_target(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, text: &str) {
    cx.update(|_, cx| {
        let target = view.read(cx).menu.device_setup.as_ref().unwrap().fields[0].clone();
        target.update(cx, |field, cx| field.set_text_selected(text, cx));
    });
    cx.run_until_parked();
}

fn rows(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> (Vec<String>, Option<usize>) {
    cx.update(|_, cx| {
        let view = view.read(cx);
        let setup = view.menu.device_setup.as_ref().unwrap();
        let text = setup.fields[0].read(cx).text().to_owned();
        let completion = view.device_completion(setup, &text);
        (completion.fills, completion.highlight)
    })
}

fn focused_field(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> Option<usize> {
    cx.update(|window, cx| {
        let setup = view.read(cx).menu.device_setup.as_ref().unwrap();
        setup
            .fields
            .iter()
            .position(|field| field.read(cx).focus.is_focused(window))
    })
}

#[gpui::test]
fn typing_filters_the_list_and_tab_completes(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open(cx, report);
    settle(cx);
    // Nothing typed: every unsaved row, none chosen, so Tab moves on.
    assert_eq!(
        rows(&view, cx),
        (
            vec!["m4max.tail1.ts.net".into(), "ssh://nas.local:2222".into()],
            None
        )
    );

    type_target(&view, cx, "me@m4");
    assert_eq!(
        rows(&view, cx),
        (vec!["me@m4max.tail1.ts.net".into()], Some(0))
    );
    settle(cx);
    assert!(cx.debug_bounds("device-suggestion-0").is_some());
    assert!(cx.debug_bounds("device-suggestion-1").is_none());

    cx.simulate_keystrokes("tab");
    assert_eq!(
        fields(&view, cx),
        ("me@m4max.tail1.ts.net".into(), "m4max".into())
    );
    assert_eq!(focused_field(&view, cx), Some(0), "completing stays put");
    // Now the field holds the choice, so Tab moves on as usual.
    cx.simulate_keystrokes("tab");
    assert_eq!(focused_field(&view, cx), Some(1));
}

#[gpui::test]
fn arrows_choose_a_row_and_enter_fills_it(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open(cx, report);
    settle(cx);
    cx.simulate_keystrokes("down down");
    assert_eq!(rows(&view, cx).1, Some(1));
    cx.simulate_keystrokes("up");
    assert_eq!(rows(&view, cx).1, Some(0));
    cx.simulate_keystrokes("down enter");
    assert_eq!(
        fields(&view, cx),
        ("ssh://nas.local:2222".into(), "nas".into())
    );
    // Every row still shows, the chosen one highlighted, and the form is
    // still a form.
    assert_eq!(
        rows(&view, cx),
        (
            vec!["m4max.tail1.ts.net".into(), "ssh://nas.local:2222".into()],
            Some(1)
        )
    );
    cx.update(|_, cx| {
        let setup = view.read(cx).menu.device_setup.as_ref().unwrap();
        assert!(matches!(
            setup.step,
            crate::menu::devices::add_device::Step::Form
        ));
    });

    // Typing something else drops the arrow choice. Neither target starts
    // with `n`, so nothing is highlighted until an arrow picks a row.
    type_target(&view, cx, "n");
    assert_eq!(
        rows(&view, cx),
        (
            vec!["ssh://nas.local:2222".into(), "m4max.tail1.ts.net".into()],
            None
        )
    );
}

/// A Bonjour host announcing the name of a host the user means to type.
fn spoof(source: Source, sender: SyncSender<Event>) {
    if source == Source::Bonjour {
        let _ = sender.send(Event::Found(candidate(
            Source::Bonjour,
            "prod.example.com",
            "evil.local",
            &["evil.local"],
        )));
    }
    let _ = sender.send(Event::Done(source, Ok(())));
}

fn key(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, key: &str) -> bool {
    cx.update(|window, cx| view.update(cx, |view, cx| view.complete_device_target(key, window, cx)))
}

#[gpui::test]
fn an_announced_name_never_replaces_a_typed_host(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open(cx, spoof);
    settle(cx);
    type_target(&view, cx, "prod.example.com");
    // The row shows, since it matches, but is not highlighted or drawn
    // after the caret, and neither Tab nor Enter puts it in the field.
    assert_eq!(rows(&view, cx), (vec!["evil.local".into()], None));
    assert!(!key(&view, cx, "tab"));
    assert!(!key(&view, cx, "enter"));
    assert_eq!(fields(&view, cx).0, "prod.example.com");
    // Only an explicit pick fills it.
    assert!(key(&view, cx, "down"));
    assert!(key(&view, cx, "enter"));
    assert_eq!(fields(&view, cx).0, "evil.local");
}

#[gpui::test]
fn enter_sends_what_is_typed_even_with_a_completion_shown(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open(cx, report);
    settle(cx);
    type_target(&view, cx, "m4");
    assert_eq!(rows(&view, cx).1, Some(0));
    assert!(!key(&view, cx, "enter"), "left to submit the form");
    assert_eq!(fields(&view, cx).0, "m4");
}

#[test]
fn a_filled_ssh_url_names_its_user_after_the_scheme() {
    let list = list(&[(Source::Bonjour, "nas", "ssh://nas.local:2222")]);
    let query = Query::new("ssh://root@nas.local:2222");
    let rows = matching(&list, &query);
    assert_eq!(targets(&rows), ["ssh://nas.local:2222"]);
    // Filling the row again gives back exactly what the field holds.
    assert_eq!(query.fill(rows[0]), "ssh://root@nas.local:2222");
}

#[gpui::test]
fn a_completed_ssh_url_still_lists_every_row(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open(cx, report);
    settle(cx);
    type_target(&view, cx, "root@na");
    cx.simulate_keystrokes("down enter");
    assert_eq!(fields(&view, cx).0, "ssh://root@nas.local:2222");
    assert_eq!(
        rows(&view, cx),
        (
            vec![
                "root@m4max.tail1.ts.net".into(),
                "ssh://root@nas.local:2222".into()
            ],
            Some(1)
        )
    );
}

#[gpui::test]
fn an_arrow_choice_follows_its_device_as_rows_arrive(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open(cx, report);
    settle(cx);
    cx.simulate_keystrokes("down down");
    assert_eq!(rows(&view, cx).1, Some(1), "nas is chosen");
    // A later report sorts in before it.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let discovery = view
                .menu
                .device_setup
                .as_mut()
                .and_then(|setup| setup.discovery.as_mut())
                .unwrap();
            merge(
                &mut discovery.suggestions,
                candidate(Source::Bonjour, "alpha", "alpha.local", &["alpha.local"]),
            );
        })
    });
    let (fills, highlight) = rows(&view, cx);
    assert_eq!(fills[0], "alpha.local");
    assert_eq!(highlight, Some(2), "still nas, now one row lower");
    assert!(key(&view, cx, "enter"));
    assert_eq!(fields(&view, cx).0, "ssh://nas.local:2222");
}

#[gpui::test]
fn an_arrow_choice_survives_another_source_changing_its_target(cx: &mut gpui::TestAppContext) {
    let (view, cx) = open(cx, report);
    settle(cx);
    type_target(&view, cx, "n");
    cx.simulate_keystrokes("down");
    let (fills, highlight) = rows(&view, cx);
    assert_eq!(
        highlight
            .and_then(|index| fills.get(index))
            .map(String::as_str),
        Some("ssh://nas.local:2222")
    );
    // Tailscale reports the same machine; its name now wins the target.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let discovery = view
                .menu
                .device_setup
                .as_mut()
                .and_then(|setup| setup.discovery.as_mut())
                .unwrap();
            merge(
                &mut discovery.suggestions,
                candidate(
                    Source::Tailscale,
                    "nas",
                    "nas.tail1.ts.net",
                    &["nas.tail1.ts.net"],
                ),
            );
        })
    });
    let (fills, highlight) = rows(&view, cx);
    assert_eq!(
        highlight
            .and_then(|index| fills.get(index))
            .map(String::as_str),
        Some("nas.tail1.ts.net"),
        "still the picked machine"
    );
    assert!(key(&view, cx, "enter"));
    assert_eq!(fields(&view, cx).0, "nas.tail1.ts.net");
}
