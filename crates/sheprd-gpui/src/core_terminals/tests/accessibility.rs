use super::worker::{Scripted, size, until};
use crate::{
    core_terminals::{
        CoreTerminalView, Look,
        worker::{self, Command},
    },
    i18n::{fl, leak_tests::under_en_xa},
    terminal_painter::TerminalPainter,
};
use gpui::{
    Context, Entity, EntityInputHandler as _, TestAppContext, VisualTestContext, Window,
    accesskit::{Action, ActionData, Node, Role},
    div, hsla,
    prelude::*,
};
use sheprd_backend::{ClientPaneInputEvent, Role as AttachRole, TerminalId};
use std::{cell::RefCell, rc::Rc};

fn look() -> Look {
    Look {
        background: hsla(0., 0., 0., 1.),
        cell_height: 17.,
        cell_width: 8.5,
        font: gpui::font("Menlo"),
        ground: Default::default(),
        option_as_alt: Default::default(),
        padding: Default::default(),
    }
}

fn open(
    cx: &mut TestAppContext,
    observe: bool,
) -> (Entity<CoreTerminalView>, &mut VisualTestContext, Scripted) {
    open_on(cx, Scripted::default(), observe)
}

fn open_on(
    cx: &mut TestAppContext,
    backend: Scripted,
    observe: bool,
) -> (Entity<CoreTerminalView>, &mut VisualTestContext, Scripted) {
    let link = worker::spawn(
        backend.clone(),
        TerminalId::new("t1").unwrap(),
        size(80, 24),
        AttachRole::Active,
    )
    .unwrap();
    let painter = Rc::new(RefCell::new(TerminalPainter::default()));
    let (view, cx) = cx.add_window_view(|_, cx| {
        CoreTerminalView::new(
            TerminalId::new("t1").unwrap(),
            link,
            look(),
            painter,
            observe,
            cx,
        )
    });
    (view, cx, backend)
}

fn node(view: &CoreTerminalView, window: &Window, cx: &Context<CoreTerminalView>) -> Node {
    let element = view
        .accessible(div().id("terminal"), window, cx)
        .into_element();
    let mut node = Node::new(element.a11y_role().unwrap());
    element.write_a11y_info(&mut node);
    node
}

fn value(text: &str) -> ActionData {
    ActionData::Value(text.into())
}

/// The pastes the terminal received, once `count` of them arrived.
fn pastes(backend: &Scripted, count: usize) -> Vec<String> {
    until(|| {
        let pastes: Vec<String> = backend
            .received
            .lock()
            .unwrap()
            .iter()
            .filter_map(|command| match command {
                Command::Input(ClientPaneInputEvent::Paste(text)) => Some(text.clone()),
                _ => None,
            })
            .collect();
        (pastes.len() >= count).then_some(pastes)
    })
}

#[gpui::test]
fn reads_as_a_terminal_and_offers_insertion_only_while_it_takes_typing(cx: &mut TestAppContext) {
    let (view, cx, _) = open(cx, false);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let terminal = node(view, window, cx);
            assert_eq!(terminal.role(), Role::Terminal);
            assert_eq!(
                terminal.label(),
                Some(fl!("window-input-terminal").as_str())
            );
            assert!(!terminal.supports_action(Action::SetValue));

            window.focus(view.focus_handle(), cx);
            assert!(node(view, window, cx).supports_action(Action::SetValue));

            view.replace_and_mark_text_in_range(None, "候補", None, window, cx);
            assert!(!node(view, window, cx).supports_action(Action::SetValue));
            view.unmark_text(window, cx);
            assert!(node(view, window, cx).supports_action(Action::SetValue));

            let field = cx.focus_handle();
            window.focus(&field, cx);
            assert!(!node(view, window, cx).supports_action(Action::SetValue));
        })
    });
}

#[test]
fn the_label_follows_the_language() {
    under_en_xa("core terminal label en-XA leak", |cx, _| {
        let (view, cx, _) = open(cx, false);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let terminal = node(view, window, cx);
                let label = terminal.label().unwrap();
                assert!(sheprd_i18n::pseudo::is_marked(label), "{label}");
            })
        });
    });
}

#[gpui::test]
fn a_mirror_never_offers_insertion(cx: &mut TestAppContext) {
    let (view, cx, _) = open(cx, true);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            window.focus(view.focus_handle(), cx);
            let terminal = node(view, window, cx);
            assert_eq!(terminal.role(), Role::Terminal);
            assert!(!terminal.supports_action(Action::SetValue));
        })
    });
}

#[gpui::test]
fn a_failed_terminal_stops_offering_insertion(cx: &mut TestAppContext) {
    let (view, cx, backend) = open_on(cx, Scripted::failing_attach(), false);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            window.focus(view.focus_handle(), cx);
            assert!(node(view, window, cx).supports_action(Action::SetValue));
        })
    });
    until(|| {
        cx.update(|_, cx| view.update(cx, |view, cx| view.poll(cx)))
            .then_some(())
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(!view.ended());
            assert!(!node(view, window, cx).supports_action(Action::SetValue));
            view.insert_accessible_text(Some(&value("after the failure")), window);
        })
    });
    assert!(backend.received.lock().unwrap().is_empty());
}

#[gpui::test]
fn insertion_pastes_only_text_set_while_the_view_takes_typing(cx: &mut TestAppContext) {
    let (view, cx, backend) = open(cx, false);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.insert_accessible_text(Some(&value("before focus")), window);
            window.focus(view.focus_handle(), cx);
            view.insert_accessible_text(None, window);
            view.insert_accessible_text(Some(&value("")), window);
            view.insert_accessible_text(Some(&ActionData::NumericValue(1.)), window);
            view.insert_accessible_text(Some(&value("hello 世界\nsecond line")), window);
            let field = cx.focus_handle();
            window.focus(&field, cx);
            view.insert_accessible_text(Some(&value("after focus left")), window);
            window.focus(view.focus_handle(), cx);
            view.insert_accessible_text(Some(&value("last")), window);
        })
    });
    assert_eq!(pastes(&backend, 2), ["hello 世界\nsecond line", "last"]);
}
