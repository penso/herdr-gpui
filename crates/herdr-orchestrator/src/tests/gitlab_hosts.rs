use super::*;
use crate::gitlab::{
    self, Api, Credential, Scheme,
    cli_hosts::{self, CliHost},
    issues_url,
};
use crate::repo::parse_remote_url;
use secrecy::SecretString;
use std::{
    ffi::OsString,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::{Path, PathBuf},
};

/// glab's config for an instance whose remotes use an SSH host of their own,
/// as glab writes it, beside gitlab.com.
const GLAB_CONFIG: &str = "\
# What protocol to use when performing Git operations. Supported values: ssh, https.
git_protocol: ssh
hosts:
    gitlab.com:
        # What protocol to use to access the API endpoint. Supported values: http, https.
        api_protocol: https
        # Configure host for API endpoint. Defaults to the host itself.
        api_host: gitlab.com
        token: glpat-not-a-real-token
    gitlab.example.com:
        api_host: gitlab.example.com
        ssh_host: gitlab-ssh.example.com
        custom_headers:
            - name: CF-Access-Client-Id
              valueFromEnv: CF_ACCESS_CLIENT_ID
        user: penso
";

fn api_for(remote: &str, config: &str) -> (SourceKey, Api) {
    let key = parse_remote_url(remote).unwrap();
    let api = Api::new(
        &key.host,
        Scheme::of_remote(remote),
        &cli_hosts::parse(config),
    );
    (key, api)
}

fn instance(host: &str) -> Api {
    Api {
        instance: host.into(),
        host: host.into(),
        scheme: Scheme::Https,
    }
}

/// #766: the issue listing went to the SSH host, which serves no HTTPS, and
/// timed out. It now goes to the instance glab maps that SSH host to.
#[test]
fn an_ssh_host_remote_is_read_from_the_instance_glab_maps_it_to() {
    let (key, api) = api_for("git@gitlab-ssh.example.com:group/project.git", GLAB_CONFIG);
    assert_eq!(api, instance("gitlab.example.com"));
    let url = issues_url((&key, &api), None).unwrap();
    assert!(
        url.as_str()
            .starts_with("https://gitlab.example.com/api/v4/projects/group%2Fproject/issues?"),
        "{url}"
    );
    // The shared source key still names the remote's host, as agent-launcher's does.
    assert_eq!(key.host, "gitlab-ssh.example.com");
    assert_eq!(key.provider, Provider::Gitlab);
}

#[test]
fn scp_and_ssh_remotes_map_with_or_without_a_port() {
    let with_port = GLAB_CONFIG.replace(
        "ssh_host: gitlab-ssh.example.com",
        "ssh_host: gitlab-ssh.example.com:2222",
    );
    for config in [GLAB_CONFIG, with_port.as_str()] {
        for remote in [
            "git@gitlab-ssh.example.com:group/project.git",
            "git@gitlab-ssh.example.com:group/sub/project",
            "ssh://git@gitlab-ssh.example.com/group/project.git",
            "ssh://git@gitlab-ssh.example.com:2222/group/project.git",
            "ssh://git@GITLAB-SSH.example.com:2222/group/project",
        ] {
            let (_, api) = api_for(remote, config);
            assert_eq!(
                api,
                instance("gitlab.example.com"),
                "{remote} with {config}"
            );
        }
    }
}

/// A remote on an instance's own host, or on a host glab does not know, is
/// read exactly as before.
#[test]
fn a_remote_glab_does_not_map_keeps_its_own_host() {
    let own = |remote: &str| {
        let key = parse_remote_url(remote).unwrap();
        Api::new(&key.host, Scheme::of_remote(remote), &[])
    };
    for remote in [
        "git@gitlab.example.com:group/project.git",
        "https://gitlab.example.com/group/project.git",
        "ssh://git@gitlab.example.com:2222/group/project.git",
        "http://gitlab.internal:8080/group/app.git",
        "git@gitlab.com:group/app.git",
        "git@gitlab.other.com:group/app.git",
    ] {
        let (_, api) = api_for(remote, GLAB_CONFIG);
        assert_eq!(api, own(remote), "{remote}");
        assert_eq!(api.host, api.instance, "{remote}");
    }
    assert_eq!(
        api_for("http://gitlab.internal:8080/group/app.git", GLAB_CONFIG)
            .1
            .scheme,
        Scheme::Http
    );
    // An instance's own name wins over another instance's `ssh_host`.
    let both = format!("{GLAB_CONFIG}    gitlab.ssh.example.com:\n        ssh_host: gitlab.com\n");
    assert_eq!(
        api_for("git@gitlab.com:group/app.git", &both).1,
        instance("gitlab.com")
    );
    // Without glab's config nothing maps.
    let (_, api) = api_for("git@gitlab-ssh.example.com:group/project.git", "");
    assert_eq!(api, instance("gitlab-ssh.example.com"));
}

#[test]
fn a_mapped_instance_uses_its_api_host_and_protocol() {
    let config = "\
hosts:
  git.corp.example:
    api_host: 'api.corp.example:8443' # behind a gateway
    api_protocol: \"http\"
    ssh_host: ssh.corp.example
";
    let (key, api) = api_for("git@ssh.corp.example:team/app.git", config);
    assert_eq!(
        api,
        Api {
            instance: "git.corp.example".into(),
            host: "api.corp.example:8443".into(),
            scheme: Scheme::Http,
        }
    );
    let url = issues_url((&key, &api), None).unwrap();
    assert!(
        url.as_str()
            .starts_with("http://api.corp.example:8443/api/v4/projects/team%2Fapp/issues?"),
        "{url}"
    );
    // A token is still withheld over plain HTTP.
    let token = SecretString::from("glpat-test");
    assert!(matches!(
        Credential::new(api.scheme, Some(&token)),
        Credential::Withheld
    ));
}

#[test]
fn only_the_fields_glab_writes_under_each_host_are_read() {
    let config = "\
editor: vim
hosts:
    # A comment between hosts.
    \"gitlab.example.com:8443\":
        ssh_host: gitlab-ssh.example.com   # trailing comment
        custom_headers:
            - name: X-Ssh-Host
              ssh_host: not-a-field.example.com
    flow.example.com: {ssh_host: ignored.example.com}
        ssh_host: orphan.example.com
    empty.example.com:
        api_host:
aliases:
    ssh_host: not-a-host.example.com
";
    assert_eq!(
        cli_hosts::parse(config),
        [
            CliHost {
                host: "gitlab.example.com:8443".into(),
                ssh_host: Some("gitlab-ssh.example.com".into()),
                ..CliHost::default()
            },
            CliHost {
                host: "empty.example.com".into(),
                ..CliHost::default()
            },
        ]
    );
    for text in ["", "hosts: {}\n", "hosts:\n", "editor: vim\n"] {
        assert!(cli_hosts::parse(text).is_empty(), "{text:?}");
    }
}

#[test]
fn glabs_config_is_found_where_glab_looks() {
    let home = Path::new("/home/me");
    let path = |pairs: &[(&str, &str)], legacy: bool| {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(name, value)| ((*name).into(), (*value).into()))
            .collect();
        cli_hosts::config_path(
            |name| {
                pairs
                    .iter()
                    .find(|(key, _)| key == name)
                    .map(|(_, value)| OsString::from(value))
            },
            Some(home),
            |path| legacy && path == Path::new("/home/me/.config/glab-cli/config.yml"),
        )
    };
    assert_eq!(
        path(&[("GLAB_CONFIG_DIR", "/etc/glab")], true),
        Some(PathBuf::from("/etc/glab/config.yml"))
    );
    assert_eq!(
        path(&[("XDG_CONFIG_HOME", "/xdg")], true),
        Some(PathBuf::from("/home/me/.config/glab-cli/config.yml"))
    );
    assert_eq!(
        path(
            &[("GLAB_CONFIG_DIR", ""), ("XDG_CONFIG_HOME", "/xdg")],
            false
        ),
        Some(PathBuf::from("/xdg/glab-cli/config.yml"))
    );
    let platform = if cfg!(target_os = "macos") {
        Some(PathBuf::from(
            "/home/me/Library/Application Support/glab-cli/config.yml",
        ))
    } else if cfg!(windows) {
        Some(PathBuf::from("C:/Local/glab-cli/config.yml"))
    } else {
        Some(PathBuf::from("/home/me/.config/glab-cli/config.yml"))
    };
    assert_eq!(path(&[("LOCALAPPDATA", "C:/Local")], false), platform);
}

/// The listing reaches the mapped API over the wire, not the SSH host: both
/// the issues and the merge requests.
#[test]
fn a_sync_requests_the_mapped_api_host() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        (0..2)
            .map(|_| {
                let (stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut head = Vec::new();
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line.trim_end().is_empty() {
                        break;
                    }
                    head.push(line.trim_end().to_owned());
                }
                (&stream)
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n[]")
                    .unwrap();
                head
            })
            .collect::<Vec<_>>()
    });
    let config = format!(
        "hosts:\n    gitlab.example.com:\n        api_host: {address}\n        api_protocol: http\n        ssh_host: gitlab-ssh.example.com\n"
    );
    let (key, api) = api_for("git@gitlab-ssh.example.com:group/project.git", &config);
    let listing = gitlab::sync((&key, &api), None, None, &|| false, &mut None).unwrap();
    assert!(listing.complete && listing.items.is_empty());
    let heads = server.join().unwrap();
    for (head, collection) in heads.iter().zip(["issues", "merge_requests"]) {
        assert!(
            head[0].starts_with(&format!(
                "GET /api/v4/projects/group%2Fproject/{collection}?"
            )),
            "{head:?}"
        );
        assert!(
            head.iter()
                .any(|line| line.eq_ignore_ascii_case(&format!("host: {address}"))),
            "{head:?}"
        );
    }
}

/// A host that cannot be reached is named, so a wrong one is visible.
#[test]
fn an_unreachable_api_names_the_address_it_tried() {
    let address = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let key = parse_remote_url("git@gitlab-ssh.example.com:group/project.git").unwrap();
    let api = Api {
        instance: "gitlab.example.com".into(),
        host: address.to_string(),
        scheme: Scheme::Http,
    };
    let error = gitlab::sync((&key, &api), None, None, &|| false, &mut None)
        .err()
        .unwrap();
    let origin = format!("http://{address}");
    assert!(
        matches!(&error, Error::Gitlab(gitlab::Error::Network { origin: tried, .. }) if *tried == origin),
        "{error:?}"
    );
    assert!(error.to_string().contains(&origin), "{error}");
}

/// A host whose name reads as neither GitLab nor GitHub, holding a project
/// one group deep, was taken for GitHub and marked unsupported before the
/// mapping was ever read. glab knowing the host makes it a GitLab project.
#[test]
fn a_host_glab_knows_is_a_gitlab_source() {
    let config = "hosts:\n    git.corp.example:\n        ssh_host: ssh.corp.example\n";
    let output =
        "common\t/repo/.git\nmain\t/repo\nremote\torigin\tgit@ssh.corp.example:team/app.git\n";
    let mut info = crate::repo::parse(&herdr_client::ConnectTarget::Local, output).unwrap();
    assert_eq!(
        info.remote.as_ref().unwrap().source.provider,
        Provider::Github
    );
    let remote = info.remote.as_mut().unwrap();
    gitlab::recognize(&mut remote.source, || cli_hosts::parse(config));
    let expected = SourceKey {
        provider: Provider::Gitlab,
        host: "ssh.corp.example".into(),
        repository: "team/app".into(),
    };
    assert_eq!(crate::service::sources(&info), vec![(expected, true)]);

    for (remote, provider) in [
        ("git@git.corp.example:team/app.git", Provider::Gitlab),
        ("git@other.example:team/app.git", Provider::Github),
        ("git@github.com:team/app.git", Provider::Github),
    ] {
        let mut source = parse_remote_url(remote).unwrap();
        gitlab::recognize(&mut source, || cli_hosts::parse(config));
        assert_eq!(source.provider, provider, "{remote}");
    }
    // github.com is never looked up in glab's config.
    let mut source = parse_remote_url("git@github.com:team/app.git").unwrap();
    gitlab::recognize(&mut source, || unreachable!());
    assert_eq!(source.provider, Provider::Github);
}
