use super::*;

fn scratch(name: &str) -> PathBuf {
    let dir = env::temp_dir().join(format!("herdr-wsl-catalog-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir.join("client").join(FILE_NAME)
}

#[test]
fn a_missing_file_is_an_empty_list() {
    let path = scratch("missing");
    assert_eq!(read(&path).unwrap(), WslHosts::default());
}

#[test]
fn hosts_are_added_once_selected_and_removed_with_their_selection() {
    let path = scratch("lifecycle");
    add(&path, "Ubuntu", "default").unwrap();
    add(&path, "Debian", "work").unwrap();
    assert!(matches!(
        add(&path, "Ubuntu", "other"),
        Err(crate::Error::Wsl(Error::HostExists))
    ));
    select(&path, Some("Ubuntu")).unwrap();
    let hosts = read(&path).unwrap();
    assert_eq!(
        hosts.hosts,
        [
            WslHost {
                distro: "Ubuntu".into(),
                session: "default".into()
            },
            WslHost {
                distro: "Debian".into(),
                session: "work".into()
            },
        ]
    );
    assert_eq!(
        hosts.selected_host().map(|host| host.distro.as_str()),
        Some("Ubuntu")
    );
    assert!(matches!(
        select(&path, Some("Arch")),
        Err(crate::Error::Wsl(Error::HostMissing))
    ));
    remove(&path, "Ubuntu").unwrap();
    let hosts = read(&path).unwrap();
    assert_eq!(hosts.selected, None);
    assert_eq!(hosts.hosts.len(), 1);
    assert!(matches!(
        remove(&path, "Ubuntu"),
        Err(crate::Error::Wsl(Error::HostMissing))
    ));
    let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
}

#[test]
fn invalid_names_never_reach_the_file() {
    let path = scratch("invalid");
    assert!(matches!(
        add(&path, "-d", "default"),
        Err(crate::Error::Wsl(Error::InvalidDistro))
    ));
    assert!(matches!(
        add(&path, "Ubuntu", "../x"),
        Err(crate::Error::InvalidSession)
    ));
    assert!(!path.exists());
}

#[test]
fn damaged_files_are_refused_rather_than_trusted() {
    for bytes in [
        &br#"{"version":2,"hosts":[]}"#[..],
        br#"{"hosts":[{"distro":"-x","session":"default"}]}"#,
        br#"{"hosts":[{"distro":"A","session":"default"},{"distro":"A","session":"w"}]}"#,
        br#"{"hosts":[{"distro":"A","session":"../w"}]}"#,
    ] {
        assert!(
            matches!(parse(bytes), Err(crate::Error::Wsl(Error::Catalog))),
            "{bytes:?}"
        );
    }
    assert!(matches!(
        parse(br#"{"hosts":[],"extra":1}"#),
        Err(crate::Error::Wsl(Error::CatalogSchema(_)))
    ));
    assert!(matches!(
        parse(&vec![b' '; MAX_BYTES as usize + 1]),
        Err(crate::Error::Wsl(Error::Catalog))
    ));
    // A selection naming no saved distribution is kept but never resolved.
    let hosts = parse(br#"{"selected":"Gone","hosts":[]}"#).unwrap();
    assert_eq!(hosts.selected_host(), None);
}
