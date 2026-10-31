//! A herdr-core terminal drawn by the app: its own focus, keys, and IME
//! text, painted with the window's terminal painter. One per tab showing it;
//! any number may show the same terminal, as observers.

pub(super) mod keys;
pub(super) mod paste;
mod pointer;

use super::{
    screen::Screen,
    selection::{Cell, Selection},
    worker::{Command, Link},
};
use crate::{
    config::{OptionAsAlt, Padding},
    i18n::fl,
    terminal::{HeldKeys, key_input, viewport},
    terminal_painter::{Ground, TerminalPainter},
};
use gpui::{prelude::*, *};
use herdr_backend::{ClientPaneInputEvent, FrameData, Size, TerminalId};
use std::{cell::RefCell, ops::Range, rc::Rc, sync::Arc};

/// How the window draws terminals, copied into each view and refreshed when
/// fonts, theme, or options change.
#[derive(Clone, PartialEq)]
pub(crate) struct Look {
    pub(crate) background: Hsla,
    pub(crate) cell_height: f32,
    pub(crate) cell_width: f32,
    pub(crate) font: Font,
    pub(crate) ground: Ground,
    pub(crate) option_as_alt: OptionAsAlt,
    pub(crate) padding: Padding,
}

/// The least room a split's cells keep from their pane's edges, across
/// and down.
const PANE_PADDING: (f32, f32) = (6., 3.);

/// How a pane of a split draws its cells, from the tab layout's chrome.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PaneLook {
    /// The cells' top and bottom corners, matching a rounded pane.
    pub(crate) corners: (f32, f32),
    /// The veil over a pane not in use, out of 255. Its cursor hides too, so
    /// only the pane keys go to shows one.
    pub(crate) veil: Option<u8>,
}

pub(crate) struct CoreTerminalView {
    /// Where the cells were last painted, for the IME's candidate window.
    bounds: Option<Bounds<Pixels>>,
    focus: FocusHandle,
    /// Releases for keys pressed while the program asked for them.
    held_keys: HeldKeys,
    /// The cell a button-less pointer last reported, so movement inside
    /// one cell is not reported again.
    hovered: Option<Cell>,
    id: TerminalId,
    link: Link,
    look: Look,
    /// Text an IME is composing; committed text goes to the terminal.
    marked: String,
    /// A mirror's view: it watches and never types.
    observe: bool,
    painter: Rc<RefCell<TerminalPainter>>,
    /// Set while the view is a pane of a split.
    pane: Option<PaneLook>,
    /// Where the button held down is going.
    press: Option<pointer::Press>,
    selection: Option<Selection>,
    /// The last frame painted with its cursor hidden, and the frame it came
    /// from, so a veiled pane copies a frame once rather than every paint.
    sleeping: Option<(Arc<FrameData>, Arc<FrameData>)>,
    screen: Screen,
    sent_focus: Option<bool>,
    sent_size: Option<Size>,
    /// Wheel motion short of a whole row, kept for the next event.
    wheel_rest: f32,
}

impl CoreTerminalView {
    pub(crate) fn new(
        id: TerminalId,
        link: Link,
        look: Look,
        painter: Rc<RefCell<TerminalPainter>>,
        observe: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            bounds: None,
            focus: cx.focus_handle(),
            held_keys: HeldKeys::default(),
            hovered: None,
            id,
            link,
            look,
            marked: String::new(),
            observe,
            painter,
            pane: None,
            press: None,
            selection: None,
            sleeping: None,
            screen: Screen::default(),
            sent_focus: None,
            sent_size: None,
            wheel_rest: 0.,
        }
    }

    pub(crate) fn title(&self) -> Option<&str> {
        self.screen.title.as_deref()
    }

    pub(crate) fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    /// Whether the terminal's program ended.
    pub(crate) fn ended(&self) -> bool {
        self.screen.ended.is_some()
    }

    /// The size the terminal was last given, in cells.
    pub(crate) fn size(&self) -> Option<Size> {
        self.sent_size
    }

    /// Draws the view as a pane of a split, or bare. Read by this frame's
    /// render, which follows the window's, so it needs no notify.
    pub(crate) fn set_pane(&mut self, pane: Option<PaneLook>) {
        self.pane = pane;
    }

    /// The frame to paint: a veiled pane's with its cursor hidden.
    fn frame_to_paint(&mut self) -> Option<Arc<FrameData>> {
        let frame = self.screen.frame.clone()?;
        let veiled = self.pane.is_some_and(|pane| pane.veil.is_some());
        if !veiled || frame.cursor.is_none() {
            self.sleeping = None;
            return Some(frame);
        }
        if let Some((source, hidden)) = &self.sleeping
            && Arc::ptr_eq(source, &frame)
        {
            return Some(hidden.clone());
        }
        let mut hidden = (*frame).clone();
        hidden.cursor = None;
        let hidden = Arc::new(hidden);
        self.sleeping = Some((frame, hidden.clone()));
        Some(hidden)
    }

    pub(crate) fn changed(&self) -> bool {
        self.link.mailbox.changed()
    }

    /// Takes what the worker produced; true when there was something.
    pub(crate) fn poll(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(screen) = self.link.mailbox.take() else {
            return false;
        };
        self.screen = screen;
        cx.notify();
        true
    }

    pub(crate) fn set_look(&mut self, look: Look, cx: &mut Context<Self>) {
        self.look = look;
        cx.notify();
    }

    fn send(&self, input: ClientPaneInputEvent) {
        self.link.send(Command::Input(input));
    }

    /// Sizes the terminal to the cells that fit where it is laid out.
    fn fit(&mut self, bounds: Bounds<Pixels>) {
        self.bounds = Some(bounds);
        let fits = viewport(
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
            self.look.cell_width,
            self.look.cell_height,
        );
        let Ok(size) = Size::new(fits.cols, fits.rows) else {
            return;
        };
        if self.sent_size != Some(size) {
            self.sent_size = Some(size);
            self.link.send(Command::Resize(size));
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let modifiers = event.keystroke.modifiers;
        // Cmd-C copies a selection, as the window's copy does a Herdr pane's.
        if modifiers.platform
            && !modifiers.control
            && event.keystroke.key.eq_ignore_ascii_case("c")
            && self.copy_selection(cx)
        {
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        // Pasting reads the clipboard here, as the window does for a Herdr
        // pane; the Herdr pane behind this one must not take it.
        let has_text = || {
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .is_some_and(|text| !text.is_empty())
        };
        if paste::pastes(&event.keystroke, cfg!(target_os = "windows"), has_text) {
            self.paste_clipboard(cx);
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        // The window's own shortcuts keep working over the terminal.
        self.held_keys.forget(&event.keystroke.key);
        if modifiers.platform {
            return;
        }
        // Typing ends a selection.
        if self.selection.take().is_some() {
            cx.notify();
        }
        let option_keys = match event.keystroke.modifiers.alt {
            true => crate::input::held_option_keys(),
            false => crate::config::OptionKeys::LEFT,
        };
        let alt_keys = self
            .look
            .option_as_alt
            .sends_alt(cx.keyboard_layout().id(), option_keys);
        if !self.marked.is_empty() {
            return;
        }
        // Printable text must bubble on: macOS calls the input handler only
        // for a key nothing handled. The window ignores keys while this view
        // holds focus, so none reach a Herdr 1 pane behind it. A program that
        // asked for every key gets printable keys as key events instead.
        let keys = self.screen.keys;
        let input = key_input(event, alt_keys).or_else(|| {
            keys.every_key
                .then(|| keys::printable_key(&event.keystroke, event.is_held))
                .flatten()
        });
        if let Some(input) = input {
            let input = self
                .held_keys
                .press(&event.keystroke.key, input, keys.releases);
            self.send(input);
            window.prevent_default();
            cx.stop_propagation();
        }
    }

    fn report_focus(&mut self, window: &Window) {
        let focused = self.focus.is_focused(window);
        if self.sent_focus != Some(focused) {
            self.sent_focus = Some(focused);
            self.link.send(Command::Focus(focused));
        }
    }

    fn status(&self) -> Option<String> {
        if let Some(failure) = &self.screen.failure {
            return Some(failure.clone());
        }
        let ended = self.screen.ended.map(|exit| match exit.code {
            Some(code) => fl!("core-terminals-exited", code = code),
            None => fl!("core-terminals-ended"),
        });
        ended.or_else(|| self.observe.then(|| fl!("core-terminals-watching")))
    }
}

impl Focusable for CoreTerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for CoreTerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.report_focus(window);
        let entity = cx.entity();
        let fitting = entity.clone();
        let painter = self.painter.clone();
        let look = self.look.clone();
        let frame = self.frame_to_paint();
        let highlights = self.highlights();
        let pane = self.pane;
        let background = self.look.background;
        let focus = self.focus.clone();
        let status = self.status();
        let body = div()
            .relative()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .child(
                canvas(
                    move |bounds, _, cx| fitting.update(cx, |view, _| view.fit(bounds)),
                    move |bounds, _, window, cx| {
                        window.handle_input(
                            &focus,
                            crate::input::TerminalInputHandler::new(bounds, entity, false),
                            cx,
                        );
                        let Some(frame) = &frame else {
                            return;
                        };
                        window.with_content_mask(Some(ContentMask { bounds }), |window| {
                            painter.borrow_mut().paint_frame(
                                frame,
                                bounds.origin,
                                Some(bounds.size),
                                look.ground,
                                look.cell_width,
                                &look.font,
                                &highlights,
                                &[],
                                None,
                                None,
                                window,
                                cx,
                            );
                        });
                    },
                )
                .size_full(),
            )
            .when_some(pane.and_then(|pane| pane.veil), |body, veil| {
                body.child(
                    div()
                        .absolute()
                        .inset_0()
                        .bg(background.opacity(f32::from(veil) / 255.)),
                )
            })
            .when_some(status, |body, status| {
                body.child(
                    div()
                        .absolute()
                        .bottom_2()
                        .right_3()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(rgba(0x0000_0080))
                        .text_color(white())
                        .text_xs()
                        .child(status),
                )
            });
        self.look
            .padding
            .pad(div())
            .id(SharedString::from(format!("core-terminal-{}", self.id)))
            .key_context("CoreTerminal")
            .track_focus(&self.focus)
            .flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .size_full()
            .bg(self.look.background)
            .when_some(pane, |root, pane| {
                // A split's cells never run into its gutter, whatever the
                // configured padding.
                let padding = self.look.padding;
                root.pl(px(padding.x.start().max(PANE_PADDING.0)))
                    .pr(px(padding.x.end().max(PANE_PADDING.0)))
                    .pt(px(padding.y.start().max(PANE_PADDING.1)))
                    .pb(px(padding.y.end().max(PANE_PADDING.1)))
                    .overflow_hidden()
                    .rounded_t(px(pane.corners.0))
                    .rounded_b(px(pane.corners.1))
            })
            .on_key_down(cx.listener(Self::key_down))
            .on_key_up(cx.listener(Self::key_up))
            .on_action(cx.listener(|this, _: &crate::actions::Paste, _, cx| {
                this.paste_clipboard(cx);
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up(MouseButton::Right, cx.listener(Self::mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_scroll_wheel(cx.listener(Self::scroll_wheel))
            .child(body)
    }
}

impl EntityInputHandler for CoreTerminalView {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let text: Vec<u16> = self.marked.encode_utf16().collect();
        let range = range.start.min(text.len())..range.end.min(text.len());
        *adjusted = Some(range.clone());
        Some(String::from_utf16_lossy(&text[range]))
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let end = self.marked.encode_utf16().count();
        Some(UTF16Selection {
            range: end..end,
            reversed: false,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        (!self.marked.is_empty()).then(|| 0..self.marked.encode_utf16().count())
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked.clear();
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked.clear();
        if !text.is_empty() {
            self.send(ClientPaneInputEvent::TextCommit(text.into()));
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked = text.into();
        cx.notify();
    }

    /// The cursor's cell, so the IME's candidate window opens beside it.
    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let bounds = self.bounds?;
        let cursor = self.screen.frame.as_ref()?.cursor.as_ref()?;
        let origin = bounds.origin
            + point(
                px(f32::from(cursor.x) * self.look.cell_width),
                px(f32::from(cursor.y) * self.look.cell_height),
            );
        Some(Bounds::new(
            origin,
            size(px(self.look.cell_width), px(self.look.cell_height)),
        ))
    }

    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}
