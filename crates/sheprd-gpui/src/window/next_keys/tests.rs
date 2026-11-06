use super::*;
use crate::keymap::DaemonKeys;
use crate::state::ConnectionStatus;
use gpui::{Entity, TestAppContext, VisualTestContext};
use sheprd_client::protocol::ClientShellCommandAction;
use std::collections::BTreeMap;
use std::sync::Arc;

fn keymap(keys: &str, overrides: &[(&str, &str)]) -> Keymap {
    let table: toml::Table = keys.parse().unwrap();
    let overrides: BTreeMap<_, _> = overrides
        .iter()
        .map(|(name, key)| {
            (
                (*name).to_owned(),
                crate::keymap::Binding::One((*key).to_owned()),
            )
        })
        .collect();
    Keymap::with_overrides(
        &overrides,
        &Default::default(),
        &DaemonKeys::from_table(Some(&table)),
    )
    .unwrap()
}

fn command(id: &str, label: &str) -> ClientShellCommand {
    ClientShellCommand {
        command_id: id.into(),
        description: Some(format!("Run {id}")),
        action: ClientShellCommandAction::PluginAction,
        binding_label: label.into(),
        binding_labels: vec![label.into()],
    }
}

fn find<'a>(groups: &'a [Group], key: &str) -> Option<&'a Row> {
    groups
        .iter()
        .flat_map(|group| &group.rows)
        .find(|row| row.key == key)
}

fn group(title: &str, rows: usize) -> Group {
    Group {
        title: title.into(),
        rows: (0..rows)
            .map(|row| Row {
                key: format!("{title}{row}"),
                description: String::new(),
            })
            .collect(),
    }
}

#[test]
fn lists_chords_by_topic_and_only_chords() {
    let groups = groups(
        &keymap(
            "prefix = 'ctrl+a'\nzoom = 'prefix+m'\nsettings = 'ctrl+alt+s'",
            &[],
        ),
        &[],
    );
    let zoom = find(&groups, "m").unwrap();
    let catalog = crate::controls::COMMANDS
        .iter()
        .find(|info| info.command == Command::Zoom)
        .unwrap();
    assert_eq!(zoom.description, crate::i18n::command(catalog));
    let workspaces = groups
        .iter()
        .find(|group| group.title == Section::Workspaces.title())
        .unwrap();
    assert!(workspaces.rows.contains(zoom));
    assert!(
        find(&groups, "ctrl-alt-s").is_none(),
        "a direct shortcut needs no prefix"
    );
    assert!(
        groups.iter().all(|group| !group.rows.is_empty()),
        "an empty topic has no heading"
    );
}

#[test]
fn a_gui_binding_takes_its_command_out_of_the_panel() {
    let daemon = "prefix = 'ctrl+a'\nzoom = 'prefix+m'";
    assert!(find(&groups(&keymap(daemon, &[]), &[]), "m").is_some());
    let moved = keymap(daemon, &[("toggle_zoom", "cmd-shift-m")]);
    assert!(find(&groups(&moved, &[]), "m").is_none());
}

#[test]
fn custom_chords_are_listed_only_when_they_run() {
    let keymap = keymap("prefix = 'ctrl+a'\nzoom = 'prefix+m'", &[]);
    let commands = [
        command("deploy", "prefix+y"),
        command("shadowed", "prefix+m"),
        command("direct", "ctrl+alt+y"),
    ];
    let groups = groups(&keymap, &commands);
    let custom = groups
        .iter()
        .find(|group| group.title == Section::Commands.title())
        .unwrap();
    assert_eq!(
        custom.rows,
        [Row {
            key: "y".into(),
            description: "Run deploy".into()
        }]
    );
}

#[test]
fn numbered_commands_on_their_digits_read_as_one_row() {
    let groups = groups(&keymap("prefix = 'ctrl+a'", &[]), &[]);
    let tabs = find(&groups, "1…9").unwrap();
    assert_eq!(tabs.description, fl!("next-keys-focus-tab"));
    assert!(find(&groups, "2").is_none());
}

#[test]
fn a_digit_bound_out_of_step_keeps_its_own_row() {
    let tab = |number| {
        crate::controls::COMMANDS
            .iter()
            .find(|info| info.command == Command::TabNumber(number))
            .unwrap()
    };
    let mut rows = Vec::new();
    let mut run = None;
    for (number, key) in [(1, "1"), (2, "2"), (3, "x"), (4, "4")] {
        run = extend_run(&mut rows, run, tab(number), &Keystroke::parse(key).unwrap());
    }
    let keys: Vec<_> = rows.iter().map(|row| row.key.as_str()).collect();
    assert_eq!(keys, ["1…2", "x", "4"]);
}

#[test]
fn flow_balances_columns_and_keeps_every_key_when_they_fit() {
    let groups = [group("a", 5), group("b", 5)];
    let (laid, hidden) = flow(&groups, 2, 100);
    assert_eq!(hidden, 0);
    assert_eq!(laid.len(), 2);
    assert!(laid.iter().all(|column| column.len() <= 7));
}

#[test]
fn flow_counts_what_does_not_fit() {
    let groups = [group("a", 10), group("b", 10)];
    let (laid, hidden) = flow(&groups, 2, 4);
    let keys: usize = laid
        .iter()
        .flatten()
        .filter(|line| matches!(line, Line::Key(_)))
        .count();
    assert_eq!(laid.len(), 2);
    assert!(laid.iter().all(|column| column.len() <= 4));
    assert_eq!(keys + hidden, 20);
}

#[test]
fn flow_never_ends_a_column_on_a_heading() {
    let groups = [group("a", 2), group("b", 2), group("c", 2)];
    for capacity in 2..8 {
        for columns in 1..4 {
            let (laid, _) = flow(&groups, columns, capacity);
            assert!(
                laid.iter()
                    .all(|column| !matches!(column.last(), Some(Line::Heading(_)))),
                "{columns} columns of {capacity}"
            );
        }
    }
}

#[test]
fn flow_with_no_room_shows_nothing_and_counts_it_all() {
    let groups = [group("a", 3)];
    let (laid, hidden) = flow(&groups, 2, 0);
    assert!(laid.iter().all(Vec::is_empty));
    assert_eq!(hidden, 3);
}

fn press(keystroke: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.dispatch_keystroke(Keystroke::parse(keystroke).unwrap(), cx);
    });
    cx.run_until_parked();
}

fn window(cx: &mut TestAppContext) -> (Entity<SheprdWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    let table: toml::Table = "prefix = 'cmd+j'\ntoggle_sidebar = 'prefix+cmd+b'"
        .parse()
        .unwrap();
    view.update(cx, |view, _| {
        view.config.keybindings = Keymap::with_overrides(
            &Default::default(),
            &Default::default(),
            &DaemonKeys::from_table(Some(&table)),
        )
        .unwrap();
    });
    cx.update(|window, cx| {
        window.focus(&view.read(cx).focus.clone(), cx);
        window.draw(cx).clear(cx);
    });
    (view, cx)
}

fn open(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> bool {
    view.read_with(cx, |view, _| view.next_keys.is_open())
}

fn wait(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(DELAY);
    cx.run_until_parked();
}

#[gpui::test]
fn it_shows_after_the_pause_and_leaves_with_the_next_key(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    press("cmd-j", cx);
    assert!(!open(&view, cx), "not before the pause");
    wait(cx);
    assert!(open(&view, cx));
    assert!(
        cx.debug_bounds("next-keys").is_some(),
        "the panel draws once open"
    );
    press("cmd-b", cx);
    assert!(!open(&view, cx));
    view.read_with(cx, |view, _| {
        assert!(!view.sidebar_visible, "the chord ran")
    });
}

#[gpui::test]
fn escape_closes_it(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    press("cmd-j", cx);
    wait(cx);
    press("escape", cx);
    assert!(!open(&view, cx));
    view.read_with(cx, |view, _| assert!(!view.prefix_armed));
}

#[gpui::test]
fn a_chord_typed_without_a_pause_never_shows_it(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    press("cmd-j", cx);
    press("cmd-b", cx);
    wait(cx);
    assert!(!open(&view, cx));
}

#[gpui::test]
fn leaving_the_window_closes_it(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    press("cmd-j", cx);
    wait(cx);
    view.update(cx, |view, _| view.disarm_prefix());
    assert!(!open(&view, cx));
}

#[gpui::test]
fn custom_commands_appear_only_while_connected(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    view.update(cx, |view, _| {
        Arc::make_mut(view.live.snapshot.as_mut().unwrap()).commands =
            vec![command("deploy", "prefix+y")];
        view.live.status = ConnectionStatus::Disconnected;
    });
    let listed = |view: &Entity<SheprdWindow>, cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| find(&view.next_keys.groups, "y").is_some())
    };
    press("cmd-j", cx);
    wait(cx);
    assert!(!listed(&view, cx));
    press("escape", cx);
    view.update(cx, |view, _| view.live.status = ConnectionStatus::Connected);
    press("cmd-j", cx);
    wait(cx);
    assert!(listed(&view, cx));
}

#[test]
fn columns_leave_room_for_the_gaps_between_them() {
    let width = 380.;
    assert_eq!(columns_within(width * 2., width), 1, "no room for the gap");
    assert_eq!(columns_within(width * 2. + COLUMN_GAP, width), 2);
    assert_eq!(columns_within(10., width), 1, "always one");
    assert_eq!(columns_within(100_000., width), MAX_COLUMNS);
}

#[test]
fn its_fade_out_still_counts_as_showing() {
    let closed_at = Instant::now();
    let mut panel = NextKeys::default();
    assert!(!panel.showing(closed_at), "never opened");
    panel.toggle.set(true, closed_at, true);
    panel.toggle.set(false, closed_at, false);
    assert!(!panel.is_open());
    assert!(panel.showing(closed_at), "it is still fading");
    assert!(!panel.showing(closed_at + motion::LEAVE * 2));
}

#[gpui::test]
fn a_new_custom_command_joins_the_open_panel(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    view.update(cx, |view, _| view.live.status = ConnectionStatus::Connected);
    press("cmd-j", cx);
    wait(cx);
    view.update(cx, |view, cx| {
        Arc::make_mut(view.live.snapshot.as_mut().unwrap()).commands =
            vec![command("deploy", "prefix+y")];
        view.sync_next_keys(cx);
    });
    view.read_with(cx, |view, _| {
        assert!(view.next_keys.is_open());
        assert!(find(&view.next_keys.groups, "y").is_some());
    });
}

#[gpui::test]
fn a_lost_connection_takes_custom_commands_off_the_open_panel(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    view.update(cx, |view, _| {
        Arc::make_mut(view.live.snapshot.as_mut().unwrap()).commands =
            vec![command("deploy", "prefix+y")];
        view.live.status = ConnectionStatus::Connected;
    });
    press("cmd-j", cx);
    wait(cx);
    view.update(cx, |view, cx| {
        view.live.status = ConnectionStatus::Disconnected;
        view.sync_next_keys(cx);
    });
    view.read_with(cx, |view, _| {
        assert!(view.next_keys.is_open(), "the keymap's chords remain");
        assert!(find(&view.next_keys.groups, "y").is_none());
    });
}

#[gpui::test]
fn a_reload_that_moves_the_prefix_ends_the_chord(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    press("cmd-j", cx);
    wait(cx);
    view.update(cx, |view, cx| {
        view.config.keybindings = keymap("prefix = 'cmd+k'\ntoggle_sidebar = 'prefix+b'", &[]);
        view.sync_next_keys(cx);
    });
    assert!(!open(&view, cx));
    view.read_with(cx, |view, _| assert!(!view.prefix_armed));
}

#[gpui::test]
fn a_reload_that_drops_a_chord_takes_it_off_the_open_panel(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    press("cmd-j", cx);
    wait(cx);
    view.update(cx, |view, cx| {
        view.config.keybindings = keymap("prefix = 'cmd+j'\ntoggle_sidebar = 'prefix+x'", &[]);
        view.sync_next_keys(cx);
    });
    view.read_with(cx, |view, _| {
        assert!(view.next_keys.is_open());
        assert!(find(&view.next_keys.groups, "x").is_some());
        assert!(find(&view.next_keys.groups, "cmd-b").is_none());
    });
}

#[gpui::test]
fn the_poll_keeps_the_panel_current(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    press("cmd-j", cx);
    wait(cx);
    view.update(cx, |view, cx| {
        view.config.keybindings = keymap("prefix = 'cmd+k'\ntoggle_sidebar = 'prefix+b'", &[]);
        view.poll_endpoints(cx);
    });
    assert!(!open(&view, cx));
}
