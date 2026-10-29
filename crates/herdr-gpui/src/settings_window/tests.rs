use super::*;
use crate::config::FontFace;
use core::prelude::v1::test;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

mod activity_bar_windows;
mod appearance_messages;
mod config_watch;
mod en_xa;
mod gpu_settings;
mod language_reload;
#[cfg(windows)]
mod launch_at_login;
mod layout_drafts;
mod load_save;
mod navigation_groups;
mod navigation_resize;
mod navigation_scroll;
mod page_messages;
mod pages_behaviour;
mod quit_saves;
mod remote_control;
mod remote_history_reload;
mod save_now;
mod scrollbars;
mod search;
mod section_order;
mod sidebar_drafts;
mod tab_layout;
mod theme_drafts;
// Saving a shared Herdr theme is Unix-only.
mod theme_hover;
mod theme_mode;
#[cfg(unix)]
mod theme_recovery;
mod theme_sources;
mod window_commands;
mod window_lifecycle;

fn fixture_load() -> crate::Result<Loaded> {
    Ok(fixture())
}

type SizeWrites = Arc<Mutex<Vec<Vec<(FontFace, f32)>>>>;

fn recording_sizes(writes: SizeWrites) -> SizeIo {
    SizeIo {
        write: Arc::new(move |sizes| {
            writes.lock().unwrap().push(sizes);
            Ok(())
        }),
        load: fixture_load,
    }
}

fn recording_themes(writes: Arc<Mutex<Vec<String>>>, fail: bool) -> themes::ThemeIo {
    let disk = Arc::new(Mutex::new("Default".to_owned()));
    let saved = disk.clone();
    themes::ThemeIo {
        shared: None,
        resolve: None,
        write: Arc::new(move |name, shared| {
            assert!(shared.is_none());
            writes.lock().unwrap().push(name.clone());
            if fail {
                return Err(crate::Error::MissingHome);
            }
            *saved.lock().unwrap() = name;
            Ok(())
        }),
        load: Arc::new(move || {
            let mut loaded = fixture();
            loaded.config.theme = disk.lock().unwrap().clone();
            loaded.theme = loaded.config.theme(false)?;
            Ok(loaded)
        }),
    }
}

fn recording_layouts(
    writes: Arc<Mutex<Vec<crate::config::LayoutMode>>>,
    fail: bool,
) -> layouts::LayoutIo {
    let disk = Arc::new(Mutex::new(Config::default().layout.mode));
    let saved = disk.clone();
    layouts::LayoutIo {
        write: Arc::new(move |mode| {
            writes.lock().unwrap().push(mode);
            if fail {
                return Err(crate::Error::MissingHome);
            }
            *saved.lock().unwrap() = mode;
            Ok(())
        }),
        load: Arc::new(move || {
            let mut loaded = fixture();
            loaded.config.layout.mode = *disk.lock().unwrap();
            Ok(loaded)
        }),
    }
}

fn choose(view: &mut SettingsWindow, name: &str, cx: &mut Context<SettingsWindow>) {
    view.accept_theme_choice(
        themes::Choice {
            scope: themes::Scope::App,
            name: name.into(),
        },
        cx,
    );
}

pub(super) fn fixture() -> Loaded {
    Loaded {
        config: Config::default(),
        theme: Theme::default(),
        shared: None,
        error: None,
    }
}

fn open_fixture(source: WeakEntity<HerdrWindow>, cx: &mut App) {
    open_with(source, cx, |_, _, _| {});
}

pub(super) fn skill_fixture(
    window: &mut Window,
    cx: &mut Context<SettingsWindow>,
) -> SettingsWindow {
    let source = cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx));
    let mut view = SettingsWindow::new(source.downgrade(), cx);
    view.section = Section::General;
    view
}

pub(super) fn skill_load() -> crate::Result<Loaded> {
    Ok(Loaded {
        config: Config::default(),
        theme: Default::default(),
        shared: None,
        error: None,
    })
}
