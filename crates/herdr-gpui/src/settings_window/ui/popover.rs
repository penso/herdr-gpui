//! Dropdown: a button that opens a floating option list beneath itself. The
//! list renders inside the button's subtree via `deferred(anchored(..))`, so
//! no bounds are passed around and it paints above the page.
use crate::{
    config::{Theme, corners},
    i18n::fl,
    settings_window::inputs::DropdownField,
};
use gpui::{prelude::*, *};
use std::{cell::RefCell, collections::HashSet, ops::Range, rc::Rc};

thread_local! {
    /// Dropdowns whose list was open when the button went down; the click that
    /// follows closes instead of reopening.
    static CLOSING: RefCell<HashSet<&'static str>> = RefCell::new(HashSet::new());
}

/// The open popover the view holds in `self.dropdown: Option<Dropdown>`.
pub(in crate::settings_window) struct Dropdown {
    pub field: DropdownField,
    pub options: Vec<SharedString>,
    pub selected: usize,
    focus: FocusHandle,
}

impl Dropdown {
    pub(in crate::settings_window) fn open(
        field: DropdownField,
        options: Vec<SharedString>,
        selected: usize,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Self {
            field,
            options,
            selected,
            focus,
        }
    }

    pub(in crate::settings_window) fn focus(&self) -> &FocusHandle {
        &self.focus
    }

    /// Up/Down/Enter/Escape. Returns `Some(index)` on Enter, `Some(usize::MAX)`
    /// on Escape (the caller closes), else `None`.
    pub(in crate::settings_window) fn key(&mut self, event: &KeyDownEvent) -> Option<usize> {
        let count = self.options.len();
        match event.keystroke.key.as_str() {
            "down" if count > 0 => self.selected = (self.selected + 1) % count,
            "up" if count > 0 => self.selected = (self.selected + count - 1) % count,
            "enter" => return (self.selected < count).then_some(self.selected),
            "escape" => return Some(usize::MAX),
            _ => {}
        }
        None
    }
}

/// The button, plus its list when `open` is this dropdown.
#[allow(clippy::too_many_arguments)]
pub(in crate::settings_window) fn dropdown(
    theme: &Theme,
    id: &'static str,
    label: impl Into<SharedString>,
    shared_prefix: bool,
    enabled: bool,
    open: Option<&Dropdown>,
    on_open: impl Fn(&mut Window, &mut App) + 'static,
    on_pick: impl Fn(usize, &mut Window, &mut App) + 'static,
    on_dismiss: impl Fn(&mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let label: SharedString = label.into();
    let open = open.filter(|_| enabled);
    let was_open = open.is_some();
    let on_dismiss = Rc::new(on_dismiss);
    let dismiss_button = on_dismiss.clone();
    let dismiss_list = on_dismiss;
    let label_el = match follow_herdr_value(&label).filter(|_| shared_prefix) {
        // One line, so the words around Herdr's value stay a single message.
        Some(value) => {
            let muted = HighlightStyle {
                color: Some(rgb(theme.muted).into()),
                ..Default::default()
            };
            let around = [0..value.start, value.end..label.len()];
            div().child(
                StyledText::new(label.clone()).with_highlights(
                    around
                        .into_iter()
                        .filter(|range| !range.is_empty())
                        .map(|range| (range, muted)),
                ),
            )
        }
        None => div().child(label.clone()),
    };
    div()
        .id(id)
        .debug_selector(move || id.to_string())
        .relative()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.))
        .flex_none()
        .min_w(px(150.))
        .px(px(10.))
        .py(px(4.))
        .rounded(px(corners::CONTROL))
        .border_1()
        .border_color(rgb(theme.active))
        .bg(rgb(theme.surface))
        .text_color(rgb(theme.foreground))
        .when(!enabled, |button| button.opacity(0.5))
        .when(enabled, |button| {
            button
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    if was_open {
                        CLOSING.with(|closing| closing.borrow_mut().insert(id));
                        dismiss_button(window, cx);
                    } else {
                        // A press that never became a click leaves a stale flag.
                        CLOSING.with(|closing| closing.borrow_mut().remove(id));
                    }
                })
                .on_click(move |_, window, cx| {
                    if !CLOSING.with(|closing| closing.borrow_mut().remove(id)) {
                        on_open(window, cx);
                    }
                })
        })
        .child(label_el)
        .child(div().text_color(rgb(theme.subtext())).child("⌄"))
        .children(open.map(|open| {
            deferred(
                anchored()
                    .snap_to_window_with_margin(px(8.))
                    .child(crate::motion::pop_in(
                        popover_list(theme, open, on_pick, move |window, cx| {
                            dismiss_list(window, cx)
                        }),
                        SharedString::from(format!("{id}-list")),
                    )),
            )
            .with_priority(1)
        }))
}

/// The floating list; also usable on its own where the view wants it elsewhere.
pub(in crate::settings_window) fn popover_list(
    theme: &Theme,
    open: &Dropdown,
    on_pick: impl Fn(usize, &mut Window, &mut App) + 'static,
    on_dismiss: impl Fn(&mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let id = open.field.selector();
    let on_pick = Rc::new(on_pick);
    div()
        .id(SharedString::from(format!("{id}-list")))
        .debug_selector(move || format!("{id}-list"))
        .absolute()
        .top_full()
        .left_0()
        .mt(px(4.))
        .min_w(px(180.))
        .max_h(px(260.))
        .overflow_y_scroll()
        .p(px(4.))
        .rounded(px(corners::CONTROL))
        .border_1()
        .border_color(rgb(theme.active))
        .bg(rgb(theme.surface))
        .shadow_md()
        .occlude()
        .track_focus(&open.focus)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(|_, _, cx| cx.stop_propagation())
        .on_mouse_down_out(move |_, window, cx| on_dismiss(window, cx))
        .children(open.options.iter().enumerate().map(|(index, option)| {
            let on_pick = on_pick.clone();
            div()
                .id((id, index))
                .debug_selector(move || format!("{id}-option-{index}"))
                .px(px(8.))
                .py(px(4.))
                .rounded(px(corners::SMALL))
                .whitespace_nowrap()
                .cursor_pointer()
                .text_color(rgb(theme.foreground))
                .when(index == open.selected, |item| item.bg(rgb(theme.active)))
                .on_click(move |_, window, cx| on_pick(index, window, cx))
                .child(option.clone())
        }))
}

/// Where Herdr's value sits in a label that follows Herdr
/// (`settings-use-herdr`), so the button can mute the words around it.
fn follow_herdr_value(label: &str) -> Option<Range<usize>> {
    // Formatted around a character no value holds, the message shows where
    // the value goes in any language.
    const MARK: &str = "\u{0}";
    let template = fl!("settings-use-herdr", value = MARK);
    let (before, after) = template.split_once(MARK)?;
    label.strip_prefix(before)?.strip_suffix(after)?;
    Some(before.len()..label.len() - after.len())
}

#[cfg(test)]
mod tests;
