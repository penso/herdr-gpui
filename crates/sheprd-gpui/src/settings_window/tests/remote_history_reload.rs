//! Reload on the Tabs page re-reads every SSH host, so a failed host retries.
use super::*;
use crate::herdr_settings::RemotePaneHistory;
use crate::settings_window::remote_history::HostState;
use sheprd_client::ConnectTarget;

#[gpui::test]
fn reload_on_tabs_rereads_a_failed_host(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| source.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let reads = Arc::new(AtomicUsize::new(0));
    let counted = reads.clone();
    let (view, cx) = cx.add_window_view(move |_, cx| {
        let mut view = SettingsWindow::new(weak, cx);
        view.section = Section::Tabs;
        view.remote_history.read = Arc::new(move |_, _| {
            // The first read fails like an unreachable host; later ones work.
            if counted.fetch_add(1, Ordering::SeqCst) == 0 {
                return Err(herdr_settings::Error::RemoteConflict.into());
            }
            Ok(RemotePaneHistory::parse_text(Some(
                "[experimental]\npane_history = true\n".into(),
            ))?)
        });
        view
    });
    cx.update(|_, cx| {
        source
            .update(cx, |window, _, cx| {
                let endpoint = &mut window.endpoints[0];
                endpoint.connection.target = ConnectTarget::Ssh {
                    target: "box".into(),
                    session: "default".into(),
                };
                endpoint.live.status = crate::state::ConnectionStatus::Connected;
                cx.notify();
            })
            .unwrap()
    });
    view.update(cx, |view, cx| view.sync_remote_history(false, cx));
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let hosts = view.remote_history.hosts();
        assert_eq!(hosts.len(), 1);
        assert!(matches!(hosts[0].state, HostState::Failed(_)));
    });

    view.update(cx, |view, cx| view.reload_everything(fixture_load, cx));
    view.read_with(cx, |view, _| {
        assert!(matches!(
            view.remote_history.hosts()[0].state,
            HostState::Loading
        ));
    });
    cx.run_until_parked();
    assert_eq!(reads.load(Ordering::SeqCst), 2);
    view.read_with(cx, |view, _| {
        assert!(matches!(
            &view.remote_history.hosts()[0].state,
            HostState::Ready(history) if history.enabled
        ));
    });
}
