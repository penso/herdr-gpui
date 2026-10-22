#![allow(clippy::unwrap_used)]
use super::*;
use herdr_client::protocol::*;
use std::{ops::Range, sync::Arc};

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

/// A frame of `lines`, one row each, padded with blanks to `width`.
fn rows(lines: &[&str], width: u16) -> FrameData {
    let text: String = lines
        .iter()
        .map(|line| {
            let padding = usize::from(width) - line.chars().count();
            format!("{line}{}", " ".repeat(padding))
        })
        .collect();
    frame(&text, width, lines.len() as u16)
}

/// Reads as a client whose daemon does not resolve links, so an address
/// joins a raw wrap too.
const JOINED: LinkReading = LinkReading {
    raw_url_wraps: true,
    windows_paths: false,
};

fn link_at(surface: &PaneSurfaceFrame, x: f32, y: f32, w: f32, h: f32) -> Option<String> {
    JOINED.link_at(surface, x, y, w, h)
}

fn pane_link_at(surface: &PaneSurfaceFrame, x: f32, y: f32, w: f32, h: f32) -> Option<PaneLink> {
    JOINED.pane_link_at(surface, x, y, w, h)
}

/// What `frame` reads at a cell, with the pane between columns `start`
/// and `end` and over every row.
fn frame_link(frame: &FrameData, column: u16, row: u16, start: u16, end: u16) -> Option<RowLink> {
    let bounds = Bounds {
        columns: start..end,
        rows: 0..frame.height,
    };
    JOINED.frame_link(frame, column, row, &bounds)
}

/// The columns of a link that lies on one row.
fn columns(link: &RowLink) -> Range<u16> {
    assert_eq!(link.rows.len(), 1, "{link:?}");
    link.rows[0].1.clone()
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
    assert!(web_url(&format!("https://example.com/{}", "a".repeat(8192))).is_none());
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
fn plain_links_do_not_cross_panes_and_read_wrapped_destinations_whole() {
    // The pane ends where its cells do: what lies beyond is another pane's,
    // and a blank row below says the address ends at the edge.
    let mut s = surface("https://example.com");
    s.panes[0].inner_rect.width = 12;
    assert_eq!(
        link_at(&s, 1., 1., 10., 20.).as_deref(),
        Some("https://exam/")
    );
    // Filling the pane's bottom row, it may continue off-screen.
    let wrapped = frame("https://example.com/long", 12, 2);
    assert!(frame_link(&wrapped, 0, 0, 0, 12).is_none());
    let wrapped = frame("https://example.com/long", 12, 3);
    assert_eq!(
        frame_link(&wrapped, 0, 0, 0, 12),
        Some(RowLink {
            target: RowTarget::Web("https://example.com/long".into()),
            rows: vec![(0, 0..12), (1, 0..12)],
        })
    );
    for punctuation in ['.', ',', ';', ':', '!', '?', ')', ']', '}'] {
        let text = format!("https://example{punctuation}com/path");
        let wrapped = frame(&text, 16, 2);
        assert_eq!(
            frame_link(&wrapped, 0, 0, 0, 16),
            web_url(&text).map(|url| RowLink {
                target: RowTarget::Web(url),
                rows: vec![(0, 0..16), (1, 0..8)],
            }),
            "{text}"
        );
        let mut s = surface(&text);
        s.panes[0].inner_rect.width = 16;
        assert_eq!(
            link_at(&s, 1., 1., 10., 20.).as_deref(),
            Some("https://example/"),
            "{text}"
        );
    }
    let unicode = frame("界 https://example.com ", 40, 1);
    assert_eq!(
        frame_link(&unicode, 3, 0, 0, 40),
        Some(RowLink {
            target: RowTarget::Web("https://example.com/".into()),
            rows: vec![(0, 2..21)],
        })
    );
}

#[gpui::test]
fn link_modifier_click_bypasses_mouse_reporting_only_on_links(cx: &mut gpui::TestAppContext) {
    use gpui::{Modifiers, point, px};
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        let mut s = surface("https://example.com/app plain");
        s.panes[0].mouse_reporting = true;
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
    let origin = view.read_with(cx, |view, _| view.bounds.origin);
    let link = origin + point(px(1.), px(1.));
    let plain = origin + point(px(251.), px(1.));
    cx.simulate_click(link, Modifiers::default());
    assert!(cx.opened_url().is_none());
    view.read_with(cx, |view, _| {
        assert!(!view.terminal_link_hovered(link, Modifiers::default()));
        assert!(view.terminal_link_hovered(link, Modifiers::secondary_key()));
        assert!(!view.terminal_link_hovered(plain, Modifiers::secondary_key()));
        assert!(!view.link_modifier_held(plain, Modifiers::secondary_key()));
    });
    cx.simulate_click(link, Modifiers::secondary_key());
    assert_eq!(cx.opened_url().as_deref(), Some("https://example.com/app"));
}

#[gpui::test]
fn click_dispatch_opens_browser_and_respects_menu_and_revision(cx: &mut gpui::TestAppContext) {
    use gpui::{point, px};
    let (view, cx) = cx.add_window_view(|window, cx| {
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
            let mut event = gpui::MouseClickEvent::default();
            event.down.position = position;
            event.up.position = position + point(px(20.), px(0.));
            event.down.click_count = 1;
            view.pressed_terminal_link = Some(crate::window::PressedLink {
                url: Some("https://example.com/click".into()),
                cell: None,
                file: None,
                other: None,
                position,
            });
            view.open_terminal_link(&gpui::ClickEvent::Mouse(event.clone()), window, cx);
            event.up.position = position;
            view.pressed_terminal_link = Some(crate::window::PressedLink {
                url: Some("https://different.example/".into()),
                cell: None,
                file: None,
                other: None,
                position,
            });
            view.open_terminal_link(&gpui::ClickEvent::Mouse(event), window, cx);
        })
    });
    assert!(cx.opened_url().is_none());
    for away in [
        position + point(px(20.), px(0.)),
        position + point(px(0.), px(20.)),
        position - point(px(20.), px(20.)),
    ] {
        let held = gpui::Modifiers::secondary_key();
        cx.simulate_mouse_down(position, gpui::MouseButton::Left, held);
        cx.simulate_mouse_move(away, gpui::MouseButton::Left, held);
        cx.simulate_mouse_move(position, gpui::MouseButton::Left, held);
        cx.simulate_mouse_up(position, gpui::MouseButton::Left, held);
        assert!(cx.opened_url().is_none());
        view.read_with(cx, |view, _| assert!(view.pressed_terminal_link.is_none()));
    }
    // Only a click with the link modifier held opens a link.
    cx.simulate_click(position, Default::default());
    assert!(cx.opened_url().is_none());
    view.read_with(cx, |view, _| assert!(view.pressed_terminal_link.is_none()));
    cx.simulate_click(position, gpui::Modifiers::secondary_key());
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

mod drive_paths;
mod escaped_paths;
mod external;
mod paths;
mod queries;
mod wraps;
