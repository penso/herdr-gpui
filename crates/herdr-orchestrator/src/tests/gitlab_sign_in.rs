//! GitLab tokens pasted in the account menu: the saved record, the order tokens
//! are tried in, and which failures ask the user to sign in.
use crate::gitlab::{
    self, Credential,
    account::{Source, resolve},
    saved::{MAX_HOSTS, Saved, normalize_host},
    status_error,
};
use secrecy::{ExposeSecret, SecretString};
use std::{cell::Cell, ffi::OsString};

fn secret(text: &str) -> SecretString {
    SecretString::from(text)
}

#[test]
fn a_host_is_kept_as_a_remote_names_it() {
    for (input, host) in [
        ("gitlab.com", "gitlab.com"),
        ("  GitLab.Acme.com ", "gitlab.acme.com"),
        ("https://gitlab.acme.com/", "gitlab.acme.com"),
        ("gitlab.acme.com:8443", "gitlab.acme.com:8443"),
        ("http://code.internal:8080", "code.internal:8080"),
    ] {
        assert_eq!(normalize_host(input).as_deref(), Some(host), "{input}");
    }
    for invalid in [
        "",
        "gitlab.com/group",
        "-gitlab.com",
        "gitlab.com:0",
        "gitlab.com:99999",
        "git lab.com",
        "gitlab.com\ttoken",
    ] {
        assert_eq!(normalize_host(invalid), None, "{invalid:?}");
    }
}

#[test]
fn saved_tokens_read_back_by_host_and_forget_one_at_a_time() {
    let mut saved = Saved::default();
    saved.set("GitLab.com", Some(secret("glpat-one"))).unwrap();
    saved
        .set("https://gitlab.acme.com/", Some(secret(" glpat-two ")))
        .unwrap();
    assert_eq!(
        saved.hosts().collect::<Vec<_>>(),
        ["gitlab.acme.com", "gitlab.com"]
    );
    assert_eq!(
        saved.get("gitlab.ACME.com").unwrap().expose_secret(),
        "glpat-two"
    );

    // What is written is what is read.
    let mut record = String::new();
    for host in saved.hosts() {
        record.push_str(&format!(
            "{host}\t{}\n",
            saved.get(host).unwrap().expose_secret()
        ));
    }
    let again = Saved::parse(&secret(&record)).unwrap();
    assert_eq!(
        again.get("gitlab.com").unwrap().expose_secret(),
        "glpat-one"
    );

    saved.set("gitlab.com", None).unwrap();
    assert_eq!(saved.hosts().collect::<Vec<_>>(), ["gitlab.acme.com"]);
}

#[test]
fn a_bad_host_or_token_is_refused_before_it_is_saved() {
    let mut saved = Saved::default();
    assert!(matches!(
        saved.set("gitlab.com/group", Some(secret("glpat-x"))),
        Err(gitlab::Error::Host(_))
    ));
    assert!(matches!(
        saved.set("gitlab.com", Some(secret("glpat with space"))),
        Err(gitlab::Error::Token)
    ));
    assert!(matches!(
        saved.set("gitlab.com", Some(secret(""))),
        Err(gitlab::Error::Token)
    ));
    assert_eq!(saved.hosts().count(), 0);
}

#[test]
fn the_record_holds_a_bounded_number_of_hosts() {
    let mut saved = Saved::default();
    for index in 0..MAX_HOSTS {
        saved
            .set(
                &format!("gitlab{index}.example.com"),
                Some(secret("glpat-x")),
            )
            .unwrap();
    }
    assert!(matches!(
        saved.set("one-more.example.com", Some(secret("glpat-x"))),
        Err(gitlab::Error::TooManyHosts)
    ));
    // Replacing a saved host's token is not a new host.
    saved
        .set("gitlab0.example.com", Some(secret("glpat-y")))
        .unwrap();

    let record: String = (0..=MAX_HOSTS)
        .map(|index| format!("gitlab{index}.example.com\tglpat-x\n"))
        .collect();
    assert!(matches!(
        Saved::parse(&secret(&record)),
        Err(gitlab::Error::Record)
    ));
}

#[test]
fn a_malformed_record_is_refused_unread() {
    for record in [
        "gitlab.com glpat-x\n",
        "gitlab.com\t\n",
        "GitLab.com\tglpat-x\n",
        "gitlab.com/group\tglpat-x\n",
        "gitlab.com\tglpat x\n",
    ] {
        assert!(
            matches!(Saved::parse(&secret(record)), Err(gitlab::Error::Record)),
            "{record:?}"
        );
    }
    assert_eq!(Saved::parse(&secret("")).unwrap().hosts().count(), 0);
}

pub(super) fn environment(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
    let pairs: Vec<(String, OsString)> = pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), OsString::from(value)))
        .collect();
    move |name| {
        pairs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    }
}

#[test]
fn the_environment_beats_a_saved_token_which_beats_the_cli() {
    let asked = Cell::new(0);
    let saved = || {
        asked.set(asked.get() + 1);
        Some(secret("glpat-saved"))
    };
    let cli = || Some(secret("glpat-cli"));
    let found = |pairs: &[(&str, &str)], saved: Option<&str>, cli: Option<&str>| {
        resolve(
            "gitlab.com",
            || None,
            environment(pairs),
            || saved.map(secret),
            || cli.map(secret),
        )
        .map(|found| (found.source, found.token.expose_secret().to_owned()))
    };

    let env = [("GITLAB_TOKEN", "glpat-env")];
    assert_eq!(
        found(&env, Some("glpat-saved"), Some("glpat-cli")),
        Some((Source::Environment, "glpat-env".to_owned()))
    );
    assert_eq!(
        found(&[], Some("glpat-saved"), Some("glpat-cli")),
        Some((Source::Saved, "glpat-saved".to_owned()))
    );
    assert_eq!(
        found(&[], None, Some("glpat-cli")),
        Some((Source::Cli, "glpat-cli".to_owned()))
    );
    assert!(found(&[], None, None).is_none());

    // An environment token meant for another host leaves this one to the
    // saved token; one meant for it never asks the store.
    let elsewhere = [("GITLAB_HOST", "gitlab.acme.com"), ("GITLAB_TOKEN", "x")];
    assert_eq!(
        resolve("gitlab.com", || None, environment(&elsewhere), saved, cli)
            .map(|found| found.source),
        Some(Source::Saved)
    );
    assert_eq!(asked.get(), 1);
    resolve("gitlab.com", || None, environment(&env), saved, cli);
    assert_eq!(asked.get(), 1);
}

#[test]
fn only_a_missing_or_refused_token_offers_the_sign_in() {
    let token = secret("glpat-x");
    let with = Credential::Token(&token);
    let sign_in = |status, credential| status_error(status, "g/app", credential).needs_sign_in();
    assert!(sign_in(404, Credential::Anonymous));
    assert!(sign_in(401, with));
    // A working token without the scope or the project, and a token withheld
    // over HTTP, are not fixed by signing in again.
    assert!(!sign_in(403, with));
    assert!(!sign_in(404, with));
    assert!(!sign_in(401, Credential::Withheld));
    assert!(!sign_in(500, with));
}

#[test]
fn a_refused_gitlab_token_variable_asks_for_it_to_change() {
    let refused = |source| gitlab::Error::Authentication.from_source(source);
    let error = refused(Source::Environment);
    assert!(matches!(error, gitlab::Error::EnvironmentRefused));
    // A saved token cannot replace it, so the view offers no sign-in.
    assert!(!error.needs_sign_in());
    assert!(error.needs_user());
    assert!(error.to_string().contains("GITLAB_TOKEN"));
    for source in [Source::Saved, Source::Cli] {
        assert!(matches!(refused(source), gitlab::Error::Authentication));
    }
    assert!(matches!(
        gitlab::Error::Scope.from_source(Source::Environment),
        gitlab::Error::Scope
    ));
}

/// The private file's store, in `dir`.
#[cfg(unix)]
fn file_record(dir: &std::path::Path, record: &str) {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(dir.join("gitlab-credentials"))
        .unwrap();
    std::io::Write::write_all(&mut file, record.as_bytes()).unwrap();
}

#[cfg(unix)]
#[test]
fn a_malformed_saved_record_is_replaced_by_the_next_save_or_forget() {
    use crate::gitlab::saved::{read_in, save_in};
    use herdr_credentials::Store;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path();
    file_record(path, "gitlab.com glpat-x\n");
    assert!(matches!(
        read_in(Store::File, path),
        Err(gitlab::Error::Record)
    ));

    save_in(Store::File, path, "gitlab.com", Some(secret("glpat-new"))).unwrap();
    let saved = read_in(Store::File, path).unwrap();
    assert_eq!(
        saved
            .get("gitlab.com")
            .map(|token| token.expose_secret().to_owned()),
        Some("glpat-new".to_owned())
    );

    file_record(path, "not a record\n");
    save_in(Store::File, path, "gitlab.com", None).unwrap();
    assert_eq!(read_in(Store::File, path).unwrap().hosts().count(), 0);
}

#[test]
fn no_credential_store_says_how_to_opt_in_for_gitlab() {
    let error = gitlab::Error::from(herdr_credentials::Error::Policy);
    assert!(matches!(error, gitlab::Error::NoStore(_)));
    let text = error.to_string();
    assert!(text.contains("[gitlab] allow_plaintext_credentials"));
    // Linux's keyrings are named only where they exist.
    assert!(!text.contains("GNOME"), "{text}");
    #[cfg(target_os = "linux")]
    {
        let error = gitlab::Error::from(herdr_credentials::Error::SecretServiceUnavailable);
        assert!(matches!(error, gitlab::Error::NoKeyring(_)));
        assert!(error.to_string().contains("GNOME Keyring"));
    }
    assert!(std::error::Error::source(&error).is_some());
    assert!(matches!(
        gitlab::Error::from(herdr_credentials::Error::Permissions),
        gitlab::Error::Store(_)
    ));
}

#[test]
fn the_store_follows_the_plaintext_opt_in() {
    let config = gitlab::Config::default();
    assert_eq!(config.store(), herdr_credentials::Store::for_policy(false));
    let opted = gitlab::Config {
        allow_plaintext_credentials: true,
        ..Default::default()
    };
    assert_eq!(opted.store(), herdr_credentials::Store::for_policy(true));
}

#[test]
fn an_instance_is_checked_where_glab_sends_its_api() {
    use crate::gitlab::{Api, Scheme, account::whoami, cli_hosts::CliHost};
    let cli = [CliHost {
        host: "gitlab.example.com".into(),
        api_host: Some("api.gitlab.example.com".into()),
        api_protocol: Some("HTTP".into()),
        ..CliHost::default()
    }];
    let api = Api::of_instance("GitLab.Example.com", &cli);
    assert_eq!(api.host, "api.gitlab.example.com");
    assert_eq!(api.scheme, Scheme::Http);
    // A token is never sent to an API served only over HTTP, so nothing is asked.
    assert!(matches!(
        whoami(&api, &secret("glpat-x")),
        Err(gitlab::Error::InsecureHost(_))
    ));
    let plain = Api::of_instance("gitlab.com", &cli);
    assert_eq!(
        (plain.host.as_str(), plain.scheme),
        ("gitlab.com", Scheme::Https)
    );
}
