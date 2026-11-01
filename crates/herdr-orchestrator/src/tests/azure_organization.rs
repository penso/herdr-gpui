//! An Azure DevOps organization read without a checkout: its catalog, the
//! reader's own pull requests, and the worker that shows them.

use super::azure::{pull_request, replies};
use super::*;
use crate::azure::{self, organization};
use crate::service::remote::{self, RemoteRequest, RemoteScope};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

fn repository(project: &str, name: &str) -> Value {
    json!({ "name": name, "project": { "name": project } })
}

fn mine_pull_request(id: u64, project: &str, name: &str, status: &str) -> Value {
    pull_request(
        id,
        json!({ "status": status, "repository": repository(project, name) }),
    )
}

fn mine_source() -> SourceKey {
    RemoteScope::Mine {
        organization: "contoso".into(),
    }
    .source()
}

fn reader() -> Value {
    json!({ "authenticatedUser": { "id": "00000000-0000-0000-0000-00000000000a", "providerDisplayName": "Dev One" } })
}

#[test]
fn organization_addresses_escape_each_segment() {
    assert_eq!(
        organization::repositories_url("contoso").unwrap().as_str(),
        "https://dev.azure.com/contoso/_apis/git/repositories?api-version=7.1"
    );
    assert_eq!(
        organization::mine_url("contoso", "abc", 200)
            .unwrap()
            .as_str(),
        "https://dev.azure.com/contoso/_apis/git/pullrequests?searchCriteria.creatorId=abc\
         &searchCriteria.status=all&%24top=100&%24skip=200&api-version=7.1"
    );
    for bad in ["", "contoso/Fabrikam"] {
        assert!(matches!(
            organization::url(bad, &[]),
            Err(azure::Error::Address(_))
        ));
    }
}

#[test]
fn a_catalog_groups_enabled_repositories_by_project_by_name() {
    let (mut fetch, asked) = replies(vec![json!({ "value": [
        repository("Fabrikam", "web"),
        repository("contoso-tools", "cli"),
        repository("Fabrikam", "api"),
        { "name": "old", "project": { "name": "Fabrikam" }, "isDisabled": true },
        { "name": "orphan" },
    ] })]);
    let catalog = organization::catalog("contoso", &mut fetch).unwrap();
    let projects: Vec<(&str, Vec<&str>)> = catalog
        .projects
        .iter()
        .map(|project| {
            (
                project.name.as_str(),
                project.repositories.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    assert_eq!(
        projects,
        [
            ("contoso-tools", vec!["cli"]),
            ("Fabrikam", vec!["api", "web"])
        ]
    );
    assert!(!catalog.capped);
    assert_eq!(asked.borrow().len(), 1);
}

#[test]
fn a_catalog_keeps_a_bounded_number_of_repositories() {
    let many: Vec<Value> = (0..=organization::MAX_REPOSITORIES)
        .map(|index| repository("Fabrikam", &format!("repo-{index:05}")))
        .collect();
    let (mut fetch, _) = replies(vec![json!({ "value": many })]);
    let catalog = organization::catalog("contoso", &mut fetch).unwrap();
    assert!(catalog.capped);
    assert_eq!(
        catalog.projects[0].repositories.len(),
        organization::MAX_REPOSITORIES
    );
}

#[test]
fn the_readers_pull_requests_are_keyed_by_their_own_repository() {
    let (mut fetch, asked) = replies(vec![
        reader(),
        json!({ "value": [
            mine_pull_request(7, "Fabrikam", "web", "active"),
            pull_request(8, json!({ "isDraft": true, "repository": repository("Fabrikam", "api") })),
            mine_pull_request(9, "Fabrikam", "web", "completed"),
            mine_pull_request(10, "Fabrikam", "api", "abandoned"),
            // No repository named: nothing to key it by.
            pull_request(11, json!({})),
        ] }),
    ]);
    let mine = organization::mine("contoso", &mut fetch).unwrap();
    assert!(mine.complete);
    assert_eq!(mine.viewer.as_deref(), Some("Dev One"));
    let listed: Vec<(String, &str, &str)> = mine
        .items
        .iter()
        .map(|item| {
            (
                item.key.canonical(),
                item.state.as_str(),
                item.url.as_deref().unwrap_or_default(),
            )
        })
        .collect();
    assert_eq!(listed.len(), 4);
    assert_eq!(listed[0].1, "open");
    assert_eq!(listed[1].1, "draft");
    assert_eq!(listed[2].1, "merged");
    assert_eq!(listed[3].1, "closed");
    // The same key a listing of that repository gives the pull request.
    let source = SourceKey {
        provider: Provider::Azure,
        host: "dev.azure.com".into(),
        repository: "contoso/Fabrikam/web".into(),
    };
    assert_eq!(mine.items[0].key.source, source);
    assert_eq!(mine.items[0].key.native_id, "pr/7");
    assert_eq!(
        listed[0].2,
        "https://dev.azure.com/contoso/Fabrikam/_git/web/pullrequest/7"
    );
    let asked = asked.borrow();
    assert!(asked[0].ends_with("/contoso/_apis/connectionData"));
    assert!(
        asked[1].contains("searchCriteria.creatorId=00000000-0000-0000-0000-00000000000a"),
        "{}",
        asked[1]
    );
}

#[test]
fn the_readers_pull_requests_page_up_to_a_cap() {
    let page = |from: u64| {
        json!({ "value": (from..from + 100)
            .map(|id| mine_pull_request(id, "Fabrikam", "web", "active"))
            .collect::<Vec<_>>() })
    };
    let pages: Vec<Value> = std::iter::once(reader())
        .chain((0..10).map(|index| page(index * 100 + 1)))
        .collect();
    let (mut fetch, asked) = replies(pages);
    let mine = organization::mine("contoso", &mut fetch).unwrap();
    assert!(!mine.complete);
    assert_eq!(mine.items.len(), organization::MAX_MINE);
    // The reader, then just enough pages for the cap.
    assert_eq!(asked.borrow().len(), 1 + organization::MAX_MINE / 100);
    assert!(asked.borrow()[2].contains("%24skip=100"));
}

#[test]
fn without_the_readers_id_their_pull_requests_cannot_be_found() {
    let (mut fetch, asked) = replies(vec![json!({ "authenticatedUser": {} })]);
    assert!(matches!(
        organization::mine("contoso", &mut fetch),
        Err(Error::Azure(azure::Error::Reader))
    ));
    assert_eq!(asked.borrow().len(), 1);
}

#[test]
fn a_browsed_repository_reads_pull_requests_and_no_work_items() {
    let scope = RemoteScope::Repository(SourceKey {
        provider: Provider::Azure,
        host: "dev.azure.com".into(),
        repository: "contoso/Fabrikam/web".into(),
    });
    let (mut fetch, asked) = replies(vec![
        json!({ "value": [pull_request(1, json!({}))] }),
        json!({ "value": [pull_request(2, json!({ "status": "completed" }))] }),
        json!({ "value": [] }),
        reader(),
    ]);
    let read = remote::read(&scope, true, &mut fetch).unwrap();
    assert!(read.catalog.is_none());
    assert_eq!(read.items.len(), 2);
    assert!(!asked.borrow().iter().any(|line| line.contains("/wit/")));
    let mut snapshot = scope.snapshot();
    remote::apply(&mut snapshot, read);
    assert!(matches!(
        snapshot.sources[0].state,
        SyncState::Synced { count: 2, .. }
    ));
    assert_eq!(snapshot.viewer.as_deref(), Some("Dev One"));
}

#[test]
fn the_organization_scope_reads_its_catalog_only_when_asked() {
    let scope = RemoteScope::Mine {
        organization: "contoso".into(),
    };
    let (mut fetch, asked) = replies(vec![
        json!({ "value": [repository("Fabrikam", "web")] }),
        reader(),
        json!({ "value": [mine_pull_request(3, "Fabrikam", "web", "active")] }),
    ]);
    let read = remote::read(&scope, true, &mut fetch).unwrap();
    assert_eq!(read.catalog.as_ref().map(|c| c.projects.len()), Some(1));
    let mut snapshot = scope.snapshot();
    remote::apply(&mut snapshot, read);
    assert!(snapshot.catalog.is_some());
    assert_eq!(asked.borrow().len(), 3);

    let (mut fetch, asked) = replies(vec![reader(), json!({ "value": [] })]);
    let read = remote::read(&scope, false, &mut fetch).unwrap();
    assert!(read.catalog.is_none());
    remote::apply(&mut snapshot, read);
    // The catalog read before stays.
    assert!(snapshot.catalog.is_some());
    assert!(snapshot.items.is_empty());
    assert_eq!(asked.borrow().len(), 2);
}

#[test]
fn a_remote_scope_names_its_organization_and_stands_in_as_a_repository() {
    let mine = RemoteScope::Mine {
        organization: "contoso".into(),
    };
    assert_eq!(mine.organization(), "contoso");
    assert_eq!(mine.source().repository, "contoso");
    let browsed = RemoteScope::Repository(SourceKey {
        provider: Provider::Azure,
        host: "dev.azure.com".into(),
        repository: "contoso/Fabrikam/web".into(),
    });
    assert_eq!(browsed.organization(), "contoso");
    let info = browsed.info();
    assert_eq!(
        info.remote.map(|remote| remote.source),
        Some(browsed.source())
    );
    assert!(info.main_root.is_empty() && !info.beads);
}

#[test]
fn a_remote_worker_without_a_token_asks_to_sign_in_and_takes_no_action() {
    let mut request = RemoteRequest::new(RemoteScope::Mine {
        organization: "contoso".into(),
    });
    request.look_up_token = false;
    request.timing = Timing {
        reload: Duration::from_millis(20),
        sync: Duration::from_secs(3600),
    };
    let service = Service::start_remote(request).unwrap();
    let snapshot = service::wait(&service, |snapshot| {
        matches!(snapshot.sources[0].state, SyncState::Failed(_))
    });
    assert!(snapshot.repo.is_some());
    assert!(snapshot.access.is_none());
    let SyncState::Failed(error) = &snapshot.sources[0].state else {
        unreachable!()
    };
    assert!(matches!(
        **error,
        Error::Azure(azure::Error::SignIn { ref repository }) if repository == "contoso"
    ));
    assert!(service.act(Action::DeleteBead {
        source: mine_source(),
        id: "x".into(),
    }));
    let deadline = Instant::now() + Duration::from_secs(10);
    let notice = loop {
        if let Some(notice) = service.notices().pop() {
            break notice;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    };
    let Err(error) = notice.outcome else {
        panic!("an action was taken without a checkout");
    };
    assert!(matches!(
        *error,
        Error::Azure(azure::Error::NoCheckout { .. })
    ));
}

#[test]
fn the_readers_own_are_matched_by_identity_not_by_their_shared_name() {
    let by = |id: &str| json!({ "createdBy": { "id": id, "displayName": "Dev One" } });
    let (mut fetch, _) = replies(vec![
        json!({ "value": [
            pull_request(1, by("00000000-0000-0000-0000-00000000000a")),
            // A teammate with the same display name.
            pull_request(2, by("00000000-0000-0000-0000-00000000000b")),
        ] }),
        json!({ "value": [] }),
        json!({ "value": [] }),
        reader(),
    ]);
    let listing = azure::list_pull_requests(
        &SourceKey {
            provider: Provider::Azure,
            host: "dev.azure.com".into(),
            repository: "contoso/Fabrikam/web".into(),
        },
        None,
        &mut fetch,
    )
    .unwrap();
    let names: Vec<_> = listing
        .items
        .iter()
        .map(|item| item.author.as_deref())
        .collect();
    assert_eq!(names, [Some("Dev One"), Some("Dev One")]);
    let authored: Vec<&String> = listing.authored.iter().collect();
    assert_eq!(authored, [&listing.items[0].key.canonical()]);
}
