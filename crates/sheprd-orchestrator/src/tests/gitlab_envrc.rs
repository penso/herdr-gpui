//! A checkout's `.envrc` token: read from what direnv loads there, tried
//! before every other source, and never the token the app's own process has.
use super::gitlab_sign_in::environment;
use crate::gitlab::{
    self,
    account::{Source, resolve},
    envrc::from_env,
};
use secrecy::{ExposeSecret, SecretString};

fn secret(text: &str) -> SecretString {
    SecretString::from(text)
}

#[test]
fn a_checkouts_envrc_token_is_tried_first() {
    let found = resolve(
        "gitlab.com",
        || Some(secret("glpat-envrc")),
        environment(&[("GITLAB_TOKEN", "glpat-shell")]),
        || Some(secret("glpat-saved")),
        || Some(secret("glpat-cli")),
    )
    .unwrap();
    assert_eq!(found.source, Source::Envrc);
    assert_eq!(found.token.expose_secret(), "glpat-envrc");
}

#[test]
fn the_envrc_token_is_read_for_the_instance_it_names() {
    let env = b"PATH=/usr/bin\nGITLAB_TOKEN=glpat-envrc\nHOME=/home/me\n";
    assert_eq!(
        from_env(env, "gitlab.com").map(|token| token.expose_secret().to_owned()),
        Some("glpat-envrc".to_owned())
    );
    // GITLAB_HOST in the .envrc keeps its token for that instance alone.
    let env = b"GITLAB_HOST=https://gitlab.acme.com\nGITLAB_TOKEN=glpat-acme\n";
    assert!(from_env(env, "gitlab.com").is_none());
    assert_eq!(
        from_env(env, "gitlab.acme.com").map(|token| token.expose_secret().to_owned()),
        Some("glpat-acme".to_owned())
    );
    assert!(from_env(b"", "gitlab.com").is_none());
    assert!(from_env(b"GITLAB_HOST=gitlab.acme.com\n", "gitlab.acme.com").is_none());
}

#[test]
fn a_refused_envrc_token_asks_for_the_envrc_to_change() {
    let error = gitlab::Error::Authentication.from_source(Source::Envrc);
    assert!(matches!(error, gitlab::Error::EnvrcRefused));
    assert!(!error.needs_sign_in());
    assert!(error.to_string().contains(".envrc"));
}
