//! The `en-XA` leak test for the Overview: the empty page, the list with
//! its stats, actions, and snooze menu, the Snoozed, Settled, and Closed
//! shelves opened, and the host column.
use super::page::{draw, row, window};
use crate::{
    i18n::leak_tests::{assert_translated, uncut, under_en_xa},
    overview::{Marks, State},
    sidebar::ActivityPanel,
    state::ConnectionStatus,
};
use gpui::{px, size};
use std::time::{Duration, SystemTime};

#[test]
fn the_overview_shows_only_messages_and_its_agents() {
    under_en_xa("Overview en-XA leak", |cx, recorder| {
        let (view, cx) = window(cx);
        cx.simulate_resize(size(px(2400.), px(1200.)));
        let connected = cx.update(|_, cx| {
            view.update(cx, |view, _| {
                std::mem::replace(&mut view.live.status, ConnectionStatus::Disconnected)
            })
        });
        // The window around the page is other modules' text.
        draw(cx);
        recorder.take();

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.show_activity_panel(ActivityPanel::Overview, window, cx);
                view.poll_overview(cx);
            })
        });
        draw(cx);
        let host = cx.update(|_, cx| view.read(cx).endpoints[0].label.clone());
        assert_translated(
            "empty Overview",
            &uncut(recorder.take()),
            std::slice::from_ref(&host),
        );

        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.live.status = connected;
                view.poll_overview(cx);
            })
        });
        draw(cx);
        // Agents, their checkouts and states, which the sidebar names, and
        // the host.
        let data = cx.update(|_, cx| {
            view.read(cx)
                .overview_rows(cx)
                .into_iter()
                .flat_map(|row| {
                    [row.title, row.detail, row.workspace, row.host]
                        .into_iter()
                        .chain(row.branch)
                })
                .chain([host.clone()])
                .collect::<Vec<_>>()
        });
        assert_translated("Overview list", &uncut(recorder.take()), &data);

        let blocked = row(&view, cx, State::Blocked);
        let done = row(&view, cx, State::Done);
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                let rows = view.overview_rows(cx);
                let key = |selector: &str| {
                    rows.iter()
                        .find(|row| row.key.selector() == selector)
                        .map(|row| (row.key.clone(), row.state))
                        .unwrap()
                };
                let (snoozed, snoozed_state) = key(&blocked);
                let (settled, settled_state) = key(&done);
                let until = SystemTime::now() + Duration::from_secs(2 * 86_400);
                Marks::update(cx, |marks| {
                    marks.snooze(&snoozed, snoozed_state, until);
                    marks.settle(&settled, settled_state);
                    marks.toggle_pin(&settled);
                });
                view.overview.snoozed_open = true;
                view.overview.settled_open = true;
                let working = view
                    .overview_rows(cx)
                    .into_iter()
                    .find(|row| row.state == State::Working)
                    .unwrap();
                view.overview.menu = Some(working.key);
                cx.notify();
            })
        });
        draw(cx);
        assert_translated(
            "Overview shelves and snooze menu",
            &uncut(recorder.take()),
            &data,
        );

        // A closed agent waits under Closed, with Reopen and Forget.
        super::closed_page::close_working(&view, cx);
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.overview.menu = None;
                view.overview.closed_open = true;
                cx.notify();
            })
        });
        draw(cx);
        assert_translated("Overview Closed shelf", &uncut(recorder.take()), &data);
    });
}
