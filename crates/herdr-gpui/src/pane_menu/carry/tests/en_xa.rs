//! The `en-XA` leak test (`crate::i18n::leak_tests`) for the pane menu's
//! move to another device: its review, progress, and failure, and the flash
//! a finished move leaves, each only translated messages around the names
//! and paths the move reports.
use super::*;
use crate::i18n::leak_tests::{assert_translated, draw, uncut, under_en_xa};
use crate::teleport::{AgentKind, Error, Warning, carry::session_review};

fn window(cx: &mut TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        view.live.snapshot = Some(Arc::new(crate::pane_menu::tests::snapshot()));
        view.endpoints[0].connection.target = ConnectTarget::Local;
        view
    });
    cx.simulate_resize(size(px(1000.), px(800.)));
    draw(cx);
    (view, cx)
}

/// A carry at `stage` whose worker stays reachable through the returned
/// sender, so polling neither fails it nor moves it on.
fn waiting(stage: Stage) -> (Carry, mpsc::Sender<Event>) {
    let (sender, events) = mpsc::channel();
    let mut carry = abandoned(stage);
    carry.events = events;
    (carry, sender)
}

/// Opens the move's page at `stage`, returning the worker's sender.
fn show(
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
    stage: Stage,
) -> mpsc::Sender<Event> {
    let (carry, sender) = waiting(stage);
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.dismiss_menu(window, cx);
            v.open_pane_menu("inactive", point(px(20.), px(20.)), window, cx);
            v.pane_carry = Some(carry);
            v.menu.page = Some(Page::CarryPane);
            cx.notify();
        })
    });
    draw(cx);
    sender
}

fn repository(change: impl FnOnce(&mut carry::RepositoryReview)) -> Stage {
    let mut review = shell_review(place("ssh:there"));
    if let Review::Repository(review) = &mut review {
        change(review);
    }
    Stage::Ready(review)
}

#[test]
fn moving_a_pane_to_another_device_shows_only_messages_under_en_xa() {
    under_en_xa("pane move en-XA leak", |cx, recorder| {
        let (view, cx) = window(cx);
        recorder.take();

        let _worker = show(&view, cx, Stage::Reviewing);
        assert_translated("Move pane (reviewing)", &uncut(recorder.take()), &[]);

        let stages = [
            ("shell into a new worktree", repository(|_| {})),
            (
                "agent session resumed in an open space",
                repository(|review| {
                    review.landing = Landing::Space {
                        workspace_id: "w2".into(),
                        checkout: "/repo".into(),
                        head: "abc".into(),
                        fast_forward: true,
                    };
                    review.action = carry::Action::Resume(AgentKind::Claude);
                }),
            ),
            (
                "program missing there",
                repository(|review| {
                    review.landing = Landing::Worktree {
                        branch: Some("feature".into()),
                    };
                    review.action = carry::Action::Missing("aider".into());
                }),
            ),
            (
                "session only",
                Stage::Ready(session_review(
                    place("ssh:there"),
                    "~/notes",
                    AgentKind::Codex,
                )),
            ),
        ];
        for (surface, stage) in stages {
            let _worker = show(&view, cx, stage);
            assert_translated(
                &format!("Move pane review: {surface}"),
                &uncut(recorder.take()),
                &[],
            );
        }

        for step in [None, Some(Step::Capture), Some(Step::Sessions)] {
            let _worker = show(&view, cx, Stage::Moving(step));
            assert_translated(
                &format!("Move pane progress at {step:?}"),
                &uncut(recorder.take()),
                &[],
            );
        }

        let _worker = show(&view, cx, Stage::Failed(Error::WorkerStopped.localize()));
        assert_translated("Move pane failed", &uncut(recorder.take()), &[]);
    });
}

/// Every warning a move can end with, each against its own cause.
fn warnings() -> Vec<Warning> {
    vec![
        Warning::AgentStartedAfresh {
            agent: AgentKind::Claude,
            source: Error::PaneGone,
        },
        Warning::EnvrcTracked,
        Warning::GitHubAccess {
            source: Error::InvalidToken,
        },
        Warning::NoHandoffNote {
            pane: Some("w1:p2".into()),
            source: Error::PaneGone,
        },
        Warning::NoHandoffNote {
            pane: None,
            source: Error::PaneGone,
        },
        Warning::ProgramMissing {
            program: "aider".into(),
            host: "there".into(),
        },
        Warning::SourcePaneOpen {
            source: Error::WorkerStopped,
        },
    ]
}

#[test]
fn a_move_that_finished_with_a_warning_flashes_only_messages_under_en_xa() {
    under_en_xa("pane move warnings en-XA leak", |cx, recorder| {
        let (view, cx) = window(cx);
        for warning in warnings() {
            let surface = format!("Move pane flash: {warning:?}");
            let worker = show(&view, cx, Stage::Moving(Some(Step::Launch)));
            recorder.take();
            worker
                .send(Event::Moved(Ok(Carried {
                    endpoint_id: "ssh:there".into(),
                    workspace_id: "w9".into(),
                    pane_id: "w9:p1".into(),
                    warnings: vec![warning],
                })))
                .unwrap();
            cx.update(|window, cx| view.update(cx, |v, cx| v.poll_carry(window, cx)));
            draw(cx);
            // Closing the menu shows the fixture daemon's announcement again.
            let announcement = ["Announcement".to_owned(), "Body".to_owned()];
            assert_translated(&surface, &uncut(recorder.take()), &announcement);
            cx.update(|_, cx| {
                view.update(cx, |v, _| {
                    v.teleport_follow = None;
                })
            });
        }
    });
}
