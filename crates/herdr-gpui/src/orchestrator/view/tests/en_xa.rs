//! The `en-XA` leak test (`crate::i18n::leak_tests`) for the Pull requests
//! and Issues sections: the issues list with its filters, an issue's page
//! on each tab, the dispatch dialog, a confirmation, and the pull request
//! list, full and empty.
use super::*;
use crate::{
    i18n::leak_tests::{assert_translated, draw, uncut, under_en_xa},
    orchestrator::{
        PullRequest,
        view::{DetailTab, dispatch::Confirm, rows::Tab},
    },
};
use std::sync::Arc;

/// What the repository holds: titles, identifiers, authors, labels,
/// branches, agents, and what an agent says it is doing.
fn data() -> Vec<String> {
    [
        "app",
        "penso/app",
        "penso",
        "Icons",
        "Menus",
        "Fonts",
        "Speed up start",
        "377",
        "378",
        "bd-12",
        "#377",
        "#378",
        "#41",
        "claude",
        "b-r1",
        "feature",
        "main",
        "ui",
        "Running tests",
        "herdr-gpui",
        // An item's state, as its provider words it.
        "open",
        // Backends, by their products' names.
        "GitHub",
        "Beads",
        "bd",
        // The raw key and RFC 3339 times the Details tab lists.
        "github:github.com:penso/app:377",
        "2026-10-04T00:00:00+00:00",
        "https://github.com/penso/app/issues/377",
        "The icons are blurry.",
        "feature @ 0123456789ab",
        "main @ ba9876543210",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

/// A line that joins facts with " · " is checked fact by fact.
fn facts(shown: Vec<String>) -> Vec<String> {
    uncut(shown)
        .into_iter()
        .flat_map(|line| {
            if herdr_i18n::pseudo::is_marked(&line) {
                vec![line]
            } else {
                line.split(" \u{00b7} ").map(str::to_owned).collect()
            }
        })
        .collect()
}

fn issues() -> (Vec<Item>, Run) {
    let mut icons = item(Provider::Github, "377", "Icons", "2026-10-04T00:00:00Z");
    icons.identifier = "#377".into();
    icons.description = Some("The icons are blurry.".into());
    icons.labels = vec!["ui".into()];
    icons.url = Some("https://github.com/penso/app/issues/377".into());
    let mut menus = item(Provider::Github, "378", "Menus", "2026-10-03T00:00:00Z");
    menus.identifier = "#378".into();
    menus.blocked_by = vec!["#377".into()];
    let mut fonts = item(Provider::Beads, "bd-12", "Fonts", "2026-10-02T00:00:00Z");
    fonts.priority = Some(1);
    let run = run("r1", &icons, RunState::Running, "2026-10-04T01:00:00Z");
    (vec![icons, menus, fonts], run)
}

fn pull_request() -> Item {
    let mut pr = item(
        Provider::Github,
        "pr/41",
        "Speed up start",
        "2026-10-04T00:00:00Z",
    );
    pr.identifier = "#41".into();
    pr.pull_request = Some(PullRequest {
        number: 41,
        additions: Some(120),
        deletions: Some(4),
        base_ref: "main".into(),
        head_ref: "feature".into(),
        base_sha: "ba9876543210".into(),
        head_sha: "0123456789ab".into(),
        head_repository: None,
    });
    pr
}

#[test]
fn the_sections_show_only_messages_and_what_repositories_hold() {
    under_en_xa("orchestrator en-XA leak", |cx, recorder| {
        let (_dir, view, cx) = empty::open(cx);
        cx.simulate_resize(gpui::size(gpui::px(1800.), gpui::px(1000.)));
        let data = data();
        let (items, run) = issues();
        let key = items[0].key.canonical();
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                let snapshot = &mut view.repo_mut().snapshot;
                snapshot.items = Arc::new(items);
                snapshot.runs = Arc::new(vec![run]);
                view.tab = Tab::Issues;
                view.filters_open = true;
                view.refresh_rows();
                cx.notify();
            })
        });
        draw(cx);
        assert_translated("issues", &facts(recorder.take()), &data);

        for tab in [DetailTab::Description, DetailTab::Agent, DetailTab::Details] {
            cx.update(|_, cx| {
                view.update(cx, |view, cx| {
                    view.open_detail(key.clone(), cx);
                    if let Some(detail) = &mut view.detail {
                        detail.tab = tab;
                    }
                    cx.notify();
                })
            });
            draw(cx);
            assert_translated(
                &format!("an issue on {tab:?}"),
                &facts(recorder.take()),
                &data,
            );
        }

        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.open_dispatch(key.clone(), window, cx))
        });
        draw(cx);
        assert!(cx.debug_bounds("orchestrator-overlay-card").is_some());
        // The branch field holds the name the dialog makes up for the item.
        let shown: Vec<String> = facts(recorder.take())
            .into_iter()
            .filter(|line| !line.starts_with("agent/377-icons-"))
            .collect();
        assert_translated("the dispatch dialog", &shown, &data);

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.close_overlays(window, cx);
                view.ask(
                    Confirm::Stop {
                        run: "r1".into(),
                        agent: "claude".into(),
                    },
                    cx,
                );
            })
        });
        draw(cx);
        assert_translated("stopping an agent", &facts(recorder.take()), &data);

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.close_overlays(window, cx);
                view.detail = None;
                view.repo_mut().snapshot.items = Arc::new(vec![pull_request()]);
                view.show_list(Tab::PullRequests, cx);
            })
        });
        draw(cx);
        assert_translated("pull requests", &facts(recorder.take()), &data);

        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.repo_mut().snapshot.items = Arc::new(Vec::new());
                view.refresh_rows();
                cx.notify();
            })
        });
        draw(cx);
        assert!(cx.debug_bounds("orchestrator-empty").is_some());
        assert_translated("no pull requests", &facts(recorder.take()), &data);
    });
}
