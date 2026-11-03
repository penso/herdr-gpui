use super::{Direction, PaletteKey};
use gpui::Keystroke;

fn key(text: &str) -> Option<PaletteKey> {
    PaletteKey::of(&Keystroke::parse(text).unwrap())
}

#[test]
fn tab_and_picker_chords_move_the_selection() {
    for text in ["down", "tab", "ctrl-n", "ctrl-j"] {
        assert_eq!(key(text), Some(PaletteKey::Move(Direction::Down)), "{text}");
    }
    for text in ["up", "shift-tab", "ctrl-p", "ctrl-k"] {
        assert_eq!(key(text), Some(PaletteKey::Move(Direction::Up)), "{text}");
    }
}

#[test]
fn ctrl_tab_and_alt_brackets_cycle_filters() {
    for text in ["ctrl-tab", "alt-]"] {
        assert_eq!(
            key(text),
            Some(PaletteKey::CycleFilter(Direction::Down)),
            "{text}"
        );
    }
    for text in ["ctrl-shift-tab", "alt-["] {
        assert_eq!(
            key(text),
            Some(PaletteKey::CycleFilter(Direction::Up)),
            "{text}"
        );
    }
}

#[test]
fn query_editing_keys_stay_with_the_search_field() {
    for text in [
        "n",
        "j",
        "shift-n",
        "cmd-n",
        "ctrl-shift-n",
        "alt-n",
        "[",
        "cmd-tab",
        "alt-tab",
    ] {
        assert_eq!(key(text), None, "{text}");
    }
    assert_eq!(key("alt-p"), Some(PaletteKey::TogglePreview));
    assert_eq!(key("alt-shift-p"), None);
    assert_eq!(key("enter"), Some(PaletteKey::Activate));
    assert_eq!(key("escape"), Some(PaletteKey::Dismiss));
}
