use crate::config::Theme;
use gpui::{
    Bounds, KeyDownEvent, Keystroke, Modifiers, Pixels, Point, ScrollDelta, ScrollWheelEvent,
    TouchPhase, point, px, size,
};
use herdr_client::protocol::{
    CellData, ClientKeyCode, ClientKeyKind, ClientMouseGeometry, ClientMouseKind,
    ClientMousePosition, ClientPaneInputEvent, ClientSurfaceSize, CursorState, FrameData,
    PaneSurfaceFrame, SurfaceRect,
};

#[cfg(test)]
pub const BACKGROUND: u32 = 0x101419;
#[cfg(test)]
pub const FOREGROUND: u32 = 0xd8dee9;
#[cfg(test)]
pub const FONT_SIZE: f32 = 14.;
#[cfg(test)]
pub const CELL_HEIGHT: f32 = 20.;

pub(crate) const BOLD: u16 = 1;
pub(crate) const DIM: u16 = 1 << 1;
pub(crate) const ITALIC: u16 = 1 << 2;
pub(crate) const UNDERLINE: u16 = 1 << 3;
pub(crate) const REVERSED: u16 = 1 << 6;
pub(crate) const HIDDEN: u16 = 1 << 7;
pub(crate) const STRIKETHROUGH: u16 = 1 << 8;

pub(crate) fn popup_origin(
    frame: &FrameData,
    popup: &FrameData,
    cell_width: f32,
    cell_height: f32,
) -> Point<Pixels> {
    point(
        px((frame.width.saturating_sub(popup.width) as f32 * cell_width / 2.).floor()),
        px(frame.height.saturating_sub(popup.height) as f32 * cell_height / 2.),
    )
}

pub(crate) fn cursor_offset(
    cursor: &CursorState,
    cell_width: f32,
    cell_height: f32,
) -> Point<Pixels> {
    point(
        px(cursor.x as f32 * cell_width),
        px(cursor.y as f32 * cell_height),
    )
}

pub(crate) fn input_cursor_bounds(
    surface: Option<&PaneSurfaceFrame>,
    origin: Point<Pixels>,
    cell_width: f32,
    cell_height: f32,
) -> Bounds<Pixels> {
    let mut origin = origin;
    if let Some(surface) = surface {
        let frame = if let Some(popup) = &surface.popup {
            origin += popup_origin(&surface.frame, &popup.frame, cell_width, cell_height);
            &popup.frame
        } else {
            &surface.frame
        };
        if let Some(cursor) = &frame.cursor {
            origin += cursor_offset(cursor, cell_width, cell_height);
        }
    }
    Bounds::new(origin, size(px(cell_width), px(cell_height)))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InputTarget {
    Pane(String),
    Popup(String),
}

#[derive(Default)]
pub struct WheelAccumulator {
    target: Option<InputTarget>,
    remainder: f32,
}

impl WheelAccumulator {
    pub fn lines(
        &mut self,
        target: &InputTarget,
        event: &ScrollWheelEvent,
        cell_height: f32,
    ) -> i16 {
        if self.target.as_ref() != Some(target) || matches!(event.touch_phase, TouchPhase::Started)
        {
            self.remainder = 0.;
            self.target = Some(target.clone());
        }
        let delta = match event.delta {
            ScrollDelta::Pixels(delta) => delta.y.to_f64() as f32 / cell_height,
            ScrollDelta::Lines(delta) => delta.y,
        };
        if !delta.is_finite() {
            return 0;
        }
        if delta != 0. && delta.signum() != self.remainder.signum() {
            self.remainder = 0.;
        }
        // Bound each event's work; keep sub-cell trackpad motion, not an input backlog.
        let total = (self.remainder + delta).clamp(-128., 128.);
        let lines = total.trunc() as i16;
        self.remainder = total - f32::from(lines);
        lines
    }
}

pub struct WheelTarget {
    pub target: InputTarget,
    position: ClientMousePosition,
    geometry: Option<ClientMouseGeometry>,
}

impl WheelTarget {
    pub fn event(&self, lines: i16, modifiers: Modifiers) -> ClientPaneInputEvent {
        ClientPaneInputEvent::Mouse {
            kind: if lines > 0 {
                ClientMouseKind::ScrollUp
            } else {
                ClientMouseKind::ScrollDown
            },
            position: self.position,
            geometry: self.geometry,
            modifiers: u8::from(modifiers.shift)
                | (u8::from(modifiers.control) << 1)
                | (u8::from(modifiers.alt) << 2)
                | (u8::from(modifiers.platform) << 3),
            lines: lines.unsigned_abs(),
        }
    }
}

pub fn wheel_target(
    surface: &PaneSurfaceFrame,
    x: f32,
    y: f32,
    cell_width: f32,
    cell_height: f32,
) -> Option<WheelTarget> {
    if !x.is_finite()
        || !y.is_finite()
        || !cell_width.is_finite()
        || !cell_height.is_finite()
        || cell_width <= 0.
        || cell_height <= 0.
        || x < 0.
        || y < 0.
    {
        return None;
    }
    let (target, rect, pixel_mouse, width_px, height_px, origin_x, origin_y) =
        if let Some(popup) = &surface.popup {
            let origin = popup_origin(&surface.frame, &popup.frame, cell_width, cell_height);
            (
                InputTarget::Popup(popup.terminal_id.clone()),
                SurfaceRect {
                    x: 0,
                    y: 0,
                    width: popup.frame.width,
                    height: popup.frame.height,
                },
                popup.sgr_pixel_mouse,
                popup.pixel_width,
                popup.pixel_height,
                origin.x.to_f64() as f32,
                origin.y.to_f64() as f32,
            )
        } else {
            let pane = surface.panes.iter().find(|pane| {
                let r = pane.inner_rect;
                x >= r.x as f32 * cell_width
                    && x < (u32::from(r.x) + u32::from(r.width)) as f32 * cell_width
                    && y >= r.y as f32 * cell_height
                    && y < (u32::from(r.y) + u32::from(r.height)) as f32 * cell_height
            })?;
            (
                InputTarget::Pane(pane.pane_id.clone()),
                pane.inner_rect,
                pane.sgr_pixel_mouse,
                pane.pixel_width,
                pane.pixel_height,
                pane.inner_rect.x as f32 * cell_width,
                pane.inner_rect.y as f32 * cell_height,
            )
        };
    let x = x - origin_x;
    let y = y - origin_y;
    if x < 0.
        || y < 0.
        || x >= rect.width as f32 * cell_width
        || y >= rect.height as f32 * cell_height
    {
        return None;
    }
    let column = (x / cell_width).floor() as u16;
    let row = (y / cell_height).floor() as u16;
    let geometry = (pixel_mouse && width_px > 0 && height_px > 0).then_some(ClientMouseGeometry {
        cols: rect.width,
        rows: rect.height,
        width_px,
        height_px,
    });
    let position = if geometry.is_some() {
        ClientMousePosition::Pixels {
            x: (x / (rect.width as f32 * cell_width) * width_px as f32).floor() as u32,
            y: (y / (rect.height as f32 * cell_height) * height_px as f32).floor() as u32,
            column,
            row,
        }
    } else {
        ClientMousePosition::Cell { column, row }
    };
    Some(WheelTarget {
        target,
        position,
        geometry,
    })
}

pub fn color(value: u32, default: u32, theme: &Theme) -> u32 {
    match value >> 24 {
        0 => match value & 255 {
            1..=16 => theme.palette[((value & 255) - 1) as usize],
            _ => default,
        },
        1 => theme.palette[(value & 255) as usize],
        2 => value & 0xffffff,
        _ => default,
    }
}

pub fn cell_colors(cell: &CellData, theme: &Theme) -> (u32, u32) {
    let mut fg = color(cell.fg, theme.foreground, theme);
    let mut bg = color(cell.bg, theme.background, theme);
    if cell.modifier & REVERSED != 0 {
        std::mem::swap(&mut fg, &mut bg);
    }
    if cell.modifier & DIM != 0 {
        fg = ((fg & 0xfefefe) >> 1) + ((bg & 0xfefefe) >> 1);
    }
    if cell.modifier & HIDDEN != 0 {
        fg = bg;
    }
    (fg, bg)
}

pub fn viewport(width: f32, height: f32, cell_width: f32, cell_height: f32) -> ClientSurfaceSize {
    let cols = (width / cell_width.max(1.)).floor().clamp(1., 4096.) as u16;
    let rows = (height / cell_height.max(1.)).floor().clamp(1., 4096.) as u16;
    ClientSurfaceSize {
        cols,
        rows: rows.min((1_000_000 / u32::from(cols)) as u16),
    }
}

// Printable text belongs to EntityInputHandler, not key-down: this preserves
// keyboard layouts, dead keys and IME commits without double-sending characters.
pub fn key_input(event: &KeyDownEvent) -> Option<ClientPaneInputEvent> {
    key_code(&event.keystroke).map(|code| ClientPaneInputEvent::Key {
        code,
        modifiers: u8::from(event.keystroke.modifiers.shift)
            | (u8::from(event.keystroke.modifiers.control) << 1)
            | (u8::from(event.keystroke.modifiers.alt) << 2),
        kind: if event.is_held {
            ClientKeyKind::Repeat
        } else {
            ClientKeyKind::Press
        },
        repeat_count: 1,
        shifted_codepoint: None,
        generated_text: None,
        tracks_release: false,
        physical_key_id: None,
        windows_record: None,
    })
}

fn key_code(key: &Keystroke) -> Option<ClientKeyCode> {
    use ClientKeyCode::*;
    if key.modifiers.platform {
        return None;
    }
    Some(match key.key.as_str() {
        "enter" => Enter,
        "backspace" | "back" => Backspace,
        "escape" => Esc,
        "tab" if key.modifiers.shift => BackTab,
        "tab" => Tab,
        "up" => Up,
        "down" => Down,
        "left" => Left,
        "right" => Right,
        "home" => Home,
        "end" => End,
        "pageup" => PageUp,
        "pagedown" => PageDown,
        "delete" => Delete,
        "insert" => Insert,
        name if name.starts_with('f')
            && name[1..].parse::<u8>().is_ok_and(|n| (1..=24).contains(&n)) =>
        {
            F(name[1..].parse().ok()?)
        }
        "space" if key.modifiers.control => Char(' '),
        name if key.modifiers.control && name.chars().count() == 1 => Char(name.chars().next()?),
        _ => return None,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn wheel_preserves_fractions_and_resets_on_target_direction_or_gesture_change() {
        let mut wheel = WheelAccumulator::default();
        let pane = InputTarget::Pane("pane".into());
        let other = InputTarget::Pane("other".into());
        let popup = InputTarget::Popup("other".into());
        let mut event = ScrollWheelEvent {
            delta: ScrollDelta::Pixels(point(px(0.), px(12.))),
            touch_phase: TouchPhase::Moved,
            ..Default::default()
        };
        assert_eq!(wheel.lines(&pane, &event, CELL_HEIGHT), 0);
        assert_eq!(wheel.lines(&pane, &event, CELL_HEIGHT), 1);
        assert_eq!(wheel.lines(&other, &event, CELL_HEIGHT), 0);
        assert_eq!(wheel.lines(&popup, &event, CELL_HEIGHT), 0);
        event.touch_phase = TouchPhase::Started;
        assert_eq!(wheel.lines(&popup, &event, CELL_HEIGHT), 0);
        event.touch_phase = TouchPhase::Moved;
        event.delta = ScrollDelta::Lines(point(0., -1.));
        assert_eq!(wheel.lines(&popup, &event, CELL_HEIGHT), -1);
        event.delta = ScrollDelta::Lines(point(0., 1e9));
        assert_eq!(wheel.lines(&popup, &event, CELL_HEIGHT), 128);
        event.delta = ScrollDelta::Lines(point(10., 0.));
        assert_eq!(wheel.lines(&popup, &event, CELL_HEIGHT), 0);
    }

    #[test]
    fn nonfinite_wheel_deltas_do_not_poison_fractional_motion() {
        let pane = InputTarget::Pane("pane".into());
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut wheel = WheelAccumulator::default();
            let mut event = ScrollWheelEvent {
                delta: ScrollDelta::Lines(point(0., 0.75)),
                touch_phase: TouchPhase::Moved,
                ..Default::default()
            };
            assert_eq!(wheel.lines(&pane, &event, CELL_HEIGHT), 0);
            event.delta = ScrollDelta::Lines(point(0., invalid));
            assert_eq!(wheel.lines(&pane, &event, CELL_HEIGHT), 0);
            event.delta = ScrollDelta::Lines(point(0., 0.25));
            assert_eq!(wheel.lines(&pane, &event, CELL_HEIGHT), 1);
            event.delta = ScrollDelta::Lines(point(0., -1e9));
            assert_eq!(wheel.lines(&pane, &event, CELL_HEIGHT), -128);
            event.delta = ScrollDelta::Lines(point(0., 0.));
            assert_eq!(wheel.lines(&pane, &event, CELL_HEIGHT), 0);
        }
    }

    #[test]
    fn wheel_hits_inner_pane_and_uses_relative_coordinates_and_semantic_modes() {
        use herdr_client::protocol::*;
        let frame = FrameData {
            cells: vec![],
            width: 80,
            height: 24,
            cursor: None,
            hyperlinks: vec![],
            graphics: vec![],
        };
        let mut surface = PaneSurfaceFrame {
            boot_id: "boot".into(),
            projection_revision: 1,
            surface_revision: 1,
            frame: frame.clone(),
            splits: vec![],
            popup: None,
            graphics: Default::default(),
            panes: vec![PaneSurfacePane {
                pane_id: "pane".into(),
                content_revision: 1,
                rect: SurfaceRect {
                    x: 0,
                    y: 0,
                    width: 40,
                    height: 24,
                },
                inner_rect: SurfaceRect {
                    x: 1,
                    y: 1,
                    width: 38,
                    height: 22,
                },
                scrollbar_rect: None,
                scroll: None,
                focused: false,
                mouse_reporting: false,
                sgr_pixel_mouse: false,
                alternate_screen_active: false,
                pixel_width: 380,
                pixel_height: 440,
            }],
        };
        assert!(wheel_target(&surface, -1., 25., 10., CELL_HEIGHT).is_none());
        assert!(wheel_target(&surface, 5., 25., 10., CELL_HEIGHT).is_none());
        assert!(wheel_target(&surface, 400., 25., 10., CELL_HEIGHT).is_none());
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0., -1.] {
            assert!(wheel_target(&surface, 35., 65., invalid, CELL_HEIGHT).is_none());
            assert!(wheel_target(&surface, 35., 65., 10., invalid).is_none());
        }
        for alternate in [false, true] {
            surface.panes[0].alternate_screen_active = alternate;
            let target = wheel_target(&surface, 35., 65., 10., CELL_HEIGHT).unwrap();
            assert_eq!(target.target, InputTarget::Pane("pane".into()));
            assert_eq!(
                target.position,
                ClientMousePosition::Cell { column: 2, row: 2 }
            );
            assert!(matches!(
                target.event(-3, Modifiers::default()),
                ClientPaneInputEvent::Mouse {
                    kind: ClientMouseKind::ScrollDown,
                    lines: 3,
                    ..
                }
            ));
        }
        surface.panes[0].sgr_pixel_mouse = true;
        let target = wheel_target(&surface, 35., 65., 10., CELL_HEIGHT).unwrap();
        assert_eq!(
            target.position,
            ClientMousePosition::Pixels {
                x: 25,
                y: 45,
                column: 2,
                row: 2
            }
        );
        assert_eq!(target.geometry.unwrap().cols, 38);
        let target = wheel_target(&surface, 35., 97.5, 10., 30.).unwrap();
        assert_eq!(
            target.position,
            ClientMousePosition::Pixels {
                x: 25,
                y: 45,
                column: 2,
                row: 2,
            }
        );
        surface.popup = Some(Box::new(ClientShellPopupSurface {
            terminal_id: "popup".into(),
            title: String::new(),
            width: None,
            height: None,
            frame: FrameData {
                width: 20,
                height: 10,
                ..frame
            },
            mouse_reporting: true,
            sgr_pixel_mouse: false,
            pixel_width: 200,
            pixel_height: 200,
        }));
        assert!(wheel_target(&surface, 35., 65., 10., CELL_HEIGHT).is_none());
        let target = wheel_target(&surface, 315., 165., 10., CELL_HEIGHT).unwrap();
        assert_eq!(target.target, InputTarget::Popup("popup".into()));
        assert_eq!(
            target.position,
            ClientMousePosition::Cell { column: 1, row: 1 }
        );
        let target = wheel_target(&surface, 315., 247.5, 10., 30.).unwrap();
        assert_eq!(
            target.position,
            ClientMousePosition::Cell { column: 1, row: 1 }
        );

        let origin = point(px(17.), px(29.));
        let cursor = CursorState {
            x: 2,
            y: 3,
            visible: false,
            shape: 0,
        };
        surface.frame.cursor = Some(CursorState {
            x: 70,
            y: 20,
            ..cursor.clone()
        });
        surface.popup.as_mut().unwrap().frame.cursor = Some(cursor);
        // A hidden popup cursor still anchors IME; never use the base cursor.
        assert_eq!(
            input_cursor_bounds(Some(&surface), origin, 8.5, CELL_HEIGHT),
            Bounds::new(origin + point(px(272.), px(200.)), size(px(8.5), px(20.)))
        );
        assert_eq!(
            input_cursor_bounds(Some(&surface), origin, 8.5, 30.5),
            Bounds::new(origin + point(px(272.), px(305.)), size(px(8.5), px(30.5)))
        );
        surface.popup.as_mut().unwrap().frame.cursor = None;
        assert_eq!(
            input_cursor_bounds(Some(&surface), origin, 8.5, CELL_HEIGHT).origin,
            origin + point(px(255.), px(140.))
        );
        assert_eq!(
            input_cursor_bounds(Some(&surface), origin, 8.5, 30.5).origin,
            origin + point(px(255.), px(213.5))
        );
        surface.popup = None;
        assert_eq!(
            input_cursor_bounds(Some(&surface), origin, 8.5, CELL_HEIGHT).origin,
            origin + point(px(595.), px(400.))
        );
        assert_eq!(
            input_cursor_bounds(Some(&surface), origin, 8.5, 30.5).origin,
            origin + point(px(595.), px(610.))
        );
        surface.frame.cursor = None;
        assert_eq!(
            input_cursor_bounds(Some(&surface), origin, 8.5, CELL_HEIGHT).origin,
            origin
        );
        assert_eq!(
            input_cursor_bounds(None, origin, 8.5, CELL_HEIGHT).origin,
            origin
        );
        for surface in [Some(&surface), None] {
            assert_eq!(
                input_cursor_bounds(surface, origin, 8.5, 30.5),
                Bounds::new(origin, size(px(8.5), px(30.5)))
            );
        }
    }

    #[test]
    fn popup_origin_rounds_horizontal_pixels_and_saturates_oversized_frames() {
        let frame = FrameData {
            width: 81,
            height: 25,
            cells: vec![],
            cursor: None,
            hyperlinks: vec![],
            graphics: vec![],
        };
        let popup = FrameData {
            width: 20,
            height: 10,
            ..frame.clone()
        };
        assert_eq!(
            popup_origin(&frame, &popup, 8.5, CELL_HEIGHT),
            point(px(259.), px(150.))
        );
        assert_eq!(
            popup_origin(&popup, &frame, 8.5, CELL_HEIGHT),
            Point::default()
        );
        assert_eq!(
            popup_origin(&frame, &popup, 8.5, 30.5),
            point(px(259.), px(228.75))
        );
        assert_eq!(popup_origin(&popup, &frame, 8.5, 30.5), Point::default());
    }

    #[test]
    fn wire_colors_are_not_argb() {
        let theme = Theme::default();
        assert_eq!(color(0, FOREGROUND, &theme), FOREGROUND);
        assert_eq!(color(0, BACKGROUND, &theme), BACKGROUND);
        for (i, expected) in theme.palette[..16].iter().enumerate() {
            assert_eq!(color(i as u32 + 1, 0, &theme), *expected);
            assert_eq!(color(0x01000000 | i as u32, 0, &theme), *expected);
        }
        assert_eq!(color(0x02123456, 0, &theme), 0x123456);
        assert_eq!(color(0x01000010, 1, &theme), 0);
        assert_eq!(color(0x01000015, 0, &theme), 0x0000ff);
        assert_eq!(color(0x010000e7, 0, &theme), 0xffffff);
        assert_eq!(color(0x010000e8, 0, &theme), 0x080808);
        assert_eq!(color(0x010000ff, 0, &theme), 0xeeeeee);
        assert_eq!(color(0xff000000, 42, &theme), 42);
    }

    #[test]
    fn reverse_and_hidden_colors() {
        let mut cell = CellData {
            symbol: "x".into(),
            fg: 0x02ff0000,
            bg: 0x020000ff,
            modifier: 1 << 6,
            skip: false,
            hyperlink: None,
        };
        assert_eq!(cell_colors(&cell, &Theme::default()), (0x0000ff, 0xff0000));
        cell.modifier |= 1 << 7;
        assert_eq!(cell_colors(&cell, &Theme::default()), (0xff0000, 0xff0000));
    }

    #[test]
    fn geometry_is_bounded_and_uses_terminal_viewport() {
        assert_eq!(
            viewport(800., 480., 10., CELL_HEIGHT),
            ClientSurfaceSize { cols: 80, rows: 24 }
        );
        assert_eq!(
            viewport(0., 0., 10., CELL_HEIGHT),
            ClientSurfaceSize { cols: 1, rows: 1 }
        );
        assert_eq!(
            viewport(800., 480., 12.5, 30.),
            ClientSurfaceSize { cols: 64, rows: 16 }
        );
        let huge = viewport(1e9, 1e9, 10., CELL_HEIGHT);
        assert!(u32::from(huge.cols) * u32::from(huge.rows) <= 1_000_000);
    }

    #[test]
    fn custom_palette_and_defaults_preserve_truecolor_and_modifiers() {
        let mut theme = Theme {
            foreground: 0xabcdef,
            background: 0x123456,
            ..Theme::default()
        };
        for (i, entry) in theme.palette.iter_mut().enumerate() {
            *entry = 0x654300 + i as u32;
        }
        for i in 0..256 {
            assert_eq!(color(0x01000000 | i, 0, &theme), theme.palette[i as usize]);
            if i < 16 {
                assert_eq!(color(i + 1, 0, &theme), theme.palette[i as usize]);
            }
        }
        let mut cell = CellData {
            symbol: "x".into(),
            fg: 0,
            bg: 0,
            modifier: 0,
            skip: false,
            hyperlink: None,
        };
        assert_eq!(
            cell_colors(&cell, &theme),
            (theme.foreground, theme.background)
        );
        cell.fg = 0x02123456;
        cell.bg = 0x010000ff;
        assert_eq!(cell_colors(&cell, &theme), (0x123456, theme.palette[255]));
        cell.modifier = 1 << 6;
        assert_eq!(cell_colors(&cell, &theme), (theme.palette[255], 0x123456));
        cell.modifier |= 1 << 1;
        assert_eq!(
            cell_colors(&cell, &theme).0,
            ((theme.palette[255] & 0xfefefe) >> 1) + ((0x123456 & 0xfefefe) >> 1)
        );
    }

    #[test]
    fn wheel_uses_configured_height_only_for_pixel_deltas() {
        let mut wheel = WheelAccumulator::default();
        let pane = InputTarget::Pane("pane".into());
        let mut event = ScrollWheelEvent {
            delta: ScrollDelta::Pixels(point(px(0.), px(15.))),
            touch_phase: TouchPhase::Moved,
            ..Default::default()
        };
        assert_eq!(wheel.lines(&pane, &event, 30.), 0);
        assert_eq!(wheel.lines(&pane, &event, 30.), 1);
        event.delta = ScrollDelta::Lines(point(0., 2.));
        assert_eq!(wheel.lines(&pane, &event, 30.), 2);
    }

    #[test]
    fn special_keys_and_text_are_separate() {
        let key = |s| Keystroke::parse(s).unwrap();
        assert_eq!(key_code(&key("ctrl-c")), Some(ClientKeyCode::Char('c')));
        assert_eq!(key_code(&key("shift-tab")), Some(ClientKeyCode::BackTab));
        assert_eq!(key_code(&key("alt-left")), Some(ClientKeyCode::Left));
        assert_eq!(key_code(&key("f12")), Some(ClientKeyCode::F(12)));
        assert_eq!(key_code(&key("a")), None);
        assert_eq!(key_code(&key("alt-e")), None);
        assert_eq!(key_code(&key("cmd-q")), None);
    }
}
