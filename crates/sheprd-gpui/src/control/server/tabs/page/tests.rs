#![allow(clippy::unwrap_used)]
use super::*;
use crate::browser::Scope;
use std::path::Path;

mod zoom;

#[test]
fn page_text_is_cut_until_its_json_fits() {
    assert_eq!(bounded_text("short", 100), "short");
    let quotes = "\"".repeat(1000);
    let cut = bounded_text(&quotes, 101);
    assert!(serde_json::to_string(&cut).unwrap().len() <= 101);
    assert!(!cut.is_empty());
    // Never inside a character.
    let wide = "\u{1f600}".repeat(1000);
    let cut = bounded_text(&wide, 1001);
    assert!(serde_json::to_string(&cut).unwrap().len() <= 1001);
    assert!(cut.chars().all(|c| c == '\u{1f600}'));
    assert_eq!(bounded_text("anything", 2), "");
}

#[gpui::test]
fn only_page_tabs_take_page_commands(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let scope = Scope::local(Path::new("/tmp/herdr.sock"));
        let devices = Store::update(cx, |store| {
            store.open(scope.clone(), "w_1", Some(Location::Devices), None)
        })
        .unwrap();
        let page = Store::update(cx, |store| {
            let url = WebUrl::try_from("https://a.test/").unwrap();
            store.open(scope, "w_1", Some(Location::Web { url }), None)
        })
        .unwrap();
        assert!(matches!(
            page_tab(devices.into(), cx),
            Err(Response::Error {
                code: ErrorCode::InvalidRequest,
                ..
            })
        ));
        assert!(matches!(
            page_tab(99, cx),
            Err(Response::Error {
                code: ErrorCode::NotFound,
                ..
            })
        ));
        assert!(page_tab(page.into(), cx).is_ok());
        let navigated = navigate(&Caller::default(), page.into(), "https://b.test/next", cx);
        if crate::browser::EMBEDDED {
            assert!(matches!(navigated, Response::Done { .. }));
            assert_eq!(
                cx.global::<Store>().get(page).unwrap().location,
                Some(Location::Web {
                    url: WebUrl::try_from("https://b.test/next").unwrap()
                })
            );
            assert!(matches!(
                navigate(&Caller::default(), page.into(), "file:///etc/passwd", cx),
                Response::Error {
                    code: ErrorCode::InvalidUrl,
                    ..
                }
            ));
            // A named window that does not show the tab changes nothing.
            let elsewhere = Caller {
                window: Some(999_999),
                ..Caller::default()
            };
            assert!(matches!(
                navigate(&elsewhere, page.into(), "https://b.test/other", cx),
                Response::Error {
                    code: ErrorCode::NotFound,
                    ..
                }
            ));
            assert!(matches!(
                navigate(&Caller::default(), devices.into(), "https://b.test/", cx),
                Response::Error {
                    code: ErrorCode::InvalidRequest,
                    ..
                }
            ));
        } else {
            assert!(matches!(
                navigated,
                Response::Error {
                    code: ErrorCode::Unsupported,
                    ..
                }
            ));
        }
    });
}
