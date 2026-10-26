//! A card's toggle is the app's switch, and a click on it flips only the
//! card's local value.

use super::{
    overlay::{attach_with, open, poll},
    *,
};
use crate::chat::{prepared as prepared_body, widgets::Value};
use gpui::{Modifiers, TestAppContext};
use herdr_client::ConnectTarget;

#[gpui::test]
fn clicking_a_toggle_switch_flips_its_value_without_sending(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, ConnectTarget::Local);
    let program = Program::new();
    let body = "```herdr-ui\n{\"components\":[\
        {\"type\":\"toggle\",\"id\":\"dry\",\"label\":\"Dry run\",\"value\":false}\
    ]}\n```";
    let _stream = attach_with(
        &view,
        &program,
        snapshot(cursor(1, 0), vec![agent(1, body)]),
        cx,
    );
    let toggled = |cx: &mut gpui::VisualTestContext| {
        view.update(cx, |view, _| {
            let pane = view.chats.pane_mut("w1:p1").unwrap();
            let body = pane.prepared.body(EntryId(1)).unwrap();
            let prepared_body::Body::Message(parts) = body.as_ref() else {
                panic!("message");
            };
            let prepared_body::Part::Widget(document) = &parts[0] else {
                panic!("widget");
            };
            pane.widgets
                .form((EntryId(1), 0), document)
                .map(|form| form.values["dry"].clone())
        })
    };
    for expected in [true, false] {
        let switch = cx
            .debug_bounds("widget-w1:p1-1-0-0")
            .expect("the toggle's switch");
        cx.simulate_click(switch.center(), Modifiers::default());
        poll(&view, &program, cx);
        assert_eq!(toggled(cx), Some(Value::Toggle(expected)));
    }
    view.update(cx, |view, _| {
        assert!(
            view.chats
                .pane_mut("w1:p1")
                .unwrap()
                .session
                .resend()
                .is_empty()
        );
    });
}
