#![allow(clippy::unwrap_used)]
use super::*;
use crate::settings_window::tests::{skill_fixture, skill_load};
use core::prelude::v1::test;
use gpui::TestAppContext;

#[gpui::test]
fn px0_address_edits_survive_reload_and_invalid_input_is_not_saved(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    view.update(cx, |view, cx| {
        view.open_px0_settings(cx);
        let input = view.px0.input.clone().unwrap();
        input.update(cx, |input, cx| {
            input.set_text_selected("http://example.com/", cx)
        });
        view.apply_px0_address(cx);
        assert!(view.px0.invalid);
        assert!(view.config.px0.url.is_none());
        view.sync_px0_field(cx);
        assert_eq!(input.read(cx).text(), "http://example.com/");
        view.px0.invalid = false;
        input.update(cx, |input, cx| input.set_text_selected("", cx));
        view.config.px0.url = Some(WebUrl::try_from("http://localhost:7777/").unwrap());
        view.sync_px0_field(cx);
        assert_eq!(input.read(cx).text(), "http://localhost:7777/");
    });
}

#[gpui::test]
fn successful_apply_advances_the_baseline_and_later_reload_updates_it(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    view.update(cx, |view, cx| {
        view.open_px0_settings(cx);
        let submitted = " localhost:7777 ";
        view.px0
            .input
            .as_ref()
            .unwrap()
            .update(cx, |input, cx| input.set_text_selected(submitted, cx));
        view.px0.saving = Some(submitted.into());
        view.save_with(
            || Ok(()),
            || {
                let mut loaded = skill_load()?;
                loaded.config.px0.url = Some(WebUrl::try_from("http://localhost:7777/")?);
                Ok(loaded)
            },
            false,
            cx,
        );
    });
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        assert_eq!(view.px0.shown, "http://localhost:7777/");
        assert_eq!(
            view.px0.input.as_ref().unwrap().read(cx).text(),
            view.px0.shown
        );
        let mut loaded = skill_load().unwrap();
        loaded.config.px0.url = Some(WebUrl::try_from("http://localhost:8888/").unwrap());
        view.apply_loaded(Ok(loaded), cx);
        assert_eq!(
            view.px0.input.as_ref().unwrap().read(cx).text(),
            "http://localhost:8888/"
        );
    });
}

#[gpui::test]
fn failed_saves_and_newer_edits_stay_drafts(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    for saved in [false, true] {
        view.update(cx, |view, cx| {
            view.open_px0_settings(cx);
            view.px0.input.as_ref().unwrap().update(cx, |input, cx| {
                input.set_text_selected("localhost:7777", cx)
            });
            view.px0.saving = Some("localhost:7777".into());
            view.save_with(
                move || {
                    if saved {
                        Ok(())
                    } else {
                        Err(crate::px0::Error::Timeout.into())
                    }
                },
                || {
                    let mut loaded = skill_load()?;
                    loaded.config.px0.url = Some(WebUrl::try_from("http://localhost:7777/")?);
                    Ok(loaded)
                },
                false,
                cx,
            );
            if saved {
                view.px0.input.as_ref().unwrap().update(cx, |input, cx| {
                    input.set_text_selected("localhost:9999", cx)
                });
            }
        });
        cx.run_until_parked();
        view.update(cx, |view, cx| {
            let draft = if saved {
                "localhost:9999"
            } else {
                "localhost:7777"
            };
            assert_eq!(view.px0.input.as_ref().unwrap().read(cx).text(), draft);
            assert_ne!(view.px0.shown, draft);
            assert!(view.px0.saving.is_none());
        });
    }
}
