use super::*;

fn zoom_of(id: crate::browser::TabId, cx: &App) -> u16 {
    cx.global::<Store>().get(id).unwrap().zoom.percent()
}

#[gpui::test]
fn a_page_tab_zooms_and_keeps_its_zoom(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let scope = Scope::local(Path::new("/tmp/herdr.sock"));
        let page = Store::update(cx, |store| {
            let url = WebUrl::try_from("https://a.test/").unwrap();
            store.open(scope.clone(), "w_1", Some(Location::Web { url }), None)
        })
        .unwrap();
        let blank = Store::update(cx, |store| store.open(scope, "w_1", None, None)).unwrap();
        let ask = |caller: &Caller, id: u64, change, cx: &mut App| zoom(caller, id, change, cx);
        let anywhere = Caller::default();
        let zoomed = ask(&anywhere, page.into(), ZoomChange::In, cx);
        if !crate::browser::EMBEDDED {
            assert!(matches!(
                zoomed,
                Response::Error {
                    code: ErrorCode::Unsupported,
                    ..
                }
            ));
            assert_eq!(zoom_of(page, cx), 100);
            return;
        }
        let id = u64::from(page);
        assert_eq!(
            zoomed,
            Response::Tab {
                reply: Reply::Zoom { id, percent: 110 }
            }
        );
        assert_eq!(zoom_of(page, cx), 110);
        ask(&anywhere, id, ZoomChange::Percent { percent: 175 }, cx);
        assert_eq!(zoom_of(page, cx), 175);
        ask(&anywhere, id, ZoomChange::Out, cx);
        assert_eq!(zoom_of(page, cx), 150);
        // A zoom out of range, a tab that is not a page, and a window that
        // does not show the tab change nothing.
        assert!(matches!(
            ask(&anywhere, id, ZoomChange::Percent { percent: 20 }, cx),
            Response::Error {
                code: ErrorCode::InvalidRequest,
                ..
            }
        ));
        assert!(matches!(
            ask(&anywhere, blank.into(), ZoomChange::In, cx),
            Response::Error {
                code: ErrorCode::InvalidRequest,
                ..
            }
        ));
        let elsewhere = Caller {
            window: Some(999_999),
            ..Caller::default()
        };
        assert!(matches!(
            ask(&elsewhere, id, ZoomChange::Reset, cx),
            Response::Error {
                code: ErrorCode::NotFound,
                ..
            }
        ));
        assert_eq!(zoom_of(page, cx), 150);
        assert!(matches!(
            ask(&anywhere, 99, ZoomChange::In, cx),
            Response::Error {
                code: ErrorCode::NotFound,
                ..
            }
        ));
        ask(&anywhere, id, ZoomChange::Reset, cx);
        assert_eq!(zoom_of(page, cx), 100);
    });
}
