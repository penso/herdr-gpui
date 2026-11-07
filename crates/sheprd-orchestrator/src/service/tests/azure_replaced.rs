//! A capped Azure DevOps listing shown but not stored replaces the stored
//! work items too, which are keyed by the project rather than the repository.
use super::*;

fn key(repository: &str) -> SourceKey {
    SourceKey {
        provider: Provider::Azure,
        host: "dev.azure.com".into(),
        repository: repository.into(),
    }
}

#[test]
fn an_azure_listing_replaces_its_repository_and_its_project() {
    let uncached = HashMap::from([(key("contoso/Fabrikam/web"), Vec::new())]);
    let replaced = replaced_by(&uncached);
    assert!(replaced.contains(&key("contoso/Fabrikam/web")));
    assert!(replaced.contains(&key("contoso/Fabrikam")));
    assert!(!replaced.contains(&key("contoso/Other")));
    assert_eq!(replaced.len(), 2);
}

#[test]
fn other_listings_replace_only_their_own_source() {
    let gitlab = SourceKey {
        provider: Provider::Gitlab,
        host: "gitlab.example.test".into(),
        repository: "group/sub/app".into(),
    };
    let uncached = HashMap::from([(gitlab.clone(), Vec::new())]);
    let replaced = replaced_by(&uncached);
    assert_eq!(replaced.len(), 1);
    assert!(replaced.contains(&gitlab));
}
