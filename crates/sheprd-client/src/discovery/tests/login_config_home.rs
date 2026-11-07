use super::*;

fn login() -> PathBuf {
    PathBuf::from("/shell/config")
}

/// The process's own `XDG_CONFIG_HOME` always wins; the login shell's only
/// fills in when it is unset, and no other variable falls back to it.
#[test]
fn login_config_home_only_fills_an_unset_config_home() {
    let login = login();
    let process = |name: &str| (name == "XDG_CONFIG_HOME").then(|| OsString::from("/process"));
    assert_eq!(
        with_login_config_home("XDG_CONFIG_HOME", process, Some(&login)),
        Some("/process".into())
    );
    assert_eq!(
        with_login_config_home("XDG_CONFIG_HOME", |_| None, Some(&login)),
        Some("/shell/config".into())
    );
    assert_eq!(
        with_login_config_home("XDG_CONFIG_HOME", |_| None, None),
        None
    );
    for name in ["HOME", "HERDR_SESSION", "HERDR_SOCKET_PATH", "APPDATA"] {
        assert_eq!(with_login_config_home(name, |_| None, Some(&login)), None);
    }
}

/// A launch without the shell's `XDG_CONFIG_HOME` dials the daemon its shell
/// started, and its named sessions, under the shell's root.
#[test]
fn local_sockets_follow_the_login_config_home() {
    let login = login();
    let var = |name: &str| {
        with_login_config_home(
            name,
            |name| (name == "HOME").then(|| OsString::from("/home/me")),
            Some(&login),
        )
    };
    assert_eq!(
        ConnectTarget::Local.socket_path_with(var).unwrap(),
        Path::new("/shell/config/herdr/herdr-client.sock")
    );
    assert_eq!(
        ConnectTarget::Session {
            name: "work".into(),
            development: true
        }
        .local_session_socket_path_with(var)
        .unwrap(),
        Path::new("/shell/config/herdr-dev/sessions/work/herdr-client.sock")
    );
}

/// Without a login value the old default is unchanged.
#[cfg(unix)]
#[test]
fn without_a_login_config_home_the_default_root_is_kept() {
    let var = |name: &str| {
        with_login_config_home(
            name,
            |name| (name == "HOME").then(|| OsString::from("/home/me")),
            None,
        )
    };
    assert_eq!(
        ConnectTarget::Local.socket_path_with(var).unwrap(),
        Path::new("/home/me/.config/herdr/herdr-client.sock")
    );
}

/// A relative root names nothing the daemon could share, so it is refused
/// and leaves the process's resolution alone.
#[test]
fn a_relative_login_config_home_is_refused() {
    assert!(!set_login_config_home(PathBuf::from("relative/config")));
    assert!(LOGIN_CONFIG_HOME.get().is_none());
}
