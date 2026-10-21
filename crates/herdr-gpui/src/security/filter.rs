//! The section's search and pills, as Pull requests and Issues narrow
//! theirs: the field matches an item's summary, package, advisory,
//! repository, and reporter; a pill per source (private reports, Dependabot)
//! and per repository narrows to the ones picked, none picked lists all.
//! The cards, the list beside a page, and stepping all show what passes.

use super::{Alert, Item, Repo, Report, page::Found};
use crate::{HerdrWindow, orchestrator::Look, search_input::SearchInput};
use gpui::{prelude::*, *};
use std::collections::HashSet;

/// Where an item comes from, one pill each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Source {
    Reports,
    Dependabot,
}

impl Source {
    fn label(self) -> &'static str {
        match self {
            Self::Reports => "Reported to you",
            Self::Dependabot => "Dependabot",
        }
    }
}

pub(crate) struct SecuritySearch {
    pub(super) input: Entity<SearchInput>,
    /// The field's words, lowercased; every one must match.
    words: Vec<String>,
    sources: HashSet<Source>,
    /// Repositories picked, by `owner/name`.
    repos: HashSet<String>,
    _changed: Subscription,
}

impl SecuritySearch {
    pub(crate) fn new(cx: &mut Context<HerdrWindow>) -> Self {
        let input = cx.new(|cx| {
            let mut input = SearchInput::new(cx);
            input.set_placeholder(
                "Filter: summary, package, advisory, repository, reporter\u{2026}",
                cx,
            );
            input.set_frameless(cx);
            input
        });
        let changed = cx.subscribe(
            &input,
            |this, input, _: &crate::search_input::Changed, cx| {
                this.security_search.words = words(input.read(cx).text());
                cx.notify();
            },
        );
        Self {
            input,
            words: Vec::new(),
            sources: HashSet::new(),
            repos: HashSet::new(),
            _changed: changed,
        }
    }

    /// Whether the field or a pill hides anything.
    pub(super) fn narrows(&self) -> bool {
        !self.words.is_empty() || !self.sources.is_empty() || !self.repos.is_empty()
    }

    fn picked(&self, source: Source, repo: &Repo) -> bool {
        (self.sources.is_empty() || self.sources.contains(&source))
            && (self.repos.is_empty() || self.repos.contains(&repo.name))
    }

    pub(super) fn shows_report(&self, repo: &Repo, report: &Report) -> bool {
        self.picked(Source::Reports, repo) && self.report_matches(repo, report)
    }

    pub(super) fn shows_alert(&self, repo: &Repo, alert: &Alert) -> bool {
        self.picked(Source::Dependabot, repo) && self.alert_matches(repo, alert)
    }

    fn report_matches(&self, repo: &Repo, report: &Report) -> bool {
        let rating = report
            .severity
            .map_or("unrated", |severity| severity.name());
        let fields = [
            report.summary.as_str(),
            report.advisory.as_str(),
            repo.name.as_str(),
            report.reporter.as_deref().unwrap_or_default(),
            rating,
        ];
        self.matches(&fields)
    }

    fn alert_matches(&self, repo: &Repo, alert: &Alert) -> bool {
        let number = alert.number.to_string();
        let fields = [
            alert.summary.as_str(),
            alert.package.as_str(),
            alert.ecosystem.as_str(),
            alert.advisory.as_str(),
            alert.manifest.as_str(),
            repo.name.as_str(),
            alert.severity.name(),
            number.as_str(),
        ];
        self.matches(&fields)
    }

    fn matches(&self, fields: &[&str]) -> bool {
        self.words.iter().all(|word| {
            fields
                .iter()
                .any(|field| field.to_lowercase().contains(word.as_str()))
        })
    }
}

fn words(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_lowercase).collect()
}

impl HerdrWindow {
    /// The reports in triage the search and pills let through, in the
    /// section's order.
    pub(super) fn shown_reports(&self) -> Vec<(usize, &Repo, &Report)> {
        let search = &self.security_search;
        self.security
            .reported()
            .into_iter()
            .filter(|(_, repo, report)| search.shows_report(repo, report))
            .collect()
    }

    /// `repo`'s open alerts the search and pills let through.
    pub(super) fn shown_alerts<'a>(&self, repo: &'a Repo) -> Vec<&'a Alert> {
        let search = &self.security_search;
        repo.open()
            .iter()
            .filter(|alert| search.shows_alert(repo, alert))
            .collect()
    }

    /// Every item shown, in the list's order, for stepping through.
    pub(super) fn shown_items(&self) -> Vec<Item> {
        let search = &self.security_search;
        let mut items = self.security.items();
        items.retain(|item| match self.security.find(item) {
            Some(Found::Alert(repo, alert)) => search.shows_alert(repo, alert),
            Some(Found::Report(repo, report)) => search.shows_report(repo, report),
            None => false,
        });
        items
    }

    /// The search field, then a pill per source and per repository with
    /// how many items the field lets through; a group of pills shows with
    /// more than one to choose between, or while one of them is picked.
    pub(super) fn render_security_toolbar(
        &self,
        look: &Look,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let search = &self.security_search;
        let theme = &look.theme;
        search.input.update(cx, |input, cx| {
            input.set_appearance(look.ui.clone(), look.theme.clone(), cx);
        });
        let focused = search.input.read(cx).focus.is_focused(window);
        let typed = !search.words.is_empty();
        let field = look
            .search_frame("security-search", focused)
            .debug_selector(|| "security-search".into())
            .child(div().flex_1().min_w_0().child(search.input.clone()))
            .when(typed, |field| {
                field.child(
                    look.icon_button("security-search-clear", "icons/close.svg", "Clear search")
                        .debug_selector(|| "security-search-clear".into())
                        .on_click(cx.listener(|this, _, window, cx| {
                            let input = this.security_search.input.clone();
                            input.update(cx, |input, cx| input.clear(cx));
                            window.focus(&input.read(cx).focus.clone(), cx);
                        })),
                )
            });
        // Counted by the field alone, so a pill says what picking it shows.
        let mut sources = [(Source::Reports, 0), (Source::Dependabot, 0)];
        let mut repos: Vec<(String, usize)> = Vec::new();
        let mut count = |source: Source, repo: &Repo| {
            sources
                .iter_mut()
                .filter(|(found, _)| *found == source)
                .for_each(|(_, count)| *count += 1);
            match repos.iter_mut().find(|(name, _)| *name == repo.name) {
                Some((_, count)) => *count += 1,
                None => repos.push((repo.name.clone(), 1)),
            }
        };
        for repo in self.security.repos() {
            for report in repo.reported() {
                if search.report_matches(repo, report) {
                    count(Source::Reports, repo);
                }
            }
            for alert in repo.open() {
                if search.alert_matches(repo, alert) {
                    count(Source::Dependabot, repo);
                }
            }
        }
        // A picked pill stays, to be unpicked, even once it holds nothing.
        let sources: Vec<_> = sources
            .into_iter()
            .filter(|(source, count)| *count > 0 || search.sources.contains(source))
            .collect();
        for name in &search.repos {
            if !repos.iter().any(|(found, _)| found == name) {
                repos.push((name.clone(), 0));
            }
        }
        let tally = |count: usize| {
            (count > 0).then(|| div().text_color(rgb(theme.muted)).child(count.to_string()))
        };
        let mut pills = div()
            .debug_selector(|| "security-pills".into())
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2();
        // A group shows with a choice to make, or with a pick to undo.
        let many_sources = sources.len() > 1 || !search.sources.is_empty();
        if many_sources {
            for (source, items) in sources {
                let on = search.sources.contains(&source);
                let id = match source {
                    Source::Reports => "security-pill-reports",
                    Source::Dependabot => "security-pill-dependabot",
                };
                pills = pills.child(
                    look.pill(id, on)
                        .child(source.label())
                        .children(tally(items))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let picked = &mut this.security_search.sources;
                            if !picked.remove(&source) {
                                picked.insert(source);
                            }
                            cx.notify();
                        })),
                );
            }
        }
        let many_repos = repos.len() > 1 || !search.repos.is_empty();
        if many_repos {
            if many_sources {
                pills = pills.child(div().w(px(1.)).h(px(16.)).bg(rgb(theme.active)));
            }
            for (index, (name, items)) in repos.into_iter().enumerate() {
                let on = search.repos.contains(&name);
                pills = pills.child(
                    look.pill(format!("security-pill-repo-{index}"), on)
                        .rounded(px(crate::config::corners::CONTROL))
                        .child(look.icon("icons/shield.svg", 11., theme.subtext()))
                        .child(name.clone())
                        .children(tally(items))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let picked = &mut this.security_search.repos;
                            if !picked.remove(&name) {
                                picked.insert(name.clone());
                            }
                            cx.notify();
                        })),
                );
            }
        }
        div()
            .debug_selector(|| "security-toolbar".into())
            .flex_none()
            .flex()
            .flex_col()
            .gap_2()
            .px_5()
            .pt_3()
            .pb_2()
            .child(div().flex().items_center().gap_2().child(field))
            .when(many_sources || many_repos, |el| el.child(pills))
    }
}
