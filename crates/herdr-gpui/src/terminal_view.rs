use crate::{
    config::{Config, FontConfig, Theme},
    terminal::*,
    terminal_painter::TerminalPainter,
};
use gpui::{prelude::*, *};
use herdr_client::protocol::PaneSurfaceFrame;
use std::{cell::RefCell, rc::Rc, sync::Arc};

pub(crate) struct TerminalView {
    surface: Option<Arc<PaneSurfaceFrame>>,
    painter: Rc<RefCell<TerminalPainter>>,
    font: FontConfig,
    theme: Theme,
}

impl TerminalView {
    pub fn new(painter: Rc<RefCell<TerminalPainter>>) -> Self {
        Self {
            surface: None,
            painter,
            font: Config::default().terminal,
            theme: Theme::default(),
        }
    }

    pub fn set_appearance(&mut self, font: &FontConfig, theme: &Theme, cx: &mut Context<Self>) {
        if self.font.family != font.family || self.font.size != font.size || self.theme != *theme {
            self.font = font.clone();
            self.theme = theme.clone();
            cx.notify();
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
        let font = font(self.font.family.clone());
        let cell_height = self.font.line_height();
        painter
            .borrow_mut()
            .set_appearance(self.font.size, cell_height, self.theme.clone());
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
                        let offset =
                            popup_origin(&surface.frame, &popup.frame, cell_width, cell_height);
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

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use std::cell::Cell;

    #[gpui::test]
    fn appearance_changes_invalidate_but_identical_values_retain(cx: &mut TestAppContext) {
        let view = cx.new(|_| TerminalView::new(Rc::new(RefCell::new(TerminalPainter::default()))));
        let notifications = Rc::new(Cell::new(0));
        let count = notifications.clone();
        let _observer = cx.new(|cx| {
            cx.observe(&view, move |_: &mut Empty, _, _| count.set(count.get() + 1))
                .detach();
            Empty
        });
        let mut font = Config::default().terminal;
        let mut theme = Theme::default();
        view.update(cx, |view, cx| view.set_appearance(&font, &theme, cx));
        cx.run_until_parked();
        assert_eq!(notifications.get(), 0);
        for change in 0..4 {
            match change {
                0 => font.family = "Courier".into(),
                1 => font.size = 21.,
                2 => theme.palette[1] ^= 0xffffff,
                _ => theme.cursor ^= 0xffffff,
            }
            view.update(cx, |view, cx| view.set_appearance(&font, &theme, cx));
            cx.run_until_parked();
            assert_eq!(notifications.get(), change + 1);
            view.update(cx, |view, cx| {
                assert_eq!(view.font.family, font.family);
                assert_eq!(view.font.size, font.size);
                assert_eq!(view.font.line_height(), font.line_height());
                assert_eq!(view.theme, theme);
                view.set_appearance(&font.clone(), &theme.clone(), cx);
            });
            cx.run_until_parked();
            assert_eq!(notifications.get(), change + 1);
        }
    }
}
