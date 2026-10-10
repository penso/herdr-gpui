use super::*;
use crate::orchestrator::{repo::parse, repo::parse_remote_url, service::sources};
use herdr_client::ConnectTarget;

#[test]
fn remote_urls_parse_as_agent_launcher_parses_them() {
    let cases = [
        (
            "git@github.com:penso/herdr-gpui.git",
            Provider::Github,
            "github.com",
            "penso/herdr-gpui",
        ),
        (
            "https://github.com/penso/herdr-gpui",
            Provider::Github,
            "github.com",
            "penso/herdr-gpui",
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
        parse(&ConnectTarget::Local, ""),
        Err(Error::NotARepository)
    ));
}

#[test]
fn only_github_dot_com_and_beads_are_synced() {
    let gitlab = "common\t/r/.git\nmain\t/r\nbeads\t1\nremote\torigin\tgit@gitlab.com:g/app.git\n";
    let info = parse(&ConnectTarget::Local, gitlab).unwrap();
    let found: Vec<_> = sources(&info)
        .into_iter()
        .map(|(key, supported)| (key.provider, supported))
        .collect();
    assert_eq!(found, [(Provider::Gitlab, false), (Provider::Beads, true)]);

    let enterprise = "common\t/r/.git\nmain\t/r\nremote\torigin\tgit@github.acme.com:g/app.git\n";
    let info = parse(&ConnectTarget::Local, enterprise).unwrap();
    assert!(!sources(&info)[0].1);
}
