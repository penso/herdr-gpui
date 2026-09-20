use crate::{terminal::*, terminal_painter::TerminalPainter};
use gpui::{prelude::*, *};
use herdr_client::protocol::PaneSurfaceFrame;
use std::{cell::RefCell, rc::Rc, sync::Arc};

pub(crate) struct TerminalView {
    surface: Option<Arc<PaneSurfaceFrame>>,
    painter: Rc<RefCell<TerminalPainter>>,
}

impl TerminalView {
    pub fn new(painter: Rc<RefCell<TerminalPainter>>) -> Self {
        Self {
            surface: None,
            painter,
        }
    }

    pub fn set_surface(&mut self, surface: Option<Arc<PaneSurfaceFrame>>, cx: &mut Context<Self>) {
        let unchanged = match (&self.surface, &surface) {
            (Some(old), Some(new)) => Arc::ptr_eq(old, new),
            (None, None) => true,
            _ => false,
        };
        if !unchanged {
            self.surface = surface;
            cx.notify();
        }
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let surface = self.surface.clone();
        let painter = self.painter.clone();
        let font = font("Menlo");
        let cell_width = painter.borrow_mut().cell_width(&font, window, cx);
        canvas(
            |_, _, _| (),
            move |bounds, _, window, cx| {
                if let Some(surface) = surface {
                    let mut painter = painter.borrow_mut();
                    painter.paint_frame(
                        &surface.frame,
                        bounds.origin,
                        cell_width,
                        &font,
                        window,
                        cx,
                    );
                    if let Some(popup) = &surface.popup {
                        let offset = popup_origin(&surface.frame, &popup.frame, cell_width);
                        painter.paint_frame(
                            &popup.frame,
                            bounds.origin + offset,
                            cell_width,
                            &font,
                            window,
                            cx,
                        );
                    }
                }
            },
        )
        .size_full()
    }
}
