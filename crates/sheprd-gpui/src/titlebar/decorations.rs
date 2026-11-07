//! Window chrome the app draws itself when the compositor will not. GNOME's
//! Wayland session has no server-side decorations, so GPUI falls back to
//! client-side ones there: without this module such a window has no frame,
//! no resize edges, and no buttons, and only a keyboard shortcut can close it.
//! Everywhere else the platform reports server decorations and nothing here
//! draws.

use crate::direction::Logical;
use crate::i18n::fl;
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

/// Whether the app draws its own window control buttons (minimize, maximize,
/// close) in the menu bar. True for client-decorated windows where the buttons
/// are needed, or when the user explicitly enables them. With server-side
/// decorations minimize/maximize may not work on some compositors (GNOME
/// Wayland ignores client requests), but close and the user's choice to show
/// them are respected.
pub(crate) fn show_controls(enabled: bool, window: &Window) -> bool {
    enabled || client(window)
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
    fn of(enabled: bool, window: &Window) -> Option<Self> {
        let controls = window.window_controls();
        // When the user explicitly enables window controls, show all three
        // buttons regardless of what the WM reports. The WM-reported controls
        // only matter for client-decorated windows where the compositor decides.
        let force_all = enabled;
        show_controls(enabled, window).then(|| Self {
            minimize: force_all || controls.minimize,
            maximize: force_all || controls.maximize,
            maximized: window.is_maximized(),
        })
    }
}

/// Minimize, maximize or restore, and close, at the end of the title bar of
/// a client-decorated window: the right, or the left right to left, as
/// Windows puts its own. `close` runs the window's own close
/// path, so a window that asks before closing still asks.
pub(crate) fn controls(
    enabled: bool,
    window: &Window,
    theme: &crate::config::Theme,
    close: impl Fn(&mut Window, &mut App) + 'static,
) -> Option<AnyElement> {
    Buttons::of(enabled, window).map(|buttons| render_controls(buttons, theme, close))
}

fn render_controls(
    buttons: Buttons,
    theme: &crate::config::Theme,
    close: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    let (active, foreground) = (theme.active, theme.foreground);
    // `label` is what a screen reader calls the button.
    let button = |id: &'static str, icon: &'static str, label: String| {
        div()
            .id(id)
            .debug_selector(move || id.into())
            .role(Role::Button)
            .aria_label(label)
            .flex_none()
            .size(px(28.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .cursor_pointer()
            .hover(move |style| style.bg(rgb(active)))
            // Stop the press from bubbling to the movable bar, which would start
            // a window move instead of letting the button's click fire.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(svg().path(icon).size(px(14.)).text_color(rgb(foreground)))
    };
    div()
        .debug_selector(|| "window-controls".into())
        .row()
        .flex_none()
        .items_center()
        .gap(px(4.))
        .ms(auto())
        .ps(px(4.))
        .pe(px(6.))
        .h_full()
        .when(buttons.minimize, |bar| {
            bar.child(
                button(
                    "window-minimize",
                    "icons/window-minimize.svg",
                    fl!("titlebar-minimize"),
                )
                .on_click(|_, window, cx| {
                    cx.stop_propagation();
                    window.minimize_window();
                }),
            )
        })
        .when(buttons.maximize, |bar| {
            let (icon, label) = if buttons.maximized {
                ("icons/window-restore.svg", fl!("titlebar-restore"))
            } else {
                ("icons/window-maximize.svg", fl!("titlebar-maximize"))
            };
            bar.child(
                button("window-maximize", icon, label).on_click(|_, window, cx| {
                    cx.stop_propagation();
                    window.zoom_window();
                }),
            )
        })
        .child(
            button("window-close", "icons/close.svg", fl!("titlebar-close")).on_click(
                move |_, window, cx| {
                    cx.stop_propagation();
                    close(window, cx);
                },
            ),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests;
