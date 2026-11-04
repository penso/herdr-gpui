use super::super::tests::posted;
use super::*;
use gpui::TestAppContext;
use std::{
    cell::RefCell,
    future::poll_fn,
    rc::Rc,
    task::{Poll, Waker},
};

#[derive(Default)]
struct Reply {
    result: Option<Result<u32, Error>>,
    waker: Option<Waker>,
}

struct Request {
    notification: SystemNotification,
    replaces: u32,
    reply: Rc<RefCell<Reply>>,
}

#[derive(Clone, Default)]
struct Server(Rc<RefCell<Vec<Request>>>);

impl Server {
    fn post(&self, tag: &str, body: &str, cx: &mut TestAppContext) {
        let calls = self.0.clone();
        cx.update(|cx| {
            cx.global_mut::<Registry>().record(posted(tag));
            post_with(
                SystemNotification {
                    tag: tag.to_owned().into(),
                    title: "test".into(),
                    body: body.to_owned().into(),
                    actions: Vec::new(),
                },
                cx,
                move |notification, replaces| {
                    let reply = Rc::new(RefCell::new(Reply::default()));
                    calls.borrow_mut().push(Request {
                        notification,
                        replaces,
                        reply: reply.clone(),
                    });
                    poll_fn(move |cx| {
                        let mut reply = reply.borrow_mut();
                        match reply.result.take() {
                            Some(result) => Poll::Ready(result),
                            None => {
                                reply.waker = Some(cx.waker().clone());
                                Poll::Pending
                            }
                        }
                    })
                },
            );
        });
        cx.run_until_parked();
    }

    fn reply(&self, index: usize, result: Result<u32, Error>, cx: &mut TestAppContext) {
        let reply = self.0.borrow()[index].reply.clone();
        let waker = {
            let mut reply = reply.borrow_mut();
            reply.result = Some(result);
            reply.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
        cx.run_until_parked();
    }

    fn sent(&self) -> Vec<(String, String, u32)> {
        self.0
            .borrow()
            .iter()
            .map(|call| {
                (
                    call.notification.tag.to_string(),
                    call.notification.body.to_string(),
                    call.replaces,
                )
            })
            .collect()
    }
}

fn fixture(cx: &mut TestAppContext) -> Server {
    cx.update(|cx| cx.set_global(Registry::default()));
    Server::default()
}

#[gpui::test]
fn overlapping_posts_wait_for_the_reply_and_replace_with_the_latest_body(cx: &mut TestAppContext) {
    let server = fixture(cx);
    server.post("a", "first", cx);
    server.post("a", "second", cx);
    server.post("a", "latest", cx);
    assert_eq!(server.sent(), [("a".into(), "first".into(), 0)]);

    server.reply(0, Ok(7), cx);
    assert_eq!(
        server.sent(),
        [
            ("a".into(), "first".into(), 0),
            ("a".into(), "latest".into(), 7)
        ]
    );
    server.post("a", "last", cx);
    cx.update(|cx| {
        assert_eq!(
            cx.global::<Registry>()
                .find_bus_id(7)
                .map(|p| p.tag.as_ref()),
            Some("a")
        );
    });
    server.reply(1, Ok(8), cx);
    assert_eq!(server.sent().last(), Some(&("a".into(), "last".into(), 8)));
    cx.update(|cx| assert!(cx.global::<Registry>().find_bus_id(7).is_none()));
    server.reply(2, Ok(8), cx);
    cx.update(|cx| assert!(!cx.global::<Registry>().outbox.running));

    // A later event starts a fresh worker and still replaces the last banner.
    server.post("a", "after idle", cx);
    assert_eq!(
        server.sent().last(),
        Some(&("a".into(), "after idle".into(), 8))
    );
    server.reply(3, Ok(8), cx);
}

#[gpui::test]
fn rejection_keeps_the_previous_id_and_continues_other_tags(cx: &mut TestAppContext) {
    let server = fixture(cx);
    cx.update(|cx| {
        let registry = cx.global_mut::<Registry>();
        registry.record(posted("a"));
        registry.set_bus_id("a", 7);
    });
    server.post("a", "rejected", cx);
    server.post("a", "retry", cx);
    server.post("b", "other", cx);
    assert_eq!(server.sent(), [("a".into(), "rejected".into(), 7)]);
    server.reply(
        0,
        Err(super::super::zbus::Error::Failure("rejected".into()).into()),
        cx,
    );
    assert_eq!(server.sent().last(), Some(&("a".into(), "retry".into(), 7)));
    server.reply(1, Ok(8), cx);
    assert_eq!(server.sent().last(), Some(&("b".into(), "other".into(), 0)));
    server.reply(2, Ok(9), cx);
    cx.update(|cx| {
        let registry = cx.global::<Registry>();
        assert_eq!(registry.find_bus_id(8).map(|p| p.tag.as_ref()), Some("a"));
        assert_eq!(registry.find_bus_id(9).map(|p| p.tag.as_ref()), Some("b"));
        assert!(!registry.outbox.running);
    });
}

#[gpui::test]
fn a_stalled_server_keeps_only_bounded_recent_targets(cx: &mut TestAppContext) {
    let server = fixture(cx);
    let limit = super::super::super::REGISTRY_LIMIT;
    server.post("evicted", "first", cx);
    for index in 0..limit * 2 {
        server.post(&index.to_string(), "pending", cx);
    }
    assert_eq!(server.sent(), [("evicted".into(), "first".into(), 0)]);
    cx.update(|cx| {
        let registry = cx.global::<Registry>();
        assert_eq!(registry.posted.len(), limit);
        assert_eq!(registry.outbox.pending.len(), limit);
    });
    server.reply(0, Ok(7), cx);
    cx.update(|cx| assert!(cx.global::<Registry>().find_bus_id(7).is_none()));
    for index in 0..limit {
        assert_eq!(
            server.sent().last(),
            Some(&((limit + index).to_string(), "pending".into(), 0))
        );
        server.reply(index + 1, Ok(100 + index as u32), cx);
    }
    cx.update(|cx| assert!(!cx.global::<Registry>().outbox.running));
}
