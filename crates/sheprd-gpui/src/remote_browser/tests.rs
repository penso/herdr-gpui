#![allow(clippy::unwrap_used)]
use super::*;

/// The shim's line endings, on every platform it is built on.
mod line_endings;

/// The shim run with a fake `herdr`, as an agent on a host runs it.
#[cfg(unix)]
mod shim;

/// The scripts that put the shim and the skill on a host.
#[cfg(unix)]
mod install;

/// Windows acting on requests in their snapshots. Opening a tab without
/// switching to it makes no native page, but only builds that show pages
/// open a tab at all.
#[cfg(any(target_os = "macos", windows))]
mod window;

pub(super) fn tokens(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

fn one(pairs: &[(&str, &str)]) -> Option<Request> {
    let mut all = Request::all(&tokens(pairs));
    assert!(all.len() <= 1, "{all:?}");
    all.pop()
}

#[test]
fn a_request_is_read_from_its_tokens() {
    let request = one(&[
        ("ci", "green"),
        ("herdr-gpui-17-42", "open w1:p2"),
        ("herdr-gpui-17-42_0", "http://localhost:3000/a"),
        ("herdr-gpui-17-42_1", "?b=c"),
    ])
    .unwrap();
    assert_eq!(
        request,
        Request {
            id: "17-42".into(),
            pane: Some("w1:p2".into()),
            action: Action::Open {
                address: "http://localhost:3000/a?b=c".into(),
                focus: true,
            },
        }
    );
    let quiet = one(&[
        ("herdr-gpui-1", "open-background"),
        ("herdr-gpui-1_0", "example.com"),
    ])
    .unwrap();
    assert_eq!(
        quiet.action,
        Action::Open {
            address: "example.com".into(),
            focus: false
        }
    );
    assert_eq!(quiet.pane, None);
    let reload = one(&[("herdr-gpui-2", "reload p")]).unwrap();
    assert_eq!(reload.action, Action::Reload);
    assert_eq!(reload.pane.as_deref(), Some("p"));
}

#[test]
fn requests_made_together_are_all_read() {
    let all = Request::all(&tokens(&[
        ("herdr-gpui-5-1", "open w1:p1"),
        ("herdr-gpui-5-1_0", "a.test"),
        ("herdr-gpui-5-2", "open w1:p2"),
        ("herdr-gpui-5-2_0", "b.test"),
        ("herdr-gpui-5-3", "reload w1:p3"),
    ]));
    let found: Vec<_> = all
        .iter()
        .map(|request| (request.id.as_str(), request.pane.as_deref()))
        .collect();
    assert_eq!(
        found,
        [
            ("5-1", Some("w1:p1")),
            ("5-2", Some("w1:p2")),
            ("5-3", Some("w1:p3"))
        ]
    );
    assert!(matches!(&all[1].action, Action::Open { address, .. } if address == "b.test"));
}

#[test]
fn incomplete_or_unknown_requests_are_ignored() {
    let open = |extra: &[(&str, &str)]| {
        let mut pairs = vec![("herdr-gpui-1", "open")];
        pairs.extend_from_slice(extra);
        one(&pairs)
    };
    // No address, or one that skips its first part.
    assert_eq!(open(&[]), None);
    assert_eq!(open(&[("herdr-gpui-1_1", "x")]), None);
    // Another request's parts are not this one's.
    assert_eq!(open(&[("herdr-gpui-2_0", "a.test")]), None);
    // A gap ends the address at the last part before it.
    assert_eq!(
        open(&[("herdr-gpui-1_0", "a.test"), ("herdr-gpui-1_2", "/x")])
            .unwrap()
            .action,
        Action::Open {
            address: "a.test".into(),
            focus: true
        }
    );
    // Parts past the last number are never read.
    let parts: Vec<(String, String)> = (0..=URL_CHUNKS)
        .map(|index| (format!("herdr-gpui-1_{index}"), "a".to_owned()))
        .collect();
    let mut many = tokens(&[("herdr-gpui-1", "open")]);
    many.extend(parts);
    let all = Request::all(&many);
    let [
        Request {
            action: Action::Open { address, .. },
            ..
        },
    ] = all.as_slice()
    else {
        panic!("expected one open request");
    };
    assert_eq!(address.len(), URL_CHUNKS);

    for (key, value) in [
        ("herdr-gpui-1", "run"),
        ("herdr-gpui-1", ""),
        ("herdr-gpui-", "open"),
        ("herdr-gpui-a.b", "open"),
        ("herdr-gpui-123456789012345678x", "open"),
    ] {
        let part = format!("{key}_0");
        let pairs = [(key, value), (part.as_str(), "a.test")];
        assert_eq!(one(&pairs), None, "{key}={value}");
    }
    // A pane that is not a plain identifier is dropped, not trusted.
    let odd = open(&[("herdr-gpui-1_0", "a.test")]);
    assert_eq!(odd.unwrap().pane, None);
    let odd = one(&[("herdr-gpui-1", "open p 1"), ("herdr-gpui-1_0", "a.test")]).unwrap();
    assert_eq!(odd.pane, None);
}

#[test]
fn request_tokens_are_told_from_other_plugins() {
    assert!(is_request_token("herdr-gpui-url-3"));
    assert!(!is_request_token("ci"));
    assert!(!is_request_token("herdr-status"));
}

#[test]
fn each_request_is_acted_on_once_and_the_memory_is_bounded() {
    let key = |id: usize, window: Option<u64>| SeenKey {
        scope: Scope::endpoint("host"),
        workspace: "w1".into(),
        id: id.to_string(),
        window,
    };
    let mut seen = Seen::default();
    assert!(seen.first(key(0, None)));
    assert!(!seen.first(key(0, None)));
    // A reload is each window's own.
    assert!(seen.first(key(0, Some(1))));
    assert!(seen.first(key(0, Some(2))));
    assert!(!seen.first(key(0, Some(2))));
    for id in 1..=MAX_SEEN {
        assert!(seen.first(key(id, None)));
    }
    assert_eq!(seen.0.len(), MAX_SEEN);
    // The oldest were forgotten to make room.
    assert!(seen.first(key(0, None)));
}
