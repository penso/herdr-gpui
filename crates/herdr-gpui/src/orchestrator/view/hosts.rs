//! The dispatch dialog's host picker: smart dispatch's ranking of every
//! connected host by the room it has, as tiles, best first. The window
//! pushes the candidates while the dialog is open.

use super::{
    OrchestratorView,
    dispatch::Dialog,
    look::{GREEN, RED, YELLOW},
};
use crate::{
    config::corners,
    dispatch::{Candidate, Repository, rank},
};
use gpui::{prelude::*, *};

/// Tiles shown; the ranking puts the hosts worth picking first.
const TILES: usize = 4;

impl OrchestratorView {
    /// The candidate hosts best first, offline ones dropped.
    pub(super) fn ranked_hosts(&self) -> Vec<&Candidate> {
        rank(&self.hosts)
            .into_iter()
            .map(|index| &self.hosts[index])
            .filter(|candidate| candidate.online)
            .take(TILES)
            .collect()
    }

    /// The tiles, or nothing when this is the only host.
    pub(super) fn render_hosts(
        &self,
        dialog: &Dialog,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let hosts = self.ranked_hosts();
        if hosts.len() < 2 {
            return None;
        }
        let look = &self.look;
        let theme = &look.theme;
        let chosen = dialog.host.as_deref();
        let tiles = hosts.into_iter().enumerate().map(|(rank, candidate)| {
            let on = match chosen {
                Some(endpoint) => endpoint == candidate.endpoint_id,
                None => candidate.current,
            };
            let spare = candidate.load.and_then(|load| load.spare_cores());
            let cores = candidate
                .load
                .and_then(|load| load.cores)
                .unwrap_or(1)
                .max(1) as f32;
            let busy = spare.map_or(0., |spare| (1. - spare / cores).clamp(0., 1.));
            let hue = if busy > 0.7 {
                RED
            } else if busy > 0.4 {
                YELLOW
            } else {
                GREEN
            };
            let note = match candidate.repository {
                Repository::Present => "repository here",
                Repository::Missing => "clones it first",
                Repository::Unknown => "",
            };
            let endpoint = candidate.endpoint_id.clone();
            let current = candidate.current;
            div()
                .id(("orchestrator-host", rank))
                .flex_1()
                .min_w(px(110.))
                .p_2()
                .flex()
                .flex_col()
                .gap_1()
                .rounded(px(corners::CONTROL))
                .border_1()
                .border_color(rgb(if on { theme.primary() } else { theme.active }))
                .when(on, |el| el.bg(rgb(theme.primary_wash())))
                .cursor_pointer()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(look.icon("icons/devices.svg", 12., theme.subtext()))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(candidate.label.clone()),
                        )
                        .when(rank == 0, |el| el.child(look.muted("best"))),
                )
                .child(
                    div()
                        .w_full()
                        .h(px(4.))
                        .rounded_full()
                        .bg(rgb(theme.active))
                        .child(
                            div()
                                .h_full()
                                .w(relative(busy))
                                .rounded_full()
                                .bg(rgb(look.hue(hue))),
                        ),
                )
                .child(look.muted(format!("{} working", candidate.working)))
                .when(!note.is_empty(), |el| el.child(look.muted(note)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(dialog) = &mut this.dialog {
                        dialog.host = (!current).then(|| endpoint.clone());
                    }
                    this.select_agent_host(cx);
                    cx.notify();
                }))
        });
        Some(
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .children(tiles)
                .into_any_element(),
        )
    }
}
