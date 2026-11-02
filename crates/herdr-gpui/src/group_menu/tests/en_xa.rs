//! The `en-XA` leak test (`crate::i18n::leak_tests`) for a group's "…"
//! menu: it draws only translated messages, except for product and agent
//! names, ports, and the keys bound to its rows.
use super::*;
use crate::i18n::leak_tests::{assert_translated, under_en_xa};

#[test]
fn the_group_menu_shows_only_messages_under_en_xa() {
    under_en_xa("group menu en-XA leak", |cx, recorder| {
        let (view, cx) = window(cx);
        open_pages(&view, cx, 2);
        let group = groups(&view, cx)[0];
        draw(cx);
        recorder.take();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_group_menu(group, Point::default(), window, cx)
            })
        });
        draw(cx);
        let data = cx.update(|_, cx| {
            let view = view.read(cx);
            let keys = [Command::NewBrowserTab, Command::OpenCode, Command::OpenPx0]
                .map(|command| view.keymap().primary(command).to_owned());
            let rows = view
                .group_actions(group, cx)
                .into_iter()
                .flat_map(|action| match action {
                    Action::VsCode | Action::Px0 | Action::Chat(_) => vec![action.label().into()],
                    Action::Port { process, link, .. } => vec![link.label(), process],
                    _ => Vec::new(),
                });
            keys.into_iter().chain(rows).collect::<Vec<String>>()
        });
        assert_translated("Group menu", &recorder.take(), &data);
    });
}
