#![allow(clippy::unwrap_used)]

use super::*;
use std::sync::mpsc;

#[test]
fn a_run_of_choices_writes_only_the_newest() {
    let saves = LatestSave::default();
    saves.want(TabLayoutMode::Pills);
    saves.want(TabLayoutMode::Cards);
    let mut written = Vec::new();
    for _ in 0..2 {
        saves
            .write_latest(|mode| {
                written.push(mode);
                Ok(())
            })
            .unwrap();
    }
    assert_eq!(written, [TabLayoutMode::Cards]);
}

#[test]
fn a_choice_made_during_a_write_lands_after_it() {
    let saves = LatestSave::default();
    let (started_tx, started) = mpsc::channel();
    let (release, release_rx) = mpsc::channel::<()>();
    let (written_tx, written) = mpsc::channel();
    saves.want(TabLayoutMode::Pills);
    let first = {
        let saves = saves.clone();
        let written_tx = written_tx.clone();
        std::thread::spawn(move || {
            saves.write_latest(|mode| {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                written_tx.send(mode).unwrap();
                Ok(())
            })
        })
    };
    started.recv().unwrap();
    // The UI thread is not held up by the write in progress.
    saves.want(TabLayoutMode::Cards);
    let second = {
        let saves = saves.clone();
        std::thread::spawn(move || {
            saves.write_latest(|mode| {
                written_tx.send(mode).unwrap();
                Ok(())
            })
        })
    };
    release.send(()).unwrap();
    first.join().unwrap().unwrap();
    second.join().unwrap().unwrap();
    assert_eq!(
        written.iter().collect::<Vec<_>>(),
        [TabLayoutMode::Pills, TabLayoutMode::Cards]
    );
}

fn chats(mode: ChatLayoutMode) -> Config {
    let mut config = Config::default();
    config.chat.layout = mode;
    config
}

fn write(saves: &LayoutSaves) {
    saves.chat.write_latest(|_| Ok(())).unwrap();
}

#[test]
fn a_load_with_nothing_pending_shows_the_file() {
    let saves = LayoutSaves::default();
    let stamps = saves.stamp();
    let mut loaded = chats(ChatLayoutMode::Cards);
    saves.keep_newer(stamps, &mut loaded);
    assert_eq!(loaded.chat.layout, ChatLayoutMode::Cards);
}

#[test]
fn a_choice_made_during_a_load_outlives_it() {
    let saves = LayoutSaves::default();
    let stamps = saves.stamp();
    saves.chat.want(ChatLayoutMode::Bubbles);
    let mut loaded = chats(ChatLayoutMode::Cards);
    saves.keep_newer(stamps, &mut loaded);
    assert_eq!(loaded.chat.layout, ChatLayoutMode::Bubbles);
}

#[test]
fn a_load_started_before_a_save_finished_may_be_stale_even_after_it() {
    let saves = LayoutSaves::default();
    saves.chat.want(ChatLayoutMode::Bubbles);
    let stamps = saves.stamp();
    write(&saves);
    let mut loaded = chats(ChatLayoutMode::Cards);
    saves.keep_newer(stamps, &mut loaded);
    assert_eq!(loaded.chat.layout, ChatLayoutMode::Bubbles);
}

#[test]
fn the_reload_of_a_finished_save_shows_the_file_again() {
    let saves = LayoutSaves::default();
    saves.chat.want(ChatLayoutMode::Bubbles);
    write(&saves);
    let stamps = saves.stamp();
    let mut loaded = chats(ChatLayoutMode::Cards);
    saves.keep_newer(stamps, &mut loaded);
    assert_eq!(loaded.chat.layout, ChatLayoutMode::Cards);
}

#[test]
fn a_failed_save_leaves_nothing_pending() {
    let saves = LayoutSaves::default();
    saves.chat.want(ChatLayoutMode::Bubbles);
    let failed = saves
        .chat
        .write_latest(|_| Err(crate::Error::from(std::io::Error::other("full"))));
    assert!(failed.is_err());
    let stamps = saves.stamp();
    let mut loaded = chats(ChatLayoutMode::Cards);
    saves.keep_newer(stamps, &mut loaded);
    assert_eq!(loaded.chat.layout, ChatLayoutMode::Cards);
}

#[test]
fn each_layout_keeps_only_its_own_choice() {
    let saves = LayoutSaves::default();
    let stamps = saves.stamp();
    saves.tabs.want(TabLayoutMode::Cards);
    let mut loaded = chats(ChatLayoutMode::Timeline);
    saves.keep_newer(stamps, &mut loaded);
    assert_eq!(loaded.tab_layout, TabLayoutMode::Cards);
    assert_eq!(loaded.chat.layout, ChatLayoutMode::Timeline);
}

#[test]
fn a_window_that_did_not_choose_still_gets_the_choice() {
    let saves = LayoutSaves::default();
    saves.chat.want(ChatLayoutMode::Bubbles);
    // Another window's reload starts with the save unfinished, so its load
    // may be stale, and no later change to the file would correct it.
    let stamps = saves.stamp();
    write(&saves);
    let mut loaded = chats(ChatLayoutMode::Timeline);
    saves.keep_newer(stamps, &mut loaded);
    assert_eq!(loaded.chat.layout, ChatLayoutMode::Bubbles);
}
