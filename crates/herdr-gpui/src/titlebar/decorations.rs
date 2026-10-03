//! Window chrome the app draws itself when the compositor will not. GNOME's
//! Wayland session has no server-side decorations, so GPUI falls back to
//! client-side ones there: without this module such a window has no frame,
//! no resize edges, and no buttons, and only a keyboard shortcut can close it.
//! Everywhere else the platform reports server decorations and nothing here
//! draws.

use gpui::{prelude::*, *};

/// The transparent band around a client-decorated window that holds its
/// shadow and resize handles. GPUI keeps it outside the window geometry the
/// compositor tiles and snaps, so a maximized window has no gap.
pub(crate) const INSET: f32 = 10.;

/// Draws the frame around `content`: the shadow band, a hairline border, and
/// resize handles on every edge the compositor has not tiled.
pub(crate) fn frame(window: &mut Window, border: u32, content: impl IntoElement) -> Div {
    let decorations = window.window_decorations();
    if matches!(decorations, Decorations::Client { .. }) {
        window.set_client_inset(px(INSET));
    }
    framed(decorations, border, content)
}

fn framed(decorations: Decorations, border: u32, content: impl IntoElement) -> Div {
    let Decorations::Client { tiling } = decorations else {
        return div().size_full().child(content);
    };
    let inset = px(INSET);
    div()
        .debug_selector(|| "window-frame".into())
        .size_full()
        .relative()
        .when(!tiling.top, |frame| frame.pt(inset))
        .when(!tiling.bottom, |frame| frame.pb(inset))
        .when(!tiling.left, |frame| frame.pl(inset))
        .when(!tiling.right, |frame| frame.pr(inset))
        .child(
            div()
                .debug_selector(|| "window-frame-content".into())
                .size_full()
                .overflow_hidden()
                .cursor(CursorStyle::Arrow)
                .border_color(rgb(border))
                .when(!tiling.top, |content| content.border_t_1())
                .when(!tiling.bottom, |content| content.border_b_1())
                .when(!tiling.left, |content| content.border_l_1())
                .when(!tiling.right, |content| content.border_r_1())
                .when(!tiling.is_tiled(), |content| {
                    content.shadow(vec![BoxShadow {
                        color: hsla(0., 0., 0., 0.4),
                        offset: point(px(0.), px(0.)),
                        blur_radius: inset / 2.,
                        spread_radius: px(0.),
                        inset: false,
                    }])
                })
                .child(content),
        )
        .children(handles(tiling))
}

/// One handle per untiled edge, and one per corner whose two edges are both
/// untiled. They cover only the inset band, never the window's own content.
fn handles(tiling: Tiling) -> impl Iterator<Item = Div> {
    let inset = px(INSET);
    [
        (ResizeEdge::Top, !tiling.top),
        (ResizeEdge::Bottom, !tiling.bottom),
        (ResizeEdge::Left, !tiling.left),
        (ResizeEdge::Right, !tiling.right),
        (ResizeEdge::TopLeft, !(tiling.top || tiling.left)),
        (ResizeEdge::TopRight, !(tiling.top || tiling.right)),
        (ResizeEdge::BottomLeft, !(tiling.bottom || tiling.left)),
        (ResizeEdge::BottomRight, !(tiling.bottom || tiling.right)),
    ]
    .into_iter()
    .filter(|(_, untiled)| *untiled)
    .map(move |(edge, _)| {
        let handle = div()
            .debug_selector(move || selector(edge).into())
            .absolute()
            .cursor(cursor(edge))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                cx.stop_propagation();
                window.start_window_resize(edge);
            });
        match edge {
            ResizeEdge::Top => handle.top_0().left(inset).right(inset).h(inset),
            ResizeEdge::Bottom => handle.bottom_0().left(inset).right(inset).h(inset),
            ResizeEdge::Left => handle.left_0().top(inset).bottom(inset).w(inset),
            ResizeEdge::Right => handle.right_0().top(inset).bottom(inset).w(inset),
            ResizeEdge::TopLeft => handle.top_0().left_0().size(inset),
            ResizeEdge::TopRight => handle.top_0().right_0().size(inset),
            ResizeEdge::BottomLeft => handle.bottom_0().left_0().size(inset),
            ResizeEdge::BottomRight => handle.bottom_0().right_0().size(inset),
        }
    })
}

fn selector(edge: ResizeEdge) -> &'static str {
    match edge {
        ResizeEdge::Top => "window-resize-top",
        ResizeEdge::Bottom => "window-resize-bottom",
        ResizeEdge::Left => "window-resize-left",
        ResizeEdge::Right => "window-resize-right",
        ResizeEdge::TopLeft => "window-resize-top-left",
        ResizeEdge::TopRight => "window-resize-top-right",
        ResizeEdge::BottomLeft => "window-resize-bottom-left",
        ResizeEdge::BottomRight => "window-resize-bottom-right",
    }
}

fn cursor(edge: ResizeEdge) -> CursorStyle {
    match edge {
        ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
        ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
        ResizeEdge::TopLeft | ResizeEdge::BottomRight => CursorStyle::ResizeUpLeftDownRight,
        ResizeEdge::TopRight | ResizeEdge::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
    }
}

/// Whether the title bar has to move the window itself, rather than leaving
/// it to AppKit or a server-drawn frame.
pub(crate) fn client(window: &Window) -> bool {
    matches!(window.window_decorations(), Decorations::Client { .. })
}

/// Which buttons to draw, from what the compositor says it supports. Close
/// is always offered, since nothing else on screen can close the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Buttons {
    minimize: bool,
    maximize: bool,
    maximized: bool,
}

impl Buttons {
    fn of(window: &Window) -> Option<Self> {
        let controls = window.window_controls();
        client(window).then(|| Self {
            minimize: controls.minimize,
            maximize: controls.maximize,
            maximized: window.is_maximized(),
        })
    }
}

/// Minimize, maximize or restore, and close, at the right end of the title
/// bar of a client-decorated window. `close` runs the window's own close
/// path, so a window that asks before closing still asks.
pub(crate) fn controls(
    window: &Window,
    theme: &crate::config::Theme,
    close: impl Fn(&mut Window, &mut App) + 'static,
) -> Option<AnyElement> {
    Buttons::of(window).map(|buttons| render_controls(buttons, theme, close))
}

fn render_controls(
    buttons: Buttons,
    theme: &crate::config::Theme,
    close: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    let (active, foreground) = (theme.active, theme.foreground);
    let button = |id: &'static str, icon: &'static str| {
        div()
            .id(id)
            .debug_selector(move || id.into())
            .flex_none()
            .size(px(28.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .cursor_pointer()
            .hover(move |style| style.bg(rgb(active)))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(svg().path(icon).size(px(14.)).text_color(rgb(foreground)))
    };
    div()
        .debug_selector(|| "window-controls".into())
        .flex()
        .flex_none()
        .items_center()
        .gap(px(4.))
        .ml_auto()
        .pl(px(4.))
        .pr(px(6.))
        .h_full()
        .when(buttons.minimize, |bar| {
            bar.child(
                button("window-minimize", "icons/window-minimize.svg").on_click(|_, window, cx| {
                    cx.stop_propagation();
                    window.minimize_window();
                }),
            )
        })
        .when(buttons.maximize, |bar| {
            let icon = if buttons.maximized {
                "icons/window-restore.svg"
            } else {
                "icons/window-maximize.svg"
            };
            bar.child(button("window-maximize", icon).on_click(|_, window, cx| {
                cx.stop_propagation();
                window.zoom_window();
            }))
        })
        .child(
            button("window-close", "icons/close.svg").on_click(move |_, window, cx| {
                cx.stop_propagation();
                close(window, cx);
            }),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::{Buttons, framed, render_controls, selector};
    use gpui::{
        Bounds, Context, Decorations, InteractiveElement, IntoElement, Modifiers, ParentElement,
        Pixels, Render, ResizeEdge, Styled, TestAppContext, Tiling, VisualTestContext, Window, div,
        point, px, size,
    };
    use std::{cell::Cell, rc::Rc};

    const EDGES: [ResizeEdge; 8] = [
        ResizeEdge::Top,
        ResizeEdge::Bottom,
        ResizeEdge::Left,
        ResizeEdge::Right,
        ResizeEdge::TopLeft,
        ResizeEdge::TopRight,
        ResizeEdge::BottomLeft,
        ResizeEdge::BottomRight,
    ];

    const ALL: Buttons = Buttons {
        minimize: true,
        maximize: true,
        maximized: false,
    };

    const FLOATING: Decorations = Decorations::Client {
        tiling: Tiling {
            top: false,
            left: false,
            right: false,
            bottom: false,
        },
    };

    struct Fixture {
        decorations: Decorations,
        buttons: Buttons,
        closed: Rc<Cell<usize>>,
    }

    impl Render for Fixture {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let closed = self.closed.clone();
            let theme = crate::config::Theme::default();
            framed(
                self.decorations,
                theme.active,
                div()
                    .debug_selector(|| "fixture-body".into())
                    .size_full()
                    .flex()
                    .child(div().flex_1())
                    .child(render_controls(self.buttons, &theme, move |_, _| {
                        closed.set(closed.get() + 1);
                    })),
            )
        }
    }

    fn open(
        cx: &mut TestAppContext,
        decorations: Decorations,
        buttons: Buttons,
    ) -> (Rc<Cell<usize>>, &mut VisualTestContext) {
        let closed = Rc::new(Cell::new(0));
        let shared = closed.clone();
        let (_, cx) = cx.add_window_view(move |_, _| Fixture {
            decorations,
            buttons,
            closed: shared,
        });
        cx.simulate_resize(size(px(400.), px(300.)));
        cx.update(|window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        });
        (closed, cx)
    }

    fn full() -> Bounds<Pixels> {
        Bounds::new(point(px(0.), px(0.)), size(px(400.), px(300.)))
    }

    #[gpui::test]
    fn server_decorations_add_no_frame(cx: &mut TestAppContext) {
        let (_, cx) = open(cx, Decorations::Server, ALL);
        assert!(cx.debug_bounds("window-frame").is_none());
        for edge in EDGES {
            assert!(cx.debug_bounds(selector(edge)).is_none(), "{edge:?}");
        }
        assert_eq!(cx.debug_bounds("fixture-body").unwrap(), full());
    }

    #[gpui::test]
    fn floating_window_insets_content_and_keeps_handles_in_the_band(cx: &mut TestAppContext) {
        let (_, cx) = open(cx, FLOATING, ALL);
        let body = cx.debug_bounds("fixture-body").unwrap();
        // The inset, then the 1px border.
        assert_eq!(
            body,
            Bounds::new(point(px(11.), px(11.)), size(px(378.), px(278.)))
        );
        for edge in EDGES {
            let handle = cx.debug_bounds(selector(edge)).unwrap();
            assert!(!handle.intersects(&body), "{edge:?} covers content");
            assert!(full().contains(&handle.origin), "{edge:?}");
            assert!(handle.size.width > px(0.) && handle.size.height > px(0.));
        }
        assert_eq!(
            cx.debug_bounds(selector(ResizeEdge::TopRight)).unwrap(),
            Bounds::new(point(px(390.), px(0.)), size(px(10.), px(10.)))
        );
        assert_eq!(
            cx.debug_bounds(selector(ResizeEdge::Bottom)).unwrap(),
            Bounds::new(point(px(10.), px(290.)), size(px(380.), px(10.)))
        );
    }

    #[gpui::test]
    fn tiled_edges_lose_their_inset_and_handles(cx: &mut TestAppContext) {
        let decorations = Decorations::Client {
            tiling: Tiling {
                top: true,
                left: true,
                right: false,
                bottom: false,
            },
        };
        let (_, cx) = open(cx, decorations, ALL);
        assert_eq!(
            cx.debug_bounds("fixture-body").unwrap(),
            Bounds::new(point(px(0.), px(0.)), size(px(389.), px(289.)))
        );
        for edge in EDGES {
            let kept = matches!(
                edge,
                ResizeEdge::Bottom | ResizeEdge::Right | ResizeEdge::BottomRight
            );
            assert_eq!(cx.debug_bounds(selector(edge)).is_some(), kept, "{edge:?}");
        }
    }

    #[gpui::test]
    fn maximized_window_fills_the_surface(cx: &mut TestAppContext) {
        let maximized = Decorations::Client {
            tiling: Tiling::tiled(),
        };
        let (_, cx) = open(cx, maximized, ALL);
        assert_eq!(cx.debug_bounds("fixture-body").unwrap(), full());
        for edge in EDGES {
            assert!(cx.debug_bounds(selector(edge)).is_none(), "{edge:?}");
        }
    }

    #[gpui::test]
    fn close_runs_the_windows_own_close_path(cx: &mut TestAppContext) {
        let (closed, cx) = open(cx, FLOATING, ALL);
        let minimize = cx.debug_bounds("window-minimize").unwrap();
        let maximize = cx.debug_bounds("window-maximize").unwrap();
        let close = cx.debug_bounds("window-close").unwrap();
        assert!(minimize.right() <= maximize.left());
        assert!(maximize.right() <= close.left());
        assert!(close.right() <= cx.debug_bounds("fixture-body").unwrap().right());
        assert_eq!(close.size, size(px(28.), px(28.)));

        cx.simulate_click(close.center(), Modifiers::default());
        assert_eq!(closed.get(), 1);
    }

    #[gpui::test]
    fn unsupported_controls_are_not_offered(cx: &mut TestAppContext) {
        let close_only = Buttons {
            minimize: false,
            maximize: false,
            maximized: false,
        };
        let (_, cx) = open(cx, FLOATING, close_only);
        assert!(cx.debug_bounds("window-minimize").is_none());
        assert!(cx.debug_bounds("window-maximize").is_none());
        assert!(cx.debug_bounds("window-close").is_some());
    }
}
