use super::*;
use crate::sidebar::layout_tests::fixture_window;
use core::prelude::v1::test;
use herdr_chat_protocol::{Entry, EntryId};
use std::{cell::RefCell, rc::Rc};

#[gpui::test]
fn document_prompt_body_keeps_normal_text_below_its_speaker_label(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    let entry = Entry {
        id: EntryId(1),
        kind: EntryKind::User {
            text: "A normal prompt".into(),
            files: 0,
        },
    };
    let measured = Rc::new(RefCell::new(None));
    let body_style = measured.clone();
    let expected = view.read_with(cx, |view, _| view.chat_text_size());
    cx.draw(Point::default(), size(px(800.), px(600.)), |_, cx| {
        let content = div().child(
            canvas(
                |_, _, _| (),
                move |_, _, window, _| *body_style.borrow_mut() = Some(window.text_style()),
            )
            .size(px(1.)),
        );
        view.read_with(cx, |view, _| {
            Document.row(
                Row {
                    entry: &entry,
                    previous: None,
                    next: Some(Side::Agent),
                    agent: "Codex",
                    live: false,
                    content,
                },
                &view.chat_look(),
            )
        })
    });
    let style = measured.borrow();
    let Some(style) = style.as_ref() else {
        panic!("prompt body was not drawn")
    };
    assert_eq!(style.font_weight, FontWeight::NORMAL);
    assert_eq!(style.font_size, px(expected).into());
}
