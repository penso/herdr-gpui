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
