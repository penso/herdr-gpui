//! The sidebar's two resizable boundaries: its width against the terminal and
//! the split between spaces and agents. The kit's panel groups own the drag;
//! this keeps them and the stored chrome preferences in step.

use super::metrics::{
    MAX_WIDTH, MIN_WIDTH, SECTION_MIN_HEIGHT, TERMINAL_MIN_WIDTH, sidebar_width, split_fraction,
};
use crate::HerdrWindow;
use gpui_kit::component::{ResizableState, h_resizable, resizable_panel, v_resizable};
use gpui_kit::{prelude::*, *};

pub(crate) struct Panels {
    pub(super) width: Entity<ResizableState>,
    pub(super) split: Entity<ResizableState>,
    _redraws: [Subscription; 2],
}

impl Panels {
    pub(crate) fn new(cx: &mut Context<HerdrWindow>) -> Self {
        let width = cx.new(|_| ResizableState::default());
        let split = cx.new(|_| ResizableState::default());
        Self {
            _redraws: [redraw_on_resize(&width, cx), redraw_on_resize(&split, cx)],
            width,
            split,
        }
    }
}

/// A panel group notifies its state on every prepaint, whether or not a size
/// changed. Only a changed size needs the window rebuilt: redrawing for each
/// notification would keep an idle window drawing forever.
fn redraw_on_resize(state: &Entity<ResizableState>, cx: &mut Context<HerdrWindow>) -> Subscription {
    let mut last = Vec::new();
    cx.observe(state, move |_, state, cx| {
        let sizes = state.read(cx).sizes();
        if *sizes != last {
            last.clone_from(sizes);
            cx.notify();
        }
    })
}

/// Moves a group's first boundary to `size` unless a drag is moving it. The
/// kit rescales panels proportionally when their container changes, while the
/// stored preferences are a width in pixels and a split fraction; each frame
/// re-applies the preference once the group has been measured.
fn hold(state: &Entity<ResizableState>, size: Pixels, window: &mut Window, cx: &mut App) {
    if cx.has_active_drag() {
        return;
    }
    let state_now = state.read(cx);
    let Some(current) = state_now.sizes().first().copied() else {
        return;
    };
    if state_now.sizes().len() < 2
        || state_now.container_size() <= px(0.)
        || (f32::from(current) - f32::from(size)).abs() < 0.5
    {
        return;
    }
    state.update(cx, |state, cx| state.resize_panel(0, size, window, cx));
}

impl HerdrWindow {
    /// The window body: the sidebar and whatever the window adds as children,
    /// divided by the kit's resize handle.
    pub(crate) fn sidebar_body(&self, window: &mut Window, cx: &mut Context<Self>) -> Body {
        if !self.sidebar_visible {
            return Body {
                sidebar: None,
                children: Vec::new(),
            };
        }
        let width = sidebar_width(self.sidebar_width, f32::from(window.viewport_size().width));
        hold(&self.sidebar_panels.width, px(width), window, cx);
        Body {
            sidebar: Some(SidebarPanel {
                view: super::cached_view(&self.sidebar_view).into_any_element(),
                state: self.sidebar_panels.width.clone(),
                width,
                window: cx.entity().downgrade(),
            }),
            children: Vec::new(),
        }
    }

    /// Re-applies the stored split to the sidebar's own panel group.
    pub(super) fn hold_split(&self, window: &mut Window, cx: &mut App) {
        let state = &self.sidebar_panels.split;
        let height = f32::from(state.read(cx).container_size());
        hold(
            state,
            px(height * split_fraction(self.sidebar_split)),
            window,
            cx,
        );
    }

    /// Stores the split a drag left behind.
    pub(super) fn split_resized(&mut self, state: &ResizableState) {
        let [spaces, agents] = state.sizes()[..] else {
            return;
        };
        let total = f32::from(spaces + agents);
        if total <= 0. {
            return;
        }
        self.sidebar_split = Some(split_fraction(Some(f32::from(spaces) / total)));
        self.sidebar_split_modified = true;
        self.save_chrome();
    }
}

struct SidebarPanel {
    view: AnyElement,
    state: Entity<ResizableState>,
    width: f32,
    window: WeakEntity<HerdrWindow>,
}

/// See `HerdrWindow::sidebar_body`.
#[derive(IntoElement)]
pub(crate) struct Body {
    sidebar: Option<SidebarPanel>,
    children: Vec<AnyElement>,
}

impl ParentElement for Body {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Body {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let body = div()
            .debug_selector(|| "window-body".into())
            .flex()
            .flex_1()
            .min_h_0();
        let Some(sidebar) = self.sidebar else {
            return body.children(self.children);
        };
        let window = sidebar.window;
        body.child(
            h_resizable("sidebar-width")
                .with_state(&sidebar.state)
                .on_resize(move |state, _, cx| {
                    let Some(width) = state.read(cx).sizes().first().copied() else {
                        return;
                    };
                    let _ = window.update(cx, |this, cx| {
                        this.sidebar_width = Some(f32::from(width));
                        this.save_sidebar_width();
                        cx.notify();
                    });
                })
                .child(
                    resizable_panel()
                        .size(px(sidebar.width))
                        .size_range(px(MIN_WIDTH)..px(MAX_WIDTH))
                        .child(sidebar.view),
                )
                .child(
                    resizable_panel()
                        .size_range(px(TERMINAL_MIN_WIDTH)..Pixels::MAX)
                        .child(
                            div()
                                .flex()
                                .size_full()
                                .min_w_0()
                                .min_h_0()
                                .children(self.children),
                        ),
                ),
        )
    }
}

/// The spaces and agents sections, split by the kit's resize handle.
pub(super) fn split(
    state: &Entity<ResizableState>,
    window: WeakEntity<HerdrWindow>,
    spaces: impl IntoElement,
    agents: impl IntoElement,
) -> impl IntoElement {
    v_resizable("sidebar-split")
        .with_state(state)
        .on_resize(move |state, _, cx| {
            let _ = window.update(cx, |this, cx| {
                this.split_resized(state.read(cx));
                cx.notify();
            });
        })
        .child(
            resizable_panel()
                .size_range(px(SECTION_MIN_HEIGHT)..Pixels::MAX)
                .child(spaces),
        )
        .child(
            resizable_panel()
                .size_range(px(SECTION_MIN_HEIGHT)..Pixels::MAX)
                .child(agents),
        )
}
