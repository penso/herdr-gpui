use super::*;

#[test]
fn an_entry_reads_back_until_replaced_or_removed() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::open_at(&path_in(dir.path())).unwrap();
    assert_eq!(cache.get::<Vec<u32>>(Kind::Security, "a").unwrap(), None);
    cache.put(Kind::Security, "a", &vec![1, 2]).unwrap();
    let entry = cache.get::<Vec<u32>>(Kind::Security, "a").unwrap().unwrap();
    assert_eq!(entry.value, vec![1, 2]);
    // Kinds keep their own keys.
    assert_eq!(cache.get::<Vec<u32>>(Kind::Repository, "a").unwrap(), None);
    cache.put(Kind::Security, "a", &vec![3]).unwrap();
    // Another connection, as another window opens one, sees the write.
    let other = Cache::open_at(&path_in(dir.path())).unwrap();
    assert_eq!(
        other
            .get::<Vec<u32>>(Kind::Security, "a")
            .unwrap()
            .unwrap()
            .value,
        vec![3]
    );
    cache.remove(Kind::Security, "a").unwrap();
    assert_eq!(other.get::<Vec<u32>>(Kind::Security, "a").unwrap(), None);
}

#[test]
fn a_value_that_no_longer_decodes_is_a_miss() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::open_at(&path_in(dir.path())).unwrap();
    cache.put(Kind::Repository, "k", &"text").unwrap();
    assert_eq!(cache.get::<Vec<u32>>(Kind::Repository, "k").unwrap(), None);
}

#[test]
#[cfg(unix)]
fn the_file_is_private() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = path_in(&dir.path().join("state"));
    Cache::open_at(&path).unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o077, 0, "{mode:o}");
}
