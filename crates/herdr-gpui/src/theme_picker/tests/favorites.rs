use super::*;
use crate::config::favorite_themes::{self, FavoriteEdit};
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
        assert_eq!(
            picker.favorite_edits,
            [FavoriteEdit {
                name: "Nord".into(),
                starred: true,
            }]
        );
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

/// The list the stand-in file holds, and each batch of stars written to it.
type File = Arc<Mutex<Vec<String>>>;
type Batches = Arc<Mutex<Vec<Vec<FavoriteEdit>>>>;

/// A stand-in for the local file: each write applies its stars to the list
/// it holds then, as the real one does under the config lock.
fn file_with(
    names: &[&str],
) -> (
    File,
    Batches,
    impl Fn(Vec<FavoriteEdit>) -> crate::Result<Vec<String>> + Clone + Send + 'static,
) {
    let file = Arc::new(Mutex::new(
        names.iter().map(|name| (*name).into()).collect(),
    ));
    let batches = Arc::new(Mutex::new(Vec::new()));
    let save = {
        let (file, batches) = (file.clone(), batches.clone());
        move |edits: Vec<FavoriteEdit>| {
            let mut names = file.lock().unwrap();
            favorite_themes::apply(&mut names, &edits);
            batches.lock().unwrap().push(edits);
            Ok(names.clone())
        }
    };
    (file, batches, save)
}

fn star(name: &str, starred: bool) -> FavoriteEdit {
    FavoriteEdit {
        name: name.into(),
        starred,
    }
}

#[gpui::test]
fn star_writes_run_one_at_a_time_and_keep_other_windows_stars(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    open_with(&view, &[], cx);
    // Another window starred Solarized Dark after this one loaded its list.
    let (file, batches, save) = file_with(&["Solarized Dark"]);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            for name in ["Nord", "Dracula", "Nord"] {
                assert!(view.toggle_favorite_theme(name));
                view.save_favorite_themes_with(save.clone(), cx);
            }
        })
    });
    cx.run_until_parked();
    // The first write took the first star; the two clicked while it ran
    // went together after it.
    assert_eq!(
        *batches.lock().unwrap(),
        [
            vec![star("Nord", true)],
            vec![star("Dracula", true), star("Nord", false)]
        ]
    );
    assert_eq!(*file.lock().unwrap(), ["Solarized Dark", "Dracula"]);
    view.read_with(cx, |view, _| {
        let picker = view.menu.themes.as_ref().unwrap();
        // The window shows what the file holds, the other window's star too.
        assert_eq!(view.config.favorite_themes, ["Solarized Dark", "Dracula"]);
        assert_eq!(picker.filtered[..2], ["Dracula", "Solarized Dark"]);
        assert!(!picker.favorites_saving);
        assert!(picker.favorite_edits.is_empty());
        assert!(picker.favorites_error.is_none());
    });
}

#[gpui::test]
fn a_failed_star_write_is_retried_and_its_error_clears(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    open_with(&view, &[], cx);
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
        let error = picker.favorites_error.as_deref().unwrap();
        assert!(
            error.starts_with("Could not save favorite themes"),
            "{error}"
        );
        // The window keeps what was clicked, and the star waits to be retried.
        assert_eq!(view.config.favorite_themes, ["Default"]);
        assert_eq!(picker.favorite_edits, [star("Default", true)]);
    });
    let (file, batches, save) = file_with(&[]);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            assert!(view.toggle_favorite_theme("Nord"));
            view.save_favorite_themes_with(save, cx);
        })
    });
    cx.run_until_parked();
    assert_eq!(
        *batches.lock().unwrap(),
        [vec![star("Default", true), star("Nord", true)]]
    );
    assert_eq!(*file.lock().unwrap(), ["Default", "Nord"]);
    view.read_with(cx, |view, _| {
        let picker = view.menu.themes.as_ref().unwrap();
        assert!(picker.favorites_error.is_none());
        assert!(picker.favorite_edits.is_empty());
    });
}

#[gpui::test]
fn a_starred_row_scrolls_into_view_where_it_moves(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.simulate_resize(size(px(800.), px(600.)));
    open_with(&view, &[], cx);
    cx.update(|window, cx| {
        view.update(cx, |view, _| {
            let picker = view.menu.themes.as_mut().unwrap();
            picker.names = (0..80).map(|index| format!("Theme {index:02}")).collect();
            picker.filter("", &[]);
            picker.selected = 70;
            picker.scroll.scroll_to_item(70, ScrollStrategy::Top);
        });
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("theme-row-0").is_none());
    assert!(cx.debug_bounds("theme-star-70").is_some());
    cx.update(|window, cx| {
        view.update(cx, |view, _| {
            // Hold the write slot so nothing reaches the personal config.
            view.menu.themes.as_mut().unwrap().favorites_saving = true;
            assert!(view.toggle_favorite_theme("Theme 70"));
        });
        window.draw(cx).clear(cx);
    });
    // The star moved to the top, and the list followed it there.
    assert!(cx.debug_bounds("theme-star-0").is_some());
    cx.update(|window, cx| {
        view.update(cx, |view, _| {
            assert!(view.toggle_favorite_theme("Theme 70"));
        });
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("theme-row-70").is_some());
    assert!(cx.debug_bounds("theme-row-0").is_none());
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
