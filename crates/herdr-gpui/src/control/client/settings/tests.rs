use super::text;
use crate::control::protocol::settings::{BindingInfo, CustomInfo};

fn custom(label: &str, reach: &str) -> CustomInfo {
    CustomInfo {
        label: label.into(),
        keys: "ctrl-b g".into(),
        reach: reach.into(),
    }
}

#[test]
fn only_bindings_that_run_are_reachable_and_each_reason_reads_plainly() {
    for reach in [
        "shadowed",
        "taken",
        "needs_modifier",
        "no_prefix",
        "over_limit",
        "unsupported",
        "something_newer",
    ] {
        assert!(!text::reachable(reach), "{reach}");
        assert_ne!(text::reach_note(reach), "runs", "{reach}");
    }
    assert!(text::reachable("runs"));
    assert_eq!(
        text::unreachable(&[custom("Lazygit", "shadowed")]),
        "1 binding of Herdr's custom commands cannot run from this window:\n  Lazygit (ctrl-b g): used by a Herdr or Herdr UI shortcut"
    );
}

#[test]
fn the_shortcut_list_shows_bound_commands_then_custom_ones() {
    let bindings = [
        BindingInfo {
            name: "toggle_sidebar".into(),
            label: "Toggle Sidebar".into(),
            keys: vec!["cmd-b".into(), "ctrl-b b".into()],
        },
        BindingInfo {
            name: "unbound".into(),
            label: "Unbound".into(),
            keys: Vec::new(),
        },
    ];
    let listed = text::keybinds(&bindings, &[custom("Lazygit", "runs")]);
    assert!(listed.starts_with("Toggle Sidebar"));
    assert!(listed.contains("cmd-b, ctrl-b b"));
    assert!(!listed.contains("Unbound"));
    assert!(listed.contains("\n\nHerdr custom commands\nLazygit"));
    assert!(listed.ends_with("runs"));
}

#[test]
fn caffeine_and_durations_read_plainly() {
    assert_eq!(text::duration(90), "1m 30s");
    assert_eq!(text::duration(7200), "2h");
    assert_eq!(text::duration(3661), "1h 1m 1s");
    assert_eq!(
        text::caffeine("on", Some(1800)),
        "Caffeine is on for another 30m"
    );
    assert_eq!(text::caffeine("off", None), "Caffeine is off");
    assert_eq!(text::caffeine("pending", None), "Caffeine is changing");
    // A hold until the agents finish is on, not off.
    assert!(text::caffeine("until_agents_finish", None).starts_with("Caffeine is on until"));
}

#[test]
fn update_states_say_what_to_do() {
    assert_eq!(
        text::update("disabled", None, Some("Local build")),
        "Updates are off in this build: Local build"
    );
    assert_eq!(
        text::update("available", Some("1.4.0"), None),
        "1.4.0 is available; update from Herdr UI"
    );
    assert_eq!(
        text::update("current", None, None),
        "Herdr UI is up to date"
    );
    assert_eq!(
        text::update("error", None, Some("offline")),
        "The last update check failed: offline"
    );
}

#[test]
fn the_selected_theme_is_marked() {
    assert_eq!(
        text::themes(&["Default".into(), "Nord".into()], "Nord"),
        "  Default\n* Nord"
    );
}

mod language;
