//! A right-click holds until no window has a working agent.
use super::*;
use crate::sidebar::layout_tests::{fixture_window, snapshot};
use herdr_client::protocol::AgentStatus;

fn until_agents_finish(cx: &mut gpui::TestAppContext, log: &Log, reports: &Reports) {
    cx.update(|cx| toggle_until_agents_finish_with(cx, holder(log), reports.sink()));
    cx.run_until_parked();
}

fn observe_in(cx: &mut gpui::TestAppContext, id: u64, work: Work, reports: &Reports) {
    cx.update(|cx| observe(window(id), work, cx, reports.sink()));
    cx.run_until_parked();
}

#[gpui::test]
fn nothing_to_wait_for_holds_nothing(cx: &mut gpui::TestAppContext) {
    let (log, reports) = (Log::default(), Reports::default());
    observe_in(cx, 1, Work::Idle, &reports);
    until_agents_finish(cx, &log, &reports);

    assert_eq!(cup_of(cx), Cup::Off);
    assert!(log.entries().is_empty());
    assert!(matches!(
        reports.take().as_slice(),
        [crate::Error::Caffeine(Error::NoWorkingAgents)]
    ));
}

#[gpui::test]
fn releases_once_every_window_is_idle(cx: &mut gpui::TestAppContext) {
    let (log, reports) = (Log::default(), Reports::default());
    observe_in(cx, 1, Work::Working, &reports);
    observe_in(cx, 2, Work::Working, &reports);
    until_agents_finish(cx, &log, &reports);
    assert_eq!(cup_of(cx), Cup::On(Mode::UntilAgentsFinish));

    observe_in(cx, 1, Work::Idle, &reports);
    // A reconnecting endpoint's agents are likely still running.
    observe_in(cx, 2, Work::Unknown, &reports);
    assert_eq!(cup_of(cx), Cup::On(Mode::UntilAgentsFinish));

    observe_in(cx, 2, Work::Idle, &reports);
    assert_eq!(cup_of(cx), Cup::Off);
    assert_eq!(commands(&log), [Command::Hold, Command::Release]);
    assert!(reports.take().is_empty());
}

#[gpui::test]
fn agents_finishing_while_the_hold_is_made_release_it(cx: &mut gpui::TestAppContext) {
    let (log, reports) = (Log::default(), Reports::default());
    observe_in(cx, 1, Work::Working, &reports);
    cx.update(|cx| {
        toggle_until_agents_finish_with(cx, holder(&log), reports.sink());
        observe(window(1), Work::Idle, cx, reports.sink());
    });
    cx.run_until_parked();

    assert_eq!(cup_of(cx), Cup::Off);
    assert_eq!(commands(&log), [Command::Hold, Command::Release]);
    assert!(reports.take().is_empty());
}

#[gpui::test]
fn an_indefinite_hold_ends_with_the_agents_once_asked(cx: &mut gpui::TestAppContext) {
    let (log, reports) = (Log::default(), Reports::default());
    observe_in(cx, 1, Work::Working, &reports);
    cx.update(|cx| toggle_with(cx, holder(&log), reports.sink()));
    cx.run_until_parked();

    // An indefinite hold outlasts the agents.
    observe_in(cx, 1, Work::Idle, &reports);
    assert_eq!(cup_of(cx), Cup::On(Mode::Indefinite));
    observe_in(cx, 1, Work::Working, &reports);

    // Switching keeps the existing hold rather than making another.
    until_agents_finish(cx, &log, &reports);
    assert_eq!(cup_of(cx), Cup::On(Mode::UntilAgentsFinish));
    assert_eq!(commands(&log), [Command::Hold]);

    observe_in(cx, 1, Work::Idle, &reports);
    assert_eq!(cup_of(cx), Cup::Off);
    assert_eq!(commands(&log), [Command::Hold, Command::Release]);
    assert!(reports.take().is_empty());
}

#[gpui::test]
fn either_click_ends_an_until_agents_hold(cx: &mut gpui::TestAppContext) {
    let (log, reports) = (Log::default(), Reports::default());
    observe_in(cx, 1, Work::Working, &reports);
    until_agents_finish(cx, &log, &reports);
    cx.update(|cx| toggle_with(cx, holder(&log), reports.sink()));
    cx.run_until_parked();
    assert_eq!(cup_of(cx), Cup::Off);

    until_agents_finish(cx, &log, &reports);
    assert_eq!(cup_of(cx), Cup::On(Mode::UntilAgentsFinish));
    until_agents_finish(cx, &log, &reports);
    assert_eq!(cup_of(cx), Cup::Off);
    assert_eq!(
        commands(&log),
        [
            Command::Hold,
            Command::Release,
            Command::Hold,
            Command::Release
        ]
    );
    assert!(reports.take().is_empty());
}

#[gpui::test]
fn a_window_reports_its_working_agents(cx: &mut gpui::TestAppContext) {
    let handle = cx.add_window(fixture_window);
    handle
        .update(cx, |view, window, cx| {
            let id = window.window_handle().window_id();
            let working = |cx: &mut App| cx.default_global::<Caffeine>().working.contains(&id);
            let mut busy = snapshot(1);
            busy.agents[0].agent_status = AgentStatus::Working;
            view.endpoints[0]
                .live
                .apply(herdr_client::ClientEvent::Snapshot(Arc::new(busy.clone())));
            view.observe_agent_work(window, cx);
            assert!(working(cx));

            busy.revision += 1;
            for agent in &mut busy.agents {
                agent.agent_status = AgentStatus::Done;
            }
            view.endpoints[0]
                .live
                .apply(herdr_client::ClientEvent::Snapshot(Arc::new(busy)));
            view.observe_agent_work(window, cx);
            assert!(!working(cx));
        })
        .unwrap();
}

#[gpui::test]
fn asking_for_on_keeps_the_hold_after_the_agents_finish(cx: &mut gpui::TestAppContext) {
    let (log, reports) = (Log::default(), Reports::default());
    observe_in(cx, 1, Work::Working, &reports);
    until_agents_finish(cx, &log, &reports);
    assert!(cx.update(hold_indefinitely));
    assert_eq!(cup_of(cx), Cup::On(Mode::Indefinite));
    // Only a hold until agents finish is turned: nothing else changes.
    assert!(!cx.update(hold_indefinitely));
    observe_in(cx, 1, Work::Idle, &reports);
    assert_eq!(cup_of(cx), Cup::On(Mode::Indefinite));
    assert_eq!(commands(&log), [Command::Hold]);
}
