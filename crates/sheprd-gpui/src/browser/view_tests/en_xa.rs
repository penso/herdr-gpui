//! The `en-XA` leak test (`crate::i18n::leak_tests`) for browser tabs: a
//! blank tab with its toolbar and placeholder, the notes panel with a note,
//! and a VS Code tab whose server does not answer.
use super::*;
use crate::{
    browser::{TabId, annotate::Anchor},
    code_server::Server,
    controls::Command,
    i18n::leak_tests::{assert_translated, draw, uncut, under_en_xa},
};

fn refuses(_: &WebUrl) -> crate::Result<Server> {
    Err(crate::code_server::Error::TokenRefused.into())
}

#[test]
fn browser_tabs_show_only_messages_under_en_xa() {
    under_en_xa("browser en-XA leak", |cx, recorder| {
        let (view, cx) = window(cx);
        draw(cx);
        recorder.take();

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.command(Command::NewBrowserTab, window, cx)
            })
        });
        draw(cx);
        assert_translated("blank browser tab", &uncut(recorder.take()), &[]);

        let id = TabId::test(0);
        cx.update(|window, cx| view.update(cx, |view, cx| view.toggle_annotating(id, window, cx)));
        draw(cx);
        assert_translated("empty notes panel", &uncut(recorder.take()), &[]);

        let comment = "Make it blue".to_owned();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.begin_note(id, Anchor::Page, None, window, cx);
                let input = view.browser.annotations.input(id, cx);
                input.update(cx, |input, cx| input.set_text_selected(&comment, cx));
            })
        });
        draw(cx);
        assert_translated(
            "notes composer",
            &uncut(recorder.take()),
            std::slice::from_ref(&comment),
        );
        cx.update(|window, cx| view.update(cx, |view, cx| view.add_note(id, window, cx)));
        draw(cx);
        assert_translated(
            "notes list",
            &uncut(recorder.take()),
            std::slice::from_ref(&comment),
        );

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.config.code.url =
                    Some(WebUrl::try_from("http://127.0.0.1:8000/?tkn=x").unwrap());
                view.browser.code_server.probe = refuses;
                view.command(Command::OpenCode, window, cx);
            })
        });
        cx.run_until_parked();
        draw(cx);
        assert!(cx.debug_bounds("code-unreachable").is_some());
        // The tab is named by the product.
        let name = Editor::VsCode.name().to_owned();
        assert_translated(
            "VS Code tab without its server",
            &uncut(recorder.take()),
            std::slice::from_ref(&name),
        );
    });
}
