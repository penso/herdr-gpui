//! The picker's keyboard selection follows its row, not its place, while an
//! unfolded device's sessions arrive above it.
use super::*;
use crate::menu::devices::picker::Line;

fn session(name: &str) -> herdr_client::LocalSession {
    herdr_client::LocalSession {
        name: name.into(),
        state: herdr_client::SessionState::Running,
        socket: herdr_client::ConnectTarget::Session {
            name: name.into(),
            development: true,
        }
        .socket_path()
        .unwrap(),
    }
}

#[gpui::test]
fn arriving_sessions_leave_the_keys_on_the_device_below(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, _| {
        view.endpoints.push(crate::endpoint::Endpoint::new(
            "ssh:build".into(),
            "Build".into(),
            herdr_client::ConnectTarget::Ssh {
                target: "build.invalid".into(),
                session: "default".into(),
            },
            true,
        ));
        view.sessions.entries = Vec::new();
        view.menu.page = Some(Page::Devices);
        // This machine unfolded while its scan is still out; Build is below.
        view.unfold_device(0);
        let build = |line: &Line| matches!(line, Line::Device(1));
        view.menu.selected = view.picker_ordinal(build);
        let before = view.menu.selected;

        // Its sessions arrive, adding rows above Build.
        assert!(view.keep_picker_selection(|view| {
            view.sessions.entries = vec![session("default"), session("work")];
            true
        }));
        assert_ne!(view.menu.selected, before, "Build moved down");
        assert_eq!(view.menu.selected, view.picker_ordinal(build));

        // A row that goes away leaves nothing selected, not its neighbour.
        let work = |line: &Line| {
            matches!(line, Line::Session(crate::menu::sessions::Entry::Row {
                row: crate::menu::sessions::Row::Local(name), ..
            }) if name == "work")
        };
        view.menu.selected = view.picker_ordinal(work);
        assert!(view.menu.selected.is_some());
        view.keep_picker_selection(|view| {
            view.sessions.entries = vec![session("default")];
            true
        });
        assert_eq!(view.menu.selected, None);
    });
}
