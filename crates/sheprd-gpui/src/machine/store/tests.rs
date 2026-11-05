#![allow(clippy::unwrap_used)]
use super::*;
use crate::machine::Display;

fn installed(root: &Path, text: &str) -> PathBuf {
    let folder = root.join(text);
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join(DISK), vec![1; 8192]).unwrap();
    folder
}

#[test]
fn no_folder_lists_no_machines() {
    let root = tempfile::tempdir().unwrap();
    assert!(list_in(&root.path().join("none")).unwrap().is_empty());
}

#[test]
fn only_folders_with_a_disk_and_a_valid_name_are_machines() {
    let root = tempfile::tempdir().unwrap();
    installed(root.path(), "work");
    installed(root.path(), "alpha");
    installed(root.path(), "-bad");
    fs::create_dir(root.path().join("empty")).unwrap();
    let names: Vec<String> = list_in(root.path())
        .unwrap()
        .into_iter()
        .map(|machine| machine.name.to_string())
        .collect();
    assert_eq!(names, ["alpha", "work"]);
}

#[test]
fn a_listing_counts_what_the_files_take_and_notes_a_saved_state() {
    let root = tempfile::tempdir().unwrap();
    let folder = installed(root.path(), "work");
    let before = list_in(root.path()).unwrap().remove(0);
    assert!(!before.saved);
    assert!(before.disk_bytes >= 8192);
    fs::write(folder.join(STATE), vec![1; 8192]).unwrap();
    let after = list_in(root.path()).unwrap().remove(0);
    assert!(after.saved);
    assert!(after.disk_bytes > before.disk_bytes);
}

#[test]
fn a_sparse_disk_counts_only_what_it_holds() {
    let root = tempfile::tempdir().unwrap();
    let folder = installed(root.path(), "work");
    let disk = fs::File::options()
        .write(true)
        .open(folder.join(DISK))
        .unwrap();
    disk.set_len(1 << 40).unwrap();
    assert!(disk_bytes(&folder) < 1 << 30);
}

#[test]
fn a_fresh_boot_records_the_chosen_settings() {
    let root = tempfile::tempdir().unwrap();
    let folder = installed(root.path(), "work");
    let chosen = Settings {
        cpus: 6,
        ..Settings::default()
    };
    write_json(&folder.join(SETTINGS), &chosen).unwrap();
    assert_eq!(boot_settings(&folder).unwrap(), chosen);
    assert_eq!(read_json::<Settings>(&folder.join(BOOTED)), Some(chosen));
}

#[test]
fn a_saved_machine_keeps_the_settings_it_booted_with() {
    let root = tempfile::tempdir().unwrap();
    let folder = installed(root.path(), "work");
    let booted = boot_settings(&folder).unwrap();
    fs::write(folder.join(STATE), b"saved").unwrap();
    let chosen = Settings {
        memory_gib: 16,
        display: Display::PRESETS[0],
        ..booted
    };
    write_json(&folder.join(SETTINGS), &chosen).unwrap();
    assert_eq!(boot_settings(&folder).unwrap(), booted);
    fs::remove_file(folder.join(STATE)).unwrap();
    assert_eq!(boot_settings(&folder).unwrap(), chosen);
}

#[test]
fn damaged_settings_fall_back_to_the_defaults() {
    let root = tempfile::tempdir().unwrap();
    let folder = installed(root.path(), "work");
    fs::write(folder.join(SETTINGS), b"{").unwrap();
    assert_eq!(boot_settings(&folder).unwrap(), Settings::default());
}

#[test]
fn deleting_removes_the_folder_and_only_it() {
    let root = tempfile::tempdir().unwrap();
    let doomed = installed(root.path(), "doomed");
    let kept = installed(root.path(), "kept");
    delete_folder(&doomed).unwrap();
    assert!(!doomed.exists());
    assert!(kept.join(DISK).exists());
    // Deleting what is already gone is what the user wanted.
    delete_folder(&doomed).unwrap();
}

#[test]
fn deleting_does_not_follow_a_link_out_of_the_folder() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let precious = outside.path().join("precious");
    fs::write(&precious, b"keep").unwrap();
    let doomed = installed(root.path(), "doomed");
    std::os::unix::fs::symlink(outside.path(), doomed.join("link")).unwrap();
    delete_folder(&doomed).unwrap();
    assert!(precious.exists());
}

#[test]
fn updating_settings_starts_from_what_the_folder_holds() {
    let root = tempfile::tempdir().unwrap();
    let folder = installed(root.path(), "work");
    update_in(&folder, |settings| {
        Ok(Settings {
            cpus: 6,
            ..settings
        })
    })
    .unwrap();
    let settings = update_in(&folder, |settings| {
        Ok(Settings {
            memory_gib: 12,
            ..settings
        })
    })
    .unwrap();
    assert_eq!((settings.cpus, settings.memory_gib), (6, 12));
}

#[test]
fn a_refused_update_changes_nothing() {
    let root = tempfile::tempdir().unwrap();
    let folder = installed(root.path(), "work");
    update_in(&folder, |_| Err(Error::InvalidDisplay)).unwrap_err();
    assert!(!folder.join(SETTINGS).exists());
}

#[test]
fn updating_a_machine_that_is_not_installed_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let error = update_in(&root.path().join("none"), Ok).unwrap_err();
    assert!(matches!(error, Error::NotInstalled(_)));
}

#[test]
fn writers_at_once_leave_a_whole_file_and_nothing_else() {
    let root = tempfile::tempdir().unwrap();
    let folder = installed(root.path(), "work");
    let path = folder.join(SETTINGS);
    std::thread::scope(|scope| {
        for cpus in 2..10 {
            let path = &path;
            scope.spawn(move || {
                let settings = Settings {
                    cpus,
                    ..Settings::default()
                };
                write_json(path, &settings).unwrap();
            });
        }
    });
    assert!(read_json::<Settings>(&path).is_some());
    let left: Vec<_> = fs::read_dir(&folder)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name())
        .collect();
    assert_eq!(left.len(), 2, "{left:?}");
}

fn name(text: &str) -> Name {
    Name::try_from(text.to_owned()).unwrap()
}

fn with_cpus(cpus: u16) -> Settings {
    Settings {
        cpus,
        ..Settings::default()
    }
}

#[test]
fn the_first_push_asks_for_a_writer_and_later_ones_do_not() {
    let queue = SettingsQueue::default();
    assert!(queue.push(name("work"), with_cpus(4)));
    assert!(!queue.push(name("work"), with_cpus(6)));
    queue.drain(|_, _| Ok(())).unwrap();
    assert!(queue.push(name("work"), with_cpus(8)));
}

#[test]
fn only_a_machines_latest_choice_is_written() {
    let queue = SettingsQueue::default();
    queue.push(name("work"), with_cpus(4));
    queue.push(name("work"), with_cpus(6));
    let mut written = Vec::new();
    queue
        .drain(|_, settings| {
            written.push(settings.cpus);
            Ok(())
        })
        .unwrap();
    assert_eq!(written, [6]);
}

#[test]
fn a_choice_made_during_a_write_is_written_by_the_same_writer() {
    let queue = SettingsQueue::default();
    queue.push(name("work"), with_cpus(4));
    let mut written = Vec::new();
    queue
        .drain(|_, settings| {
            written.push(settings.cpus);
            if settings.cpus == 4 {
                assert!(!queue.push(name("work"), with_cpus(6)));
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(written, [4, 6]);
}

#[test]
fn a_failed_write_is_reported_and_the_rest_still_written() {
    let queue = SettingsQueue::default();
    queue.push(name("one"), with_cpus(4));
    queue.push(name("two"), with_cpus(6));
    let mut tried = 0;
    let outcome = queue.drain(|_, _| {
        tried += 1;
        if tried == 1 {
            Err(Error::InvalidDisplay)
        } else {
            Ok(())
        }
    });
    assert!(matches!(outcome, Err(Error::InvalidDisplay)));
    assert_eq!(tried, 2);
}
