//! The `en-XA` leak test (`crate::i18n::leak_tests`) for the account menu's
//! Azure DevOps page: its cards and form draw only translated messages,
//! except for organizations, account names, and the fields' placeholders.
use super::*;
use crate::i18n::leak_tests::{assert_translated, under_en_xa};
use herdr_orchestrator::azure::{account::Account, token::Source};

/// `names` as a narrow row may show them: whole, cut short at any point
/// and ended with "…", and the single letters shaped to measure the cut.
fn cut(names: &[&str]) -> Vec<String> {
    names
        .iter()
        .flat_map(|name| {
            let cuts = name
                .char_indices()
                .skip(1)
                .map(|(at, _)| format!("{}\u{2026}", name[..at].trim_end()))
                .collect::<Vec<_>>();
            let letters = name.chars().map(String::from).collect::<Vec<_>>();
            std::iter::once((*name).to_owned())
                .chain(cuts)
                .chain(letters)
        })
        .collect()
}

#[test]
fn the_azure_devops_page_shows_only_messages_under_en_xa() {
    under_en_xa("Azure DevOps page en-XA leak", |cx, recorder| {
        let (view, cx) = page(cx);
        recorder.take();
        view.update_in(cx, |view, _, cx| {
            let account = Account {
                name: "Dev Example".into(),
                email: None,
                projects: Some(vec!["web".into()]),
            };
            let azure = &mut view.menu.azure;
            azure.organizations = vec![
                status(Lookup::Found {
                    source: Source::Saved,
                    account: Some(Ok(account)),
                }),
                OrganizationStatus {
                    organization: "fabrikam".into(),
                    saved: false,
                    lookup: Lookup::None,
                },
            ];
            cx.notify();
        });
        draw(cx);
        let data = cut(&[
            "GitHub",
            "GitLab",
            "Azure DevOps",
            "contoso",
            "fabrikam",
            "Dev Example",
            "\u{2022}\u{2022}\u{2022}\u{2022}",
        ]);
        assert_translated("Azure DevOps > organizations", &recorder.take(), &data);
    });
}
