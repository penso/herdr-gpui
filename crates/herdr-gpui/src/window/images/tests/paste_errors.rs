use super::*;
use crate::window::clipboard;

#[gpui::test]
fn clipboard_failures_report_a_discarded_paste(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    for error in [
        clipboard::Error::Process(io::ErrorKind::NotFound.into()),
        clipboard::Error::Timeout,
        clipboard::Error::Size {
            limit: 2 * 1024 * 1024,
        },
    ] {
        let error = Error::Clipboard(error);
        view.update(cx, |view, cx| {
            view.endpoints[0].toasts.entries.clear();
            view.report_preparation_error(PreparationKind::Input, &error, cx);
            assert_eq!(view.local_error, Some(format!("Paste not sent: {error}")));
            let (_, notice) = view.endpoints[0].toasts.entries.back().unwrap();
            assert_eq!(notice.title, "Paste discarded");
            assert!(notice.visible);
            assert_eq!(
                notice.body.as_deref(),
                Some(format!("{error} Nothing was uploaded or pasted.").as_str())
            );
        });
    }
}
