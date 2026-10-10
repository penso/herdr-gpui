use super::*;
use crate::orchestrator::location::{data_local_dir_with, database_path_in};
use std::{ffi::OsString, path::PathBuf};

/// Values from Python's `uuid.uuid5(uuid.NAMESPACE_URL, identity)`, the
/// derivation agent-launcher uses through the uuid crate.
#[test]
fn repository_ids_match_agent_launcher() {
    let local = Repository::Local("/Users/me/src/app/.git".into());
    assert_eq!(
        local.id().to_string(),
        "872b7964-8ccf-5eaf-80cc-e036166c2f30"
    );
    let ssh = Repository::Ssh {
        destination: "devbox".into(),
        git_dir: "/home/me/src/app/.git".into(),
    };
    assert_eq!(ssh.id().to_string(), "0eb4afab-5507-533b-9739-bb0e6c7ba259");
}

#[test]
fn database_path_is_agent_launchers_repository_directory() {
    let path = database_path_in(
        std::path::Path::new("/data"),
        &Repository::Local("/Users/me/src/app/.git".into()),
    );
    assert_eq!(
        path,
        PathBuf::from(
            "/data/agent-launcher/repositories/872b7964-8ccf-5eaf-80cc-e036166c2f30/state.sqlite3"
        )
    );
}

fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
    let pairs: Vec<(String, OsString)> = pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), OsString::from(value)))
        .collect();
    move |name| {
        pairs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    }
}

#[test]
#[cfg(target_os = "macos")]
fn data_directory_is_application_support_on_macos() {
    assert_eq!(
        data_local_dir_with(vars(&[("HOME", "/Users/me"), ("XDG_DATA_HOME", "/x")])),
        Some(PathBuf::from("/Users/me/Library/Application Support"))
    );
    assert_eq!(data_local_dir_with(vars(&[("HOME", "")])), None);
}

#[test]
#[cfg(all(unix, not(target_os = "macos")))]
fn data_directory_follows_xdg_on_other_unixes() {
    assert_eq!(
        data_local_dir_with(vars(&[("HOME", "/home/me"), ("XDG_DATA_HOME", "/xdg")])),
        Some(PathBuf::from("/xdg"))
    );
    // A relative XDG value is ignored, as the XDG specification requires.
    assert_eq!(
        data_local_dir_with(vars(&[("HOME", "/home/me"), ("XDG_DATA_HOME", "rel")])),
        Some(PathBuf::from("/home/me/.local/share"))
    );
}

#[test]
#[cfg(windows)]
fn data_directory_is_local_app_data_on_windows() {
    assert_eq!(
        data_local_dir_with(vars(&[("LOCALAPPDATA", r"C:\Users\me\AppData\Local")])),
        Some(PathBuf::from(r"C:\Users\me\AppData\Local"))
    );
}
