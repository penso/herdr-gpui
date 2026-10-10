#![allow(clippy::unwrap_used)]

use super::{HerdrWindow, annotate};
use crate::browser::{Store, view::scope};
use gpui::{Image, ImageFormat, TestAppContext};
use std::sync::Arc;

#[gpui::test]
fn pending_send_covers_screenshot_saving_and_failure(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    let tab = cx.update(|_, cx| {
        let tab_scope = scope(&view.read(cx).endpoints[0]);
        let id = Store::update(cx, |store| {
            store.open(tab_scope, "w0", None, Some("w0:p1".into()))
        })
        .unwrap();
        let tab = cx.global::<Store>().get(id).cloned().unwrap();
        view.update(cx, |view: &mut HerdrWindow, _| {
            let mut note = annotate::Note::new(annotate::Anchor::Page, "Send once.").unwrap();
            note.image = Some(Arc::new(Image::from_bytes(ImageFormat::Png, vec![1, 2, 3])));
            view.tab_notes(tab.id).notes.push(note);
        });
        tab
    });
    // The second send happens before the asynchronous UI callback can run.
    // Both successful saving and failure must release the guard via feedback.
    for fail in [false, true] {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.send_notes_with(
                    &tab,
                    move |images| {
                        assert_eq!(images.len(), 1);
                        if fail {
                            Err(crate::Error::MissingStateRoot)
                        } else {
                            Ok(vec![None])
                        }
                    },
                    cx,
                );
                view.send_notes_with(&tab, |_| panic!("duplicate screenshot save"), cx);
            });
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            let target = crate::browser::FeedbackKey {
                scope: scope(&view.read(cx).endpoints[0]),
                pane_id: "w0:p1".into(),
            };
            let text = if cfg!(unix) {
                cx.default_global::<crate::browser::Feedback>()
                    .take(&target)
            } else {
                cx.read_from_clipboard().and_then(|item| item.text())
            }
            .unwrap();
            assert_eq!(text.matches("Note: Send once.").count(), 1);
        });
    }
}
