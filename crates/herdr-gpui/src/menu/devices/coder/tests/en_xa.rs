//! The `en-XA` leak test (`crate::i18n::leak_tests`) for Add Device's
//! Coder form: it draws only translated messages, except for the account's
//! templates, workspaces, presets, and the deployment.
use super::*;
use crate::i18n::leak_tests::{assert_translated, draw, under_en_xa};
use gpui::{px, size};

#[test]
fn the_coder_form_shows_only_messages_under_en_xa() {
    under_en_xa("Coder en-XA leak", |cx, recorder| {
        if crate::cloud::unavailable().is_some() {
            return;
        }
        let (view, cx) = cx.add_window_view(fixture_window);
        cx.simulate_resize(size(px(1000.), px(900.)));
        draw(cx);
        recorder.take();
        view.update_in(cx, |view, window, cx| view.open_coder_fixture(window, cx));
        draw(cx);
        let mut data: Vec<String> = view.read_with(cx, |view, cx| {
            let wizard = view.menu.coder.as_ref().unwrap();
            let account = wizard.account.as_ref().unwrap();
            let mut data = vec![
                format!("{} · {}", account.user.username, wizard.settings.base),
                wizard.name.read(cx).text().to_owned(),
            ];
            for template in &account.templates {
                // The fixture's long name is cut short, measured a character
                // at a time.
                let label = template.label();
                data.extend(label.chars().map(String::from));
                data.extend(
                    label
                        .char_indices()
                        .map(|(at, _)| format!("{}…", &label[..at])),
                );
                data.push(label.to_owned());
                data.push(template.organization_name.clone());
            }
            for workspace in &account.workspaces {
                data.push(workspace.name.clone());
                data.push(workspace.template_name.clone());
            }
            data.extend(
                wizard
                    .presets
                    .iter()
                    .flatten()
                    .map(|preset| preset.name.clone()),
            );
            data
        });
        // The rail's sources, and the setup script's summary.
        data.push("WSL".into());
        data.extend(
            crate::cloud::CloudProvider::ALL
                .iter()
                .map(|&provider| crate::cloud::name(provider).to_owned()),
        );
        assert_translated("Add Device > Coder", &recorder.take(), &data);
    });
}
