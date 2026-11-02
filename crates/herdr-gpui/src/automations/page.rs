//! A script's or run's page beside the list: its breadcrumb (Hosts or
//! Timeline, the script, the run), the list toggle, and Close.

mod run;
mod script;

#[cfg(test)]
pub(crate) use {run::total_usage, script::daily_tokens};

use super::{HostState, Opened};
use crate::{HerdrWindow, i18n::fl};
use gpui::{prelude::*, *};
use herdr_automation::report::ScriptStatus;

/// The longest output line drawn; the rest of a longer one is cut.
const LINE: usize = 1000;

impl HerdrWindow {
    pub(super) fn render_automation_page(
        &self,
        opened: &Opened,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let found = self.automations.host(opened.machine()).and_then(|host| {
            let script = host
                .status()?
                .scripts
                .iter()
                .find(|script| script.path == opened.path())?;
            Some((host, script))
        });
        let body = match (opened, found) {
            (Opened::Script { .. }, Some((host, script))) => self
                .automation_script_page(host, script, cx)
                .into_any_element(),
            (Opened::Run { run, .. }, Some((host, script))) => self
                .automation_run_page(host, script, *run, cx)
                .into_any_element(),
            (_, None) => div()
                .flex()
                .flex_col()
                .gap_3()
                .child(self.automation_crumbs(opened, None, cx))
                .child(
                    div()
                        .text_color(rgb(self.theme.muted))
                        .child(fl!("automations-page-gone")),
                )
                .into_any_element(),
        };
        div()
            .id("automations-page")
            .debug_selector(|| "automations-page".into())
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.automations.page_scroll)
            .p_5()
            .child(body)
    }

    /// The page's top line: the list toggle, the trail back, and Close.
    pub(super) fn automation_crumbs(
        &self,
        opened: &Opened,
        script: Option<(&HostState, &ScriptStatus)>,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let mut trail: Vec<(SharedString, Option<Opened>)> =
            vec![(self.automations.mode.title().into(), None)];
        if let Some((host, script)) = script {
            trail.push((
                script.name.clone().into(),
                Some(Opened::Script {
                    machine: host.machine.clone(),
                    path: script.path.clone(),
                }),
            ));
        }
        if let Opened::Run { run, .. } = opened {
            trail.push((
                fl!("automations-run-number", run = *run).into(),
                Some(opened.clone()),
            ));
        }
        let mut row = div()
            .flex()
            .items_center()
            .gap_2()
            .min_w_0()
            .text_size(self.automation_small())
            .text_color(rgb(theme.muted))
            .child(
                div()
                    .id("automations-toggle-list")
                    .debug_selector(|| "automations-toggle-list".into())
                    .flex_none()
                    .cursor_pointer()
                    .child(
                        svg()
                            .path("icons/panel-left.svg")
                            .size(px(14.))
                            .text_color(rgb(theme.subtext())),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.automations.list_hidden = !this.automations.list_hidden;
                        cx.notify();
                    })),
            );
        let last = trail.len() - 1;
        for (index, (label, target)) in trail.into_iter().enumerate() {
            if index > 0 {
                row = row.child(div().flex_none().child("›"));
            }
            let current = index == last;
            row = row.child(
                div()
                    .id(SharedString::from(format!("automations-crumb-{index}")))
                    .debug_selector(move || format!("automations-crumb-{index}"))
                    .min_w_0()
                    .truncate()
                    .when(!current, |crumb| {
                        crumb.cursor_pointer().text_color(rgb(theme.primary()))
                    })
                    .when(current, |crumb| crumb.text_color(rgb(theme.foreground)))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| match &target {
                        Some(target) => this.open_automation(target.clone(), cx),
                        None => this.close_automation(cx),
                    })),
            );
        }
        row.child(div().flex_1()).child(
            div()
                .id("automations-close")
                .debug_selector(|| "automations-close".into())
                .flex_none()
                .cursor_pointer()
                .child(
                    svg()
                        .path("icons/close.svg")
                        .size(px(12.))
                        .text_color(rgb(theme.subtext())),
                )
                .on_click(cx.listener(|this, _, _, cx| this.close_automation(cx))),
        )
    }

    /// A block of program output in the terminal font: its last `lines`
    /// lines, each stripped of control characters, since it is untrusted.
    pub(super) fn automation_output(&self, text: &str, lines: usize, selector: String) -> Div {
        use crate::fonts::StyledFont;
        let theme = &self.theme;
        let terminal = &self.config.terminal;
        let all: Vec<&str> = text.lines().collect();
        let skipped = all.len().saturating_sub(lines);
        let shown = all[skipped..]
            .iter()
            .map(|line| crate::notifications::safe_text(line, LINE));
        div()
            .debug_selector(move || selector.clone())
            .p_3()
            .rounded(px(crate::config::corners::CONTROL))
            .bg(rgb(theme.surface))
            .text_font(terminal)
            .text_size(px(terminal.size - 1.))
            .text_color(rgb(theme.subtext()))
            .flex()
            .flex_col()
            .min_w_0()
            .when(skipped > 0, |block| {
                block.child(
                    div()
                        .text_color(rgb(theme.muted))
                        .child(fl!("automations-earlier-lines", count = skipped)),
                )
            })
            .children(shown.map(|line| {
                div()
                    .min_w_0()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child(line)
            }))
    }

    /// A plan, every line of it, long lines wrapped: what approving it
    /// approves is never cut. Its text was cleaned when it was read.
    pub(super) fn automation_plan(&self, plan: &str, selector: &'static str) -> Div {
        use crate::fonts::StyledFont;
        let theme = &self.theme;
        let terminal = &self.config.terminal;
        div()
            .debug_selector(move || selector.into())
            .p_3()
            .rounded(px(crate::config::corners::CONTROL))
            .bg(rgb(theme.surface))
            .text_font(terminal)
            .text_size(px(terminal.size - 1.))
            .text_color(rgb(theme.subtext()))
            .flex()
            .flex_col()
            .min_w_0()
            .children(plan.lines().map(|line| {
                // An empty line still takes its height.
                div().min_w_0().child(if line.is_empty() {
                    " ".to_owned()
                } else {
                    line.to_owned()
                })
            }))
    }
}
