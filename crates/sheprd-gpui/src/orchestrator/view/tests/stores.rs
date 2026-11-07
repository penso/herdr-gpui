//! A repository kept across `set_repos` follows the setting that says where
//! pasted tokens are saved, as a newly started one would.
use super::*;
use crate::orchestrator::view::RepoSpec;
use gpui::TestAppContext;
use sheprd_credentials::Store;

#[gpui::test]
fn a_kept_repository_takes_the_new_stores(cx: &mut TestAppContext) {
    let (_dir, view, cx) = empty::open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let repo = &view.repos[0];
            let mut spec = RepoSpec {
                key: repo.key.clone(),
                host: repo.host.clone(),
                request: repo.request.clone(),
            };
            spec.request.azure_store = Store::File;
            spec.request.gitlab_store = Store::File;
            let stores = spec.request.stores();

            view.set_repos(vec![spec], cx);

            let repo = &view.repos[0];
            assert_eq!(repo.request.stores(), stores);
            assert!(repo.service.is_some(), "the worker is kept");
        });
    });
}
