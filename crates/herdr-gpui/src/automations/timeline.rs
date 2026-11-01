//! The timeline: what needs the user, what runs now, what fires next, and
//! the runs each host keeps, newest first and grouped by day, for every
//! machine or the one a chip picks.

use super::{Look, Machine, Opened, format, model};
use crate::{HerdrWindow, config::corners, i18n::fl};
use chrono::TimeZone;
use gpui::{prelude::*, *};
use herdr_automation::report::{RECENT_RUNS, RunState, ScriptState, Status};

/// The timeline's groups, in the order it shows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Group {
    NeedsYou,
    Now,
    Next,
    Today,
    Yesterday,
    ThisWeek,
    Older,
}

impl Group {
    pub(crate) fn title(self) -> String {
        match self {
            Self::NeedsYou => fl!("automations-group-needs-you"),
            Self::Now => fl!("automations-group-now"),
            Self::Next => fl!("automations-group-next"),
            Self::Today => fl!("automations-group-today"),
            Self::Yesterday => fl!("automations-group-yesterday"),
            Self::ThisWeek => fl!("automations-group-this-week"),
            Self::Older => fl!("automations-group-older"),
        }
    }
}

/// One line of the timeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(crate) group: Group,
    /// What orders the line in its group: when it happened or will.
    pub(crate) at: i64,
    pub(crate) when: String,
    pub(crate) look: Look,
    pub(crate) host: String,
    pub(crate) name: String,
    pub(crate) text: String,
    pub(crate) open: Opened,
    /// The line offers Review: the script waits for approval.
    pub(crate) review: bool,
}

/// A machine's last read, as the timeline takes it.
pub(crate) struct Source<'a> {
    pub(crate) machine: &'a Machine,
    pub(crate) label: &'a str,
    pub(crate) status: &'a Status,
}

/// Every line, grouped and ordered, read in `zone`.
pub(crate) fn entries<Tz: TimeZone>(sources: &[Source<'_>], now: i64, zone: &Tz) -> Vec<Entry>
where
    Tz::Offset: std::fmt::Display,
{
    let today = format::day(now, zone);
    let mut entries = Vec::new();
    for source in sources {
        let status = source.status;
        let script_page = |path: &str| Opened::Script {
            machine: source.machine.clone(),
            path: path.to_owned(),
        };
        let run_page = |path: &str, run: i64| Opened::Run {
            machine: source.machine.clone(),
            path: path.to_owned(),
            run,
        };
        let entry = |group, at, when: String, look, name: &str, text: String, open, review| Entry {
            group,
            at,
            when,
            look,
            host: source.label.to_owned(),
            name: name.to_owned(),
            text,
            open,
            review,
        };
        for script in &status.scripts {
            if script.state == ScriptState::AwaitingApproval {
                let text = match script.reason.as_deref() {
                    Some(why) => fl!("automations-review-reason", why = why),
                    None => fl!("automations-review-new"),
                };
                entries.push(entry(
                    Group::NeedsYou,
                    i64::MAX,
                    String::new(),
                    Look::Waiting,
                    &script.name,
                    text,
                    script_page(&script.path),
                    true,
                ));
            }
            if let Some(run) = script
                .last_run
                .as_ref()
                .filter(|run| model::failed(run.state))
            {
                entries.push(entry(
                    Group::NeedsYou,
                    run.started_at,
                    format::when_in(run.started_at, now, zone),
                    Look::Failed,
                    &script.name,
                    model::failure(run),
                    run_page(&script.path, run.id),
                    false,
                ));
            }
            if let Some(id) = script.running {
                let run = model::find_run(status, id);
                entries.push(entry(
                    Group::Now,
                    run.map_or(now, |run| run.started_at),
                    run.map_or_else(String::new, |run| {
                        format::when_in(run.started_at, now, zone)
                    }),
                    Look::Running,
                    &script.name,
                    run.map_or_else(
                        || fl!("automations-running"),
                        |run| model::progress(run, now),
                    ),
                    run_page(&script.path, id),
                    false,
                ));
            }
            if script.state == ScriptState::Active {
                for trigger in &script.triggers {
                    let Some(next) = trigger.next else {
                        continue;
                    };
                    let text = match &trigger.name {
                        Some(name) => format!("{name} · {}", trigger.schedule),
                        None => trigger.schedule.clone(),
                    };
                    entries.push(entry(
                        Group::Next,
                        next,
                        format::until(next, now),
                        Look::Paused,
                        &script.name,
                        text,
                        script_page(&script.path),
                        false,
                    ));
                }
            }
        }
        for run in status
            .runs
            .iter()
            .filter(|run| run.state != RunState::Running)
        {
            let days = match (format::day(run.started_at, zone), today) {
                (Some(day), Some(today)) => (today - day).num_days(),
                _ => i64::MAX,
            };
            let group = match days {
                ..=0 => Group::Today,
                1 => Group::Yesterday,
                2..=6 => Group::ThisWeek,
                _ => Group::Older,
            };
            entries.push(entry(
                group,
                run.started_at,
                format::when_in(run.started_at, now, zone),
                model::run_look(run.state),
                &run.name,
                model::summary(run, now),
                run_page(&run.script, run.id),
                false,
            ));
        }
    }
    // Next soonest first; everything else newest first.
    entries.sort_by(|a, b| {
        a.group.cmp(&b.group).then_with(|| match a.group {
            Group::Next => a.at.cmp(&b.at),
            _ => b.at.cmp(&a.at),
        })
    });
    entries
}

impl HerdrWindow {
    pub(super) fn automations_timeline(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let small = self.automation_small();
        let filter = self.automations.filter.clone();
        let mut chips = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .text_size(small);
        let chip =
            |id: String, label: String, on: bool, pick: Option<Machine>, cx: &mut Context<Self>| {
                let selector = id.clone();
                div()
                    .id(SharedString::from(id))
                    .debug_selector(move || selector.clone())
                    .px_2()
                    .py(px(2.))
                    .rounded(px(corners::SMALL))
                    .cursor_pointer()
                    .when(on, |el| {
                        el.bg(rgb(theme.primary_wash()))
                            .text_color(rgb(theme.primary()))
                    })
                    .when(!on, |el| {
                        el.bg(rgb(theme.active)).text_color(rgb(theme.subtext()))
                    })
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.automations.filter = pick.clone();
                        cx.notify();
                    }))
            };
        chips = chips.child(chip(
            "automations-filter-all".into(),
            fl!("automations-all-hosts"),
            filter.is_none(),
            None,
            cx,
        ));
        let mut sources = Vec::new();
        let mut kept = 0;
        for (index, host) in self.automations.hosts.iter().enumerate() {
            let Some(status) = host.status() else {
                continue;
            };
            let on = filter.as_ref() == Some(&host.machine);
            chips = chips.child(chip(
                format!("automations-filter-{index}"),
                host.label.clone(),
                on,
                Some(host.machine.clone()),
                cx,
            ));
            if filter.is_none() || on {
                kept += status.runs.len();
                sources.push(Source {
                    machine: &host.machine,
                    label: &host.label,
                    status,
                });
            }
        }
        let entries = entries(&sources, format::now(), &chrono::Local);
        chips = chips.child(div().flex_1()).child(
            div()
                .text_color(rgb(theme.muted))
                .child(fl!("automations-recent-count", count = kept)),
        );
        let mut body = div().p_5().flex().flex_col().gap_4().child(chips);
        if entries.is_empty() {
            return body.child(
                div()
                    .debug_selector(|| "automations-timeline-empty".into())
                    .text_color(rgb(theme.muted))
                    .child(fl!("automations-timeline-empty")),
            );
        }
        let mut group = None;
        let mut rows = div().flex().flex_col();
        for (index, entry) in entries.into_iter().enumerate() {
            if group != Some(entry.group) {
                if group.is_some() {
                    body = body.child(rows);
                    rows = div().flex().flex_col();
                }
                group = Some(entry.group);
                let key = entry.group;
                rows = rows.child(
                    div()
                        .pb_1()
                        .debug_selector(move || format!("automations-group-{key:?}"))
                        .child(self.automation_title(key.title())),
                );
            }
            rows = rows.child(self.automation_timeline_row(index, entry, cx));
        }
        body.child(rows).child(
            div()
                .debug_selector(|| "automations-older".into())
                .flex()
                .justify_center()
                .text_size(small)
                .text_color(rgb(theme.muted))
                .child(fl!("automations-older-runs", count = RECENT_RUNS)),
        )
    }

    fn automation_timeline_row(
        &self,
        index: usize,
        entry: Entry,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = &self.theme;
        let small = self.automation_small();
        let hover = theme.active;
        let open = entry.open.clone();
        let review = entry.review.then(|| entry.open.clone());
        div()
            .id(SharedString::from(format!("automations-feed-{index}")))
            .debug_selector(|| "automations-feed".into())
            .py(px(6.))
            .flex()
            .items_center()
            .gap_3()
            .border_b_1()
            .border_color(rgb(theme.active))
            .cursor_pointer()
            .hover(move |style| style.bg(rgb(hover)))
            .child(
                div()
                    .w(px(96.))
                    .flex_none()
                    .text_size(small)
                    .text_color(rgb(theme.muted))
                    .child(entry.when),
            )
            .child(self.automation_icon(entry.look, 14.))
            .child(div().w(px(220.)).flex_none().truncate().child(entry.name))
            .child(self.automation_chip(entry.host, theme.subtext()))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(small)
                    .text_color(rgb(entry.look.detail_color(theme)))
                    .child(entry.text),
            )
            .children(review.map(|opened| {
                self.automation_button(
                    format!("automations-review-{index}"),
                    "icons/check.svg",
                    fl!("automations-review"),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.review_automation(opened.machine(), opened.path(), window, cx);
                }))
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open_automation(open.clone(), cx);
            }))
    }
}
