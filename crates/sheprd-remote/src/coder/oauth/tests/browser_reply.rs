//! The browser tab learns how the sign-in ended only after the token exchange.

use super::*;

/// Sign in against a deployment whose token endpoint answers `status`/`body`,
/// keeping the credential with `keep`, and return the outcome and the page the
/// browser was shown.
fn sign_in_keeping<T>(
    status: u16,
    body: &'static str,
    keep: impl FnOnce(token::Credential) -> Result<T>,
) -> (Result<T>, String) {
    let (base, _) = crate::cloud::tests::server::serve(move |_| (status, body.into()));
    let mut settings = settings();
    settings.base = base;
    settings.redirect.address = "127.0.0.1:0".parse().unwrap();
    let pending = begin(&settings).unwrap();
    let address = pending.listener.local_addr().unwrap();
    let state = pending.state.clone();
    let browser = std::thread::spawn(move || {
        let mut stream = TcpStream::connect(address).unwrap();
        write!(
            stream,
            "GET /callback?code=code-1&state={state} HTTP/1.1\r\nHost: localhost\r\n\r\n"
        )
        .unwrap();
        let mut reply = String::new();
        stream.read_to_string(&mut reply).unwrap();
        reply
    });
    let result = pending.finish(&settings, || false, keep);
    (result, browser.join().unwrap())
}

fn sign_in(status: u16, body: &'static str) -> (Result<token::Credential>, String) {
    sign_in_keeping(status, body, Ok)
}

#[test]
fn the_browser_is_told_signed_in_only_after_coder_issues_a_token() {
    let (result, page) = sign_in(
        200,
        r#"{"access_token":"access-1","token_type":"Bearer","expires_in":3600}"#,
    );
    assert_eq!(result.unwrap().access_token.expose_secret(), "access-1");
    assert!(page.starts_with("HTTP/1.1 200"));
    assert!(page.contains("Signed in to Coder"));
}

#[test]
fn a_rejected_client_secret_is_named_and_the_browser_is_not_told_signed_in() {
    let (result, page) = sign_in(
        401,
        r#"{"error":"invalid_client","error_description":"The client credentials are invalid"}"#,
    );
    let error = result.err().unwrap();
    assert!(matches!(error, Error::Client), "{error:?}");
    assert!(error.to_string().contains("client secret"));
    assert!(page.starts_with("HTTP/1.1 500"));
    assert!(!page.contains("Signed in"));
    assert!(page.contains("Return to Herdr to see why"));
}

#[test]
fn a_refused_grant_keeps_coder_s_reason() {
    let (result, page) = sign_in(
        400,
        r#"{"error":"invalid_grant","error_description":"The authorization code is invalid or expired"}"#,
    );
    assert_eq!(
        result.err().unwrap().to_string(),
        "Coder request failed (HTTP 400): The authorization code is invalid or expired"
    );
    assert!(!page.contains("Signed in"));
}

#[test]
fn a_refresh_with_a_rejected_client_secret_does_not_read_as_a_lost_sign_in() {
    let (base, _) =
        crate::cloud::tests::server::serve(|_| (401, r#"{"error":"invalid_client"}"#.into()));
    let mut settings = settings();
    settings.base = base;
    let saved = token::Credential::new(&settings, "a".into(), Some("r".into()), None).unwrap();
    assert!(matches!(refresh(&settings, &saved), Err(Error::Client)));
}

#[test]
fn a_credential_that_cannot_be_saved_is_not_reported_as_signed_in() {
    let (result, page) = sign_in_keeping(
        200,
        r#"{"access_token":"access-1","token_type":"Bearer","expires_in":3600}"#,
        |_| Err::<(), _>(Error::Token),
    );
    assert!(matches!(result, Err(Error::Token)));
    assert!(page.starts_with("HTTP/1.1 500"));
    assert!(!page.contains("Signed in"));
    assert!(page.contains("Return to Herdr to see why"));
}
