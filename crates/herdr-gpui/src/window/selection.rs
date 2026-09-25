//! Selecting terminal cells with the pointer and copying them. The gesture is
//! client-local: it reads the live surface the client already has, never sends
//! input to the daemon, and writes the clipboard only when the user releases a
//! selection they made.

use super::HerdrWindow;
use crate::terminal::Selection;
use gpui_kit::{ClipboardItem, Context, Pixels, Point};

impl HerdrWindow {
    /// Starts a selection under the pointer, discarding the previous one. A
    /// press that lands outside the painted cells only clears.
    pub(crate) fn begin_selection(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let cleared = self.selection.take().is_some();
        if let Some(surface) = self.selectable_surface(position) {
            let (x, y) = Self::terminal_offset(self.bounds, position);
            self.selection = Selection::begin(
                surface,
                x,
                y,
                self.cell_width,
                self.config.terminal.line_height(),
            );
        }
        if cleared || self.selection.is_some() {
            cx.notify();
        }
    }

    /// Follows the pointer while the button that started the drag is down.
    /// Returns whether a drag is in progress, so the caller can keep the
    /// gesture to itself.
    pub(crate) fn extend_selection(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let (x, y) = Self::terminal_offset(self.bounds, position);
        let cell_width = self.cell_width;
        let cell_height = self.config.terminal.line_height();
        let (Some(selection), Some(surface)) = (&mut self.selection, &self.live.surface) else {
            return false;
        };
        if !selection.dragging() {
            return false;
        }
        if selection.extend(surface, x, y, cell_width, cell_height) {
            cx.notify();
        }
        true
    }

    /// Ends a drag: what it chose goes to the clipboard, the highlight goes
    /// away, and the flash says so. Returns whether the release belonged to
    /// the selection, since a press that chose no cells is still the click
    /// that opens a link under the pointer.
    pub(crate) fn release_selection(&mut self, cx: &mut Context<Self>) -> bool {
        if !self
            .selection
            .as_mut()
            .is_some_and(|selection| selection.release())
        {
            return false;
        }
        let selected = !self.selection_is_empty();
        let copied = selected && self.copy_selection(cx);
        // The gesture is over either way: nothing stays highlighted behind it.
        self.selection = None;
        if copied && self.config.clipboard_toast.enabled {
            self.show_flash(super::Flash::success("copied to clipboard"), cx);
        }
        cx.notify();
        selected
    }

    /// Writes the current selection to the clipboard, reporting whether the
    /// text got there.
    fn copy_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let text = {
            let (Some(selection), Some(surface)) = (&self.selection, &self.live.surface) else {
                return false;
            };
            selection.text(surface, self.cell_width, self.config.terminal.line_height())
        };
        match text {
            Ok(text) => {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                true
            }
            Err(error) => {
                self.local_error = Some(format!("Selection not copied: {error}"));
                false
            }
        }
    }

    /// Whether the selection covers no cell the client can still show, either
    /// because the pointer never left the half-cell it pressed in or because
    /// the surface behind it is gone.
    fn selection_is_empty(&self) -> bool {
        let (Some(selection), Some(surface)) = (&self.selection, &self.live.surface) else {
            return true;
        };
        !self.live.surface_ready()
            || selection
                .rows(surface, self.cell_width, self.config.terminal.line_height())
                .next()
                .is_none()
    }

    /// The surface a press at `position` may select from, if the terminal area
    /// is showing one. Hit testing reads the live surface, never the frame a
    /// gap may still be presenting.
    fn selectable_surface(
        &self,
        position: Point<Pixels>,
    ) -> Option<&herdr_client::protocol::PaneSurfaceFrame> {
        if self.menu.page.is_some()
            || !self.live.surface_ready()
            || !self.bounds.contains(&position)
        {
            return None;
        }
        self.live.surface.as_deref()
    }

    fn terminal_offset(bounds: gpui_kit::Bounds<Pixels>, position: Point<Pixels>) -> (f32, f32) {
        (
            f32::from(position.x - bounds.origin.x),
            f32::from(position.y - bounds.origin.y),
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::sidebar::layout_tests::fixture_window;
    use gpui_kit::{Modifiers, MouseButton, TestAppContext, point, px};
    use herdr_client::protocol::*;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn surface(rows: &[&str], width: u16) -> PaneSurfaceFrame {
        let height = rows.len() as u16;
        let rect = SurfaceRect {
            x: 0,
            y: 0,
            width,
            height,
        };
        PaneSurfaceFrame {
            boot_id: "boot".into(),
            projection_revision: 1,
            surface_revision: 1,
            frame: FrameData {
                width,
                height,
                cells: rows
                    .iter()
                    .flat_map(|row| {
                        let mut symbols = row.chars();
                        (0..width).map(move |_| CellData {
                            symbol: symbols.next().unwrap_or(' ').to_string(),
                            fg: 0,
                            bg: 0,
                            modifier: 0,
                            skip: false,
                            hyperlink: None,
                        })
                    })
                    .collect(),
                cursor: None,
                hyperlinks: vec![],
                graphics: vec![],
            },
            splits: vec![],
            popup: None,
            graphics: Default::default(),
            panes: vec![PaneSurfacePane {
                pane_id: "pane".into(),
                content_revision: 1,
                rect,
                inner_rect: rect,
                scrollbar_rect: None,
                scroll: None,
                focused: true,
                mouse_reporting: false,
                sgr_pixel_mouse: false,
                alternate_screen_active: false,
                pixel_width: 100,
                pixel_height: 60,
            }],
        }
    }

    /// A drag across the painted cells copies what it covered when the button
    /// comes up, leaves nothing selected behind it, and says so; a press alone
    /// leaves the clipboard alone.
    #[gpui_kit::test]
    fn dragging_copies_on_release_then_deselects_and_reports(cx: &mut TestAppContext) {
        let (view, cx) = crate::test_support::add_window_view(cx, |window, cx| {
            let mut view = fixture_window(window, cx);
            let mut frame = surface(&["hello there", "second row"], 12);
            let snapshot = view.live.snapshot.as_ref().unwrap();
            frame.boot_id = snapshot.boot_id.clone();
            frame.projection_revision = snapshot.revision;
            view.live.surface = Some(Arc::new(frame));
            view
        });
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        let (origin, cell) = view.read_with(cx, |view, _| {
            (
                view.bounds.origin,
                (view.cell_width, view.config.terminal.line_height()),
            )
        });
        let at = |column: f32, row: f32| -> Point<Pixels> {
            origin + point(px(column * cell.0), px(row * cell.1))
        };

        cx.simulate_mouse_down(at(0., 0.), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(at(5., 0.), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(at(5., 0.), MouseButton::Left, Modifiers::default());
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some("hello".into())
        );
        let expires = view.read_with(cx, |view, _| {
            assert!(view.selection.is_none(), "the release deselects");
            view.flash.clone().expect("the release reports the copy").1
        });
        assert!(cx.update(|_, _| expires) > Instant::now());
        assert!(cx.debug_bounds("flash").is_some());

        // A drag over two rows keeps the rows apart and drops the padding the
        // terminal added to the row it carried through to the edge.
        cx.simulate_mouse_down(at(6., 0.), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(at(6., 1.), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(at(6., 1.), MouseButton::Left, Modifiers::default());
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some("there\nsecond".into())
        );

        // A press with no drag selects nothing, so neither the clipboard nor
        // the flash reports one.
        view.update(cx, |view, _| view.flash = None);
        cx.simulate_click(at(2., 0.), Modifiers::default());
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some("there\nsecond".into())
        );
        view.read_with(cx, |view, _| {
            assert!(view.selection.is_none());
            assert!(view.flash.is_none());
        });

        // The flash retires on its own once its two seconds are up. Whether it
        // is still painted is state, not layout: gpui keeps every debug bound
        // a frame ever registered, so a removed element still has one.
        cx.simulate_mouse_down(at(0., 0.), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(at(5., 0.), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(at(5., 0.), MouseButton::Left, Modifiers::default());
        view.update(cx, |view, _| {
            let (flash, expires) = view.flash.clone().expect("a copy reports itself");
            assert_eq!(flash, crate::window::Flash::success("copied to clipboard"));
            assert!(!view.tick_flash(expires - Duration::from_nanos(1)));
            assert!(view.flash.is_some());
            assert!(view.tick_flash(expires));
            assert!(view.flash.is_none());
            assert!(!view.tick_flash(expires));
        });
    }

    #[gpui_kit::test]
    fn chinese_mouse_selection_copies_exact_text_only_on_release(cx: &mut TestAppContext) {
        let (view, cx) = crate::test_support::add_window_view(cx, |window, cx| {
            let mut view = fixture_window(window, cx);
            // Daemon-style wide cells: ordinary blank continuations, skip=false.
            let mut frame = surface(&["你 好 世 界 ", "A你  B"], 12);
            let snapshot = view.live.snapshot.as_ref().unwrap();
            frame.boot_id = snapshot.boot_id.clone();
            frame.projection_revision = snapshot.revision;
            view.live.surface = Some(Arc::new(frame));
            view
        });
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        let (origin, width, height) = view.read_with(cx, |view, _| {
            (
                view.bounds.origin,
                view.cell_width,
                view.config.terminal.line_height(),
            )
        });
        let at =
            |column: f32, row: f32| origin + point(px(column * width), px((row + 0.5) * height));
        for (from, to, row, expected) in [
            (0.1, 7.9, 0., "你好世界"),
            (7.9, 0.1, 0., "你好世界"),
            (0.1, 8.9, 0., "你好世界 "),
            (0.1, 4.9, 1., "A你 B"),
        ] {
            cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("before".into())));
            cx.simulate_mouse_down(at(from, row), MouseButton::Left, Modifiers::default());
            cx.simulate_mouse_move(at(to, row), MouseButton::Left, Modifiers::default());
            assert_eq!(
                cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
                Some("before".into())
            );
            cx.simulate_mouse_up(at(to, row), MouseButton::Left, Modifiers::default());
            assert_eq!(
                cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
                Some(expected.into())
            );
            assert!(view.read_with(cx, |view, _| view.selection.is_none()));
        }
    }

    /// A link is a destination for a click and text for a drag: the same
    /// press must be able to become either one.
    #[gpui_kit::test]
    fn dragging_across_a_link_copies_it_instead_of_opening_it(cx: &mut TestAppContext) {
        let url = "https://example.com/x";
        let (view, cx) = crate::test_support::add_window_view(cx, |window, cx| {
            let mut view = fixture_window(window, cx);
            let mut frame = surface(&[url], 24);
            let snapshot = view.live.snapshot.as_ref().unwrap();
            frame.boot_id = snapshot.boot_id.clone();
            frame.projection_revision = snapshot.revision;
            view.live.surface = Some(Arc::new(frame));
            view
        });
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        let (origin, width) = view.read_with(cx, |view, _| (view.bounds.origin, view.cell_width));
        let at = |column: f32| origin + point(px(column * width), px(10.));

        cx.simulate_mouse_down(at(0.), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(at(20.6), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(at(20.6), MouseButton::Left, Modifiers::default());
        assert!(cx.opened_url().is_none());
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some(url.into())
        );

        // The press that never left its half-cell is still the click that opens
        // the link, and it copies nothing.
        cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("kept".into())));
        cx.simulate_click(at(1.), Modifiers::default());
        assert_eq!(cx.opened_url().as_deref(), Some(url));
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some("kept".into())
        );
    }

    #[gpui_kit::test]
    fn application_mouse_takes_precedence_and_shift_keeps_copy_and_links(cx: &mut TestAppContext) {
        let url = "https://example.com/x";
        let (view, cx) = crate::test_support::add_window_view(cx, |window, cx| {
            let mut view = fixture_window(window, cx);
            let mut frame = surface(&[url], 24);
            let snapshot = view.live.snapshot.as_ref().unwrap();
            frame.boot_id = snapshot.boot_id.clone();
            frame.projection_revision = snapshot.revision;
            frame.panes[0].mouse_reporting = true;
            view.live.surface = Some(Arc::new(frame));
            view
        });
        cx.update(|window, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string("kept".into()));
            window.refresh();
            window.draw(cx).clear(cx);
        });
        let (origin, width) = view.read_with(cx, |view, _| (view.bounds.origin, view.cell_width));
        let at = |column: f32| origin + point(px(column * width), px(10.));
        cx.simulate_mouse_down(at(0.), MouseButton::Left, Modifiers::default());
        view.read_with(cx, |view, _| {
            assert!(view.selection.is_none());
            assert!(view.pressed_terminal_link.is_none());
        });
        cx.simulate_mouse_move(at(20.6), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(at(20.6), MouseButton::Left, Modifiers::default());
        cx.simulate_click(at(1.), Modifiers::default());
        assert!(cx.opened_url().is_none());
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some("kept".into())
        );
        cx.simulate_mouse_down(at(1.), MouseButton::Right, Modifiers::default());
        assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));
        cx.simulate_mouse_up(at(1.), MouseButton::Right, Modifiers::default());

        let shift = Modifiers {
            shift: true,
            ..Default::default()
        };
        cx.simulate_mouse_down(at(0.), MouseButton::Left, shift);
        assert!(view.read_with(cx, |view, _| view.selection.is_some()));
        cx.simulate_mouse_move(at(20.6), MouseButton::Left, shift);
        cx.simulate_mouse_up(at(20.6), MouseButton::Left, shift);
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some(url.into())
        );
        assert!(cx.opened_url().is_none());
        cx.simulate_click(at(1.), shift);
        assert_eq!(cx.opened_url().as_deref(), Some(url));
    }

    #[gpui_kit::test]
    fn external_file_drag_cancels_local_selection_without_copying(cx: &mut TestAppContext) {
        let (view, cx) = crate::test_support::add_window_view(cx, |window, cx| {
            let mut view = fixture_window(window, cx);
            let mut frame = surface(&["hello there"], 12);
            let snapshot = view.live.snapshot.as_ref().unwrap();
            frame.boot_id = snapshot.boot_id.clone();
            frame.projection_revision = snapshot.revision;
            view.live.surface = Some(Arc::new(frame));
            view
        });
        cx.update(|window, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string("kept".into()));
            window.refresh();
            window.draw(cx).clear(cx);
        });
        let (origin, width) = view.read_with(cx, |view, _| (view.bounds.origin, view.cell_width));
        let at = |column: f32| origin + point(px(column * width), px(10.));
        cx.simulate_mouse_down(at(0.), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(at(5.), MouseButton::Left, Modifiers::default());
        assert!(view.read_with(cx, |view, _| view.selection.is_some()));
        cx.simulate_event(gpui_kit::FileDropEvent::Entered {
            position: at(5.),
            paths: gpui_kit::ExternalPaths::default(),
        });
        assert!(view.read_with(cx, |view, _| view.selection.is_none()));
        cx.simulate_event(gpui_kit::FileDropEvent::Submit { position: at(5.) });
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some("kept".into())
        );
    }

    /// The flash obeys the resolved clipboard-toast settings: turned off, a
    /// copy still happens silently, and each position puts it where it says.
    #[gpui_kit::test]
    fn the_flash_follows_the_clipboard_toast_configuration(cx: &mut TestAppContext) {
        use crate::config::ClipboardToastPosition::*;
        let (view, cx) = crate::test_support::add_window_view(cx, |window, cx| {
            let mut view = fixture_window(window, cx);
            let mut frame = surface(&["configured"], 12);
            let snapshot = view.live.snapshot.as_ref().unwrap();
            frame.boot_id = snapshot.boot_id.clone();
            frame.projection_revision = snapshot.revision;
            view.live.surface = Some(Arc::new(frame));
            view
        });
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        let (origin, width) = view.read_with(cx, |view, _| (view.bounds.origin, view.cell_width));
        let at = |column: f32| origin + point(px(column * width), px(10.));
        let drag = |view: &gpui_kit::Entity<HerdrWindow>, cx: &mut gpui_kit::VisualTestContext| {
            cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("stale".into())));
            cx.simulate_mouse_down(at(0.), MouseButton::Left, Modifiers::default());
            cx.simulate_mouse_move(at(10.), MouseButton::Left, Modifiers::default());
            cx.simulate_mouse_up(at(10.), MouseButton::Left, Modifiers::default());
            view.read_with(cx, |view, _| view.flash.is_some())
        };

        view.update(cx, |view, _| view.config.clipboard_toast.enabled = false);
        assert!(!drag(&view, cx), "a silent copy is still a copy");
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some("configured".into())
        );

        // Each corner lands where it says, measured against the terminal area.
        view.update(cx, |view, _| view.config.clipboard_toast.enabled = true);
        let bounds = view.read_with(cx, |view, _| view.bounds);
        let mut seen = Vec::new();
        for position in [
            TopLeft,
            TopCenter,
            TopRight,
            BottomLeft,
            BottomCenter,
            BottomRight,
        ] {
            view.update(cx, |view, _| {
                view.config.clipboard_toast.position = position
            });
            assert!(drag(&view, cx));
            let flash = cx.debug_bounds("flash").expect("the flash paints");
            let top = matches!(position, TopLeft | TopCenter | TopRight);
            assert_eq!(
                flash.origin.y - bounds.origin.y < bounds.size.height / 2.,
                top,
                "{position:?}"
            );
            let left = flash.origin.x - bounds.origin.x;
            let right = bounds.size.width - (left + flash.size.width);
            match position {
                TopLeft | BottomLeft => assert!(left < right, "{position:?}"),
                TopRight | BottomRight => assert!(right < left, "{position:?}"),
                TopCenter | BottomCenter => {
                    assert!((left - right).abs() <= px(1.), "{position:?}")
                }
            }
            assert!(
                !seen.contains(&(flash.origin.x, flash.origin.y)),
                "{position:?}"
            );
            seen.push((flash.origin.x, flash.origin.y));
        }
    }

    /// A menu page holds the whole gesture: nothing is selected, copied, or
    /// reported while one is up.
    #[gpui_kit::test]
    fn a_menu_page_holds_the_gesture(cx: &mut TestAppContext) {
        let (view, cx) = crate::test_support::add_window_view(cx, |window, cx| {
            let mut view = fixture_window(window, cx);
            let mut frame = surface(&["copied text"], 12);
            let snapshot = view.live.snapshot.as_ref().unwrap();
            frame.boot_id = snapshot.boot_id.clone();
            frame.projection_revision = snapshot.revision;
            view.live.surface = Some(Arc::new(frame));
            view
        });
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        let (origin, cell) = view.read_with(cx, |view, _| {
            (
                view.bounds.origin,
                (view.cell_width, view.config.terminal.line_height()),
            )
        });
        let at = |column: f32| origin + point(px(column * cell.0), px(0.5 * cell.1));

        cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("kept".into())));
        view.update(cx, |view, cx| {
            view.menu.page = Some(crate::menu::Page::Menu);
            view.begin_selection(at(0.), cx);
            assert!(view.selection.is_none());
            assert!(!view.extend_selection(at(6.), cx));
            assert!(!view.release_selection(cx));
            assert!(view.flash.is_none());
            view.menu.page = None;
        });
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some("kept".into())
        );

        // The same drag with the menu gone copies and reports.
        cx.simulate_mouse_down(at(0.), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(at(6.), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(at(6.), MouseButton::Left, Modifiers::default());
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some("copied".into())
        );
        view.read_with(cx, |view, _| assert!(view.flash.is_some()));
    }
}
