//! The real window scheduler, with jobs held explicitly until completion.

use super::*;
use crate::{
    HerdrWindow,
    browser::{Pick, Store},
    chat::visual::Output,
    window::MockPeer,
};
use gpui::{Entity, VisualTestContext};
use herdr_client::ConnectTarget;
use std::rc::Rc;

type Pending = (Target, Rc<()>, visual::Job);

fn targets() -> Vec<Target> {
    [
        ("w1:p1", "0123456789abcdef0123456789abcdef"),
        ("w1:p2", "fedcba9876543210fedcba9876543210"),
    ]
    .into_iter()
    .map(|(pane, endpoint)| Target {
        endpoint: endpoint.parse().unwrap(),
        existing: None,
        pane_id: pane.into(),
        route: Route::Local,
    })
    .collect()
}

fn install(view: &mut HerdrWindow, targets: Vec<Target>) {
    view.chats
        .reconcile(targets, Instant::now(), &mut |_, _, _| {
            Err(super::super::Error::Closed)
        });
    for pane in view.chats.panes_mut() {
        pane.session.receive(welcome()).unwrap();
        pane.session
            .receive(snapshot(cursor(1, 0), vec![agent(1, "Prepared source")]))
            .unwrap();
        for n in 0..3 {
            let id = VisualId::of((&pane.target.pane_id, n));
            pane.prepared.visuals.want(
                id,
                Source::Code {
                    language: "rust".into(),
                    source: format!("let value_{n} = {n};").into(),
                },
            );
        }
    }
}

fn open(cx: &mut TestAppContext) -> (Entity<HerdrWindow>, MockPeer, &mut VisualTestContext) {
    let (view, peer, cx) = overlay::open(cx, ConnectTarget::Local);
    view.update(cx, |view, _| {
        install(view, targets());
        let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
        let mut second = surface.panes[0].clone();
        second.pane_id = "w1:p2".into();
        surface.panes.push(second);
    });
    (view, peer, cx)
}

fn take(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Vec<Pending> {
    view.update(cx, |view, cx| view.take_chat_visual_jobs(cx))
}

fn finish(view: &Entity<HerdrWindow>, job: Pending, cx: &mut VisualTestContext) {
    let (target, generation, job) = job;
    view.update(cx, |view, cx| {
        view.finish_chat_visual(
            (&target, &generation),
            job.id,
            job.ticket,
            Ok(Output::Code(Vec::new())),
            cx,
        );
    });
}

#[gpui::test]
fn hidden_raw_and_unready_panes_wait_until_visible(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx);
    let surface = view.read_with(cx, |view, _| view.live.surface.clone());
    view.update(cx, |view, _| view.live.surface = None);
    assert!(take(&view, cx).is_empty());
    view.update(cx, |view, _| {
        view.live.surface = surface;
        view.chats.set_raw("w1:p1", true);
        Arc::make_mut(view.live.surface.as_mut().unwrap())
            .panes
            .retain(|pane| pane.pane_id == "w1:p1");
    });
    assert!(take(&view, cx).is_empty());
    view.update(cx, |view, _| {
        view.chats.set_raw("w1:p1", false);
        view.config.features.acp_chats = false;
    });
    assert!(take(&view, cx).is_empty());
    view.update(cx, |view, _| view.config.features.acp_chats = true);
    let jobs = take(&view, cx);
    assert_eq!(jobs.len(), visual::MAX_RUNNING);
    assert!(jobs.iter().all(|(target, _, _)| target.pane_id == "w1:p1"));
    for job in jobs {
        finish(&view, job, cx);
    }
    view.update(cx, |view, _| {
        Arc::make_mut(view.live.surface.as_mut().unwrap()).panes[0].pane_id = "w1:p2".into();
    });
    let jobs = take(&view, cx);
    assert_eq!(jobs.len(), visual::MAX_RUNNING);
    assert!(jobs.iter().all(|(target, _, _)| target.pane_id == "w1:p2"));
    for job in jobs {
        finish(&view, job, cx);
    }
}

#[gpui::test]
fn popups_and_nonterminal_tabs_pause_visual_admission(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx);
    view.update(cx, |view, _| {
        let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
        surface.popup = Some(Box::new(herdr_client::protocol::ClientShellPopupSurface {
            terminal_id: "synthetic-popup".into(),
            title: "Question".into(),
            width: None,
            height: None,
            frame: surface.frame.clone(),
            mouse_reporting: false,
            sgr_pixel_mouse: false,
            pixel_width: 640,
            pixel_height: 320,
        }));
    });
    assert!(take(&view, cx).is_empty());
    // Linux has no embedded-browser command. Install the same unsaved
    // blank-tab model directly, without opening a page or a native browser.
    let (group, page) = view.update(cx, |view, cx| {
        Arc::make_mut(view.live.surface.as_mut().unwrap()).popup = None;
        let (scope, workspace) = view.browser_key().unwrap();
        let page = Store::update(cx, |store| store.open(scope, &workspace, None, None)).unwrap();
        let layout = view.ensure_layout().unwrap();
        let group = layout.active();
        layout.choose(group, Pick::Page(page));
        assert_eq!(
            view.group_shown(group, cx),
            crate::browser::Shown::Page(page)
        );
        (group, page)
    });
    assert!(take(&view, cx).is_empty());
    view.update(cx, |view, cx| {
        view.ensure_layout().unwrap().forget(&Pick::Page(page));
        assert_eq!(view.group_shown(group, cx), crate::browser::Shown::Terminal);
    });
    let jobs = take(&view, cx);
    assert_eq!(jobs.len(), visual::MAX_RUNNING);
    for job in jobs {
        finish(&view, job, cx);
    }
}

#[gpui::test]
fn visible_splits_share_two_slots_and_hidden_inflight_jobs_still_count(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx);
    let mut jobs = take(&view, cx);
    assert_eq!(jobs.len(), visual::MAX_RUNNING);
    assert_ne!(jobs[0].0.pane_id, jobs[1].0.pane_id);
    assert!(take(&view, cx).is_empty());
    let hidden = jobs[0].0.pane_id.clone();
    view.update(cx, |view, _| {
        Arc::make_mut(view.live.surface.as_mut().unwrap())
            .panes
            .retain(|pane| pane.pane_id != hidden);
    });
    assert!(take(&view, cx).is_empty());
    finish(&view, jobs.remove(0), cx);
    let resumed = take(&view, cx);
    assert_eq!(resumed.len(), 1);
    assert_ne!(resumed[0].0.pane_id, hidden);
    assert!(take(&view, cx).is_empty());
    for job in jobs.into_iter().chain(resumed) {
        finish(&view, job, cx);
    }
    assert_eq!(
        view.read_with(cx, |view, _| view.chats.active_visual_jobs),
        0
    );
}

#[gpui::test]
fn a_third_visible_split_gets_the_next_slot_before_the_first_panes_second_job(
    cx: &mut TestAppContext,
) {
    let (view, _peer, cx) = open(cx);
    view.update(cx, |view, _| {
        let mut targets = targets();
        targets.push(Target {
            endpoint: "00112233445566778899aabbccddeeff".parse().unwrap(),
            existing: None,
            pane_id: "w1:p3".into(),
            route: Route::Local,
        });
        install(view, targets);
        let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
        let mut third = surface.panes[0].clone();
        third.pane_id = "w1:p3".into();
        surface.panes.push(third);
    });
    let mut jobs = take(&view, cx);
    assert_eq!(jobs.len(), visual::MAX_RUNNING);
    assert_eq!(jobs[0].0.pane_id, "w1:p1");
    assert_eq!(jobs[1].0.pane_id, "w1:p2");
    finish(&view, jobs.remove(0), cx);
    let next = take(&view, cx);
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].0.pane_id, "w1:p3");
    assert!(take(&view, cx).is_empty());
    for job in jobs.into_iter().chain(next) {
        finish(&view, job, cx);
    }
}

#[gpui::test]
fn removed_and_recreated_panes_do_not_free_slots_or_accept_old_results(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx);
    let old = take(&view, cx);
    view.update(cx, |view, _| {
        view.chats
            .reconcile(Vec::new(), Instant::now(), &mut |_, _, _| {
                panic!("no targets")
            });
        install(view, targets());
    });
    assert!(take(&view, cx).is_empty());
    let ids: Vec<_> = old
        .iter()
        .map(|(target, _, job)| (target.pane_id.clone(), job.id))
        .collect();
    for job in old {
        finish(&view, job, cx);
    }
    view.read_with(cx, |view, _| {
        assert_eq!(view.chats.active_visual_jobs, 0);
        for (pane, id) in ids {
            assert!(
                view.chats
                    .pane(&pane)
                    .unwrap()
                    .prepared
                    .visuals
                    .code(id)
                    .is_none()
            );
        }
    });
    let fresh = take(&view, cx);
    assert_eq!(fresh.len(), visual::MAX_RUNNING);
    for job in fresh {
        finish(&view, job, cx);
    }
}
