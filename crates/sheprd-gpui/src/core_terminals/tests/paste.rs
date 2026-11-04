use crate::core_terminals::view::paste::pastes;
use gpui::Keystroke;

fn key(text: &str) -> Keystroke {
    Keystroke::parse(text).unwrap()
}

#[test]
fn the_window_paste_keys_paste_into_a_core_pane() {
    for text in ["cmd-v", "ctrl-shift-v", "shift-insert", "ctrl-shift-insert"] {
        assert!(pastes(&key(text), false, || false), "{text}");
    }
}

#[test]
fn plain_ctrl_v_pastes_only_where_it_does_and_only_text() {
    assert!(!pastes(&key("ctrl-v"), false, || true));
    assert!(!pastes(&key("ctrl-v"), true, || false));
    assert!(pastes(&key("ctrl-v"), true, || true));
}

#[test]
fn other_keys_go_to_the_program() {
    for text in [
        "v",
        "shift-v",
        "alt-v",
        "ctrl-alt-v",
        "cmd-shift-insert",
        "ctrl-c",
    ] {
        assert!(!pastes(&key(text), true, || true), "{text}");
    }
}
