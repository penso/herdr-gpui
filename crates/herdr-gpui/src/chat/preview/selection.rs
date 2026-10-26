//! Public synthetic coordinates for the private XTest selection probe.
use anyhow::{Context as _, Result};
use gpui::{ClipboardItem, Context};
use herdr_chat_protocol::EntryId;
use serde_json::{Value, json};

use super::PANE;
use crate::HerdrWindow;

const PROSE: &str = "Read naïve café 中文 text across a narrow paragraph. These extra words deliberately wrap onto several lines before the code and table below.";
pub(super) const SOURCE: &str = "Read naïve café 中文 text across a narrow paragraph. These extra words deliberately wrap onto several lines before the code and table below.\n\n```rust\n\tlet α = 1;\n\tprintln!(\"{α}\");\n```\n\n| Name | Value |\n|:--|--:|\n| café | 42 |\n\nBefore $x^2$ after.\n\n$$\\sum_{i=1}^n i$$\n\n[Readable reference](https://example.org/selection)";
const SEED: &str = "selection probe ready";

#[derive(Default)]
pub(super) struct Probe {
    previous: Option<(Option<String>, Option<String>, bool)>,
}

impl Probe {
    pub(super) fn ready(view: &HerdrWindow, cx: &mut Context<HerdrWindow>) -> Result<()> {
        let pane = view
            .chats
            .pane(PANE)
            .context("selection preview pane disappeared")?;
        let point = |block, row, column, offset| -> Result<Value> {
            let key = pane
                .prepared
                .text()
                .key(
                    EntryId(1),
                    crate::chat::selection::Region::Body(0),
                    crate::chat::selection::Cell { block, row, column },
                )
                .context("selection preview text segment disappeared")?;
            let point = pane
                .text_geometry
                .position(key, offset)
                .context("selection preview text was not laid out")?;
            Ok(json!([f32::from(point.x), f32::from(point.y)]))
        };
        let data = json!({
            "from": point(0, 0, 0, "Read ".len())?,
            "to": point(2, 1, 1, 2)?,
            "expected": format!("{}\n\n\tlet α = 1;\n\tprintln!(\"{{α}}\");\n\nName\tValue\ncafé\t42", &PROSE["Read ".len()..]),
            "inline_from": point(3, 0, 0, 7)?,
            "inline_to": point(3, 0, 0, 10)?,
            "inline_expected": "x^2",
            "math_from": point(4, 0, 0, 0)?,
            "math_to": point(4, 0, 0, "\\sum_{i=1}^n i".len())?,
            "math_expected": "\\sum_{i=1}^n i",
            "link_from": point(5, 0, 0, 0)?,
            "link_to": point(5, 0, 0, "Readable reference".len())?,
            "link_expected": "Readable reference",
        });
        cx.write_to_clipboard(ClipboardItem::new_string(SEED.into()));
        eprintln!("RICH CHAT SELECTION PROBE {data}");
        Ok(())
    }

    pub(super) fn observe(
        &mut self,
        view: &HerdrWindow,
        window: &gpui::Window,
        cx: &mut Context<HerdrWindow>,
    ) {
        let Some(pane) = view.chats.pane(PANE) else {
            return;
        };
        let selected = pane.selection.text(pane.prepared.text());
        if selected.is_none()
            && self
                .previous
                .as_ref()
                .is_some_and(|previous| previous.0.is_some())
        {
            cx.write_to_clipboard(ClipboardItem::new_string(SEED.into()));
        }
        let clipboard = cx.read_from_clipboard().and_then(|item| item.text());
        let focused = pane
            .text_focus
            .as_ref()
            .is_some_and(|focus| focus.is_focused(window));
        let state = (selected, clipboard, focused);
        if self.previous.as_ref() != Some(&state) {
            eprintln!(
                "RICH CHAT SELECTION STATE {}",
                json!({"selected": state.0, "clipboard": state.1, "reader_focused": state.2})
            );
            self.previous = Some(state);
        }
    }
}
