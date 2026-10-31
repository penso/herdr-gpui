use super::*;
use herdr_usage::Error;
use std::io;

#[test]
fn every_variant_has_a_message() {
    let mut errors = vec![
        Error::Connect,
        Error::Header(ureq::http::HeaderValue::from_str("\n").unwrap_err()),
        Error::Json(serde_json::error::Category::Syntax),
        Error::KeychainAccess,
        Error::KeychainDenied,
        Error::MissingCurl,
        Error::MixedSecrets,
        Error::Network(ureq::Error::HostNotFound),
        Error::NoPlan,
        Error::NotSignedIn,
        Error::RateLimited,
        Error::Rejected,
        Error::Size,
        Error::Status(429),
        Error::Timeout,
        Error::Unreachable,
        Error::WslHostUnsupported,
    ];
    #[cfg(feature = "azure")]
    errors.push(Error::CloudHostUnsupported);
    errors.extend(
        [
            "agy 1.1.11 or later is needed for usage reports",
            "agy could not report usage; run agy to sign in",
            "find the Gemini CLI's OAuth client to refresh the sign-in",
            "read CodeRabbit usage",
            "read the Windsurf plan cache",
            "amp usage",
        ]
        .map(Error::Command),
    );
    errors.extend(
        [
            "read a keychain item",
            "read the browser's cookie key",
            "run a provider command",
            "start the remote usage shell",
            "read the moon",
        ]
        .map(|operation| Error::Process {
            operation,
            source: io::Error::other("no such file"),
        }),
    );
    assert_localized(&errors);
    assert_localized(&[Error::Client(herdr_client::Error::Disconnected)]);
}
