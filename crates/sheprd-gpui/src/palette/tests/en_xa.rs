//! The `en-XA` leak test (`crate::i18n::leak_tests`) for Go To: each row's
//! badge, the agent filter, and the preview's facts are messages; the names
//! and paths of the host's workspaces, tabs, panes, and agents are data.
use super::*;
use crate::i18n::leak_tests::{assert_translated, draw, under_en_xa};

#[test]
fn go_to_shows_only_translated_text_under_en_xa() {
    under_en_xa("Go To en-XA leak", |cx, recorder| {
        let (view, cx) = cx.add_window_view(fixture_window);
        cx.simulate_resize(size(px(1400.), px(900.)));
        draw(cx);
        recorder.take();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_palette(Filter::Navigation, window, cx)
            })
        });
        draw(cx);
        let data = view.read_with(cx, |view, _| {
            let palette = view.menu.palette.as_ref().unwrap();
            let snapshot = view.live.snapshot.as_ref().unwrap();
            let mut data: Vec<String> = palette
                .entries
                .iter()
                .filter(|entry| matches!(entry.action, Action::Go { .. }))
                .flat_map(|entry| [entry.label.to_string(), entry.detail.to_string()])
                .collect();
            data.extend(snapshot.workspaces.iter().map(|w| w.label.clone()));
            data.extend(snapshot.tabs.iter().map(|t| t.label.clone()));
            data.extend(
                snapshot
                    .panes
                    .iter()
                    .flat_map(|p| [p.cwd.clone(), p.foreground_cwd.clone()])
                    .flatten(),
            );
            data.extend(snapshot.workspaces.iter().filter_map(|w| w.branch.clone()));
            // Agents' names, and the state labels their integrations report.
            for agent in &snapshot.agents {
                data.push(crate::sidebar::agent_name(agent).to_owned());
                data.extend(agent.state_labels.iter().map(|(_, label)| label.clone()));
            }
            data
        });
        assert_translated("Go To", &recorder.take(), &data);
    });
}
