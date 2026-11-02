use crate::{Endpoint, Error, endpoint::Inputs};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

fn resolve(
    vars: &[(&str, &str)],
    run_user: Option<&str>,
    config: Option<&str>,
) -> crate::Result<Endpoint> {
    let vars: Vec<(String, OsString)> = vars
        .iter()
        .map(|(k, v)| ((*k).to_owned(), OsString::from(v)))
        .collect();
    let var = |name: &str| {
        vars.iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    };
    let config = config.map(str::to_owned);
    let read = move |_: &Path| config.clone();
    // Windows has no runtime folder to fall back to; the pipe is named.
    #[cfg(windows)]
    let _ = run_user;
    let inputs = Inputs {
        #[cfg(unix)]
        run_user: run_user.map(PathBuf::from),
        config: &read,
    };
    Endpoint::resolve_with(&var, &inputs)
}

#[cfg(unix)]
#[test]
fn the_runtime_folder_holds_the_default_socket() {
    let at = |vars: &[(&str, &str)], run_user| resolve(vars, run_user, None).unwrap().socket;
    assert_eq!(
        at(&[("HOME", "/h"), ("XDG_RUNTIME_DIR", "/run/me")], None),
        PathBuf::from("/run/me/herdr-core/default.sock")
    );
    assert_eq!(
        at(&[("HOME", "/h")], Some("/run/user/501")),
        PathBuf::from("/run/user/501/herdr-core/default.sock")
    );
    assert_eq!(
        at(&[("HOME", "/h")], None),
        PathBuf::from("/h/.local/state/herdr-core/default/default.sock")
    );
    assert_eq!(
        at(
            &[
                ("HOME", "/h"),
                ("XDG_STATE_HOME", "/s"),
                ("HC_SESSION", " smoke ")
            ],
            None
        ),
        PathBuf::from("/s/herdr-core/smoke/smoke.sock")
    );
}

#[cfg(windows)]
#[test]
fn windows_names_a_pipe_per_user_and_session() {
    let endpoint = resolve(
        &[
            ("USERPROFILE", r"C:\Users\Me"),
            ("USERNAME", "Me Too"),
            ("HC_SESSION", "work"),
        ],
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        endpoint.socket,
        PathBuf::from(r"\\.\pipe\herdr-core-MeToo-work.sock")
    );
}

#[test]
fn an_explicit_socket_wins_over_config_and_defaults() {
    let home = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    let configured = resolve(&[(home, "/h")], None, Some("socket = \"/cfg.sock\"\n")).unwrap();
    assert_eq!(configured.socket, PathBuf::from("/cfg.sock"));
    let explicit = resolve(
        &[(home, "/h"), ("HC_SOCKET", "/env.sock")],
        None,
        Some("socket = \"/cfg.sock\"\n"),
    )
    .unwrap();
    assert_eq!(explicit.socket, PathBuf::from("/env.sock"));
    // A config hc cannot read is ignored, as hc ignores it.
    assert_ne!(
        resolve(&[(home, "/h")], None, Some("socket = ["))
            .unwrap()
            .socket,
        PathBuf::from("/cfg.sock")
    );
}

#[test]
fn a_session_must_be_a_folder_name() {
    let home = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    for bad in ["a/b", "..", "a:b"] {
        assert!(
            matches!(resolve(&[(home, "/h"), ("HC_SESSION", bad)], None, None), Err(Error::Session(name)) if name == bad)
        );
    }
    assert!(matches!(resolve(&[], None, None), Err(Error::NoHome)));
}
