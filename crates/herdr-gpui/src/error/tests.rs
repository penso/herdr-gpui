use super::*;
use std::{error::Error as _, path::Path};

#[test]
fn github_network_message_names_the_transport_failure() {
    let error = herdr_github::Error::Network(ureq::Error::Io(io::Error::other(
        "invalid peer certificate: UnknownIssuer",
    )));
    let message = error.to_string();
    assert!(message.starts_with("GitHub network request failed or timed out"));
    assert!(
        message.contains("invalid peer certificate: UnknownIssuer"),
        "{message}"
    );
}

#[test]
fn updater_wrapper_preserves_source_chain() {
    let error = Error::from(crate::updater::Error::Io(io::Error::from(
        io::ErrorKind::BrokenPipe,
    )));
    assert!(matches!(error, Error::Update(_)));
    assert_eq!(
        error
            .source()
            .and_then(|source| source.source())
            .and_then(|source| source.downcast_ref::<io::Error>())
            .map(io::Error::kind),
        Some(io::ErrorKind::BrokenPipe)
    );
}

#[test]
fn github_failures_keep_typed_sources_and_redact_parser_diagnostics() -> anyhow::Result<()> {
    let source = serde_json::from_str::<u64>("\"fixture-private-value\"")
        .err()
        .ok_or_else(|| anyhow::anyhow!("expected JSON type error"))?;
    let error = Error::from(herdr_github::Error::json(source));
    assert!(matches!(error, Error::GitHub(herdr_github::Error::Json(_))));
    assert!(!format!("{error} {error:?}").contains("fixture-private-value"));
    assert!(
        error
            .source()
            .and_then(|source| source.source())
            .is_some_and(|source| source.is::<serde_json::Error>())
    );

    let error = Error::Credentials(herdr_credentials::Error::Io(io::Error::from(
        io::ErrorKind::PermissionDenied,
    )));
    assert!(
        error
            .source()
            .and_then(|source| source.downcast_ref::<io::Error>())
            .is_some_and(|source| source.kind() == io::ErrorKind::PermissionDenied)
    );
    let error = herdr_github::Error::Read(io::Error::from(io::ErrorKind::TimedOut));
    assert!(
        matches!(&error, herdr_github::Error::Read(source) if source.kind() == io::ErrorKind::TimedOut)
    );
    Ok(())
}

#[test]
fn io_context_retains_source_kind_and_cleanup_failure() {
    let error = Error::from(io::Error::new(io::ErrorKind::PermissionDenied, "denied"))
        .at_path(Path::new("config.toml"));
    assert_eq!(error.to_string(), "config.toml: denied");
    assert!(
        error
            .source()
            .and_then(|source| source.source())
            .and_then(|source| source.downcast_ref::<io::Error>())
            .is_some_and(|source| source.kind() == io::ErrorKind::PermissionDenied)
    );

    let error = Error::Cleanup {
        source: io::Error::other("write failed"),
        path: "temporary".into(),
        cleanup: io::Error::new(io::ErrorKind::PermissionDenied, "cleanup denied"),
    };
    assert_eq!(
        error.to_string(),
        "write failed; removing temporary: cleanup denied"
    );
    assert!(
        error
            .source()
            .is_some_and(|source| source.is::<io::Error>())
    );
    assert!(
        matches!(error, Error::Cleanup { cleanup, .. } if cleanup.kind() == io::ErrorKind::PermissionDenied)
    );
}

#[test]
fn client_schema_presentation_remains_redacted() -> anyhow::Result<()> {
    let source = serde_json::from_str::<Vec<String>>("{}")
        .err()
        .ok_or_else(|| anyhow::anyhow!("expected schema error"))?;
    let error = Error::from(herdr_client::Error::CatalogSchema(source));
    assert_eq!(error.to_string(), "invalid endpoint catalog schema");
    assert!(
        error
            .source()
            .and_then(|source| source.source())
            .is_some_and(|source| source.is::<serde_json::Error>())
    );
    Ok(())
}
