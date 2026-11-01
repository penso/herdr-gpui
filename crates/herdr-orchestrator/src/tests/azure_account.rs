//! Checking a pasted Azure DevOps token before it is saved: whom the
//! organization takes it for and which projects it shows, against a local
//! server standing in for dev.azure.com.
use super::gitlab_merge_requests::serve;
use crate::azure::{self, account, token::Token};
use secrecy::SecretString;

fn pat() -> Token {
    Token::Personal(SecretString::from("pat-example"))
}

fn connection() -> String {
    serde_json::json!({
        "authenticatedUser": {
            "id": "00000000-0000-0000-0000-000000000001",
            "providerDisplayName": "Dev Example",
            "properties": {"Account": {"$type": "System.String", "$value": "dev@example.test"}},
        },
        "instanceId": "00000000-0000-0000-0000-000000000002",
    })
    .to_string()
}

#[test]
fn a_token_is_checked_with_the_organizations_connection_data_and_projects() {
    let projects = serde_json::json!({
        "count": 2,
        "value": [{"id": "p2", "name": "web"}, {"id": "p1", "name": "Fabrikam Fiber"}],
    })
    .to_string();
    let (address, server) = serve(vec![(200, connection()), (200, projects)]);
    let found = account::check_at(&format!("http://{address}"), "contoso", &pat()).unwrap();
    assert_eq!(found.name, "Dev Example");
    assert_eq!(found.email.as_deref(), Some("dev@example.test"));
    assert_eq!(
        found.projects.as_deref(),
        Some(&["Fabrikam Fiber".to_owned(), "web".to_owned()][..])
    );
    let requests = server.join().unwrap();
    assert!(
        requests[0].starts_with("GET /contoso/_apis/connectionData "),
        "{requests:?}"
    );
    assert!(
        requests[1].starts_with("GET /contoso/_apis/projects?%24top=500&api-version=7.1 "),
        "{requests:?}"
    );
}

#[test]
fn a_token_that_may_not_list_projects_still_signs_in() {
    let (address, server) = serve(vec![(200, connection()), (401, String::new())]);
    let found = account::check_at(&format!("http://{address}"), "contoso", &pat()).unwrap();
    assert_eq!(found.name, "Dev Example");
    assert_eq!(found.projects, None);
    server.join().unwrap();
}

#[test]
fn a_refused_token_or_unknown_organization_is_not_saved() {
    for (status, expected) in [
        (401, "authentication"),
        (203, "authentication"),
        (404, "organization"),
    ] {
        let (address, server) = serve(vec![(status, "{}".to_owned())]);
        let error = account::check_at(&format!("http://{address}"), "contoso", &pat()).unwrap_err();
        match expected {
            "authentication" => assert!(matches!(error, azure::Error::Authentication), "{error}"),
            _ => assert!(matches!(error, azure::Error::NoOrganization(_)), "{error}"),
        }
        server.join().unwrap();
    }
    // An anonymous answer names no account.
    let (address, server) = serve(vec![(200, "{}".to_owned())]);
    let error = account::check_at(&format!("http://{address}"), "contoso", &pat()).unwrap_err();
    assert!(matches!(error, azure::Error::Authentication));
    server.join().unwrap();
}
