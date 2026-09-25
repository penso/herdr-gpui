//! Resolve user-selected web links within the painted pane or popup only.
use super::{HIDDEN, InputTarget, popup_origin, wheel_target};
use herdr_client::protocol::{FrameData, PaneSurfaceFrame};

const MAX_URL_BYTES: usize = 8192;
const MAX_ROW_BYTES: usize = 32768;

fn web_url(value: &str) -> Option<String> {
    if value.len() > MAX_URL_BYTES || value.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return None;
    }
    let url = url::Url::parse(value).ok()?;
    (matches!(url.scheme(), "http" | "https") && url.host_str().is_some()).then(|| url.into())
}

pub(crate) fn link_at(
    surface: &PaneSurfaceFrame,
    x: f32,
    y: f32,
    cell_width: f32,
    cell_height: f32,
) -> Option<String> {
    // Share popup isolation, pane bounds, and invalid-geometry handling with input.
    let target = wheel_target(surface, x, y, cell_width, cell_height)?;
    let (frame, x, y, start, end) = match target.target {
        InputTarget::Popup(_) => {
            let popup = surface.popup.as_ref()?;
            let origin = popup_origin(&surface.frame, &popup.frame, cell_width, cell_height);
            (
                &popup.frame,
                x - f32::from(origin.x),
                y - f32::from(origin.y),
                0,
                popup.frame.width,
            )
        }
        InputTarget::Pane(id) => {
            let pane = surface.panes.iter().find(|pane| pane.pane_id == id)?;
            (
                &surface.frame,
                x,
                y,
                pane.inner_rect.x,
                pane.inner_rect.x.saturating_add(pane.inner_rect.width),
            )
        }
    };
    frame_link(
        frame,
        (x / cell_width).floor() as u16,
        (y / cell_height).floor() as u16,
        start,
        end,
    )
}

fn frame_link(frame: &FrameData, column: u16, row: u16, start: u16, end: u16) -> Option<String> {
    if column < start || column >= end || end > frame.width || row >= frame.height {
        return None;
    }
    let offset = usize::from(row) * usize::from(frame.width);
    let cells = frame
        .cells
        .get(offset + usize::from(start)..offset + usize::from(end))?;
    let selected = usize::from(column - start);
    let mut source = selected;
    while cells.get(source)?.skip && source > 0 {
        source -= 1;
    }
    let cell = cells.get(source)?;
    if cell.modifier & HIDDEN != 0 {
        return None;
    }
    // An explicit link is authoritative, even if its destination is disallowed.
    if let Some(index) = cells[selected].hyperlink.or(cell.hyperlink) {
        return web_url(frame.hyperlinks.get(index as usize)?);
    }
    let mut text = String::new();
    let mut hit = 0;
    for (index, cell) in cells.iter().enumerate() {
        if index == source {
            hit = text.len();
        }
        if cell.skip {
            continue;
        }
        let symbol = if cell.modifier & HIDDEN != 0 || cell.symbol.is_empty() {
            " "
        } else {
            &cell.symbol
        };
        if text.len() + symbol.len() > MAX_ROW_BYTES {
            return None;
        }
        text.push_str(symbol);
    }
    // Plain URLs are row-local: the protocol doesn't distinguish soft wraps from
    // separate lines, so joining rows could silently change the destination.
    for (start, _) in text.match_indices("http") {
        if start > hit {
            break;
        }
        let tail = &text[start..];
        if !tail.starts_with("http://") && !tail.starts_with("https://") {
            continue;
        }
        let end = tail
            .find(|c: char| {
                c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | '"' | '\'' | '`')
            })
            .unwrap_or(tail.len());
        // Check the original token: punctuation at the edge may be part of a
        // destination continuing off-screen or on the next row.
        if end == tail.len() {
            return None;
        }
        let mut candidate = tail[..end].trim_end_matches(['.', ',', ';', ':', '!', '?']);
        for (open, close) in [('(', ')'), ('[', ']'), ('{', '}')] {
            let excess = candidate
                .matches(close)
                .count()
                .saturating_sub(candidate.matches(open).count());
            for _ in 0..excess {
                let Some(trimmed) = candidate.strip_suffix(close) else {
                    break;
                };
                candidate = trimmed;
            }
        }
        if hit < start + candidate.len() {
            return web_url(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use herdr_client::protocol::*;
    use std::sync::Arc;

    fn frame(text: &str, width: u16, height: u16) -> FrameData {
        let mut symbols = text.chars();
        FrameData {
            width,
            height,
            cells: (0..usize::from(width) * usize::from(height))
                .map(|_| CellData {
                    symbol: symbols.next().unwrap_or(' ').to_string(),
                    fg: 0,
                    bg: 0,
                    modifier: 0,
                    skip: false,
                    hyperlink: None,
                })
                .collect(),
            cursor: None,
            hyperlinks: vec![],
            graphics: vec![],
        }
    }

    fn surface(text: &str) -> PaneSurfaceFrame {
        let rect = SurfaceRect {
            x: 0,
            y: 0,
            width: 80,
            height: 4,
        };
        PaneSurfaceFrame {
            boot_id: "boot".into(),
            projection_revision: 1,
            surface_revision: 1,
            frame: frame(text, 80, 4),
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
                pixel_width: 800,
                pixel_height: 80,
            }],
        }
    }

    #[test]
    fn explicit_links_validate_destination_and_support_wide_cells() {
        let mut s = surface("界 ");
        s.frame.hyperlinks = vec!["https://example.com/docs?q=one#two".into()];
        s.frame.cells[0].hyperlink = Some(0);
        s.frame.cells[1].skip = true;
        for x in [1., 11.] {
            assert_eq!(
                link_at(&s, x, 1., 10., 20.).as_deref(),
                Some("https://example.com/docs?q=one#two")
            );
        }
        for invalid in [
            "file:///tmp/a",
            "javascript:alert(1)",
            "data:text/html,test",
            "mailto:a@example.com",
            "https://",
            "https://example.com/\nnext",
            "https://example.com/a b",
        ] {
            s.frame.hyperlinks[0] = invalid.into();
            assert!(link_at(&s, 1., 1., 10., 20.).is_none(), "{invalid}");
        }
        s.frame.hyperlinks[0] = "https://example.com".into();
        s.frame.cells[0].modifier = HIDDEN;
        assert!(link_at(&s, 1., 1., 10., 20.).is_none());
        assert!(
            web_url(&format!(
                "https://example.com/{}",
                "a".repeat(MAX_URL_BYTES)
            ))
            .is_none()
        );
    }

    #[test]
    fn plain_urls_trim_prose_preserve_balanced_paths_and_keep_hit_ranges() {
        for (text, expected) in [
            (
                "See (https://example.com/docs). next",
                "https://example.com/docs",
            ),
            ("https://example.com/a_(b)", "https://example.com/a_(b)"),
            (
                "https://example.com/?q=yes#section",
                "https://example.com/?q=yes#section",
            ),
            ("http://localhost:3000/path", "http://localhost:3000/path"),
        ] {
            let s = surface(text);
            let start = text.find("http").unwrap();
            assert_eq!(
                link_at(&s, start as f32 * 10. + 1., 1., 10., 20.).as_deref(),
                Some(expected)
            );
            assert!(link_at(&s, 791., 1., 10., 20.).is_none());
        }
        let s = surface("https://one.test https://two.test");
        assert_eq!(
            link_at(&s, 181., 1., 10., 20.).as_deref(),
            Some("https://two.test/")
        );
        let mut s = surface("https://example.com");
        s.frame.cells[0].hyperlink = Some(0);
        s.frame.hyperlinks = vec!["file:///tmp/no".into()];
        assert!(link_at(&s, 1., 1., 10., 20.).is_none());
        s.frame.cells[0].hyperlink = Some(99);
        assert!(link_at(&s, 1., 1., 10., 20.).is_none());
    }

    #[test]
    fn geometry_is_bounded_and_popup_blocks_underlying_links() {
        let mut s = surface("https://example.com");
        for (x, y, w, h) in [
            (-1., 1., 10., 20.),
            (800., 1., 10., 20.),
            (1., 80., 10., 20.),
            (f32::NAN, 1., 10., 20.),
            (1., 1., 0., 20.),
        ] {
            assert!(link_at(&s, x, y, w, h).is_none());
        }
        s.panes[0].inner_rect.x = 20;
        s.panes[0].inner_rect.width = 60;
        assert!(link_at(&s, 1., 1., 10., 20.).is_none());
        s.popup = Some(Box::new(ClientShellPopupSurface {
            terminal_id: "popup".into(),
            title: String::new(),
            width: None,
            height: None,
            frame: frame("https://popup.test", 20, 2),
            mouse_reporting: false,
            sgr_pixel_mouse: false,
            pixel_width: 200,
            pixel_height: 40,
        }));
        assert!(link_at(&s, 1., 1., 10., 20.).is_none());
        assert_eq!(
            link_at(&s, 301., 21., 10., 20.).as_deref(),
            Some("https://popup.test/")
        );
        s.popup.as_mut().unwrap().frame.cells[0].symbol = "a".repeat(MAX_ROW_BYTES + 1);
        assert!(link_at(&s, 301., 21., 10., 20.).is_none());
    }

    #[test]
    fn plain_links_do_not_cross_panes_or_guess_wrapped_destinations() {
        let mut s = surface("https://example.com");
        s.panes[0].inner_rect.width = 12;
        assert!(link_at(&s, 1., 1., 10., 20.).is_none());
        let wrapped = frame("https://example.com/long", 12, 2);
        assert!(frame_link(&wrapped, 0, 0, 0, 12).is_none());
        for punctuation in ['.', ',', ';', ':', '!', '?', ')', ']', '}'] {
            let text = format!("https://example{punctuation}com/path");
            let wrapped = frame(&text, 16, 2);
            assert!(frame_link(&wrapped, 0, 0, 0, 16).is_none(), "{text}");
            let mut s = surface(&text);
            s.panes[0].inner_rect.width = 16;
            assert!(link_at(&s, 1., 1., 10., 20.).is_none(), "{text}");
        }
        let unicode = frame("界 https://example.com ", 40, 1);
        assert_eq!(
            frame_link(&unicode, 3, 0, 0, 40).as_deref(),
            Some("https://example.com/")
        );
    }

    #[gpui_kit::test]
    fn click_dispatch_opens_browser_and_respects_menu_and_revision(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use gpui_kit::{point, px};
        let (view, cx) = crate::test_support::add_window_view(cx, |window, cx| {
            let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
            let mut s = surface("https://example.com/click");
            let snapshot = view.live.snapshot.as_ref().unwrap();
            s.boot_id = snapshot.boot_id.clone();
            s.projection_revision = snapshot.revision;
            view.live.surface = Some(Arc::new(s));
            view
        });
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        let position = view.read_with(cx, |view, _| view.bounds.origin + point(px(1.), px(1.)));
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let mut event = gpui_kit::MouseClickEvent::default();
                event.down.position = position;
                event.up.position = position + point(px(20.), px(0.));
                event.down.click_count = 1;
                view.pressed_terminal_link = Some(("https://example.com/click".into(), position));
                view.open_terminal_link(&gpui_kit::ClickEvent::Mouse(event.clone()), window, cx);
                event.up.position = position;
                view.pressed_terminal_link = Some(("https://different.example/".into(), position));
                view.open_terminal_link(&gpui_kit::ClickEvent::Mouse(event), window, cx);
            })
        });
        assert!(cx.opened_url().is_none());
        for away in [
            position + point(px(20.), px(0.)),
            position + point(px(0.), px(20.)),
            position - point(px(20.), px(20.)),
        ] {
            cx.simulate_mouse_down(position, gpui_kit::MouseButton::Left, Default::default());
            cx.simulate_mouse_move(away, gpui_kit::MouseButton::Left, Default::default());
            cx.simulate_mouse_move(position, gpui_kit::MouseButton::Left, Default::default());
            cx.simulate_mouse_up(position, gpui_kit::MouseButton::Left, Default::default());
            assert!(cx.opened_url().is_none());
            view.read_with(cx, |view, _| assert!(view.pressed_terminal_link.is_none()));
        }
        cx.simulate_click(position, Default::default());
        assert_eq!(
            cx.opened_url().as_deref(),
            Some("https://example.com/click")
        );
        view.update(cx, |view, _| {
            assert!(view.pending_navigation.is_none());
            assert!(view.pressed_terminal_link.is_none());
            view.menu.page = Some(crate::menu::Page::Menu);
            assert!(view.terminal_link_at(position).is_none());
            view.menu.page = None;
            Arc::make_mut(view.live.surface.as_mut().unwrap()).projection_revision += 1;
            assert!(view.terminal_link_at(position).is_none());
        });
    }
}
