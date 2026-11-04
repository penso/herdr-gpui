use super::*;

#[test]
fn http_remotes_with_userinfo_resolve_to_the_owner() {
    for scheme in ["https://", "http://"] {
        for userinfo in [
            "user@",
            "user:token@",
            "x-access-token:ghp_abc123@",
            "first.last@",
            "user%40example.com@",
        ] {
            for repo in ["repo", "repo.git"] {
                let remote = format!("{scheme}{userinfo}github.com/Some-Owner/{repo}");
                assert_eq!(
                    github_repo(&remote),
                    Some(("some-owner".to_owned(), "repo".to_owned())),
                    "{remote}"
                );
            }
        }
    }
}

#[test]
fn ssh_remotes_are_unchanged() {
    for remote in [
        "git@github.com:Some-Owner/repo.git",
        "ssh://git@github.com/Some-Owner/repo.git",
    ] {
        assert_eq!(
            github_repo(remote),
            Some(("some-owner".to_owned(), "repo".to_owned())),
            "{remote}"
        );
    }
    assert_eq!(github_owner("ssh://git@github.com:22/owner/repo"), None);
}

#[test]
fn userinfo_never_moves_the_host_off_github() {
    for remote in [
        // The host follows the userinfo, so these point at another server.
        "https://github.com@evil.test/owner/repo",
        "https://user@evil.test/owner/repo",
        "https://user@github.com.evil.test/owner/repo",
        "https://user:token@github.com:443/owner/repo",
        // Empty userinfo, or userinfo a URL parser could split differently.
        "https://@github.com/owner/repo",
        "https://a@b@github.com/owner/repo",
        "https://user#@github.com/owner/repo",
        "https://user?@github.com/owner/repo",
        "https://user\\@github.com/owner/repo",
        "https://us er@github.com/owner/repo",
        // Userinfo does not relax the path rules.
        "https://user@github.com/owner",
        "https://user@github.com/owner/repo/extra",
        "https://user@github.com/owner/repo?query",
    ] {
        assert_eq!(github_repo(remote), None, "{remote}");
    }
}
