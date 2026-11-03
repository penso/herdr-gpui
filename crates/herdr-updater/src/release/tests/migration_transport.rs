use super::*;

fn response(status: u16, location: Option<&str>) -> ureq::http::Response<()> {
    let mut response = ureq::http::Response::builder().status(status);
    if let Some(location) = location {
        response = response.header("Location", location);
    }
    match response.body(()) {
        Ok(response) => response,
        Err(error) => panic!("invalid HTTP fixture: {error}"),
    }
}

#[test]
fn redirects_preserve_exact_resource_and_accept_header_for_both_repositories() -> anyhow::Result<()>
{
    for old in [
        LATEST_URL.to_owned(),
        RELEASES_URL.into(),
        HISTORY_URL.into(),
        release_url("20260920.2", "update-manifest.sig"),
        release_url(
            "20260920.2",
            "Sheprd-20260920.2-x86_64-pc-windows-msvc-setup.exe",
        ),
    ] {
        let new = old.replace("penso/herdr-gpui", "sheprd-labs/sheprd");
        for (initial, next) in [(&old, &new), (&new, &old)] {
            let mut calls = Vec::new();
            request_with(initial, &AtomicBool::new(false), |url, accept| {
                calls.push(url.to_owned());
                assert_eq!(
                    accept,
                    if old.contains("api.github.com") {
                        "application/vnd.github+json"
                    } else {
                        "application/octet-stream"
                    }
                );
                Ok(if calls.len() == 1 {
                    response(301, Some(next))
                } else {
                    response(200, None)
                })
            })?;
            assert_eq!(calls, [initial.to_owned(), next.to_owned()]);
        }
    }
    Ok(())
}

#[test]
fn numeric_api_redirect_is_readdressed_without_trusting_an_arbitrary_repository_id()
-> anyhow::Result<()> {
    for suffix in ["/latest", "?per_page=10", "?per_page=20"] {
        let original = format!("https://api.github.com/repos/penso/herdr-gpui/releases{suffix}");
        let mut calls = Vec::new();
        request_with(&original, &AtomicBool::new(false), |url, _| {
            calls.push(url.to_owned());
            Ok(if calls.len() == 1 {
                response(
                    301,
                    Some(&format!(
                        "https://api.github.com/repositories/123456/releases{suffix}"
                    )),
                )
            } else {
                response(200, None)
            })
        })?;
        assert_eq!(
            calls,
            [
                original,
                format!("https://api.github.com/repos/sheprd-labs/sheprd/releases{suffix}")
            ]
        );
    }
    Ok(())
}

#[test]
fn asset_redirects_can_reach_cdn_but_cannot_change_repository_tag_or_filename() -> anyhow::Result<()>
{
    let original = release_url("20260920.2", "update-manifest.json");
    let cdn = "https://release-assets.githubusercontent.com/github-production-release-asset/test?signature=x";
    let mut calls = 0;
    request_with(&original, &AtomicBool::new(false), |_, _| {
        calls += 1;
        Ok(if calls == 1 {
            response(302, Some(cdn))
        } else {
            response(200, None)
        })
    })?;
    for bad in [
        original.replace("penso/herdr-gpui", "evil/herdr-gpui"),
        original.replace("penso/herdr-gpui", "sheprd-labs/sheprd-evil"),
        original.replace("v20260920.2", "v20260920.3"),
        original.replace("update-manifest.json", "other.json"),
        original.replace("https://", "http://"),
        original.replace("github.com", "github.com:443"),
        original.replace("github.com", "user@github.com"),
        original.replace("github.com", "github.com.evil.test"),
        format!("{original}?extra=1"),
        format!("{cdn}#fragment"),
        "/relative".into(),
        LATEST_URL.into(),
    ] {
        let mut calls = 0;
        let result = request_with(&original, &AtomicBool::new(false), |_, _| {
            calls += 1;
            Ok(response(302, Some(&bad)))
        });
        assert!(matches!(result, Err(Error::UntrustedUrl)), "{bad}");
        assert_eq!(calls, 1, "must reject before fetching");
    }
    assert!(matches!(
        request_with(LATEST_URL, &AtomicBool::new(false), |_, _| Ok(response(
            302,
            Some(cdn)
        ))),
        Err(Error::UntrustedUrl)
    ));
    Ok(())
}

#[test]
fn redirect_budget_cancellation_and_http_errors_are_not_hidden() -> anyhow::Result<()> {
    let cancel = AtomicBool::new(false);
    let mut calls = 0;
    assert!(matches!(
        request_with(LATEST_URL, &cancel, |_, _| {
            calls += 1;
            Ok(response(301, Some(LATEST_URL)))
        }),
        Err(Error::RedirectLimit)
    ));
    assert_eq!(calls, 6);
    assert!(matches!(
        request_with(LATEST_URL, &cancel, |_, _| Ok(response(302, None))),
        Err(Error::InvalidRedirect)
    ));
    cancel.store(true, Ordering::Relaxed);
    assert!(matches!(
        request_with(
            LATEST_URL,
            &cancel,
            |_, _| -> Result<ureq::http::Response<()>> { panic!("cancelled") }
        ),
        Err(Error::Cancelled)
    ));
    for status in [401, 403, 429, 500] {
        let mut calls = 0;
        assert!(matches!(repository::fetch(LATEST_URL, |url| {
            calls += 1;
            request_with(url, &AtomicBool::new(false), |_, _| Ok(response(status, None)))
        }), Err(Error::HttpStatus(actual)) if actual == status));
        assert_eq!(calls, 1);
    }
    for error in [
        Error::Cancelled,
        Error::ResponseLimit,
        Error::UntrustedUrl,
        Error::RateLimited { reset: None },
    ] {
        let mut error = Some(error);
        assert!(
            repository::fetch::<()>(LATEST_URL, |_| {
                let Some(error) = error.take() else {
                    panic!("must not retry")
                };
                Err(error)
            })
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn only_404_uses_the_successor_once_for_every_resource() -> anyhow::Result<()> {
    for old in [
        LATEST_URL.to_owned(),
        RELEASES_URL.into(),
        HISTORY_URL.into(),
        release_url("20260920.2", "host-manifest.json"),
        release_url("20260920.2", "host-manifest.sig"),
        release_url("20260920.2", "sheprd-20260920.2-macos-universal.app.tar.gz"),
    ] {
        let new = old.replace("penso/herdr-gpui", "sheprd-labs/sheprd");
        let mut calls = Vec::new();
        let bytes = repository::fetch(&old, |url| {
            calls.push(url.to_owned());
            if url == old {
                Err(Error::HttpStatus(404))
            } else {
                Ok(b"payload")
            }
        })?;
        assert_eq!(bytes, b"payload");
        assert_eq!(calls, [old.clone(), new.clone()]);
        let mut calls = 0;
        repository::fetch(&old, |_| {
            calls += 1;
            Ok(())
        })?;
        assert_eq!(
            calls, 1,
            "working legacy feed must not depend on an unborn repo"
        );
        let mut calls = 0;
        assert!(matches!(
            repository::fetch::<()>(&new, |_| {
                calls += 1;
                Err(Error::HttpStatus(404))
            }),
            Err(Error::HttpStatus(404))
        ));
        assert_eq!(
            calls, 1,
            "no fallback from successor to potentially stale legacy feed"
        );
    }
    Ok(())
}
