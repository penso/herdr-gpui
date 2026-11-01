//! The page in a window: the header switches to it, a token is refused
//! before any request when the organization or token typed cannot be one,
//! and a finished save empties the token field only while it still holds
//! the saved token.
use super::*;
use secrecy::SecretString;

fn type_in(
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
    organization: &str,
    token: &str,
) {
    view.update(cx, |view, cx| {
        let azure = &view.menu.azure;
        let (Some(field), Some(secret)) = (azure.organization.clone(), azure.token.clone()) else {
            panic!("the page makes both fields");
        };
        field.update(cx, |input, cx| input.set_text_selected(organization, cx));
        secret.update(cx, |input, cx| input.set_text_selected(token, cx));
    });
}

fn message_and_token(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> (String, String) {
    view.read_with(cx, |view, cx| {
        (
            plain(view.menu.azure.message.clone().unwrap_or_default()),
            view.menu
                .azure
                .token
                .as_ref()
                .map(|input| input.read(cx).text().to_owned())
                .unwrap_or_default(),
        )
    })
}

#[gpui::test]
fn the_account_header_opens_the_azure_devops_page(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.simulate_resize(size(px(1200.), px(900.)));
    view.update_in(cx, |view, window, cx| {
        assert!(view.open_menu(window, cx));
        view.open_gitlab_page(window, cx);
    });
    draw(cx);
    click(cx, "accounts-tab-azure");
    assert!(view.read_with(cx, |view, _| view.menu.page) == Some(Page::AzureDevOps));
    assert!(cx.debug_bounds("azure-organization-input").is_some());
    assert!(cx.debug_bounds("azure-token-input").is_some());
    click(cx, "accounts-tab-gitlab");
    assert!(view.read_with(cx, |view, _| view.menu.page) == Some(Page::GitLab));
}

#[gpui::test]
fn a_token_is_refused_before_any_request_when_it_cannot_be_one(cx: &mut TestAppContext) {
    let (view, cx) = page(cx);
    for (organization, token, said) in [
        ("", "pat", "Type the organization's name first."),
        (
            "two words",
            "pat",
            "\"two words\" is not an Azure DevOps organization name",
        ),
        ("contoso", "", "Paste a token first."),
        // A headless window keeps no saved credential.
        (
            "contoso",
            "pat",
            "This window does not keep Azure DevOps tokens.",
        ),
    ] {
        type_in(&view, cx, organization, token);
        click(cx, "azure-save");
        let (message, _) = message_and_token(&view, cx);
        assert_eq!(message, said, "{organization:?} {token:?}");
        assert!(!view.read_with(cx, |view, _| view.menu.azure.working));
    }
}

#[gpui::test]
fn a_finished_save_keeps_a_token_typed_after_it(cx: &mut TestAppContext) {
    let (view, cx) = page(cx);
    for (typed, cleared) in [("pat-next", false), ("pat-saved", true)] {
        type_in(&view, cx, "contoso", typed);
        view.update(cx, |view, cx| {
            let saved = Some(SecretString::from("pat-saved"));
            view.run_azure(cx, saved, || Ok("Saved.".to_owned()));
        });
        draw(cx);
        let (message, token) = message_and_token(&view, cx);
        assert_eq!(message, "Saved.");
        assert_eq!(token.is_empty(), cleared, "{typed}");
    }
    // A failed check keeps the token, which Azure DevOps shows only once.
    type_in(&view, cx, "contoso", "pat-refused");
    view.update(cx, |view, cx| {
        let saved = Some(SecretString::from("pat-refused"));
        view.run_azure(cx, saved, || {
            Err(herdr_orchestrator::azure::Error::Authentication)
        });
    });
    draw(cx);
    let (message, token) = message_and_token(&view, cx);
    assert!(
        message.starts_with("Azure DevOps rejected the token"),
        "{message}"
    );
    assert_eq!(token, "pat-refused");
}

#[gpui::test]
fn a_saved_organization_offers_forget_and_shows_its_account(cx: &mut TestAppContext) {
    let (view, cx) = page(cx);
    view.update(cx, |view, _| {
        let kept = ["contoso".to_owned()].into_iter().collect();
        view.show_azure_organizations(vec!["contoso".into(), "fabrikam".into()], &kept, || {
            Lookup::None
        });
    });
    draw(cx);
    assert!(cx.debug_bounds("azure-organization-0").is_some());
    assert!(cx.debug_bounds("azure-forget-0").is_some());
    // Nothing is saved for the second, so there is nothing to forget.
    assert!(cx.debug_bounds("azure-organization-1").is_some());
    assert!(cx.debug_bounds("azure-forget-1").is_none());
    // Forget in a headless window says it keeps no tokens.
    click(cx, "azure-forget-0");
    let message = view.read_with(cx, |view, _| view.menu.azure.message.clone());
    assert_eq!(
        message.map(plain).as_deref(),
        Some("This window does not keep Azure DevOps tokens.")
    );
}
