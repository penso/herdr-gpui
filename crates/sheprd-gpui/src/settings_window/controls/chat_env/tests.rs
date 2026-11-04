#![allow(clippy::unwrap_used)]
use crate::settings_window::tests::{skill_fixture, skill_load};
use core::prelude::v1::test;
use gpui::TestAppContext;
use std::collections::BTreeMap;

fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect()
}

#[gpui::test]
fn the_field_shows_the_saved_environment_and_refuses_bad_pairs(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    view.update(cx, |view, cx| {
        view.config.chat.env = env(&[("GH_HOST", "mycompany.ghe.com")]);
        view.open_chat_env_settings(cx);
        let input = view.chat_env.input.clone().unwrap();
        assert_eq!(input.read(cx).text(), "GH_HOST=mycompany.ghe.com");
        input.update(cx, |input, cx| {
            input.set_text_selected("HERDR_ACP=/tmp/x", cx)
        });
        view.apply_chat_env(cx);
        assert!(view.chat_env.invalid, "the app's own variable is refused");
        assert!(view.chat_env.saving.is_none(), "nothing is saved");
        // A reload does not replace the rejected draft.
        view.sync_chat_env_field(cx);
        assert_eq!(input.read(cx).text(), "HERDR_ACP=/tmp/x");
    });
}

#[gpui::test]
fn a_saved_environment_becomes_the_baseline(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    view.update(cx, |view, cx| {
        view.open_chat_env_settings(cx);
        let submitted = "GH_HOST=mycompany.ghe.com";
        view.chat_env
            .input
            .as_ref()
            .unwrap()
            .update(cx, |input, cx| input.set_text_selected(submitted, cx));
        view.chat_env.saving = Some(submitted.into());
        view.save_with(
            || Ok(()),
            || {
                let mut loaded = skill_load()?;
                loaded.config.chat.env = env(&[("GH_HOST", "mycompany.ghe.com")]);
                Ok(loaded)
            },
            false,
            cx,
        );
    });
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        assert_eq!(view.chat_env.shown, "GH_HOST=mycompany.ghe.com");
        let mut loaded = skill_load().unwrap();
        loaded.config.chat.env = env(&[("GH_HOST", "other.ghe.com")]);
        view.apply_loaded(Ok(loaded), cx);
        assert_eq!(
            view.chat_env.input.as_ref().unwrap().read(cx).text(),
            "GH_HOST=other.ghe.com"
        );
    });
}

#[gpui::test]
fn the_card_shows_only_while_chats_are_on(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    view.update(cx, |view, cx| {
        view.config.features.acp_chats = false;
        assert!(view.render_chat_env_controls(cx).is_none());
        view.config.features.acp_chats = true;
        assert!(view.render_chat_env_controls(cx).is_some());
    });
}
