use super::*;

fn state(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> (Filter, String, Identity) {
    view.read_with(cx, |view, cx| {
        let palette = view.menu.palette.as_ref().unwrap();
        (
            palette.filter,
            palette.search.read(cx).text().to_owned(),
            palette.selected_identity().unwrap(),
        )
    })
}

#[gpui::test]
fn resume_reopens_the_last_query_filter_and_selection(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_palette(Filter::Commands, window, cx)
        });
        window.draw(cx).clear(cx);
    });
    cx.simulate_input("font");
    cx.simulate_keystrokes("down");
    let left = state(&view, cx);
    assert_eq!(left.1, "font");
    assert_ne!(
        left.2,
        view.read_with(cx, |view, _| {
            let palette = view.menu.palette.as_ref().unwrap();
            palette.entries[palette.filtered[0].index].action.identity()
        }),
        "the second row was selected"
    );
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| assert!(view.menu.palette.is_none()));

    cx.update(|window, cx| view.update(cx, |view, cx| view.resume_palette(window, cx)));
    cx.run_until_parked();
    assert_eq!(state(&view, cx), left);

    // Resuming again after closing keeps working; it is not a one-shot.
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| view.update(cx, |view, cx| view.resume_palette(window, cx)));
    cx.run_until_parked();
    assert_eq!(state(&view, cx), left);
}

#[gpui::test]
fn resume_without_history_opens_a_fresh_palette(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| view.update(cx, |view, cx| view.resume_palette(window, cx)));
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        let palette = view.menu.palette.as_ref().unwrap();
        assert_eq!(palette.filter, Filter::All);
        assert_eq!(palette.search.read(cx).text(), "");
        assert_eq!(palette.selected, 0);
    });
}

#[gpui::test]
fn a_new_query_or_move_drops_the_resumed_row(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    let restore = |view: &Entity<SheprdWindow>, cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.menu.palette.as_ref().unwrap().restore.clone()
        })
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_palette(Filter::Commands, window, cx)
        });
        window.draw(cx).clear(cx);
    });
    cx.simulate_input("font");
    cx.simulate_keystrokes("down escape");
    // A row not listed yet stays wanted until something supersedes it.
    let missing = Identity::Configured("not-listed".into());
    for supersede in ["ctrl-n", "x"] {
        cx.update(|window, cx| view.update(cx, |view, cx| view.resume_palette(window, cx)));
        cx.run_until_parked();
        view.update(cx, |view, _| {
            view.menu.palette.as_mut().unwrap().restore = Some(missing.clone())
        });
        cx.update(|_, cx| view.update(cx, |view, cx| view.rank_palette(Selection::Keep, cx)));
        assert_eq!(restore(&view, cx), Some(missing.clone()));
        if supersede == "x" {
            cx.simulate_input(supersede);
        } else {
            cx.simulate_keystrokes(supersede);
        }
        assert_eq!(restore(&view, cx), None, "{supersede}");
        cx.simulate_keystrokes("escape");
    }
}

/// A click activates a row other than the selected one; resume reopens on
/// the row clicked.
#[gpui::test]
fn resume_reopens_on_a_clicked_row(cx: &mut TestAppContext) {
    // A row on another host needs only that host's snapshot to be chosen.
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        let mut remote = crate::endpoint::Endpoint::new(
            "ssh:box".into(),
            "Box".into(),
            sheprd_client::ConnectTarget::Ssh {
                target: "unused".into(),
                session: "default".into(),
            },
            true,
        );
        remote.live.snapshot = Some(Arc::new(snapshot()));
        remote.live.status = crate::state::ConnectionStatus::Connected;
        view.endpoints.push(remote);
        view
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_palette(Filter::Navigation, window, cx)
        });
        window.draw(cx).clear(cx);
    });
    let (action, clicked) = view.read_with(cx, |view, _| {
        let palette = view.menu.palette.as_ref().unwrap();
        assert_eq!(palette.selected, 0);
        let entry = palette
            .filtered
            .iter()
            .skip(1)
            .map(|hit| &palette.entries[hit.index])
            .find(|entry| {
                matches!(
                    &entry.action,
                    Action::Go {
                        endpoint,
                        target: NavigationTarget::Pane(_),
                        ..
                    } if endpoint == "ssh:box"
                )
            })
            .unwrap();
        (entry.action.clone(), entry.action.identity())
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.activate_palette(action, window, cx)));
    view.read_with(cx, |view, _| assert!(view.menu.palette.is_none()));
    cx.update(|window, cx| view.update(cx, |view, cx| view.resume_palette(window, cx)));
    cx.run_until_parked();
    assert_eq!(state(&view, cx).2, clicked);
}
