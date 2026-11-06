use super::*;
use crate::{
    device::{InstallationOrganizations, organization_coverage, organizations},
    installation::{Access, App, Listing, PAGE, assess, organization_owners},
};

fn listing(owners: Value) -> Listing {
    serde_json::from_value(serde_json::json!({ "installations": owners })).unwrap()
}

fn installed(login: &str, kind: &str, permissions: Value) -> Value {
    serde_json::json!({
        "id": 1,
        "app_slug": "herdr-ui",
        "account": { "login": login, "type": kind },
        "permissions": permissions,
    })
}

fn coverage(memberships: Option<Vec<String>>, listing: &Listing) -> Option<Vec<String>> {
    organization_coverage(
        memberships,
        InstallationOrganizations::Listed(organization_owners(listing)),
    )
}

#[test]
fn private_installations_route_without_public_membership_and_deduplicate_owners() {
    let memberships = organizations(&serde_json::json!({
        "data": { "viewer": { "organizations": { "nodes": [] } } }
    }));
    let installed = listing(serde_json::json!([
        installed("private-org", "Organization", serde_json::json!({})),
        installed("PRIVATE-ORG", "Organization", serde_json::json!({})),
        installed("personal", "User", serde_json::json!({})),
        installed("../invalid", "Organization", serde_json::json!({})),
    ]));
    let main = Profile::fixture("main", Arc::new("main-token".into()), &[]);
    let mut additional = Profile::fixture("other", Arc::new("ghu_app-token".into()), &[]);
    additional.organizations = coverage(Some(memberships), &installed);
    assert_eq!(additional.organizations, Some(vec!["private-org".into()]));
    let tokens = crate::Tokens::new(&main, [(&additional, &[][..])]);
    assert_eq!(
        tokens.for_owner("Private-Org").expose_secret(),
        "ghu_app-token"
    );
    assert_eq!(tokens.for_owner("personal").expose_secret(), "main-token");

    assert_eq!(
        coverage(
            Some(vec!["public-org".into(), "PRIVATE-ORG".into()]),
            &installed
        ),
        Some(vec!["public-org".into(), "PRIVATE-ORG".into()])
    );
}

#[test]
fn both_source_pages_fit_and_installation_owners_are_bounded() {
    let memberships: Vec<_> = (0..100).map(|i| format!("member-{i}")).collect();
    let installed = listing(Value::Array(
        (0..150)
            .map(|i| {
                installed(
                    &format!("installed-{i}"),
                    "Organization",
                    serde_json::json!({}),
                )
            })
            .collect(),
    ));
    let owners = organization_owners(&installed);
    assert_eq!(owners.len(), PAGE);
    let expected = memberships.len() + owners.len();
    let merged = coverage(Some(memberships), &installed).unwrap();
    assert_eq!(merged.len(), expected);
    assert!(merged.contains(&format!("installed-{}", PAGE - 1)));
    assert!(!merged.contains(&format!("installed-{PAGE}")));
}

#[test]
fn partial_failures_preserve_the_last_union_but_success_removes_old_owners() {
    let old = Profile::fixture(
        "other",
        Arc::new("ghu_token".into()),
        &["private-org", "public-org"],
    );
    for (memberships, installations) in [
        (
            None,
            InstallationOrganizations::Listed(vec!["private-org".into()]),
        ),
        (
            Some(vec!["public-org".into()]),
            InstallationOrganizations::Unknown,
        ),
    ] {
        let mut new = Profile::fixture("other", old.token.clone(), &[]);
        new.organizations = organization_coverage(memberships, installations);
        assert!(new.organizations.is_none());
        new.keep_organizations(&old);
        assert_eq!(new.organizations, old.organizations);
    }
    let mut new = Profile::fixture("other", old.token.clone(), &[]);
    new.organizations = coverage(Some(Vec::new()), &listing(serde_json::json!([])));
    new.keep_organizations(&old);
    assert_eq!(new.organizations, Some(Vec::new()));
    assert_eq!(
        organization_coverage(
            Some(vec!["pat-org".into()]),
            InstallationOrganizations::Unavailable
        ),
        Some(vec!["pat-org".into()])
    );
}

#[test]
fn optional_security_grants_do_not_misreport_core_repository_access() {
    let core = serde_json::json!({
        "checks": "read", "contents": "write", "issues": "read",
        "metadata": "read", "pull_requests": "write", "statuses": "read",
    });
    for permission in [
        "repository_advisories",
        "security_events",
        "vulnerability_alerts",
    ] {
        let mut requested = core.clone();
        requested[permission] = "read".into();
        let app: App =
            serde_json::from_value(serde_json::json!({ "permissions": requested })).unwrap();
        for kind in ["User", "Organization"] {
            let ready = listing(serde_json::json!([installed("owner", kind, core.clone())]));
            assert_eq!(assess(&ready, &app), Access::Ready, "{permission}, {kind}");
            // Optional grants must not hide a genuine missing core grant.
            for missing in core.as_object().unwrap().keys() {
                let mut held = core.clone();
                held.as_object_mut().unwrap().remove(missing);
                let pending = listing(serde_json::json!([installed("owner", kind, held)]));
                assert!(
                    matches!(assess(&pending, &app), Access::Pending(_)),
                    "{missing}"
                );
            }
        }
    }
}
