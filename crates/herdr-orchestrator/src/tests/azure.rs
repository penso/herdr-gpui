use super::*;
use crate::azure::{self, Call, Repository, token};
use crate::repo::parse_remote_url;
use secrecy::ExposeSecret;
use serde_json::{Value, json};
use std::ffi::OsString;

fn source() -> SourceKey {
    SourceKey {
        provider: Provider::Azure,
        host: "dev.azure.com".into(),
        repository: "contoso/Fabrikam Fiber/web".into(),
    }
}

#[test]
fn every_azure_devops_remote_form_keys_one_source() {
    for url in [
        "https://dev.azure.com/contoso/Fabrikam%20Fiber/_git/web",
        "https://someone@dev.azure.com/contoso/Fabrikam%20Fiber/_git/web",
        "https://dev.azure.com/contoso/Fabrikam%20Fiber/_git/web.git",
        "git@ssh.dev.azure.com:v3/contoso/Fabrikam%20Fiber/web",
        "ssh://git@ssh.dev.azure.com/v3/contoso/Fabrikam%20Fiber/web",
        "https://contoso.visualstudio.com/Fabrikam%20Fiber/_git/web",
        "https://contoso.visualstudio.com/DefaultCollection/Fabrikam%20Fiber/_git/web",
        "contoso@vs-ssh.visualstudio.com:v3/contoso/Fabrikam%20Fiber/web",
    ] {
        assert_eq!(parse_remote_url(url).unwrap(), source(), "{url}");
    }
}

#[test]
fn other_paths_on_azure_hosts_are_not_azure_sources() {
    // A project page, not a repository: falls back to the generic reading.
    let key = parse_remote_url("https://dev.azure.com/contoso/project/wiki").unwrap();
    assert_ne!(key.provider, Provider::Azure);
    assert!(azure::source("dev.azure.com", "contoso/project/_git/").is_none());
    assert!(azure::source("ssh.dev.azure.com", "v2/contoso/project/web").is_none());
    assert!(azure::source("gitlab.com", "group/project/_git/web").is_none());
}

#[test]
fn a_source_with_no_project_is_refused() {
    let key = SourceKey {
        repository: "contoso/web".into(),
        ..source()
    };
    assert!(matches!(
        Repository::of(&key),
        Err(azure::Error::Address(_))
    ));
}

#[test]
fn addresses_escape_each_segment_once() {
    let key = source();
    let repository = Repository::of(&key).unwrap();
    assert_eq!(
        repository.pull_requests_url(100).unwrap().as_str(),
        "https://dev.azure.com/contoso/Fabrikam%20Fiber/_apis/git/repositories/web/pullrequests\
         ?searchCriteria.status=active&%24top=100&%24skip=100&api-version=7.1"
    );
    assert_eq!(
        repository.web().unwrap(),
        "https://dev.azure.com/contoso/Fabrikam%20Fiber"
    );
    let batch = repository.work_items_url(&[7, 3]).unwrap();
    assert!(batch.as_str().contains("ids=7%2C3"), "{batch}");
    assert!(
        repository
            .query_url()
            .unwrap()
            .as_str()
            .ends_with("/_apis/wit/wiql?%24top=200&api-version=7.1")
    );
}

fn pull_request(id: u64, extra: Value) -> Value {
    let mut node = json!({
        "pullRequestId": id,
        "title": format!("Change {id}"),
        "description": "",
        "status": "active",
        "isDraft": false,
        "creationDate": "2026-01-02T03:04:05Z",
        "createdBy": { "displayName": "Dev One", "uniqueName": "dev@example.test" },
        "sourceRefName": "refs/heads/feature/x",
        "targetRefName": "refs/heads/main",
        "lastMergeSourceCommit": { "commitId": "aaa" },
        "lastMergeTargetCommit": { "commitId": "bbb" }
    });
    if let (Some(node), Some(extra)) = (node.as_object_mut(), extra.as_object()) {
        node.extend(extra.clone());
    }
    node
}

fn work_item(id: u64, state: &str) -> Value {
    json!({
        "id": id,
        "fields": {
            "System.Title": format!("Item {id}"),
            "System.State": state,
            "System.WorkItemType": "Bug",
            "System.CreatedDate": "2026-01-01T00:00:00Z",
            "System.ChangedDate": "2026-01-03T00:00:00Z",
            "System.CreatedBy": { "displayName": "Dev Two" },
            "System.Tags": "ui; urgent ;",
            "System.CommentCount": 4,
            "System.Parent": 1
        }
    })
}

/// What a fake fetch has been asked, one line per call.
type Asked = std::rc::Rc<std::cell::RefCell<Vec<String>>>;

/// Answers each call from `replies` in order, recording what was asked.
fn replies(answers: Vec<Value>) -> (impl FnMut(Call) -> Result<Vec<u8>>, Asked) {
    let asked = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let log = asked.clone();
    let mut answers = answers.into_iter();
    let fetch = move |call: Call| {
        let line = match call {
            Call::Get(url) => format!("GET {url}"),
            Call::Post(url, body) => format!("POST {url} {body}"),
        };
        log.borrow_mut().push(line);
        Ok(serde_json::to_vec(&answers.next().unwrap()).unwrap())
    };
    (fetch, asked)
}

#[test]
fn a_listing_reads_work_items_then_pull_requests() {
    let (mut fetch, asked) = replies(vec![
        json!({ "workItems": [{ "id": 9 }, { "id": 4 }] }),
        // The batch answers in id order; the query's order is kept.
        json!({ "value": [work_item(4, "Active"), work_item(9, "New")] }),
        json!({ "value": [
            pull_request(12, json!({})),
            pull_request(13, json!({ "isDraft": true, "description": "Body" })),
        ] }),
    ]);
    let listing = azure::list(&source(), None, &mut fetch).unwrap();
    assert!(listing.complete);
    let ids: Vec<_> = listing
        .items
        .iter()
        .map(|item| item.identifier.as_str())
        .collect();
    assert_eq!(ids, ["#9", "#4", "!12", "!13"]);

    let issue = &listing.items[1];
    assert_eq!(issue.key.native_id, "4");
    assert_eq!(issue.state, "open");
    assert_eq!(issue.labels, ["Active", "Bug", "ui", "urgent"]);
    assert_eq!(issue.author.as_deref(), Some("Dev Two"));
    assert_eq!(issue.parent_id.as_deref(), Some("1"));
    assert_eq!(
        issue.url.as_deref(),
        Some("https://dev.azure.com/contoso/Fabrikam%20Fiber/_workitems/edit/4")
    );
    assert_eq!(issue.activity.unwrap().comments, Some(4));
    assert!(issue.pull_request.is_none());

    let draft = &listing.items[3];
    assert_eq!(draft.key.native_id, "pr/13");
    assert_eq!(draft.state, "draft");
    assert_eq!(draft.description.as_deref(), Some("Body"));
    assert_eq!(
        draft.url.as_deref(),
        Some("https://dev.azure.com/contoso/Fabrikam%20Fiber/_git/web/pullrequest/13")
    );
    let pull_request = draft.pull_request.as_ref().unwrap();
    assert_eq!(
        (
            pull_request.base_ref.as_str(),
            pull_request.head_ref.as_str()
        ),
        ("main", "feature/x")
    );
    assert_eq!(
        (
            pull_request.base_sha.as_str(),
            pull_request.head_sha.as_str()
        ),
        ("bbb", "aaa")
    );
    assert_eq!(listing.items[2].description, None);

    let asked = asked.borrow();
    assert!(
        asked[0].starts_with("POST https://dev.azure.com/contoso/Fabrikam%20Fiber/_apis/wit/wiql")
    );
    assert!(asked[0].contains("@project"), "{}", asked[0]);
    assert!(asked[1].starts_with(
        "GET https://dev.azure.com/contoso/Fabrikam%20Fiber/_apis/wit/workitems?ids=9%2C4"
    ));
    assert!(asked[2].contains("%24skip=0"));
}

#[test]
fn a_project_with_no_open_work_items_asks_for_no_batch() {
    let (mut fetch, asked) = replies(vec![json!({ "workItems": [] }), json!({ "value": [] })]);
    let listing = azure::list(&source(), None, &mut fetch).unwrap();
    assert!(listing.complete);
    assert!(listing.items.is_empty());
    assert_eq!(asked.borrow().len(), 2);
}

#[test]
fn pull_requests_page_until_a_short_page() {
    let page = |from: u64, count: u64| json!({ "value": (from..from + count).map(|id| pull_request(id, json!({}))).collect::<Vec<_>>() });
    let (mut fetch, asked) = replies(vec![json!({ "workItems": [] }), page(1, 100), page(101, 7)]);
    let listing = azure::list(&source(), None, &mut fetch).unwrap();
    assert!(listing.complete);
    assert_eq!(listing.items.len(), 107);
    assert!(asked.borrow()[2].contains("%24skip=100"));
}

#[test]
fn a_full_work_item_query_marks_the_listing_capped() {
    let ids: Vec<_> = (1..=200).map(|id| json!({ "id": id })).collect();
    let (mut fetch, _) = replies(vec![
        json!({ "workItems": ids }),
        json!({ "value": [] }),
        json!({ "value": [] }),
    ]);
    let listing = azure::list(&source(), None, &mut fetch).unwrap();
    assert!(!listing.complete);
}

#[test]
fn refusals_name_their_cause() {
    assert!(matches!(
        azure::status_error(401, "r"),
        azure::Error::Authentication
    ));
    // A rejected personal access token gets a sign-in page with status 203.
    assert!(matches!(
        azure::status_error(203, "r"),
        azure::Error::Authentication
    ));
    assert!(matches!(
        azure::status_error(403, "r"),
        azure::Error::Forbidden { .. }
    ));
    assert!(matches!(
        azure::status_error(404, "r"),
        azure::Error::NotFound { .. }
    ));
    assert!(matches!(
        azure::status_error(429, "r"),
        azure::Error::RateLimit
    ));
    assert!(matches!(
        azure::status_error(500, "r"),
        azure::Error::Status(500)
    ));
    assert!(azure::status_error(401, "r").needs_user());
    assert!(!azure::status_error(500, "r").needs_user());
}

#[test]
fn no_token_asks_to_sign_in_without_a_request() {
    let mut cooldown = None;
    let error = match azure::sync(&source(), None, None, &|| false, &mut cooldown) {
        Err(error) => error,
        Ok(_) => panic!("a listing without a token"),
    };
    assert!(matches!(error, Error::Azure(azure::Error::SignIn { .. })));
    assert!(error.to_string().contains("az login"));
}

#[test]
fn the_environment_token_is_sent_as_basic_with_an_empty_user() {
    let variable = |name: &str| (name == token::PAT_VARIABLE).then(|| OsString::from(" secret \n"));
    let found = token::environment(variable).unwrap();
    let header = found.authorization().unwrap();
    assert!(header.is_sensitive());
    // base64(":secret")
    assert_eq!(header.to_str().unwrap(), "Basic OnNlY3JldA==");
    assert!(token::environment(|_| Some(OsString::from("two words"))).is_none());
    assert!(token::environment(|_| None).is_none());
}

#[test]
fn the_cli_token_is_one_line_sent_as_bearer() {
    let parsed = token::parse_cli(b"eyJ0eXAi.abc.def\n").unwrap();
    assert_eq!(parsed.expose_secret(), "eyJ0eXAi.abc.def");
    let header = token::Token::Entra(parsed).authorization().unwrap();
    assert_eq!(header.to_str().unwrap(), "Bearer eyJ0eXAi.abc.def");
    assert!(token::parse_cli(b"").is_none());
    assert!(token::parse_cli(b"one\ntwo\n").is_none());
}

#[test]
fn azure_keys_round_trip_and_stand_in_as_pull_requests() {
    let key = ItemKey {
        source: source(),
        native_id: "pr/12".into(),
    };
    let parsed: ItemKey = key.canonical().parse().unwrap();
    assert_eq!(parsed, key);
    assert!(key.canonical().starts_with("azure:dev.azure.com:"));
    assert_eq!(Item::stand_in(key).identifier, "!12");
}
