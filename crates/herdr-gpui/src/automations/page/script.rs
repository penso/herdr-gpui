//! A script's page: what it does (its plan, as approving it showed), what
//! it may use, its recent runs, the tokens they used over the last week,
//! and what can be done to it.

use super::super::{Action, HostState, Opened, format, model};
use crate::{HerdrWindow, i18n::fl};
use chrono::TimeZone;
use gpui::{prelude::*, *};
use herdr_automation::report::{RunSummary, ScriptState, ScriptStatus};

/// The days the token chart covers, today last.
const DAYS: usize = 7;

impl HerdrWindow {
    pub(super) fn automation_script_page(
        &self,
        host: &HostState,
        script: &ScriptStatus,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let opened = Opened::Script {
            machine: host.machine.clone(),
            path: script.path.clone(),
        };
        let runs: Vec<&RunSummary> = host
            .status()
            .map(|status| {
                status
                    .runs
                    .iter()
                    .filter(|run| run.script == script.path)
                    .collect()
            })
            .unwrap_or_default();
        let mut page = div()
            .debug_selector(|| "automations-script-page".into())
            .flex()
            .flex_col()
            .gap_4()
            .child(self.automation_crumbs(&opened, Some((host, script)), cx))
            .child(self.automation_script_heading(host, script, cx));
        if let Some(reason) = &script.reason {
            let color = match script.state {
                ScriptState::Invalid | ScriptState::AwaitingApproval => {
                    model::look(script).color(theme)
                }
                _ => theme.subtext(),
            };
            page = page.child(div().text_color(rgb(color)).child(reason.clone()));
        }
        page = page
            .child(
                div()
                    .text_size(self.automation_small())
                    .text_color(rgb(theme.muted))
                    .truncate()
                    .child(match &script.repo {
                        Some(repo) => format!("{} · {repo}", script.path),
                        None => script.path.clone(),
                    }),
            )
            .child(self.automation_title(fl!("automations-plan")))
            .child(match &script.plan {
                Some(plan) => self.automation_plan(plan, "automations-plan"),
                None => div()
                    .text_color(rgb(theme.muted))
                    .child(fl!("automations-no-plan")),
            });
        if let Some(caps) = script.caps {
            page = page.child(
                div()
                    .text_size(self.automation_small())
                    .text_color(rgb(theme.subtext()))
                    .child(model::caps(caps)),
            );
        }
        page = page
            .child(self.automation_title(fl!("automations-recent-runs")))
            .child(self.automation_script_runs(host, script, &runs, cx));
        let days = daily_tokens(&runs, format::now(), &chrono::Local);
        if days.iter().any(|(_, tokens)| *tokens > 0) {
            let total: u64 = days.iter().map(|(_, tokens)| tokens).sum();
            page = page
                .child(self.automation_title(fl!(
                    "automations-tokens-days",
                    days = DAYS,
                    tokens = format::tokens(total)
                )))
                .child(self.automation_token_bars(&days));
        }
        page
    }

    fn automation_script_heading(
        &self,
        host: &HostState,
        script: &ScriptStatus,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let path = script.path.clone();
        let mut heading = div()
            .flex()
            .items_center()
            .gap_2()
            .min_w_0()
            .child(self.automation_icon(model::look(script), 16.))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(self.config.ui.size + 2.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(script.name.clone()),
            )
            .child(self.automation_chip(host.label.clone(), theme.subtext()))
            .child(div().flex_1());
        if script.state == ScriptState::AwaitingApproval {
            let machine = host.machine.clone();
            let path = path.clone();
            heading = heading.child(
                self.automation_button(
                    "automations-review",
                    "icons/check.svg",
                    fl!("automations-review-approve"),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.review_automation(&machine, &path, window, cx);
                })),
            );
        }
        if script.state == ScriptState::Active && script.running.is_none() {
            heading = heading.child(self.automation_action_button(
                "automations-run-now",
                "icons/play.svg",
                fl!("automations-run-now"),
                host,
                Action::RunNow { path: path.clone() },
                cx,
            ));
        }
        if let Some(run) = script.running {
            heading = heading.child(self.automation_action_button(
                "automations-stop",
                "icons/x.svg",
                fl!("automations-stop-run"),
                host,
                Action::Stop { run },
                cx,
            ));
        }
        match script.state {
            ScriptState::Active => {
                heading = heading.child(self.automation_action_button(
                    "automations-pause",
                    "icons/status-outline-unknown.svg",
                    fl!("automations-pause"),
                    host,
                    Action::Pause { path },
                    cx,
                ));
            }
            ScriptState::Paused => {
                heading = heading.child(self.automation_action_button(
                    "automations-resume",
                    "icons/play.svg",
                    fl!("automations-resume"),
                    host,
                    Action::Resume { path },
                    cx,
                ));
            }
            ScriptState::AwaitingApproval | ScriptState::Disabled | ScriptState::Invalid => {}
        }
        heading
    }

    fn automation_script_runs(
        &self,
        host: &HostState,
        script: &ScriptStatus,
        runs: &[&RunSummary],
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let small = self.automation_small();
        if runs.is_empty() {
            return div()
                .text_size(small)
                .text_color(rgb(theme.muted))
                .child(fl!("automations-no-runs"));
        }
        let now = format::now();
        let mut list = div().flex().flex_col();
        for run in runs {
            let target = Opened::Run {
                machine: host.machine.clone(),
                path: script.path.clone(),
                run: run.id,
            };
            let hover = theme.active;
            list = list.child(
                div()
                    .id(SharedString::from(format!("automations-run-{}", run.id)))
                    .debug_selector(|| "automations-run".into())
                    .py(px(4.))
                    .flex()
                    .items_center()
                    .gap_3()
                    .border_b_1()
                    .border_color(rgb(theme.active))
                    .cursor_pointer()
                    .hover(move |style| style.bg(rgb(hover)))
                    .child(self.automation_icon(model::run_look(run.state), 13.))
                    .child(
                        div()
                            .w(px(150.))
                            .flex_none()
                            .text_size(small)
                            .child(format!(
                                "#{} · {}",
                                run.id,
                                format::when(run.started_at, now)
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(small)
                            .text_color(rgb(theme.subtext()))
                            .child(model::summary(run, now)),
                    )
                    .child(
                        div()
                            .w(px(48.))
                            .flex_none()
                            .flex()
                            .justify_end()
                            .text_size(small)
                            .child(run.tokens.map_or_else(|| "—".to_owned(), format::tokens)),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_automation(target.clone(), cx);
                    })),
            );
        }
        list
    }

    fn automation_token_bars(&self, days: &[(String, u64)]) -> Div {
        let theme = &self.theme;
        let most = days
            .iter()
            .map(|(_, tokens)| *tokens)
            .max()
            .unwrap_or(0)
            .max(1);
        div()
            .debug_selector(|| "automations-token-bars".into())
            .h(px(64.))
            .flex()
            .items_end()
            .gap_2()
            .children(days.iter().map(|(day, tokens)| {
                let share = *tokens as f32 / most as f32;
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .w_full()
                            .h(px((44. * share).max(1.)))
                            .rounded(px(crate::config::corners::SMALL))
                            .bg(rgb(if *tokens > 0 {
                                theme.primary()
                            } else {
                                theme.active
                            })),
                    )
                    .child(
                        div()
                            .text_size(px(self.config.ui.size - 2.))
                            .text_color(rgb(theme.muted))
                            .child(day.clone()),
                    )
            }))
    }
}

/// Tokens the runs used on each of the last [`DAYS`] days in `zone`, by
/// the day each run started, oldest first. Only runs the host still keeps
/// count, so the chart is a floor, not an account.
pub(crate) fn daily_tokens<Tz: TimeZone>(
    runs: &[&RunSummary],
    now: i64,
    zone: &Tz,
) -> Vec<(String, u64)>
where
    Tz::Offset: std::fmt::Display,
{
    let Some(today) = format::day(now, zone) else {
        return Vec::new();
    };
    (0..DAYS)
        .rev()
        .map(|back| {
            let day = today - chrono::Days::new(back as u64);
            let tokens = runs
                .iter()
                .filter(|run| format::day(run.started_at, zone) == Some(day))
                .filter_map(|run| run.tokens)
                .sum();
            (day.format("%a").to_string(), tokens)
        })
        .collect()
}
