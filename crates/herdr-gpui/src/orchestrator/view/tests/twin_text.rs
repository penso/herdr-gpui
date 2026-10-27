//! A page shows the copy of the checkout its row is: two checkouts of one
//! repository may have read the same issue differently.
use super::*;
use crate::orchestrator::{
    Request, Timing,
    view::{RepoSpec, repo::Repo},
};
use std::sync::Arc;

#[gpui::test]
fn a_page_reads_the_copy_of_the_checkout_its_row_shows(cx: &mut gpui::TestAppContext) {
    let (dir, view, cx) = empty::open(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let first = item(Provider::Github, "733", "Scrolling", "2026-10-04T00:00:00Z");
            let mut second = first.clone();
            second.title = "Scrolling, read later".into();
            let key = first.key.canonical();
            let request = Request {
                target: herdr_client::ConnectTarget::Local,
                checkout: dir.path().join("other").to_string_lossy().into_owned(),
                workspace_id: "other".into(),
                token: None,
                data_root: Some(dir.path().join("other-data")),
                gitlab_store: Default::default(),
                timing: Timing::default(),
            };
            view.repos.push(Repo::start(RepoSpec {
                key: "other".into(),
                host: "9950x3d.local".into(),
                request,
            }));
            view.repos[0].snapshot.items = Arc::new(vec![first]);
            view.repos[1].snapshot.items = Arc::new(vec![second]);
            view.tab = Tab::Issues;
            view.refresh_rows();
            assert_eq!(
                view.item(&key).map(|item| item.title.as_str()),
                Some("Scrolling")
            );

            view.projects = std::collections::HashSet::from(["other".to_owned()]);
            view.refresh_rows();
            view.open_detail(key.clone(), cx);
            assert_eq!(
                view.item(&key).map(|item| item.title.as_str()),
                Some("Scrolling, read later")
            );
        })
    });
}
