use super::*;
use std::sync::{Arc, Mutex};

/// Opens the picker on a known catalog, after discovery has finished.
fn open_with(view: &Entity<HerdrWindow>, favorites: &[&str], cx: &mut VisualTestContext) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_theme_picker(window, cx)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, _| {
            view.config.favorite_themes = favorites.iter().map(|name| (*name).into()).collect();
            let picker = view.menu.themes.as_mut().unwrap();
            picker.names = ["Default", "Dracula", "Nord", "Solarized Dark"]
                .map(Into::into)
                .into();
            picker.filter("", &view.config.favorite_themes);
        });
        window.draw(cx).clear(cx);
    });
}

#[gpui::test]
fn starred_themes_lead_the_list_and_survive_search(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    // A starred theme that is no longer installed is kept but not listed.
    open_with(&view, &["Solarized Dark", "Missing", "Nord"], cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let favorites = view.config.favorite_themes.clone();
            let picker = view.menu.themes.as_mut().unwrap();
            assert_eq!(
                picker.filtered,
                ["Nord", "Solarized Dark", "Default", "Dracula"]
            );
            assert_eq!(picker.favorites, 2);
            picker.filter("o", &favorites);
            assert_eq!(picker.filtered, ["Nord", "Solarized Dark"]);
            assert_eq!(picker.favorites, 2);
            picker.filter("dra", &favorites);
            assert_eq!(picker.filtered, ["Dracula"]);
            assert_eq!(picker.favorites, 0);
            picker.filter("", &[]);
            assert_eq!(picker.favorites, 0);
            assert_eq!(picker.filtered[0], "Default");
        })
    });
}

#[gpui::test]
fn clicking_a_star_pins_the_theme_without_applying_it(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.simulate_resize(size(px(800.), px(600.)));
    open_with(&view, &[], cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            // Hold the write slot so no click reaches the personal config.
            view.menu.themes.as_mut().unwrap().favorites_saving = true;
        })
    });
    // Only the row under the pointer offers an unstarred theme's star.
    assert!(cx.debug_bounds("theme-star-2").is_none());
    let row = cx.debug_bounds("theme-row-2").unwrap();
    cx.simulate_mouse_move(row.center(), None, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let star = cx.debug_bounds("theme-star-2").unwrap();
    cx.simulate_click(star.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    view.read_with(cx, |view, _| {
        let picker = view.menu.themes.as_ref().unwrap();
        assert_eq!(view.config.favorite_themes, ["Nord"]);
        assert_eq!(picker.filtered[..2], ["Nord", "Default"]);
        // The starred row stays selected and previewed where it moved.
        assert_eq!(picker.selected, 0);
        assert_eq!(view.theme, Theme::builtin("Nord").unwrap());
        assert_eq!(view.config.theme, "Default");
        assert!(!picker.accepting);
        assert!(picker.favorites_pending);
        assert!(view.menu.page == Some(Page::Themes));
    });
    // A star stays on a starred row the pointer has left.
    let other = cx.debug_bounds("theme-row-3").unwrap();
    cx.simulate_mouse_move(other.center(), None, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let star = cx.debug_bounds("theme-star-0").unwrap();
    cx.simulate_click(star.center(), Modifiers::default());
    view.read_with(cx, |view, _| {
        let picker = view.menu.themes.as_ref().unwrap();
        assert!(view.config.favorite_themes.is_empty());
        assert_eq!(picker.favorites, 0);
        assert_eq!(
            picker.filtered,
            ["Default", "Dracula", "Nord", "Solarized Dark"]
        );
        assert_eq!(view.config.theme, "Default");
        assert!(view.menu.page == Some(Page::Themes));
    });
}

#[gpui::test]
fn star_writes_run_one_at_a_time_and_end_with_the_latest_list(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    open_with(&view, &[], cx);
    let written = Arc::new(Mutex::new(Vec::new()));
    let record = {
        let written = written.clone();
        move |names: Vec<String>| {
            written.lock().unwrap().push(names);
            Ok(())
        }
    };
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            for name in ["Nord", "Dracula", "Nord"] {
                assert!(view.toggle_favorite_theme(name));
                view.save_favorite_themes_with(record.clone(), cx);
            }
        })
    });
    cx.run_until_parked();
    // The first write took the list as it was; the two stars clicked while
    // it ran became one write of the final list.
    assert_eq!(
        *written.lock().unwrap(),
        [vec!["Nord".to_owned()], vec!["Dracula".to_owned()]]
    );
    view.read_with(cx, |view, _| {
        let picker = view.menu.themes.as_ref().unwrap();
        assert!(!picker.favorites_saving);
        assert!(!picker.favorites_pending);
        assert!(picker.error.is_none());
    });

    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            assert!(view.toggle_favorite_theme("Default"));
            view.save_favorite_themes_with(|_| Err(crate::Error::MissingHome), cx);
        })
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let picker = view.menu.themes.as_ref().unwrap();
        assert!(!picker.favorites_saving);
        let error = picker.error.as_deref().unwrap();
        assert!(
            error.starts_with("Could not save favorite themes"),
            "{error}"
        );
        // The window keeps what was clicked; the next star writes it all.
        assert_eq!(view.config.favorite_themes, ["Dracula", "Default"]);
    });
}

#[gpui::test]
fn stars_wait_while_a_theme_is_being_applied(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    open_with(&view, &["Dracula"], cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.menu.themes.as_mut().unwrap().accepting = true;
            assert!(!view.toggle_favorite_theme("Nord"));
            assert!(!view.toggle_favorite_theme("Dracula"));
            assert_eq!(view.config.favorite_themes, ["Dracula"]);
            assert_eq!(view.menu.themes.as_ref().unwrap().favorites, 1);
        })
    });
}
