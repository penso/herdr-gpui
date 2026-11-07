//! A checkout on GitLab: the project its remote names, the rows its issues
//! become, and the note a checkout created for one carries.
use super::super::{
    Forge, Target,
    context::write as write_context,
    gitlab::{item, project},
};
use super::*;
use crate::pull_request::{Input, Remote};
use sheprd_orchestrator::gitlab::{Api, Scheme};
use std::{
    path::Path,
    time::{Duration, Instant},
};

/// A repository in `directory` with the given remotes, read without the
/// user's own Git configuration.
fn repository(directory: &Path, remotes: &[(&str, &str)]) {
    let config = directory.join("gitconfig");
    std::fs::write(&config, "").unwrap();
    let checkout = directory.join("repo");
    std::fs::create_dir_all(&checkout).unwrap();
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .env("GIT_CONFIG_GLOBAL", &config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .arg("-C")
            .arg(&checkout)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "-q", "-b", "main"]);
    for (name, url) in remotes {
        git(&["remote", "add", name, url]);
    }
}

fn found(remotes: &[(&str, &str)]) -> Option<(String, String, Remote)> {
    let temporary = tempfile::tempdir().unwrap();
    repository(temporary.path(), remotes);
    let checkout = temporary.path().join("repo").canonicalize().unwrap();
    let target = Target {
        host: crate::pull_request::Origin::Local,
        input: Input {
            checkout: Some(checkout.to_str().unwrap().to_owned()),
            repo_key: None,
            branch: "main".into(),
        },
    };
    let deadline = Instant::now() + Duration::from_secs(20);
    project(&target, deadline, &|| false)
        .unwrap()
        .map(|project| {
            (
                project.source.host,
                project.source.repository,
                project.remote,
            )
        })
}

#[test]
fn a_checkouts_gitlab_project_comes_from_origin_else_upstream() {
    assert_eq!(
        found(&[("origin", "git@gitlab.com:group/sub/app.git")]),
        Some(("gitlab.com".into(), "group/sub/app".into(), Remote::Origin))
    );
    assert_eq!(
        found(&[("upstream", "https://gitlab.example.com/team/app.git")]),
        Some((
            "gitlab.example.com".into(),
            "team/app".into(),
            Remote::Upstream
        ))
    );
    // Origin is the remote read, as the Orchestrator reads it: a GitHub
    // origin is a GitHub repository, whatever its upstream.
    assert_eq!(
        found(&[
            ("origin", "git@github.com:me/app.git"),
            ("upstream", "git@gitlab.com:team/app.git"),
        ]),
        None
    );
    assert_eq!(found(&[]), None);
}

#[test]
fn an_issue_row_links_to_the_project_asked_for_and_names_its_branch() {
    let api = Api::new("gitlab.example.com", Scheme::Https, &[]);
    let row = item(
        "12",
        "Fix: the crash\non start",
        Some("ana"),
        "team/app",
        Remote::Origin,
        &api,
    )
    .unwrap();
    assert_eq!(row.kind, Kind::Issue);
    assert_eq!(row.number, 12);
    assert_eq!(row.url, "https://gitlab.example.com/team/app/-/issues/12");
    assert_eq!(row.author, "ana");
    assert!(!row.title.contains('\n'), "{:?}", row.title);
    assert!(
        row.branch().starts_with("12-fix-the-crash"),
        "{}",
        row.branch()
    );
    assert!(row.head.is_none());
    for invalid in ["0", "x", ""] {
        assert!(
            item(invalid, "t", None, "team/app", Remote::Origin, &api).is_none(),
            "{invalid:?}"
        );
    }
}

#[test]
fn a_gitlab_issues_note_says_it_is_on_gitlab() {
    let temporary = tempfile::tempdir().unwrap();
    repository(temporary.path(), &[]);
    let checkout = temporary.path().join("repo");
    let api = Api::new("gitlab.example.com", Scheme::Https, &[]);
    let row = item(
        "7",
        "Add a flag",
        Some("ana"),
        "team/app",
        Remote::Origin,
        &api,
    )
    .unwrap();
    let origin = Origin {
        owner: "team".into(),
        repo: "app".into(),
        forge: Forge::GitLab {
            host: "gitlab.example.com".into(),
        },
        ..Origin::default()
    };
    let directory = write_context(&checkout, &row, &origin).unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(directory.join("context.json")).unwrap())
            .unwrap();
    assert_eq!(json["forge"], "gitlab");
    assert_eq!(json["repository"], "team/app");
    assert_eq!(
        json["url"],
        "https://gitlab.example.com/team/app/-/issues/7"
    );
    let markdown = std::fs::read_to_string(directory.join("CONTEXT.md")).unwrap();
    assert!(markdown.contains("for a GitLab issue"), "{markdown}");
    assert!(markdown.contains("Issue: #7"));
}
