use super::*;

fn link(path: &str, cwd: Option<&str>) -> FileLink {
    FileLink {
        path: path.into(),
        line: None,
        query: None,
        pane_id: "pane".into(),
        cwd: cwd.map(Into::into),
        explicit: false,
        remote: Some("me@devbox".into()),
        unjoined: None,
    }
}

/// The shell and path the host is asked for, or `None` when there is none.
fn asked(path: &str, cwd: Option<&str>) -> Option<(RemoteShell, String)> {
    RemotePath::from_link(&link(path, cwd)).map(|remote| (remote.shell, remote.path))
}

fn powershell(path: &str) -> Option<(RemoteShell, String)> {
    Some((RemoteShell::PowerShell, path.into()))
}

fn posix(path: &str) -> Option<(RemoteShell, String)> {
    Some((RemoteShell::Posix, path.into()))
}

#[test]
fn a_query_printed_after_the_path_is_not_the_host_s() {
    let page = link("/srv/www/index.html", None);
    let with_query = FileLink {
        query: Some("?a=1".into()),
        ..page.clone()
    };
    assert_eq!(
        RemotePath::from_link(&with_query),
        RemotePath::from_link(&page)
    );
    assert_eq!(
        asked("/srv/www/index.html", None),
        posix("/srv/www/index.html")
    );
}

#[test]
fn a_local_pane_has_no_remote_path() {
    let local = FileLink {
        remote: None,
        ..link("/home/me/a.md", None)
    };
    assert_eq!(RemotePath::from_link(&local), None);
}

#[test]
fn a_drive_path_is_read_in_powershell_with_backslashes() {
    assert_eq!(
        asked(r"C:\dev\notes.md", None),
        powershell(r"C:\dev\notes.md")
    );
    assert_eq!(
        asked("c:/dev/notes.md", None),
        powershell(r"c:\dev\notes.md")
    );
    assert_eq!(
        asked(r"D:/dev\sub/x.rs", Some("/home/me")),
        powershell(r"D:\dev\sub\x.rs")
    );
}

#[test]
fn a_drive_path_keeps_its_target_and_line() {
    let remote = RemotePath::from_link(&FileLink {
        line: Some(42),
        ..link(r"C:\dev\notes.md", None)
    })
    .unwrap();
    assert_eq!(
        remote,
        RemotePath {
            target: "me@devbox".into(),
            shell: RemoteShell::PowerShell,
            path: r"C:\dev\notes.md".into(),
            line: Some(42),
        }
    );
}

#[test]
fn a_file_url_drive_path_loses_its_leading_slash() {
    assert_eq!(
        asked("/C:/Users/me/a.md", None),
        powershell(r"C:\Users\me\a.md")
    );
    // A relative path under such a cwd joins it as a drive path too.
    assert_eq!(
        asked("a.md", Some("/C:/Users/me")),
        powershell(r"C:\Users\me\a.md")
    );
}

#[test]
fn a_unc_path_is_read_in_powershell() {
    assert_eq!(
        asked(r"\\server\share\a.md", None),
        powershell(r"\\server\share\a.md")
    );
    assert_eq!(
        asked("a.md", Some(r"\\server\share")),
        powershell(r"\\server\share\a.md")
    );
}

#[test]
fn a_posix_absolute_path_is_kept_as_is() {
    assert_eq!(asked("/home/me/x.md", None), posix("/home/me/x.md"));
    assert_eq!(
        asked("/srv/a b/../x.md", Some(r"C:\dev")),
        posix("/srv/a b/../x.md")
    );
}

#[test]
fn a_home_path_is_left_for_the_host_to_expand() {
    assert_eq!(asked("~/x.md", Some("/home/me")), posix("~/x.md"));
    assert_eq!(asked("~/x.md", None), posix("~/x.md"));
    assert_eq!(asked("~", None), posix("~"));
    // A Windows host reads it in PowerShell, by the cwd or the separator.
    assert_eq!(asked("~/x.md", Some(r"C:\Users\me")), powershell(r"~\x.md"));
    assert_eq!(asked(r"~\x.md", None), powershell(r"~\x.md"));
    // Another account's home is not resolved.
    assert_eq!(asked("~other/x.md", Some("/home/me")), None);
}

#[test]
fn a_relative_path_joins_a_posix_cwd_with_slashes() {
    assert_eq!(
        asked("src/main.rs", Some("/home/me/app")),
        posix("/home/me/app/src/main.rs")
    );
    assert_eq!(
        asked("./src/main.rs", Some("/home/me/app/")),
        posix("/home/me/app/src/main.rs")
    );
    assert_eq!(
        asked("../x.md", Some("/home/me/app")),
        posix("/home/me/app/../x.md")
    );
    assert_eq!(asked("a.md", Some("/")), posix("/a.md"));
}

#[test]
fn a_relative_path_joins_a_windows_cwd_with_backslashes() {
    assert_eq!(
        asked(r"src\main.rs", Some(r"C:\dev\app")),
        powershell(r"C:\dev\app\src\main.rs")
    );
    assert_eq!(
        asked("src/main.rs", Some("C:/dev/app/")),
        powershell(r"C:\dev\app\src\main.rs")
    );
    assert_eq!(
        asked(r".\src\main.rs", Some(r"C:\dev\app")),
        powershell(r"C:\dev\app\src\main.rs")
    );
    assert_eq!(
        asked(r"..\x.md", Some(r"C:\dev\app")),
        powershell(r"C:\dev\app\..\x.md")
    );
    assert_eq!(asked("a.md", Some(r"C:\")), powershell(r"C:\a.md"));
}

#[test]
fn a_relative_path_without_an_absolute_cwd_has_no_remote_path() {
    assert_eq!(asked("src/main.rs", None), None);
    assert_eq!(asked("src/main.rs", Some("relative/dir")), None);
    assert_eq!(asked("src/main.rs", Some("C:relative")), None);
    assert_eq!(asked("", Some("/home/me")), None);
    assert_eq!(asked(r"\dev\x.md", Some(r"C:\dev")), None);
}
