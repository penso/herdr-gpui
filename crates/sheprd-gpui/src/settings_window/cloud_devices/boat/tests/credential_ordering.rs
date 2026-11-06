use super::*;

#[gpui::test]
fn pending_credential_save_survives_refresh_and_blocks_replacement(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |view, cx| {
        // Inject persistence work so the test never touches a personal keyring
        // or configuration. The save stays pending until the executor runs.
        view.save_with(
            || Ok(()),
            || {
                let config = crate::config::Config::default();
                let theme = config.theme(false)?;
                Ok(crate::settings_window::persistence::Loaded {
                    config,
                    theme,
                    shared: None,
                    error: None,
                })
            },
            false,
            cx,
        );
        let card = view.boat_card.as_ref().unwrap();
        card.key.update(cx, |input, cx| {
            input.set_text_selected("boat_replacement_fixture", cx)
        });
        view.boat_config_changed(cx);
        view.forget_boat_key(cx);
        view.save_boat(cx);
        assert!(view.busy());
        assert!(view.save_completion.is_some());
        assert_eq!(
            view.boat_card.as_ref().unwrap().key.read(cx).text(),
            "boat_replacement_fixture"
        );
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(!view.busy());
        assert!(view.save_completion.is_none());
        assert!(view.error.is_none());
    });
}
