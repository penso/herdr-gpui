use super::*;
use crate::{repo::parse, repo::parse_remote_url, service::sources};
use sheprd_client::ConnectTarget;

#[test]
fn remote_urls_parse_as_agent_launcher_parses_them() {
    let cases = [
        (
            "git@github.com:sheprd-labs/sheprd.git",
            Provider::Github,
            "github.com",
            "sheprd-labs/sheprd",
        ),
        (
            "https://github.com/sheprd-labs/sheprd",
            Provider::Github,
            "github.com",
            "sheprd-labs/sheprd",
        ),
        (
            "ssh://git@GitHub.com:22/penso/app.git",
            Provider::Github,
            "github.com",
            "penso/app",
        ),
        (
            "https://git.example.com:8443/team/app.git",
            Provider::Github,
            "git.example.com:8443",
            "team/app",
        ),
        (
            "git@gitlab.com:group/sub/app.git",
            Provider::Gitlab,
            "gitlab.com",
            "group/sub/app",
        ),
        (
            "https://code.example.com/group/sub/app",
            Provider::Gitlab,
            "code.example.com",
            "group/sub/app",
        ),
    ];
    for (url, provider, host, repository) in cases {
        let key = parse_remote_url(url).unwrap();
        assert_eq!(
            (key.provider, key.host.as_str(), key.repository.as_str()),
            (provider, host, repository),
            "{url}"
        );
    }
    for invalid in [
        "",
        "/local/path",
        "file:///tmp/repo",
        "https://github.com/solo",
        "host:",
    ] {
        assert!(
            matches!(parse_remote_url(invalid), Err(Error::RemoteUrl(_))),
            "{invalid}"
        );
    }
}

const OUTPUT: &str = "common\t/Users/me/src/app/.git\nmain\t/Users/me/src/app\nbeads\t1\n\
remote\tfork\tgit@github.com:me/app.git\nremote\tupstream\tgit@github.com:org/app.git\n";

#[test]
fn detection_prefers_origin_then_upstream_then_the_first_remote() {
    let info = parse(&ConnectTarget::Local, OUTPUT).unwrap();
    assert_eq!(
        info.repository,
        Repository::Local("/Users/me/src/app/.git".into())
    );
    assert_eq!(info.main_root, "/Users/me/src/app");
    let remote = info.remote.unwrap();
    assert_eq!(
        (remote.name.as_str(), remote.source.repository.as_str()),
        ("upstream", "org/app")
    );

    let with_origin = format!("{OUTPUT}remote\torigin\thttps://github.com/penso/app\n");
    let remote = parse(&ConnectTarget::Local, &with_origin)
        .unwrap()
        .remote
        .unwrap();
    assert_eq!(remote.name, "origin");

    let only_fork = "common\t/r/.git\nmain\t/r\nremote\tfork\tgit@github.com:me/app.git\n";
    let remote = parse(&ConnectTarget::Local, only_fork)
        .unwrap()
        .remote
        .unwrap();
    assert_eq!(remote.name, "fork");
}

#[test]
fn ssh_repositories_are_keyed_by_destination_and_beads_by_main_root() {
    let target = ConnectTarget::Ssh {
        target: "devbox".into(),
        session: "default".into(),
    };
    let info = parse(&target, OUTPUT).unwrap();
    assert_eq!(
        info.repository,
        Repository::Ssh {
            destination: "devbox".into(),
            git_dir: "/Users/me/src/app/.git".into(),
        }
    );
    assert_eq!(
        info.beads_source().unwrap().canonical(),
        "beads:local:/Users/me/src/app"
    );
}

#[test]
fn a_folder_outside_git_is_reported() {
    assert!(matches!(
        parse(&ConnectTarget::Local, "not-repository\n"),
        Err(Error::NotARepository)
    ));
}

#[test]
fn github_dot_com_gitlab_and_beads_are_synced() {
    let gitlab = "common\t/r/.git\nmain\t/r\nbeads\t1\nremote\torigin\tgit@gitlab.com:g/app.git\n";
    let info = parse(&ConnectTarget::Local, gitlab).unwrap();
    let found: Vec<_> = sources(&info)
        .into_iter()
        .map(|(key, supported)| (key.provider, supported))
        .collect();
    assert_eq!(found, [(Provider::Gitlab, true), (Provider::Beads, true)]);

    // A self-managed GitLab is read at its own host.
    let managed = "common\t/r/.git\nmain\t/r\nremote\torigin\tgit@gitlab.acme.com:g/app.git\n";
    let info = parse(&ConnectTarget::Local, managed).unwrap();
    assert!(sources(&info)[0].1);

    let enterprise = "common\t/r/.git\nmain\t/r\nremote\torigin\tgit@github.acme.com:g/app.git\n";
    let info = parse(&ConnectTarget::Local, enterprise).unwrap();
    assert!(!sources(&info)[0].1);
}

/// A local checkout is read with `git` run directly, which every client
/// platform can do: from a linked worktree, the shared common directory, the
/// main checkout, Beads, and the `origin` remote.
#[test]
fn a_local_worktree_is_read_without_a_shell() {
    use crate::{location::Repository, repo::detect};
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("app");
    let linked = dir.path().join("linked");
    std::fs::create_dir_all(main.join(".beads")).unwrap();
    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(&main)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    };
    git(&["init", "-q"]);
    git(&[
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@t",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-q",
        "--allow-empty",
        "-m",
        "base",
    ]);
    git(&["remote", "add", "upstream", "git@gitlab.com:g/app.git"]);
    git(&[
        "remote",
        "add",
        "origin",
        "https://github.com/penso/app.git",
    ]);
    git(&[
        "worktree",
        "add",
        "-q",
        &linked.to_string_lossy(),
        "-b",
        "side",
    ]);

    let host = sheprd_remote::teleport::Host::new(&ConnectTarget::Local).unwrap();
    let never = std::sync::atomic::AtomicBool::new(false);
    let info = detect(
        &ConnectTarget::Local,
        &host,
        &linked.to_string_lossy(),
        &never,
    )
    .unwrap();
    let common = crate::repo::plain(main.join(".git").canonicalize().unwrap());
    assert_eq!(info.repository, Repository::Local(common));
    assert_eq!(
        std::path::Path::new(&info.main_root)
            .canonicalize()
            .unwrap(),
        main.canonicalize().unwrap()
    );
    assert!(info.beads);
    let remote = info.remote.unwrap();
    assert_eq!(remote.name, "origin");
    assert_eq!(remote.source.repository, "penso/app");

    // A folder outside Git, and one that is gone, are not repositories.
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    for folder in [outside, dir.path().join("gone")] {
        assert!(matches!(
            detect(
                &ConnectTarget::Local,
                &host,
                &folder.to_string_lossy(),
                &never
            ),
            Err(Error::NotARepository)
        ));
    }
}
