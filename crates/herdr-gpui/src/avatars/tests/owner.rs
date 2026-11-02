use super::*;
use crate::avatars::owner::{self, target, url, valid, valid_redirect};
use ureq::http::StatusCode;

#[test]
fn an_owner_picture_is_fetched_from_github_com() {
    for (remote, expected) in [
        (
            "git@github.com:Example-Org/app.git",
            "https://github.com/example-org.png?size=48",
        ),
        (
            "https://github.com/penso/herdr-gpui",
            "https://github.com/penso.png?size=48",
        ),
        (
            "ssh://git@github.com/some-org.io/repo.git",
            "https://github.com/some-org.io.png?size=48",
        ),
    ] {
        let owner = github_owner(remote).expect(remote);
        let url = url(&owner);
        assert_eq!(url, expected);
        assert!(valid(&url), "{url}");
    }
}

/// Only the owner address is read from the cache or downloaded: an image a
/// former build cached under the by-login address, which may be the default
/// octocat, is never read again.
#[test]
fn only_owner_addresses_are_valid() {
    for url in [
        "https://avatars.githubusercontent.com/example-org?size=48",
        "http://github.com/example-org.png?size=48",
        "https://github.com/example-org.png",
        "https://github.com/example-org.png?size=48&x=1",
        "https://github.com/.png?size=48",
        "https://github.com/a/b.png?size=48",
        "https://github.com/a%2fb.png?size=48",
        "https://github.com.evil.com/example-org.png?size=48",
        "https://user@github.com/example-org.png?size=48",
    ] {
        assert!(!valid(url), "{url}");
        assert!(cached_image(url).is_none(), "{url}");
    }
}

#[test]
fn the_redirect_must_lead_to_an_account_picture_by_id() {
    for location in [
        "https://avatars.githubusercontent.com/u/1234567?s=48&v=4",
        "https://avatars.githubusercontent.com/u/1",
        "https://avatars.githubusercontent.com/u/1?v=4",
    ] {
        assert!(valid_redirect(location), "{location}");
        for status in [
            StatusCode::MOVED_PERMANENTLY,
            StatusCode::FOUND,
            StatusCode::TEMPORARY_REDIRECT,
            StatusCode::PERMANENT_REDIRECT,
        ] {
            assert_eq!(target(status, Some(location)), Some(location));
        }
    }
}

#[test]
fn a_redirect_anywhere_else_is_refused() {
    for location in [
        // The by-login address answers some organizations with the octocat.
        "https://avatars.githubusercontent.com/example-org?size=48",
        "https://avatars.githubusercontent.com/u/",
        "https://avatars.githubusercontent.com/u/12a",
        "https://avatars.githubusercontent.com/u/1/../../evil",
        "https://avatars.githubusercontent.com/u/1?v=4#x",
        "https://avatars.githubusercontent.com/u/1?next=https://evil.example",
        "https://avatars.githubusercontent.com.evil.example/u/1",
        "https://avatars.githubusercontent.com@evil.example/u/1",
        "http://avatars.githubusercontent.com/u/1",
        "https://evil.example/u/1",
        "https://github.com/example-org.png?size=48",
        "/u/1",
        "",
    ] {
        assert!(!valid_redirect(location), "{location:?}");
        assert_eq!(target(StatusCode::FOUND, Some(location)), None);
    }
    let long = format!(
        "https://avatars.githubusercontent.com/u/1?v={}",
        "4".repeat(2048)
    );
    assert!(!valid_redirect(&long));
}

/// The picture comes only through a redirect: a picture served from the
/// owner address itself, an error, or a redirect without a location is
/// refused.
#[test]
fn only_a_redirect_is_followed() {
    let picture = "https://avatars.githubusercontent.com/u/1234567?s=48&v=4";
    for status in [
        StatusCode::OK,
        StatusCode::NOT_FOUND,
        StatusCode::INTERNAL_SERVER_ERROR,
    ] {
        assert_eq!(target(status, Some(picture)), None, "{status}");
    }
    assert_eq!(target(StatusCode::FOUND, None), None);
}

// Exercises the disk cache; other platforms have no cache root.
#[cfg(any(unix, windows))]
#[test]
fn a_download_follows_the_redirect_and_caches_under_the_owner_address() {
    let fixture = cache::tests::Fixture::new();
    let owner = url("example-org");
    let picture = "https://avatars.githubusercontent.com/u/1234567?s=48&v=4";
    let bytes = cache::tests::png();
    let now = SystemTime::now();
    let image = owner::download_with(
        &owner,
        Some(&fixture.0),
        now,
        |asked| {
            assert_eq!(asked, owner);
            Some((StatusCode::FOUND, Some(picture.to_owned())))
        },
        |target| {
            assert_eq!(target, picture);
            Some((cache::decode(&bytes)?, bytes.clone()))
        },
    )
    .expect("owner picture");
    assert_eq!(image.bytes, bytes);
    let (cached, fresh) = cache::read(&fixture.0, &owner, now).expect("cached under owner");
    assert_eq!(cached.bytes, bytes);
    assert!(fresh);
    assert!(cache::read(&fixture.0, picture, now).is_none());
}

#[cfg(any(unix, windows))]
#[test]
fn a_refused_answer_fetches_and_caches_nothing() {
    let fixture = cache::tests::Fixture::new();
    let owner = url("example-org");
    let now = SystemTime::now();
    for (status, location) in [
        (StatusCode::FOUND, Some("https://evil.example/u/1")),
        (
            StatusCode::FOUND,
            Some("https://avatars.githubusercontent.com/example-org?size=48"),
        ),
        (StatusCode::OK, None),
        (StatusCode::FOUND, None),
    ] {
        let image = owner::download_with(
            &owner,
            Some(&fixture.0),
            now,
            |_| Some((status, location.map(str::to_owned))),
            |target| panic!("{target} must not be fetched"),
        );
        assert!(image.is_none());
        assert!(cache::read(&fixture.0, &owner, now).is_none());
    }
    // An address that is not an owner's is not even asked.
    let image = owner::download_with(
        "https://avatars.githubusercontent.com/example-org?size=48",
        Some(&fixture.0),
        now,
        |asked| panic!("{asked} must not be asked"),
        |target| panic!("{target} must not be fetched"),
    );
    assert!(image.is_none());
}
