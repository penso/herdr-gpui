//! The pills under a section's search: one per backend, each with how many
//! of the shown list's items it holds. Picking some narrows the list to
//! them; none picked lists everything. The row shows only when there is more
//! than one to choose between. Projects narrow through their own menu
//! (`projects`).

use super::{
    OrchestratorView,
    inbox::{provider_mark, provider_name},
    rows::Tab,
};
use crate::orchestrator::Provider;
use gpui::{prelude::*, *};

impl OrchestratorView {
    pub(super) fn render_pills(&self, cx: &mut Context<Self>) -> Option<Div> {
        let look = &self.look;
        let theme = &look.theme;
        let tab = self.tab;
        fn rows(repo: &super::Repo, tab: Tab) -> &[super::rows::ItemRow] {
            match tab {
                Tab::Issues => &repo.issue_rows,
                Tab::PullRequests => &repo.pull_request_rows,
            }
        }
        let mut backends: Vec<(Provider, usize)> = Vec::new();
        // An item several checkouts list is one row, so it counts once.
        let mut counted = std::collections::HashSet::new();
        for repo in &self.repos {
            for row in rows(repo, tab).iter().filter(|row| row.depth == 0) {
                let key = &repo.snapshot.items[row.item].key;
                if !counted.insert(key.canonical()) {
                    continue;
                }
                let provider = key.source.provider;
                match backends.iter_mut().find(|(found, _)| *found == provider) {
                    Some((_, count)) => *count += 1,
                    None => backends.push((provider, 1)),
                }
            }
        }
        backends.sort_by_key(|(provider, _)| provider_name(*provider));
        if backends.len() < 2 {
            return None;
        }
        let pill = |id: SharedString, on: bool| look.pill(id, on);
        let count = |count: usize| {
            (count > 0).then(|| div().text_color(rgb(theme.muted)).child(count.to_string()))
        };
        let mut row = div()
            .debug_selector(|| "orchestrator-pills".into())
            .flex_none()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .px_4()
            .pb_2();
        if backends.len() > 1 {
            for (provider, items) in backends {
                let on = self.backends.contains(&provider);
                row = row.child(
                    pill(
                        format!("orchestrator-pill-backend-{}", provider_name(provider)).into(),
                        on,
                    )
                    .child(provider_mark(look, provider))
                    .child(provider_name(provider))
                    .children(count(items))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.backends.remove(&provider) {
                            this.backends.insert(provider);
                        }
                        this.refresh_rows();
                        cx.notify();
                    })),
                );
            }
        }
        Some(row)
    }

    /// A project's name, with its host's when the same name is listed from
    /// another host too.
    pub(super) fn project_label(&self, repo: &super::Repo) -> String {
        let name = repo.name();
        let shared = self
            .projects_named()
            .filter(|other| other.name() == name)
            .count()
            > 1;
        if shared {
            format!("{name} \u{00b7} {}", repo.host)
        } else {
            name
        }
    }
}
