use super::*;

static SENT: AtomicUsize = AtomicUsize::new(0);

fn recording_daemon(_: &Path, request: &Request) -> Result<Response, control::Error> {
    match request {
        Request::Status => Ok(Response::Status(status(0))),
        Request::Devices => Ok(Response::Devices { devices: vec![] }),
        Request::Indicators { set } => {
            assert_eq!(set, "tiles");
            SENT.fetch_add(1, Ordering::SeqCst);
            Ok(Response::Done)
        }
        _ => Ok(Response::Done),
    }
}

#[gpui::test]
fn the_status_marks_go_to_the_daemon_once_per_change(cx: &mut TestAppContext) {
    cx.update(|cx| {
        Supervisor::fixture(cx, on(), recording_daemon);
        Supervisor::apply_indicators(cx, "tiles");
    });
    for _ in 0..3 {
        cx.update(|cx| cx.global_mut::<Supervisor>().poke = true);
        cx.update(Supervisor::tick);
        cx.run_until_parked();
    }
    assert_eq!(SENT.load(Ordering::SeqCst), 1);
    // The same set again changes nothing.
    cx.update(|cx| Supervisor::apply_indicators(cx, "tiles"));
    cx.update(Supervisor::tick);
    cx.run_until_parked();
    assert_eq!(SENT.load(Ordering::SeqCst), 1);
}
