//! The `en-XA` leak test (`crate::i18n::leak_tests`) for the pane menu and
//! its pages: each draws only translated messages, except for the pane's
//! label, the places it can move to, and the processes under it.
use super::*;
use crate::i18n::leak_tests::{assert_translated, draw, under_en_xa};
use crate::processes::{Entry, Identity, Listing, Row, Update, Watch};

/// The window, showing the fixture with a second pane, "inactive", whose
/// menu the test opens. The host is this computer, where moves can run.
fn window(cx: &mut TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        view.live.snapshot = Some(Arc::new(snapshot()));
        view.endpoints[0].connection.target = herdr_client::ConnectTarget::Local;
        view
    });
    cx.simulate_resize(size(px(1000.), px(800.)));
    draw(cx);
    (view, cx)
}

fn open(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, action: Option<Action>) {
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.dismiss_menu(window, cx);
            v.open_pane_menu("inactive", point(px(20.), px(20.)), window, cx);
            if let Some(action) = action {
                v.activate_pane_menu(action, window, cx);
            }
        })
    });
    draw(cx);
}

/// The workspace and tab names the move picker lists.
fn places() -> Vec<String> {
    let snapshot = snapshot();
    snapshot
        .workspaces
        .iter()
        .map(|workspace| workspace.label.clone())
        .chain(snapshot.tabs.iter().map(|tab| tab.label.clone()))
        .collect()
}

#[test]
fn the_pane_menu_and_its_pages_show_only_messages_under_en_xa() {
    under_en_xa("pane menu en-XA leak", |cx, recorder| {
        let (view, cx) = window(cx);
        recorder.take();
        open(&view, cx, None);
        assert_translated("Pane menu", &recorder.take(), &[]);

        open(&view, cx, Some(Action::Rename));
        assert_translated(
            "Pane menu > Rename",
            &recorder.take(),
            &["Original label".into()],
        );

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            open(&view, cx, Some(Action::MoveTo));
            assert_translated("Pane menu > Move to", &recorder.take(), &places());
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let _ = places;
    });
}

fn row(pid: u32, depth: usize) -> Row {
    Row {
        entry: Entry {
            identity: Identity { pid, started: 1 },
            parent: None,
            name: format!("proc{pid}"),
            command: format!("proc{pid} --flag"),
            cpu: Some(1.5),
            memory: 3 << 20,
        },
        depth,
        foreground: pid == 20,
    }
}

#[test]
fn the_pane_processes_and_their_confirmation_show_only_messages_under_en_xa() {
    under_en_xa("pane processes en-XA leak", |cx, recorder| {
        let (view, cx) = window(cx);
        let (watch, updates, _commands) = Watch::fake();
        recorder.take();
        cx.update(|window, cx| {
            view.update(cx, |v, cx| {
                v.open_pane_menu("inactive", point(px(10.), px(10.)), window, cx);
                v.show_pane_processes(watch, cx);
            })
        });
        draw(cx);
        assert_translated("Pane menu > Processes (listing)", &recorder.take(), &[]);

        updates
            .send(Update::Listing(Ok(Listing {
                rows: vec![row(10, 0), row(20, 1), row(30, 2)],
                hidden: 2,
                cpu: 4.5,
                memory: 9 << 20,
            })))
            .unwrap();
        cx.update(|_, cx| view.update(cx, |v, cx| v.poll_pane_processes(cx)));
        draw(cx);
        let processes: Vec<String> = [10, 20, 30]
            .into_iter()
            .flat_map(|pid| [format!("proc{pid}"), format!("proc{pid} --flag")])
            .collect();
        assert_translated("Pane menu > Processes", &recorder.take(), &processes);

        cx.simulate_keystrokes("down down space enter");
        draw(cx);
        assert_translated(
            "Pane menu > Processes > End",
            &recorder.take(),
            &["proc20".into()],
        );
    });
}
