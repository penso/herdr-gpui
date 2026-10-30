//! A repository owner's picture. GitHub serves a user's or organization's
//! at `github.com/{owner}.png`, which redirects to the account's picture by
//! numeric ID. The by-login address on avatars.githubusercontent.com answers
//! some organizations with the default octocat instead, as a valid image.
use super::{Image, cache};
use std::{path::Path, sync::Arc, time::SystemTime};
use ureq::http::StatusCode;

const PREFIX: &str = "https://github.com/";
const SUFFIX: &str = ".png?size=48";
const TARGET: &str = "https://avatars.githubusercontent.com/u/";

/// The address of `owner`'s picture, which also keys its disk cache entry.
/// `owner` is a segment `herdr_github::github_repo` already checked.
pub(super) fn url(owner: &str) -> String {
    format!("{PREFIX}{owner}{SUFFIX}")
}

/// Whether `url` is what [`url`] builds for some owner.
pub(super) fn valid(url: &str) -> bool {
    url.strip_prefix(PREFIX)
        .and_then(|rest| rest.strip_suffix(SUFFIX))
        .is_some_and(|owner| {
            !owner.is_empty()
                && owner.len() <= 100
                && owner
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        })
}

/// Whether the owner address may redirect to `location`: only an account's
/// picture by numeric ID, with an optional plain query such as `?s=48&v=4`.
pub(super) fn valid_redirect(location: &str) -> bool {
    let Some(rest) = location.strip_prefix(TARGET) else {
        return false;
    };
    let (id, query) = rest.split_once('?').unwrap_or((rest, ""));
    location.len() <= 2048
        && !id.is_empty()
        && id.bytes().all(|b| b.is_ascii_digit())
        && query
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'=' | b'&' | b'-' | b'_' | b'.'))
}

/// Where an owner address's answer leads: a redirect to an account's
/// picture, else nowhere. A picture served directly is refused too, since
/// that is not how GitHub answers.
pub(super) fn target(status: StatusCode, location: Option<&str>) -> Option<&str> {
    location.filter(|location| status.is_redirection() && valid_redirect(location))
}

/// Follows the owner address's one redirect, when it leads to an account's
/// picture, and caches the picture under the owner address. The agent
/// follows no redirects itself, so nothing else is ever fetched.
pub(super) fn download(agent: &ureq::Agent, url: &str) -> Option<Arc<Image>> {
    download_with(
        url,
        cache::root().as_deref(),
        SystemTime::now(),
        |url| {
            let response = agent.get(url).call().ok()?;
            let location = response
                .headers()
                .get("location")
                .and_then(|location| location.to_str().ok())
                .map(str::to_owned);
            Some((response.status(), location))
        },
        |target| super::fetch(agent, target),
    )
}

/// [`download`] with its two requests and its cache given: `answer` asks the
/// owner address for its status and `Location`, and `fetch` downloads the
/// checked picture it leads to.
pub(super) fn download_with(
    url: &str,
    root: Option<&Path>,
    now: SystemTime,
    answer: impl FnOnce(&str) -> Option<(StatusCode, Option<String>)>,
    fetch: impl FnOnce(&str) -> Option<(Arc<Image>, Vec<u8>)>,
) -> Option<Arc<Image>> {
    if !valid(url) {
        return None;
    }
    let (status, location) = answer(url)?;
    let (image, bytes) = fetch(target(status, location.as_deref())?)?;
    if let Some(root) = root {
        let _ = cache::write(root, url, &bytes, now);
    }
    Some(image)
}
