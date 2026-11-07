//! Grouping a device from its menu. Move to picks the section it lists under;
//! Also tagged adds the tags it carries besides. A new name in the field
//! makes a new group. Both save through `device_tags`, which sends the change
//! on to the device's own daemon.
use super::HostMenu;
use crate::{
    SheprdWindow,
    device_tags::{self, DeviceTags},
    i18n::fl,
    menu::Page,
    search_input::SearchInput,
};
use gpui::{prelude::*, *};
use sheprd_remote_control::control::tags::normalized;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    MoveTo,
    AlsoTagged,
}

pub(crate) struct TagPicker {
    mode: Mode,
    /// The tags to offer; Move to adds No group after them.
    rows: Vec<String>,
    selected: Option<usize>,
    input: Entity<SearchInput>,
}

impl TagPicker {
    fn count(&self) -> usize {
        self.rows.len() + usize::from(self.mode == Mode::MoveTo)
    }
}

impl SheprdWindow {
    pub(super) fn open_device_tags(
        &mut self,
        mode: Mode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(SearchInput::new);
        input.update(cx, |input, cx| {
            input.set_placeholder_with(|| fl!("device-tags-new"), cx);
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
            window.focus(&input.focus, cx);
        });
        let Some(host) = &mut self.menu.host else {
            return;
        };
        host.tags = Some(TagPicker {
            mode,
            rows: device_tags::offered(&self.device_tags),
            selected: None,
            input,
        });
        self.menu.page = Some(Page::DeviceTags);
        cx.notify();
    }

    /// Applies row `index`: Move to closes, as the device has moved; Also
    /// tagged stays open for the next tick.
    fn choose_device_tag(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = &self.menu.host else {
            return;
        };
        let Some(picker) = &host.tags else {
            return;
        };
        let row = picker.rows.get(index).cloned();
        self.apply_device_tag(row, window, cx);
    }

    /// `tag` applied as the page's mode says. `None` is No group.
    fn apply_device_tag(
        &mut self,
        tag: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = &self.menu.host else {
            return;
        };
        let Some(picker) = &host.tags else {
            return;
        };
        let (id, mode) = (host.id.clone(), picker.mode);
        let tags = match (mode, &tag) {
            (Mode::MoveTo, tag) => self.device_tags.moved_to(&id, tag.as_deref()),
            (Mode::AlsoTagged, Some(tag)) => self.device_tags.toggled(&id, tag),
            (Mode::AlsoTagged, None) => return,
        };
        DeviceTags::set(cx, &id, tags);
        self.device_tags = DeviceTags::current(cx);
        if mode == Mode::MoveTo {
            self.dismiss_menu(window, cx);
            return;
        }
        let rows = device_tags::offered(&self.device_tags);
        if let Some(picker) = self.menu.host.as_mut().and_then(|host| host.tags.as_mut()) {
            picker.rows = rows;
            picker
                .input
                .update(cx, |input, cx| input.set_text_at("", 0, cx));
        }
        cx.notify();
    }

    pub(in crate::menu) fn device_tags_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = self.menu.host.as_mut().and_then(|host| host.tags.as_mut()) else {
            return;
        };
        // Unhandled text must reach the native input system, including IME.
        if picker.input.read(cx).is_composing() {
            return;
        }
        match event.keystroke.key.as_str() {
            "escape" => {
                cx.stop_propagation();
                window.prevent_default();
                self.dismiss_menu(window, cx);
            }
            key @ ("up" | "down") => {
                cx.stop_propagation();
                window.prevent_default();
                let count = picker.count();
                picker.selected = (count > 0).then(|| match (picker.selected, key) {
                    (None, "up") => count - 1,
                    (None, _) => 0,
                    (Some(index), "up") => (index + count - 1) % count,
                    (Some(index), _) => (index + 1) % count,
                });
                cx.notify();
            }
            "enter" => {
                cx.stop_propagation();
                window.prevent_default();
                let typed = normalized(picker.input.read(cx).text());
                match (typed, picker.selected) {
                    (Some(tag), _) => self.apply_device_tag(Some(tag), window, cx),
                    (None, Some(index)) => self.choose_device_tag(index, window, cx),
                    (None, None) => {}
                }
            }
            _ => {}
        }
    }

    pub(super) fn render_device_tags(&self, host: &HostMenu, cx: &mut Context<Self>) -> Div {
        let Some(picker) = &host.tags else {
            return div();
        };
        let theme = &self.theme;
        let font = &self.config.ui;
        let title = match picker.mode {
            Mode::MoveTo => fl!("device-tags-move-title", device = host.label.as_str()),
            Mode::AlsoTagged => fl!("device-tags-also-title", device = host.label.as_str()),
        };
        let section = self.device_tags.section(&host.id, None);
        let row = |index: usize, label: String, ticked: bool, cx: &mut Context<Self>| {
            div()
                .id(("device-tags-row", index))
                .debug_selector(move || format!("device-tags-row-{index}"))
                .min_h(px(font.line_height() + 12.))
                .px(px(8.))
                .flex()
                .items_center()
                .gap(px(8.))
                .cursor_pointer()
                .when(picker.selected == Some(index), |row| {
                    row.bg(rgb(theme.active))
                })
                .hover(|row| row.bg(rgb(theme.active)))
                .child(crate::toggles::checkbox(theme, font.size + 2., ticked))
                .child(div().min_w_0().truncate().child(label))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.choose_device_tag(index, window, cx);
                }))
        };
        let mut list = div().flex().flex_col();
        for (index, tag) in picker.rows.iter().enumerate() {
            let ticked = match picker.mode {
                Mode::MoveTo => {
                    section.is_some_and(|own| sheprd_remote_control::control::tags::same(own, tag))
                }
                Mode::AlsoTagged => self.device_tags.has(&host.id, tag),
            };
            list = list.child(row(index, tag.clone(), ticked, cx));
        }
        if picker.mode == Mode::MoveTo {
            let index = picker.rows.len();
            list = list.child(row(
                index,
                fl!("device-tags-remove-section"),
                section.is_none(),
                cx,
            ));
        }
        div()
            .debug_selector(|| "device-tags".into())
            .flex()
            .flex_col()
            .gap(px(6.))
            .p(px(8.))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .truncate()
                    .child(title),
            )
            .child(list)
            .child(picker.input.clone())
    }
}
