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
