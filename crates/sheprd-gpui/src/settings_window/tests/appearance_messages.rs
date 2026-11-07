//! The Theme, Sidebar, Indicators, and Fonts pages name messages as data in
//! tables, which `fl!` cannot check: each must be an English message
//! without arguments.
use super::*;
use crate::config::{Axis, Side, StatusIcons};

fn assert_message(id: &str) {
    let none: &[&str] = &[];
    assert_eq!(crate::i18n::takes(id), Some(none), "{id}");
}

#[test]
fn padding_sides_name_messages() {
    for axis in [Axis::X, Axis::Y] {
        for side in [Side::Start, Side::End] {
            assert_message(pages::padding_ids(axis, side).2);
        }
    }
}

#[test]
fn spacing_rows_name_messages() {
    for spacing in pages::Spacing::ALL {
        let (title, description) = spacing.messages();
        assert_message(title);
        assert_message(description);
    }
}

#[test]
fn status_icon_sets_name_messages() {
    for &set in StatusIcons::ALL {
        let (name, description) = controls::indicators::set_messages(set);
        assert_message(name);
        assert_message(description);
    }
}

#[test]
fn font_faces_name_messages() {
    for (face, _) in controls::FACES {
        assert_message(controls::face_message(face));
    }
}
