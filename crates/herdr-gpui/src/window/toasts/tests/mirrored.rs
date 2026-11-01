//! QA previews under `en-XB` keep the direction marks their messages carry,
//! which the cleanup of daemon text would strip.
use super::*;
use crate::i18n::fl;
use herdr_client::protocol::SemanticNotificationKind;
use herdr_i18n::Pseudo;

#[gpui::test]
fn previews_keep_their_messages_direction_marks(cx: &mut TestAppContext) {
    crate::i18n::show(&Pseudo::Bidi.locale());
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, cx| {
        view.show_toast_preview(SemanticNotificationKind::Finished, cx);
        let notice = &view.endpoints[0].toasts.entries.back().unwrap().1;
        let title = fl!("notifications-preview-finished");
        assert!(title.contains('\u{202E}'), "{title:?}");
        assert_eq!(notice.title, title);
        assert_eq!(
            notice.body.as_deref(),
            Some(fl!("notifications-preview-finished-body").as_str())
        );
    });
    crate::i18n::show(&herdr_i18n::Locale::FALLBACK);
}
