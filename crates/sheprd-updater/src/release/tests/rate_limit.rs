use super::*;
use chrono::{DateTime, TimeDelta, Utc};

fn headers(pairs: &[(&'static str, &str)]) -> anyhow::Result<ureq::http::HeaderMap> {
    let mut map = ureq::http::HeaderMap::new();
    for (name, value) in pairs {
        map.insert(*name, value.parse()?);
    }
    Ok(map)
}

fn now() -> anyhow::Result<DateTime<Utc>> {
    DateTime::from_timestamp(1_791_868_000, 0).context("valid test time")
}

#[test]
fn exhausted_quota_reports_the_reset_time() -> anyhow::Result<()> {
    let now = now()?;
    let headers = headers(&[
        ("x-ratelimit-remaining", "0"),
        ("x-ratelimit-reset", "1791868909"),
    ])?;
    for status in [403, 429] {
        let error = status_error(status, &headers, now);
        assert!(
            matches!(error, Error::RateLimited { reset: Some(reset) } if reset.timestamp() == 1_791_868_909)
        );
    }
    Ok(())
}

#[test]
fn retry_after_takes_precedence_over_the_quota_reset() -> anyhow::Result<()> {
    let now = now()?;
    let headers = headers(&[("retry-after", "60"), ("x-ratelimit-reset", "1791868909")])?;
    assert!(matches!(
        status_error(403, &headers, now),
        Error::RateLimited { reset: Some(reset) } if reset == now + TimeDelta::seconds(60)
    ));
    Ok(())
}

#[test]
fn a_missing_or_past_reset_still_reports_the_rate_limit() -> anyhow::Result<()> {
    let now = now()?;
    for pairs in [
        &[("x-ratelimit-remaining", "0")][..],
        &[("x-ratelimit-remaining", "0"), ("x-ratelimit-reset", "1")],
        &[("retry-after", "Wed, 21 Oct 2026 07:28:00 GMT")],
    ] {
        assert!(matches!(
            status_error(403, &headers(pairs)?, now),
            Error::RateLimited { reset: None }
        ));
    }
    Ok(())
}

#[test]
fn other_refusals_keep_their_status() -> anyhow::Result<()> {
    let now = now()?;
    let quota_left = headers(&[("x-ratelimit-remaining", "12")])?;
    assert!(matches!(
        status_error(403, &quota_left, now),
        Error::HttpStatus(403)
    ));
    assert!(matches!(
        status_error(403, &headers(&[])?, now),
        Error::HttpStatus(403)
    ));
    let exhausted = headers(&[("x-ratelimit-remaining", "0")])?;
    assert!(matches!(
        status_error(404, &exhausted, now),
        Error::HttpStatus(404)
    ));
    Ok(())
}

#[test]
fn retry_minute_rounds_any_part_of_a_minute_up() -> anyhow::Result<()> {
    let at = |text: &str| -> anyhow::Result<DateTime<Utc>> {
        Ok(DateTime::parse_from_rfc3339(text)?.with_timezone(&Utc))
    };
    for (reset, expected) in [
        ("2026-10-13T12:34:00Z", "2026-10-13T12:34:00Z"),
        ("2026-10-13T12:34:00.500Z", "2026-10-13T12:35:00Z"),
        ("2026-10-13T12:34:00.000000001Z", "2026-10-13T12:35:00Z"),
        ("2026-10-13T12:34:01Z", "2026-10-13T12:35:00Z"),
        ("2026-10-13T12:34:59.999Z", "2026-10-13T12:35:00Z"),
        ("2026-10-13T23:59:30Z", "2026-10-14T00:00:00Z"),
    ] {
        assert_eq!(
            crate::error::retry_minute(at(reset)?),
            at(expected)?,
            "{reset}"
        );
    }
    Ok(())
}

#[test]
fn rate_limit_says_so_instead_of_the_status() -> anyhow::Result<()> {
    let reset = now()?;
    let local = crate::error::retry_minute(reset)
        .with_timezone(&chrono::Local)
        .format("%H:%M");
    assert_eq!(
        Error::RateLimited { reset: Some(reset) }.to_string(),
        format!("GitHub rate limit reached. Retry after {local}")
    );
    assert_eq!(
        Error::RateLimited { reset: None }.to_string(),
        "GitHub rate limit reached. Retry later"
    );
    Ok(())
}
