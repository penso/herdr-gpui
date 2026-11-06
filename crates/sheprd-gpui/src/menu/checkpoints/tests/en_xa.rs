//! The `en-XA` leak test (`crate::i18n::leak_tests`) for the checkpoints
//! dialog: its listing and its restore confirmation draw only translated
//! messages, except for the checkpoints' names, their days and times.
use super::local;
use crate::{
    checkpoint::{Checkpoint, Diff, Listing},
    i18n::leak_tests::{assert_translated, draw, under_en_xa},
    menu::page::WorkspaceMenuAction,
};
use gpui::{px, size};

fn checkpoint(id: &str) -> Checkpoint {
    Checkpoint {
        id: id.into(),
        created: 0,
        label: format!("turn {id}"),
        branch: Some("worktree/sidebar-child".into()),
        diff: Diff {
            files: 2,
            additions: 7,
            deletions: 1,
        },
    }
}

#[test]
fn the_checkpoints_dialog_shows_only_messages_under_en_xa() {
    under_en_xa("checkpoints en-XA leak", |cx, recorder| {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        cx.simulate_resize(size(px(1200.), px(900.)));
        draw(cx);
        recorder.take();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                local(view);
                view.open_workspace_menu("w4", Default::default(), window, cx);
                view.activate_workspace_menu(WorkspaceMenuAction::Checkpoints, window, cx);
            })
        });
        draw(cx);
        assert_translated("Checkpoints (loading)", &recorder.take(), &[]);

        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                let checkpoints = view.checkpoints.view.as_mut().unwrap();
                checkpoints.listing = Listing::Ready(vec![checkpoint("2"), checkpoint("1")]);
                cx.notify();
            })
        });
        draw(cx);
        // The day the checkpoints were taken, in 1970, is a date chrono
        // writes in English until dates are localized.
        let day = super::super::timeline::day_heading(0, 3 << 30, &chrono::Local);
        let data: Vec<String> = ["turn 2", "turn 1"]
            .map(str::to_owned)
            .into_iter()
            .chain(day)
            .collect();
        assert_translated("Checkpoints", &recorder.take(), &data);

        cx.simulate_keystrokes("down down enter");
        draw(cx);
        assert_translated("Checkpoints > Restore", &recorder.take(), &data);
    });
}
