//! What an organization's card says, which organizations the page lists,
//! and where a new token is made.
use super::*;
use crate::menu::azure::{merge_organizations, status_line, token_page};
use herdr_orchestrator::azure::{self, account::Account, token::Source};
use std::sync::Arc;

fn found(source: Source, account: Option<Result<Account, azure::Error>>) -> Lookup {
    Lookup::Found {
        source,
        account: account.map(|account| account.map_err(Arc::new)),
    }
}

fn account(projects: Option<usize>) -> Account {
    Account {
        name: "Dev Example".into(),
        email: Some("dev@example.test".into()),
        projects: projects.map(|count| (0..count).map(|index| format!("p{index}")).collect()),
    }
}

#[test]
fn a_card_says_where_its_token_comes_from_and_what_it_reads() {
    let line = |lookup| plain(status_line(&status(lookup)));
    assert_eq!(line(Lookup::Unknown), "Looking for a token\u{2026}");
    assert_eq!(line(Lookup::None), "No token. Paste one below.");
    assert_eq!(
        line(found(Source::Saved, Some(Ok(account(Some(3)))))),
        "Signed in, saved here; 3 projects."
    );
    assert_eq!(
        line(found(Source::Cli, Some(Ok(account(Some(1)))))),
        "Signed in, from the Azure CLI (az); 1 project."
    );
    assert_eq!(
        line(found(Source::Saved, Some(Ok(account(None))))),
        "Signed in, saved here; the token does not list projects."
    );
    assert_eq!(
        line(found(Source::Saved, None)),
        "Checking the token saved here\u{2026}"
    );
    assert_eq!(
        line(found(
            Source::Saved,
            Some(Err(azure::Error::Authentication))
        )),
        "Azure DevOps refused the token saved here. Paste a new one."
    );
    // A pasted token cannot replace the shell's, which is tried first.
    assert!(
        line(found(
            Source::Environment,
            Some(Err(azure::Error::EnvironmentRefused))
        ))
        .contains("Change or unset it")
    );
}

#[test]
fn the_page_lists_each_organization_once_by_its_name() {
    assert_eq!(
        merge_organizations(
            ["Contoso".to_owned(), "fabrikam".to_owned()],
            ["contoso".to_owned(), "two words".to_owned()],
        ),
        ["contoso", "fabrikam"]
    );
}

#[test]
fn a_new_token_is_made_on_the_organizations_own_settings_page() {
    assert_eq!(
        token_page("https://dev.azure.com/Contoso/").as_deref(),
        Some("https://dev.azure.com/contoso/_usersSettings/tokens")
    );
    assert_eq!(token_page("two words"), None);
}
