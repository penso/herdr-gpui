//! A picture from the conversation shown large over its chat. Clicking a
//! picture opens it; a click anywhere or Escape closes it. It fades and
//! grows in, and fades and shrinks back out before it is let go.

use super::super::{
    chats::ChatPane,
    visual::{Shown, VisualId},
};
use crate::{HerdrWindow, config::corners, motion};
use gpui::{prelude::*, *};

/// Space kept between the picture and the chat's edges.
const MARGIN: f32 = 32.;
/// How far past its own size a small picture is enlarged.
const MAX_ZOOM: f32 = 2.;
/// How small the picture starts as it grows in, and ends as it leaves.
const FROM: f32 = 0.94;

/// The picture a chat shows large.
#[derive(Clone, Debug)]
pub(crate) struct Viewer {
    closing: bool,
    /// Holds the keyboard while the picture shows, so typing never reaches
    /// the composer hidden behind it.
    focus: FocusHandle,
    /// Bumped for each picture opened, so a close timed for an earlier one
    /// never lets a later one go, and each plays its own motion.
    generation: u64,
    id: VisualId,
}

impl Viewer {
    /// The keyboard's handle while the picture is open, not once it leaves.
    pub(crate) fn open_focus(&self) -> Option<&FocusHandle> {
        (!self.closing).then_some(&self.focus)
    }

    pub(crate) fn focus(&self) -> &FocusHandle {
        &self.focus
    }
}

impl HerdrWindow {
    /// Whether the chat in `pane_id` shows a picture large and holds the
    /// keyboard for it.
    pub(crate) fn chat_viewer_open(&self, pane_id: &str) -> bool {
        self.chats
            .pane(pane_id)
            .and_then(|pane| pane.viewer.as_ref())
            .is_some_and(|viewer| viewer.open_focus().is_some())
    }

    /// Shows the picture `id` large over the chat in `pane_id`, with the
    /// keyboard on it. The click that opened it never reached the chat, so
    /// the chat's pane is made the active one here, as a click on the chat
    /// would; otherwise the keyboard would go back to the active pane.
    pub(crate) fn open_chat_viewer(
        &mut self,
        pane_id: &str,
        id: VisualId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.chats.pane(pane_id).is_none() {
            return;
        }
        self.focus_chat(pane_id, window, cx);
        let focus = cx.focus_handle();
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return;
        };
        let generation = pane
            .viewer
            .as_ref()
            .map_or(0, |viewer| viewer.generation + 1);
        pane.viewer = Some(Viewer {
            closing: false,
            focus: focus.clone(),
            generation,
            id,
        });
        window.focus(&focus, cx);
        self.redraw_terminal(cx);
        cx.notify();
    }

    /// Starts closing the picture shown large, letting it go once it has
    /// left, and gives the keyboard back to the chat. Whether one was open.
    pub(crate) fn close_chat_viewer(
        &mut self,
        pane_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return false;
        };
        let Some(viewer) = pane.viewer.as_mut().filter(|viewer| !viewer.closing) else {
            return false;
        };
        viewer.closing = true;
        let generation = viewer.generation;
        if let Some(focus) = pane.input_focus(cx) {
            window.focus(&focus, cx);
        }
        let pane_id = pane_id.to_owned();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(motion::duration(motion::LEAVE))
                .await;
            let _ = this.update(cx, |this, cx| {
                let Some(pane) = this.chats.pane_mut(&pane_id) else {
                    return;
                };
                if pane
                    .viewer
                    .as_ref()
                    .is_some_and(|viewer| viewer.closing && viewer.generation == generation)
                {
                    pane.viewer = None;
                    this.redraw_terminal(cx);
                    cx.notify();
                }
            });
        })
        .detach();
        self.redraw_terminal(cx);
        cx.notify();
        true
    }

    /// The picture shown large over a chat drawn in `bounds`, if one is.
    pub(super) fn render_chat_viewer(
        &self,
        chat: &ChatPane,
        bounds: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let viewer = chat.viewer.as_ref()?;
        let Shown::Ready(drawn) = chat.prepared.visuals.shown(viewer.id) else {
            return None;
        };
        let room_w = (f32::from(bounds.size.width) - 2. * MARGIN).max(1.);
        let room_h = (f32::from(bounds.size.height) - 2. * MARGIN).max(1.);
        let scale = (room_w / drawn.width)
            .min(room_h / drawn.height)
            .min(MAX_ZOOM);
        let (width, height) = (drawn.width * scale, drawn.height * scale);
        let closing = viewer.closing;
        let phase = if closing { "out" } else { "in" };
        let animation = || {
            Animation::new(motion::duration(if closing {
                motion::LEAVE
            } else {
                motion::ENTER
            }))
            .with_easing(|k| 1. - (1. - k).powi(3))
        };
        // How much shows at `k` through this phase; tests see it in place.
        let shown = move |k: f32| {
            let k = if cfg!(test) { 1. } else { k };
            if closing { 1. - k } else { k }
        };
        let picture = img(drawn.texture.clone())
            .debug_selector(|| "chat-viewer-picture".into())
            .rounded(px(corners::CONTROL))
            .shadow_lg()
            .with_animation(
                SharedString::from(format!("chat-viewer-picture-{}-{phase}", viewer.generation)),
                animation(),
                move |picture, k| {
                    let grown = FROM + (1. - FROM) * shown(k);
                    picture.w(px(width * grown)).h(px(height * grown))
                },
            );
        let pane_id = chat.target.pane_id.clone();
        let backdrop = (self.theme.background << 8) | 0xec;
        Some(
            div()
                .id(SharedString::from(format!(
                    "chat-viewer-{}",
                    viewer.generation
                )))
                .debug_selector(|| "chat-viewer".into())
                .track_focus(viewer.focus())
                .absolute()
                .inset_0()
                .occlude()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgba(backdrop))
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    this.close_chat_viewer(&pane_id, window, cx);
                }))
                .child(picture)
                .with_animation(
                    SharedString::from(format!("chat-viewer-{}-{phase}", viewer.generation)),
                    animation(),
                    move |overlay, k| overlay.opacity(shown(k)),
                )
                .into_any_element(),
        )
    }
}
