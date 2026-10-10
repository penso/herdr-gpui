//! The deliberately small HTTP/1.1 subset the companion speaks: one bounded
//! request per connection with an explicit `Content-Length`, answered with a
//! JSON body and `Connection: close`. Claude Code hooks and phone clients both
//! fit inside it, and anything outside it is refused rather than guessed at.

use crate::{HttpError, Result};
use std::io::{BufRead, Read, Write};

const MAX_HEAD: u64 = 16 * 1024;
const MAX_HEADERS: usize = 64;
pub(crate) const MAX_BODY: usize = 512 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Method {
    Get,
    Post,
    Other,
}

#[derive(Debug)]
pub(crate) struct Request {
    pub(crate) method: Method,
    pub(crate) path: String,
    query: String,
    /// Names are lowercased once here so lookups need no case folding.
    headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
}

impl Request {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    pub(crate) fn query(&self, name: &str) -> Option<&str> {
        self.query
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value)
    }

    #[cfg(test)]
    pub(crate) fn new(method: Method, target: &str, headers: &[(&str, &str)], body: &[u8]) -> Self {
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        Self {
            method,
            path: path.to_owned(),
            query: query.to_owned(),
            headers: headers
                .iter()
                .map(|(key, value)| (key.to_ascii_lowercase(), (*value).to_owned()))
                .collect(),
            body: body.to_vec(),
        }
    }
}

pub(crate) fn read_request(reader: &mut impl BufRead) -> Result<Request> {
    let mut head = reader.by_ref().take(MAX_HEAD);
    let request_line = read_line(&mut head)?;
    let mut parts = request_line.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(HttpError::RequestLine.into());
    };
    if !version.starts_with("HTTP/1.") || !target.starts_with('/') {
        return Err(HttpError::RequestLine.into());
    }
    let method = match method {
        "GET" => Method::Get,
        "POST" => Method::Post,
        _ => Method::Other,
    };
    let (path, query) = target.split_once('?').unwrap_or((target, ""));

    let mut headers = Vec::new();
    loop {
        let line = read_line(&mut head)?;
        if line.is_empty() {
            break;
        }
        if headers.len() == MAX_HEADERS {
            return Err(HttpError::HeadTooLarge.into());
        }
        let (name, value) = line.split_once(':').ok_or(HttpError::Header)?;
        if name.is_empty() || name.contains(char::is_whitespace) {
            return Err(HttpError::Header.into());
        }
        headers.push((name.to_ascii_lowercase(), value.trim().to_owned()));
    }

    let mut request = Request {
        method,
        path: path.to_owned(),
        query: query.to_owned(),
        headers,
        body: Vec::new(),
    };
    if request.header("transfer-encoding").is_some() {
        return Err(HttpError::TransferEncoding.into());
    }
    let lengths: Vec<_> = request
        .headers
        .iter()
        .filter(|(key, _)| key == "content-length")
        .collect();
    let length =
        match lengths.as_slice() {
            [] if method == Method::Post => return Err(HttpError::MissingContentLength.into()),
            [] => 0,
            [(_, value)] if value.bytes().all(|byte| byte.is_ascii_digit()) => value
                .parse::<usize>()
                .map_err(|_| HttpError::ContentLength)?,
            _ => return Err(HttpError::ContentLength.into()),
        };
    if length > MAX_BODY {
        return Err(HttpError::BodyTooLarge.into());
    }
    request.body = vec![0; length];
    reader.read_exact(&mut request.body).map_err(incomplete)?;
    Ok(request)
}

fn read_line(head: &mut impl BufRead) -> Result<String> {
    let mut line = Vec::new();
    head.read_until(b'\n', &mut line).map_err(incomplete)?;
    if line.last() != Some(&b'\n') {
        // Either the peer closed early or the head limit cut the line short.
        return Err(if line.is_empty() {
            HttpError::Incomplete
        } else {
            HttpError::HeadTooLarge
        }
        .into());
    }
    line.pop();
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    String::from_utf8(line).map_err(|_| HttpError::Header.into())
}

fn incomplete(error: std::io::Error) -> crate::Error {
    if error.kind() == std::io::ErrorKind::UnexpectedEof {
        HttpError::Incomplete.into()
    } else {
        error.into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Response {
    pub(crate) status: u16,
    pub(crate) content_type: &'static str,
    /// Empty means "no decision" to Claude Code, which a JSON `{}` would not.
    pub(crate) body: Vec<u8>,
}

/// Same-origin only: the web app loads nothing from elsewhere and cannot be
/// framed, and API responses inherit the same restrictions harmlessly.
const SECURITY_HEADERS: &str = "Content-Security-Policy: default-src 'self'; img-src 'self' data:; \
object-src 'none'; base-uri 'none'; frame-ancestors 'none'\r\n\
X-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\n";

impl Response {
    pub(crate) fn json(status: u16, body: &serde_json::Value) -> Self {
        Self {
            status,
            content_type: "application/json",
            body: body.to_string().into_bytes(),
        }
    }

    pub(crate) fn empty() -> Self {
        Self {
            status: 200,
            content_type: "application/json",
            body: Vec::new(),
        }
    }

    pub(crate) fn asset(content_type: &'static str, body: &[u8]) -> Self {
        Self {
            status: 200,
            content_type,
            body: body.to_vec(),
        }
    }

    pub(crate) fn error(status: u16, error: &crate::Error) -> Self {
        Self::json(status, &serde_json::json!({ "error": error.to_string() }))
    }

    pub(crate) fn write_to(&self, writer: &mut impl Write) -> std::io::Result<()> {
        let reason = match self.status {
            200 => "OK",
            400 => "Bad Request",
            401 => "Unauthorized",
            404 => "Not Found",
            405 => "Method Not Allowed",
            409 => "Conflict",
            413 => "Payload Too Large",
            422 => "Unprocessable Content",
            500 => "Internal Server Error",
            502 => "Bad Gateway",
            503 => "Service Unavailable",
            _ => "Error",
        };
        // Assets revalidate so a new binary's web app replaces the old one;
        // API answers are live state and never cached.
        let cache = if self.content_type == "application/json" {
            "no-store"
        } else {
            "no-cache"
        };
        write!(
            writer,
            "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: {cache}\r\n{SECURITY_HEADERS}Connection: close\r\n\r\n",
            self.status,
            self.content_type,
            self.body.len()
        )?;
        writer.write_all(&self.body)?;
        writer.flush()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::Error;

    fn parse(raw: &[u8]) -> Result<Request> {
        read_request(&mut &raw[..])
    }

    fn http_error(raw: &[u8]) -> HttpError {
        match parse(raw) {
            Err(Error::Http(error)) => error,
            other => panic!("expected an HTTP error, got {other:?}"),
        }
    }

    #[test]
    fn parses_a_post_with_headers_query_and_body() {
        let request = parse(
            b"POST /hooks/claude?x=1&after=7 HTTP/1.1\r\nHost: a\r\nX-Herdr-Pane: p_1\r\nContent-Length: 2\r\n\r\n{}",
        )
        .unwrap();
        assert_eq!(request.method, Method::Post);
        assert_eq!(request.path, "/hooks/claude");
        assert_eq!(request.query("after"), Some("7"));
        assert_eq!(request.header("x-herdr-pane"), Some("p_1"));
        assert_eq!(request.body, b"{}");
    }

    #[test]
    fn accepts_bare_newlines_and_a_get_without_length() {
        let request = parse(b"GET /v1/requests HTTP/1.1\nHost: a\n\n").unwrap();
        assert_eq!(request.method, Method::Get);
        assert!(request.body.is_empty());
    }

    #[test]
    fn refuses_what_it_cannot_frame_exactly() {
        assert_eq!(
            http_error(b"POST / HTTP/1.1\r\n\r\n"),
            HttpError::MissingContentLength
        );
        assert_eq!(
            http_error(b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n"),
            HttpError::TransferEncoding
        );
        assert_eq!(
            http_error(b"POST / HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 1\r\n\r\nx"),
            HttpError::ContentLength
        );
        assert_eq!(
            http_error(b"POST / HTTP/1.1\r\nContent-Length: +1\r\n\r\nx"),
            HttpError::ContentLength
        );
        assert_eq!(
            http_error(b"GET http://x/ HTTP/1.1\r\n\r\n"),
            HttpError::RequestLine
        );
        assert_eq!(
            http_error(b"GET / HTTP/1.1\r\nbad header\r\n\r\n"),
            HttpError::Header
        );
    }

    #[test]
    fn bounds_the_head_and_the_body() {
        let mut long = b"GET / HTTP/1.1\r\nX: ".to_vec();
        long.extend(std::iter::repeat_n(b'a', MAX_HEAD as usize));
        long.extend(b"\r\n\r\n");
        assert_eq!(http_error(&long), HttpError::HeadTooLarge);

        let raw = format!(
            "POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY + 1
        );
        assert_eq!(http_error(raw.as_bytes()), HttpError::BodyTooLarge);
    }

    #[test]
    fn reports_a_truncated_request_as_incomplete() {
        assert_eq!(http_error(b""), HttpError::Incomplete);
        assert_eq!(
            http_error(b"POST / HTTP/1.1\r\nContent-Length: 5\r\n\r\nab"),
            HttpError::Incomplete
        );
    }

    #[test]
    fn writes_an_empty_body_with_a_zero_length() {
        let mut out = Vec::new();
        Response::empty().write_to(&mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(text.contains("Content-Length: 0\r\n"));
        assert!(text.ends_with("\r\n\r\n"));
        assert!(text.contains("Cache-Control: no-store\r\n"));
        assert!(text.contains("frame-ancestors 'none'"));
    }
}
