use super::*;
use crate::{
    browser::{Feedback, Shown},
    controls::Command,
};
use sheprd_client::protocol::AgentStatus;

fn shown(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> Vec<Shown> {
    draw(cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        view.group_slots()
            .into_iter()
            .map(|slot| view.group_shown(slot.id, cx))
            .collect()
    })
}

/// Opens a page over the agent's tab.
fn open_page(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> crate::browser::TabId {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::NewBrowserTab, window, cx)
        })
    });
    let page = crate::browser::TabId::test(0);
    assert_eq!(shown(view, cx), [Shown::Page(page)]);
    page
}

fn tick(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.poll_deliveries(cx);
            view.poll_reveal(window, cx);
        })
    });
}

fn set_status(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, status: AgentStatus) {
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            Arc::make_mut(view.live.snapshot.as_mut().unwrap()).agents[0].agent_status = status;
        });
    });
}

/// An agent waiting in `browser feedback --wait` takes the notes at once,
/// and its tab replaces the page so the user sees it at work.
#[gpui::test]
fn a_waiting_agent_comes_into_view(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    with_idle_agent(&view, cx);
    let target = recipient(&view, cx);
    open_page(&view, cx);
    cx.update(|_, cx| {
        cx.default_global::<Feedback>()
            .set_waiting(vec![target.clone()]);
    });

    assert_eq!(deliver(&view, cx, None, "w0:p1"), Some(NotesTo::Agent));
    tick(&view, cx);
    assert_eq!(shown(&view, cx), [Shown::Terminal]);
    cx.update(|_, cx| {
        let feedback = cx.default_global::<Feedback>();
        feedback.set_waiting(Vec::new());
        assert!(feedback.take(&target).is_some());
    });
}

/// Notes queued for a busy agent leave the page until they are handed
/// over; a paste that fails keeps them, and the page with them.
#[gpui::test]
fn queued_notes_reveal_nothing_until_handed_over(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    with_idle_agent(&view, cx);
    set_status(&view, cx, AgentStatus::Working);
    let page = open_page(&view, cx);

    assert_eq!(deliver(&view, cx, None, "w0:p1"), Some(NotesTo::Agent));
    tick(&view, cx);
    assert_eq!(shown(&view, cx), [Shown::Page(page)], "held while busy");

    // This fixture has no connection, so the paste fails.
    set_status(&view, cx, AgentStatus::Idle);
    tick(&view, cx);
    view.read_with(cx, |view, _| assert_eq!(view.deliveries.len(), 0));
    assert_eq!(shown(&view, cx), [Shown::Page(page)]);
}

/// Notes queued for a busy agent that then waits in `browser feedback
/// --wait` reach it there, and bring it into view.
#[gpui::test]
fn queued_notes_taken_by_a_waiting_agent_reveal_it(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    with_idle_agent(&view, cx);
    set_status(&view, cx, AgentStatus::Working);
    let page = open_page(&view, cx);
    let target = recipient(&view, cx);

    assert_eq!(deliver(&view, cx, None, "w0:p1"), Some(NotesTo::Agent));
    tick(&view, cx);
    assert_eq!(shown(&view, cx), [Shown::Page(page)], "held while busy");

    cx.update(|_, cx| {
        cx.default_global::<Feedback>()
            .set_waiting(vec![target.clone()]);
    });
    tick(&view, cx);
    assert_eq!(shown(&view, cx), [Shown::Terminal]);
    cx.update(|_, cx| {
        let feedback = cx.default_global::<Feedback>();
        feedback.set_waiting(Vec::new());
        assert!(feedback.take(&target).is_some());
    });
}

/// Notes the agent does not get leave the page where it is.
#[gpui::test]
fn kept_notes_leave_the_page_shown(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    with_idle_agent(&view, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            Arc::make_mut(view.live.snapshot.as_mut().unwrap())
                .agents
                .clear();
        })
    });
    let page = open_page(&view, cx);

    assert_eq!(deliver(&view, cx, None, "w0:p1"), Some(NotesTo::Kept));
    tick(&view, cx);
    assert_eq!(shown(&view, cx), [Shown::Page(page)]);
}

fn focus(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, workspace: &str, tab: &str) {
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.focused_workspace_id = Some(workspace.into());
            snapshot.focused_tab_id = Some(tab.into());
        });
    });
}

/// An agent in another workspace whose group shows a page: arriving there
/// keeps the page, so its tab is chosen once the daemon is there.
#[gpui::test]
fn an_agent_in_another_workspace_replaces_its_page_on_arrival(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    with_idle_agent(&view, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            for (workspace, tab) in snapshot
                .panes
                .iter_mut()
                .map(|pane| (&mut pane.workspace_id, &mut pane.tab_id))
                .chain(
                    snapshot
                        .agents
                        .iter_mut()
                        .map(|agent| (&mut agent.workspace_id, &mut agent.tab_id)),
                )
            {
                *workspace = "w1".into();
                *tab = "t1".into();
            }
        });
    });
    focus(&view, cx, "w1", "t1");
    let page = open_page(&view, cx);
    focus(&view, cx, "w0", "t0");
    let target = recipient(&view, cx);
    cx.update(|_, cx| {
        cx.default_global::<Feedback>()
            .set_waiting(vec![target.clone()]);
    });

    assert_eq!(deliver(&view, cx, None, "w0:p1"), Some(NotesTo::Agent));
    tick(&view, cx);
    assert_eq!(shown(&view, cx), [Shown::Terminal], "w0 is left alone");

    // The daemon arrives in the agent's workspace, where the page was.
    focus(&view, cx, "w1", "t1");
    assert_eq!(shown(&view, cx), [Shown::Page(page)]);
    tick(&view, cx);
    assert_eq!(shown(&view, cx), [Shown::Terminal]);
    cx.update(|_, cx| {
        cx.default_global::<Feedback>().set_waiting(Vec::new());
    });
}
