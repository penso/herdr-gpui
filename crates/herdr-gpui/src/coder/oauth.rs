//! Coder OAuth2 authorization code flow with PKCE (S256), which Coder requires
//! for every client. The browser returns to a one-shot listener bound to the
//! registered loopback redirect; only a reply carrying this request's `state`
//! is accepted. Everything here blocks and runs on a background worker.

use super::{Error, Result, Settings, http, token};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use secrecy::{ExposeSecret, SecretString};
use sha2::{Digest, Sha256};
use std::{
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    time::{Duration, Instant, SystemTime},
};
use zeroize::Zeroizing;

/// How long the browser has to complete the sign-in.
pub(crate) const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const ACCEPT_POLL: Duration = Duration::from_millis(50);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_LIMIT: usize = 8 * 1024;
const ERROR_CODE_LIMIT: usize = 64;

fn random(bytes: usize) -> Result<String> {
    let mut buffer = Zeroizing::new(vec![0; bytes]);
    getrandom::fill(&mut buffer).map_err(Error::Random)?;
    Ok(URL_SAFE_NO_PAD.encode(&*buffer))
}

/// A PKCE verifier: 32 random bytes encode to 43 unreserved characters.
pub(crate) struct Pkce {
    pub(crate) verifier: SecretString,
    pub(crate) challenge: String,
}

impl Pkce {
    pub(crate) fn new() -> Result<Self> {
        let verifier = Zeroizing::new(random(32)?);
        let challenge = challenge(&verifier);
        Ok(Self {
            verifier: verifier.as_str().into(),
            challenge,
        })
    }
}

fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// A sign-in waiting for the browser: the page to open and the bound listener.
pub(crate) struct Pending {
    pub(crate) url: String,
    listener: TcpListener,
    state: String,
    pkce: Pkce,
}

/// Bind the redirect listener first, so a port conflict is reported before
/// the browser is sent to a page whose redirect cannot land.
pub(crate) fn begin(settings: &Settings) -> Result<Pending> {
    let address = settings.redirect.address;
    let listener = TcpListener::bind(address).map_err(|source| Error::Listen {
        address: address.to_string(),
        source,
    })?;
    listener
        .set_nonblocking(true)
        .map_err(|source| Error::Listen {
            address: address.to_string(),
            source,
        })?;
    let state = random(16)?;
    let pkce = Pkce::new()?;
    let mut url = url::Url::parse(&settings.endpoint("/oauth2/authorize"))
        .map_err(|_| Error::Url("coder.url"))?;
    url.query_pairs_mut()
        .append_pair("client_id", &settings.client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", &settings.redirect.uri)
        .append_pair("state", &state)
        .append_pair("code_challenge", &pkce.challenge)
        .append_pair("code_challenge_method", "S256");
    Ok(Pending {
        url: url.into(),
        listener,
        state,
        pkce,
    })
}

impl Pending {
    /// Wait for the redirect, then exchange its code for a credential.
    pub(crate) fn finish(
        self,
        settings: &Settings,
        cancelled: impl Fn() -> bool,
    ) -> Result<token::Credential> {
        let deadline = Instant::now() + SIGN_IN_TIMEOUT;
        let code = self.wait(&settings.redirect.path, &cancelled, deadline)?;
        if cancelled() {
            return Err(Error::Cancelled);
        }
        exchange(settings, &code, &self.pkce.verifier)
    }

    fn wait(
        &self,
        path: &str,
        cancelled: &impl Fn() -> bool,
        deadline: Instant,
    ) -> Result<SecretString> {
        loop {
            if cancelled() {
                return Err(Error::Cancelled);
            }
            if Instant::now() >= deadline {
                return Err(Error::Timeout);
            }
            match self.listener.accept() {
                Ok((stream, _)) => {
                    if let Some(result) = self.answer(stream, path) {
                        return result;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(ACCEPT_POLL);
                }
                Err(error) => return Err(Error::Redirect(error)),
            }
        }
    }

    /// Serve one browser request. `None` keeps waiting: a request for another
    /// path, a malformed one, or one without this sign-in's `state`.
    fn answer(&self, mut stream: TcpStream, path: &str) -> Option<Result<SecretString>> {
        let request = read_request(&mut stream).ok()?;
        let reply = callback(&request, path, &self.state);
        let (status, text) = match &reply {
            Some(Ok(_)) => (
                "200 OK",
                "Signed in to Coder. You can close this tab and return to Herdr.",
            ),
            Some(Err(_)) => (
                "400 Bad Request",
                "Coder did not authorize the sign-in. Return to Herdr and try again.",
            ),
            None => ("404 Not Found", "Not found."),
        };
        let _ = respond(&mut stream, status, text);
        reply
    }
}

fn read_request(stream: &mut TcpStream) -> io::Result<Zeroizing<Vec<u8>>> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(REQUEST_TIMEOUT))?;
    stream.set_write_timeout(Some(REQUEST_TIMEOUT))?;
    let mut bytes = Zeroizing::new(Vec::with_capacity(REQUEST_LIMIT));
    let mut chunk = [0; 1024];
    while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
        if bytes.len() >= REQUEST_LIMIT {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        let room = REQUEST_LIMIT - bytes.len();
        bytes.extend_from_slice(&chunk[..read.min(room)]);
    }
    chunk.fill(0);
    Ok(bytes)
}

/// Parse `GET <path>?<query> HTTP/1.x` into this sign-in's outcome.
fn callback(request: &[u8], path: &str, state: &str) -> Option<Result<SecretString>> {
    let line = request.split(|b| *b == b'\n').next()?;
    let line = std::str::from_utf8(line).ok()?.trim_end();
    let mut parts = line.split(' ');
    let (Some("GET"), Some(target)) = (parts.next(), parts.next()) else {
        return None;
    };
    let (target_path, query) = target.split_once('?').unwrap_or((target, ""));
    if target_path != path {
        return None;
    }
    let mut code = None;
    let mut error = None;
    let mut matched = false;
    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
        match &*key {
            "state" => matched = value == state,
            "code" => code = Some(SecretString::from(value.into_owned())),
            "error" => error = Some(value.into_owned()),
            _ => {}
        }
    }
    if !matched {
        return None;
    }
    if let Some(error) = error {
        let error: String = error
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
            .take(ERROR_CODE_LIMIT)
            .collect();
        return Some(Err(Error::Authorization(error)));
    }
    Some(
        code.filter(|code| token::valid(code.expose_secret()))
            .ok_or(Error::Token),
    )
}

fn respond(stream: &mut TcpStream, status: &str, text: &str) -> io::Result<()> {
    let body = format!(
        "<!doctype html><meta charset=utf-8><title>Herdr</title><p style=\"font:16px system-ui;margin:3em\">{text}</p>"
    );
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}

#[derive(serde::Deserialize)]
struct TokenResponse {
    access_token: SecretString,
    #[serde(default)]
    token_type: Option<String>,
    #[serde(default)]
    refresh_token: Option<SecretString>,
    #[serde(default)]
    expires_in: Option<u64>,
    /// Coder serializes Go's `oauth2.Token`, whose expiry is an RFC 3339 time.
    #[serde(default)]
    expiry: Option<String>,
}

fn credential(settings: &Settings, reply: TokenResponse) -> Result<token::Credential> {
    if reply
        .token_type
        .as_deref()
        .is_some_and(|kind| !kind.eq_ignore_ascii_case("bearer"))
    {
        return Err(Error::Token);
    }
    let now = SystemTime::now();
    let expires_at = token::expiry(reply.expires_in, now).or_else(|| {
        let expiry = chrono::DateTime::parse_from_rfc3339(reply.expiry.as_deref()?).ok()?;
        u64::try_from(expiry.timestamp()).ok()
    });
    token::Credential::new(
        settings,
        reply.access_token,
        reply
            .refresh_token
            .filter(|token| !token.expose_secret().is_empty()),
        expires_at,
    )
}

pub(super) fn exchange(
    settings: &Settings,
    code: &SecretString,
    verifier: &SecretString,
) -> Result<token::Credential> {
    let reply = http::form(
        "oauth2_token",
        &settings.endpoint("/oauth2/tokens"),
        &[
            ("grant_type", "authorization_code"),
            ("code", code.expose_secret()),
            ("redirect_uri", &settings.redirect.uri),
            ("client_id", &settings.client_id),
            ("client_secret", settings.client_secret.expose_secret()),
            ("code_verifier", verifier.expose_secret()),
        ],
    )?;
    credential(settings, reply)
}

/// Trade the refresh token for a new pair. Coder rotates refresh tokens, so the
/// caller persists the result before making any other request.
pub(crate) fn refresh(settings: &Settings, saved: &token::Credential) -> Result<token::Credential> {
    let refresh = saved.refresh_token.as_ref().ok_or(Error::Authentication)?;
    tracing::info!(category = "coder_refresh", "Renewing saved Coder sign-in");
    let reply = http::form(
        "oauth2_refresh",
        &settings.endpoint("/oauth2/tokens"),
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh.expose_secret()),
            ("client_id", &settings.client_id),
            ("client_secret", settings.client_secret.expose_secret()),
        ],
    )
    .map_err(|error| match error {
        // An invalid or expired grant means signing in again, not a retry.
        Error::Status(super::Status { code: 400, .. }) => Error::Authentication,
        error => error,
    })?;
    credential(settings, reply)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::coder::tests::settings;

    #[test]
    fn pkce_matches_rfc_7636_and_uses_unreserved_characters() {
        // RFC 7636 appendix B.
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        let pkce = Pkce::new().unwrap();
        let verifier = pkce.verifier.expose_secret();
        assert_eq!(verifier.len(), 43);
        assert!(
            verifier
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~'))
        );
        assert_ne!(verifier, Pkce::new().unwrap().verifier.expose_secret());
    }

    #[test]
    fn only_the_matching_state_on_the_registered_path_is_accepted() {
        let get = |target: &str| format!("GET {target} HTTP/1.1\r\nHost: x\r\n\r\n");
        let accept = |target: &str| callback(get(target).as_bytes(), "/callback", "s1");
        assert_eq!(
            accept("/callback?code=abc&state=s1")
                .unwrap()
                .unwrap()
                .expose_secret(),
            "abc"
        );
        assert!(accept("/callback?code=abc&state=other").is_none());
        assert!(accept("/callback?code=abc").is_none());
        assert!(accept("/favicon.ico?state=s1&code=abc").is_none());
        assert!(
            callback(
                b"POST /callback?state=s1&code=abc HTTP/1.1\r\n\r\n",
                "/callback",
                "s1"
            )
            .is_none()
        );
        assert!(matches!(
            accept("/callback?state=s1&error=access_denied%0A%3Cscript%3E"),
            Some(Err(Error::Authorization(code))) if code == "access_deniedscript"
        ));
        assert!(matches!(
            accept("/callback?state=s1"),
            Some(Err(Error::Token))
        ));
        assert!(matches!(
            accept("/callback?state=s1&code=bad%20code"),
            Some(Err(Error::Token))
        ));
    }

    #[test]
    fn authorize_url_carries_pkce_state_and_the_exact_redirect() {
        let mut settings = settings();
        settings.redirect.address = "127.0.0.1:0".parse().unwrap();
        let pending = begin(&settings).unwrap();
        let url = url::Url::parse(&pending.url).unwrap();
        assert_eq!(url.path(), "/oauth2/authorize");
        let pairs: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(pairs["client_id"], settings.client_id);
        assert_eq!(pairs["redirect_uri"], settings.redirect.uri);
        assert_eq!(pairs["response_type"], "code");
        assert_eq!(pairs["code_challenge_method"], "S256");
        assert_eq!(
            pairs["code_challenge"],
            challenge(pending.pkce.verifier.expose_secret())
        );
        assert_eq!(pairs["state"], pending.state);
        assert!(!pending.url.contains(pending.pkce.verifier.expose_secret()));
        assert!(!pending.url.contains(settings.client_secret.expose_secret()));
    }

    #[test]
    fn listener_ignores_strays_answers_the_browser_and_honours_cancellation() {
        let mut settings = settings();
        settings.redirect.address = "127.0.0.1:0".parse().unwrap();
        let pending = begin(&settings).unwrap();
        let address = pending.listener.local_addr().unwrap();
        let state = pending.state.clone();
        let browser = std::thread::spawn(move || {
            let send = |target: String| {
                let mut stream = TcpStream::connect(address).unwrap();
                write!(stream, "GET {target} HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
                let mut reply = String::new();
                stream.read_to_string(&mut reply).unwrap();
                reply
            };
            let stray = send("/callback?code=forged&state=wrong".into());
            let real = send(format!("/callback?code=real-code&state={state}"));
            (stray, real)
        });
        let code = pending
            .wait(
                "/callback",
                &|| false,
                Instant::now() + Duration::from_secs(10),
            )
            .unwrap();
        assert_eq!(code.expose_secret(), "real-code");
        let (stray, real) = browser.join().unwrap();
        assert!(stray.starts_with("HTTP/1.1 404"));
        assert!(real.starts_with("HTTP/1.1 200"));
        assert!(real.contains("Cache-Control: no-store"));

        let pending = begin(&settings).unwrap();
        assert!(matches!(
            pending.wait(
                "/callback",
                &|| true,
                Instant::now() + Duration::from_secs(10)
            ),
            Err(Error::Cancelled)
        ));
        assert!(matches!(
            pending.wait("/callback", &|| false, Instant::now()),
            Err(Error::Timeout)
        ));
    }

    #[test]
    fn a_busy_redirect_port_is_reported_before_the_browser_opens() {
        let taken = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut settings = settings();
        settings.redirect.address = taken.local_addr().unwrap();
        assert!(matches!(begin(&settings), Err(Error::Listen { .. })));
    }

    #[test]
    fn token_replies_accept_relative_or_absolute_expiry() {
        let settings = settings();
        let parse = |json: &str| credential(&settings, serde_json::from_str(json).unwrap());
        let relative = parse(
            r#"{"access_token":"a","token_type":"Bearer","refresh_token":"r","expires_in":3600}"#,
        )
        .unwrap();
        assert!(relative.expires_at.is_some());
        assert!(relative.refresh_token.is_some());
        let absolute =
            parse(r#"{"access_token":"a","token_type":"bearer","expiry":"2030-01-01T00:00:00Z"}"#)
                .unwrap();
        assert_eq!(absolute.expires_at, Some(1_893_456_000));
        assert!(absolute.refresh_token.is_none());
        let empty_refresh = parse(r#"{"access_token":"a","refresh_token":""}"#).unwrap();
        assert!(empty_refresh.refresh_token.is_none());
        assert!(parse(r#"{"access_token":"a","token_type":"mac"}"#).is_err());
        assert!(parse(r#"{"access_token":""}"#).is_err());
    }
}
