//! An alert's or report's page in the section, beside the list, as Pull
//! requests and Issues open an item: which one is open on which tab, and
//! what its page holds once read.
//!
//! A page is read when it opens, on the background executor, with the
//! account that read the listing, and kept for the session so reopening it
//! is instant. A report's page is the substance of a vulnerability nobody
//! has fixed yet, so it stays in memory only: never written to disk, never
//! listed by the CLI, and dropped with the rest of the section when the
//! window reads GitHub as someone else.

mod read;

#[cfg(test)]
pub(crate) use read::{AdvisoryNode, CommentNode, body, comments, parse_alert};
pub(crate) use read::{AlertPage, Comments, MAX_COMMENTS, Relationship, ReportPage, Scope};

use super::{Alert, Error, Repo, Report, Security};
use crate::HerdrWindow;
use gpui::{prelude::*, *};
use std::sync::{Arc, atomic::Ordering};

/// Pages kept at once; past it, all but the open one are dropped.
const MAX_PAGES: usize = 64;

/// One alert or report, by what names it on GitHub.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Item {
    Alert { repo: String, number: u64 },
    Report { repo: String, advisory: String },
}

impl Item {
    pub(crate) fn repo(&self) -> &str {
        match self {
            Self::Alert { repo, .. } | Self::Report { repo, .. } => repo,
        }
    }

    /// An item a command names: an alert by its number, a report by its
    /// GHSA ID, of a checked `owner/name`.
    pub(crate) fn named(repo: &str, item: &str) -> Option<Self> {
        if !super::model::valid_name(repo) {
            return None;
        }
        let repo = repo.to_owned();
        if super::model::ghsa(item) {
            return Some(Self::Report {
                repo,
                advisory: item.to_owned(),
            });
        }
        let number = item.strip_prefix('#').unwrap_or(item).parse().ok()?;
        Some(Self::Alert { repo, number })
    }

    pub(crate) fn tabs(&self) -> &'static [Tab] {
        match self {
            Self::Alert { .. } => &[Tab::Overview, Tab::Advisory, Tab::Dependency],
            Self::Report { .. } => &[Tab::Report, Tab::Draft, Tab::Conversation],
        }
    }
}

/// A page's tabs: an alert's first three, a report's last three.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tab {
    Overview,
    Advisory,
    Dependency,
    Report,
    Draft,
    Conversation,
}

impl Tab {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Advisory => "Advisory",
            Self::Dependency => "Dependency path",
            Self::Report => "Report",
            Self::Draft => "Advisory draft",
            Self::Conversation => "Conversation",
        }
    }
}

/// The item shown, and on which tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Opened {
    pub(crate) item: Item,
    pub(crate) tab: Tab,
}

#[derive(Debug)]
pub(crate) enum Page {
    Alert(AlertPage),
    Report(ReportPage),
}

/// Where an item's page is.
pub(crate) enum Loaded {
    /// Being read; dropping the task drops its answer.
    Reading {
        _task: Task<()>,
    },
    Read(Arc<Page>),
    Failed(Arc<Error>),
}

/// The alert or report an item names, as the listing last read it.
pub(crate) enum Found<'a> {
    Alert(&'a Repo, &'a Alert),
    Report(&'a Repo, &'a Report),
}

impl Security {
    /// The reports in triage across repositories, as the section shows
    /// them: rated before unrated, most severe, then newest first.
    pub(crate) fn reported(&self) -> Vec<(usize, &Repo, &Report)> {
        let mut reported: Vec<(usize, &Repo, &Report)> = self
            .repos
            .iter()
            .enumerate()
            .flat_map(|(index, repo)| {
                repo.reported()
                    .iter()
                    .map(move |report| (index, repo, report))
            })
            .collect();
        reported.sort_by(|(_, _, a), (_, _, b)| {
            (a.severity.is_none(), a.severity)
                .cmp(&(b.severity.is_none(), b.severity))
                .then_with(|| b.created_at.cmp(&a.created_at))
        });
        reported
    }

    /// Every item in the order the list shows them, for stepping through.
    pub(crate) fn items(&self) -> Vec<Item> {
        let reports = self
            .reported()
            .into_iter()
            .map(|(_, repo, report)| Item::Report {
                repo: repo.name.clone(),
                advisory: report.advisory.clone(),
            });
        let alerts = self.repos.iter().flat_map(|repo| {
            repo.open().iter().map(|alert| Item::Alert {
                repo: repo.name.clone(),
                number: alert.number,
            })
        });
        reports.chain(alerts).collect()
    }

    pub(crate) fn find(&self, item: &Item) -> Option<Found<'_>> {
        let repo = self.repos.iter().find(|repo| repo.name == item.repo())?;
        match item {
            Item::Alert { number, .. } => repo
                .open()
                .iter()
                .find(|alert| alert.number == *number)
                .map(|alert| Found::Alert(repo, alert)),
            Item::Report { advisory, .. } => repo
                .reported()
                .iter()
                .find(|report| &report.advisory == advisory)
                .map(|report| Found::Report(repo, report)),
        }
    }

    pub(crate) fn opened(&self) -> Option<&Opened> {
        self.opened.as_ref()
    }

    pub(crate) fn list_hidden(&self) -> bool {
        self.list_hidden
    }

    pub(crate) fn page(&self, item: &Item) -> Option<&Loaded> {
        self.pages.get(item)
    }

    /// Closes a page whose item a new listing no longer holds: the alert
    /// was fixed or dismissed, or the report left triage.
    pub(super) fn keep_opened(&mut self) {
        if let Some(opened) = &self.opened
            && self.find(&opened.item).is_none()
        {
            self.opened = None;
        }
    }
}

impl HerdrWindow {
    /// Shows `item`'s page on its first tab, reading it unless it was read.
    pub(crate) fn open_security_item(&mut self, item: Item, cx: &mut Context<Self>) {
        let tab = item.tabs()[0];
        let read = matches!(
            self.security.pages.get(&item),
            Some(Loaded::Read(_) | Loaded::Reading { .. })
        );
        self.security.opened = Some(Opened {
            item: item.clone(),
            tab,
        });
        self.security.page_scroll.set_offset(Point::default());
        if !read {
            self.read_security_page(item, cx);
        }
        cx.notify();
    }

    pub(crate) fn close_security_item(&mut self, cx: &mut Context<Self>) {
        self.security.opened = None;
        cx.notify();
    }

    /// Opens the item `by` places after (or before) the one shown.
    pub(crate) fn step_security_item(&mut self, by: isize, cx: &mut Context<Self>) {
        let Some(opened) = &self.security.opened else {
            return;
        };
        // Through what the search and pills show, as the list does.
        let items = self.shown_items();
        let Some(at) = items.iter().position(|item| *item == opened.item) else {
            return;
        };
        if let Some(next) = at.checked_add_signed(by).and_then(|at| items.get(at)) {
            self.open_security_item(next.clone(), cx);
        }
    }

    pub(crate) fn show_security_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        if let Some(opened) = &mut self.security.opened
            && opened.item.tabs().contains(&tab)
        {
            opened.tab = tab;
            self.security.page_scroll.set_offset(Point::default());
            cx.notify();
        }
    }

    pub(crate) fn toggle_security_list(&mut self, cx: &mut Context<Self>) {
        self.security.list_hidden = !self.security.list_hidden;
        cx.notify();
    }

    /// Reads the open page once a read brought the account it waited for.
    pub(super) fn read_waiting_page(&mut self, cx: &mut Context<Self>) {
        if let Some(opened) = &self.security.opened
            && !self.security.pages.contains_key(&opened.item)
        {
            self.read_security_page(opened.item.clone(), cx);
        }
    }

    /// Reads `item`'s page again, after a failure.
    pub(crate) fn retry_security_page(&mut self, cx: &mut Context<Self>) {
        if let Some(opened) = &self.security.opened {
            self.read_security_page(opened.item.clone(), cx);
            cx.notify();
        }
    }

    fn read_security_page(&mut self, item: Item, cx: &mut Context<Self>) {
        let Some(repo) = self
            .security
            .repos
            .iter()
            .find(|repo| repo.name == item.repo())
        else {
            return;
        };
        let Some(account) = repo.account.clone() else {
            // Restored from the cache: the read in flight brings the
            // account, and `read_waiting_page` then reads the page.
            if !self.security.busy() {
                let error = Arc::new(Error::Output);
                self.security.pages.insert(item, Loaded::Failed(error));
            }
            return;
        };
        if self.security.pages.len() >= MAX_PAGES {
            let open = self.security.opened.as_ref().map(|opened| &opened.item);
            self.security.pages.retain(|kept, _| Some(kept) == open);
        }
        let name = repo.name.clone();
        let cancelled = self.security.cancelled.clone();
        let wanted = item.clone();
        let work = cx.background_executor().spawn(async move {
            let cancelled = || cancelled.load(Ordering::Relaxed);
            match &wanted {
                Item::Alert { number, .. } => {
                    read::alert(&name, &account.0, *number, cancelled).map(Page::Alert)
                }
                Item::Report { advisory, .. } => {
                    read::report(&name, &account.0, advisory, cancelled).map(Page::Report)
                }
            }
        });
        let key = item.clone();
        let task = cx.spawn(async move |this, cx| {
            let page = work.await;
            let _ = this.update(cx, |this, cx| {
                let loaded = match page {
                    Ok(page) => Loaded::Read(Arc::new(page)),
                    Err(error) => Loaded::Failed(Arc::new(error)),
                };
                // A sign-out in between dropped the entry with this task.
                if let Some(entry) = this.security.pages.get_mut(&key) {
                    *entry = loaded;
                }
                cx.notify();
            });
        });
        self.security
            .pages
            .insert(item, Loaded::Reading { _task: task });
    }
}
