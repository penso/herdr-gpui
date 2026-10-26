use super::{
    overlay::{attach_with, open, poll},
    *,
};
use crate::chat::prepared::{Body, Part};
use gpui::{Modifiers, ScrollDelta, ScrollWheelEvent, TestAppContext, point, px};

#[test]
fn an_indented_markdown_image_remains_literal_code() {
    for indentation in ["    ", "\t"] {
        let literal = "![plot](https://example.org/plot.png)";
        let (prepared, _) = visual::message(&format!("{indentation}{literal}\n"));
        let body = prepared.body(EntryId(1)).unwrap();
        let Body::Message(parts) = &*body else {
            panic!("message")
        };
        assert!(
            matches!(&parts[..], [Part::Rich(blocks)] if matches!(&blocks[..], [rich::Block::Code { source, .. }] if source.trim_end() == literal))
        );
    }
}

#[gpui::test]
fn long_code_scrolls_within_its_pane_and_copy_preserves_tabs(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, herdr_client::ConnectTarget::Local);
    let program = Program::new();
    let code = format!(
        "\tlet α = \"{}\";\n\n\tprintln!(\"{{α}}\");\n",
        "long_value".repeat(70)
    );
    let _stream = attach_with(
        &view,
        &program,
        snapshot(cursor(1, 0), vec![agent(1, &format!("```rust\n{code}```"))]),
        cx,
    );
    wait_until(|| {
        poll(&view, &program, cx);
        cx.debug_bounds("chat-code-scroll").is_some()
    });
    let viewport = cx.debug_bounds("chat-code-scroll").unwrap();
    let chat = cx.debug_bounds("chat-view").unwrap();
    let line = cx.debug_bounds("chat-code-line-0").unwrap();
    let blank = cx.debug_bounds("chat-code-line-1").unwrap();
    assert!(viewport.left() >= chat.left() && viewport.right() <= chat.right());
    assert!(
        line.size.width > viewport.size.width,
        "the unwrapped code must have its own scrollable width"
    );
    assert_eq!(
        line.size.height, blank.size.height,
        "blank and nonblank lines share one baseline grid"
    );
    cx.simulate_event(ScrollWheelEvent {
        position: viewport.center(),
        delta: ScrollDelta::Pixels(point(px(-120.), px(0.))),
        ..Default::default()
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    assert!(
        cx.debug_bounds("chat-code-line-0").unwrap().left() < line.left(),
        "horizontal scrolling must expose the code suffix"
    );
    let copy = cx.debug_bounds("chat-copy-code").unwrap();
    cx.simulate_click(copy.center(), Modifiers::default());
    assert_eq!(
        cx.update(|_, cx| cx.read_from_clipboard())
            .and_then(|item| item.text())
            .as_deref(),
        Some(code.as_str())
    );
}
