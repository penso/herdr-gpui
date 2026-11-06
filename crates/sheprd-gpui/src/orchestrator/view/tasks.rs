//! What the Tasks section adds to the list it shares with Issues: beads
//! alone, across every project, narrowed by status, priority, and assignee,
//! and each one's page: who holds it, what finishes it, the tasks it depends
//! on, and the way to its project. Beads stays the record: everything here
//! comes from the last sync, and `bd` makes every change.

use super::{Event, OrchestratorView, rows::closed};
use crate::{
    i18n::fl,
    orchestrator::{Item, Provider},
};
use gpui::{prelude::*, *};
use std::collections::{BTreeMap, HashSet};

/// The most assignees the filters offer, and the most tasks a page links
/// under one heading.
const FACET_ASSIGNEES: usize = 12;
const LINKS: usize = 20;

/// Whom a task is assigned to, as a filter asks.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Who {
    Nobody,
    Named(String),
}

impl Who {
    fn of(item: &Item) -> Self {
        item.assignee.clone().map_or(Self::Nobody, Self::Named)
    }

    fn label(&self) -> String {
        match self {
            Self::Nobody => fl!("orchestrator-task-unassigned"),
            Self::Named(name) => name.clone(),
        }
    }
}

/// Narrowing only the Tasks list has. A status is `bd`'s own word, since
/// projects can define their own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TaskFilters {
    pub(crate) status: Option<String>,
    pub(crate) priority: Option<i64>,
    pub(crate) who: Option<Who>,
}

impl TaskFilters {
    /// Whether the list can hold `item`: every open bead, and closed ones
    /// only when asked for.
    pub(crate) fn lists(&self, item: &Item) -> bool {
        item.key.source.provider == Provider::Beads
            && (!closed(item) || self.status.as_deref() == Some(item.state.as_str()))
    }

    /// Whether `item` passes every filter set.
    pub(crate) fn matches(&self, item: &Item) -> bool {
        self.status
            .as_deref()
            .is_none_or(|status| status == item.state)
            && self
                .priority
                .is_none_or(|priority| item.priority == Some(priority))
            && self.who.as_ref().is_none_or(|who| *who == Who::of(item))
    }

    /// How many filters are set.
    pub(super) fn active(&self) -> usize {
        usize::from(self.status.is_some())
            + usize::from(self.priority.is_some())
            + usize::from(self.who.is_some())
    }
}

/// What a status reads as. A word this build does not know, such as a
/// project's own status, is shown as it is.
pub(super) fn status_label(state: &str) -> String {
    match state {
        // keep-sorted start
        "blocked" => fl!("orchestrator-task-status-blocked"), // i18n: data
        "closed" => fl!("orchestrator-task-status-closed"),   // i18n: data
        "deferred" => fl!("orchestrator-task-status-deferred"), // i18n: data
        "in_progress" => fl!("orchestrator-task-status-in-progress"), // i18n: data
        "open" => fl!("orchestrator-task-status-open"),       // i18n: data
        // keep-sorted end
        other => other.to_owned(),
    }
}

/// A related task: its id, and the task when this database lists it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Link<'a> {
    pub(super) id: &'a str,
    pub(super) item: Option<&'a Item>,
}

/// The tasks a task is tied to, within its own database.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Links<'a> {
    pub(super) parent: Option<Link<'a>>,
    pub(super) blocked_by: Vec<Link<'a>>,
    pub(super) blocks: Vec<Link<'a>>,
    pub(super) children: Vec<Link<'a>>,
}

impl<'a> Links<'a> {
    pub(super) fn of(item: &'a Item, items: &'a [Item]) -> Self {
        let siblings = || {
            items
                .iter()
                .filter(|other| other.key.source == item.key.source)
        };
        let link = |id: &'a str| Link {
            id,
            item: siblings().find(|other| other.key.native_id == id),
        };
        let listed = |other: &'a Item| Link {
            id: other.key.native_id.as_str(),
            item: Some(other),
        };
        let id = item.key.native_id.as_str();
        Self {
            parent: item.parent_id.as_deref().map(link),
            blocked_by: item.blocked_by.iter().map(|id| link(id)).collect(),
            blocks: siblings()
                .filter(|other| other.blocked_by.iter().any(|blocker| blocker == id))
                .map(listed)
                .collect(),
            children: siblings()
                .filter(|other| other.parent_id.as_deref() == Some(id))
                .map(listed)
                .collect(),
        }
    }
}

/// A filter in use: its chip's ID, its text, and how to clear it.
pub(super) type Chip = (&'static str, String, fn(&mut OrchestratorView));

/// How often each key comes up, most often first, ties in the keys' order.
fn tally<K: Ord>(keys: impl Iterator<Item = K>) -> Vec<(K, usize)> {
    let mut counts = BTreeMap::new();
    for key in keys {
        *counts.entry(key).or_insert(0) += 1;
    }
    let mut counts: Vec<(K, usize)> = counts.into_iter().collect();
    counts.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    counts
}

impl OrchestratorView {
    /// Every bead, once however many projects share its database.
    fn every_task(&self) -> Vec<&Item> {
        let mut seen = HashSet::new();
        self.repos
            .iter()
            .flat_map(|repo| repo.snapshot.items.iter())
            .filter(|item| item.key.source.provider == Provider::Beads)
            .filter(|item| seen.insert(item.key.canonical()))
            .collect()
    }

    /// The filters' column for tasks: their statuses, priorities, and
    /// assignees, as the beads have them.
    pub(super) fn render_task_facets(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let tasks = self.every_task();
        let open = || tasks.iter().filter(|item| !closed(item));
        let mut facets = Vec::new();

        facets.push(
            self.facet_heading(fl!("orchestrator-facets-status"))
                .into_any_element(),
        );
        let statuses = tally(tasks.iter().map(|item| item.state.clone()));
        for (status, count) in statuses {
            let on = self.task_filters.status.as_deref() == Some(status.as_str());
            let id = format!("orchestrator-facet-status-{status}");
            let label = status_label(&status);
            facets.push(
                self.facet(id.into(), label.into(), Some(count), on)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let filters = &mut this.task_filters;
                        filters.status =
                            (filters.status.as_ref() != Some(&status)).then(|| status.clone());
                        this.refresh_rows();
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }

        let mut priorities = tally(open().filter_map(|item| item.priority));
        priorities.sort_by_key(|(priority, _)| *priority);
        if !priorities.is_empty() {
            facets.push(
                self.facet_heading(fl!("orchestrator-facets-priority"))
                    .into_any_element(),
            );
        }
        for (priority, count) in priorities {
            let on = self.task_filters.priority == Some(priority);
            let id = format!("orchestrator-facet-priority-{priority}");
            let label = super::list::priority_name(priority);
            facets.push(
                self.facet(id.into(), label.into(), Some(count), on)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let filters = &mut this.task_filters;
                        filters.priority = (filters.priority != Some(priority)).then_some(priority);
                        this.refresh_rows();
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }

        let mut people = tally(open().map(|item| Who::of(item)));
        people.truncate(FACET_ASSIGNEES);
        facets.push(
            self.facet_heading(fl!("orchestrator-facets-assignee"))
                .into_any_element(),
        );
        for (index, (who, count)) in people.into_iter().enumerate() {
            let on = self.task_filters.who.as_ref() == Some(&who);
            let id = format!("orchestrator-facet-assignee-{index}");
            let label = who.label();
            facets.push(
                self.facet(id.into(), label.into(), Some(count), on)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let filters = &mut this.task_filters;
                        filters.who = (filters.who.as_ref() != Some(&who)).then(|| who.clone());
                        this.refresh_rows();
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        facets
    }

    /// The tasks' filters in use, for the chips that clear them.
    pub(super) fn task_chips(&self) -> Vec<Chip> {
        let filters = &self.task_filters;
        let mut chips: Vec<Chip> = Vec::new();
        if let Some(status) = &filters.status {
            chips.push(("orchestrator-chip-status", status_label(status), |this| {
                this.task_filters.status = None;
            }));
        }
        if let Some(priority) = filters.priority {
            chips.push((
                "orchestrator-chip-priority",
                super::list::priority_name(priority),
                |this| this.task_filters.priority = None,
            ));
        }
        if let Some(who) = &filters.who {
            chips.push(("orchestrator-chip-assignee", who.label(), |this| {
                this.task_filters.who = None;
            }));
        }
        chips
    }

    /// A task's status, holder, and way to its project, then what finishes
    /// it and the tasks it is tied to.
    pub(super) fn render_task_facts(&self, item: &Item, cx: &mut Context<Self>) -> Div {
        let look = &self.look;
        let holder = match &item.assignee {
            Some(name) => fl!("orchestrator-task-assigned", name = name.as_str()),
            None => fl!("orchestrator-task-unassigned"),
        };
        let repo = self.repo().map(|repo| repo.key.clone());
        let summary = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(look.tag(
                status_label(&item.state),
                look.hue(look.state_hue(&item.state)),
            ))
            .child(look.muted(holder))
            .child(div().flex_1())
            .children(repo.map(|repo| {
                look.compact_button(
                    "orchestrator-task-project",
                    fl!("orchestrator-task-project"),
                )
                .debug_selector(|| "orchestrator-task-project".into())
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.emit(Event::OpenProject { repo: repo.clone() });
                }))
            }));
        let snapshot = self.snapshot();
        let links = Links::of(item, &snapshot.items);
        let sections = [
            (
                fl!("orchestrator-task-parent"),
                Vec::from_iter(links.parent),
            ),
            (fl!("orchestrator-task-blocked-by"), links.blocked_by),
            (fl!("orchestrator-task-blocks"), links.blocks),
            (fl!("orchestrator-task-children"), links.children),
        ];
        div()
            .debug_selector(|| "orchestrator-task-facts".into())
            .flex()
            .flex_col()
            .gap_3()
            .child(summary)
            .when_some(item.acceptance.clone(), |el, text| {
                el.child(
                    div()
                        .debug_selector(|| "orchestrator-task-acceptance".into())
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(look.label(fl!("orchestrator-task-acceptance")))
                        .child(text),
                )
            })
            .children(
                sections
                    .into_iter()
                    .enumerate()
                    .filter_map(|(section, (title, links))| {
                        (!links.is_empty()).then(|| self.render_links(section, title, &links, cx))
                    }),
            )
    }

    /// Tasks under `title`, each opening its page when it is listed.
    fn render_links(
        &self,
        section: usize,
        title: String,
        links: &[Link<'_>],
        cx: &mut Context<Self>,
    ) -> Div {
        let look = &self.look;
        let hidden = links.len().saturating_sub(LINKS);
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(look.label(title))
            .children(links.iter().take(LINKS).enumerate().map(|(index, link)| {
                let key = link.item.map(|item| item.key.canonical());
                let id = SharedString::from(format!("orchestrator-task-link-{section}-{index}"));
                let name = look
                    .compact_button(id, link.id.to_owned())
                    .when_some(key, |el, key| {
                        el.on_click(cx.listener(move |this, _, _, cx| {
                            this.open_detail(key.clone(), cx);
                        }))
                    });
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .child(name)
                    .children(
                        link.item
                            .map(|item| look.muted(item.title.clone()).min_w_0().truncate()),
                    )
            }))
            .when(hidden > 0, |el| {
                el.child(look.muted(fl!("orchestrator-task-more", count = hidden)))
            })
    }
}
