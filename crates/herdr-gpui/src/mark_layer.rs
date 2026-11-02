//! Moving status marks, drawn in a layer of their own.
//!
//! An animation asks GPUI to re-render the view drawing it on every frame,
//! and GPUI re-renders that view's ancestors with it. A working agent's
//! spinner inside the cached sidebar rebuilt and laid out every row up to 30
//! times a second, terminal output or not; one in the tab strip notified the
//! window, which notifies the sidebar too (`HerdrWindow::new` observes
//! itself). So in a window with a layer, [`layered`] only reserves the
//! mark's slot in the sidebar or the window and records where it landed. The
//! layer, a view of its own painted over the window body, draws every mark
//! there, and its animations re-render nothing but the layer and the window.
//!
//! Anywhere else (the settings window's previews, test views, and a popover
//! deferred past the layer) the mark draws in place, as it always did.
//!
//! The layer paints after everything in the window body, so what a host
//! draws over its own rows, such as the sidebar's pinned host header, marks
//! itself with [`covering`] and the layer keeps marks out from under it.
//! Nor can the layer inherit a host's hover styles, so a host that hides a
//! mark under the pointer wraps the hovered area in [`hidden_under_pointer`].

use gpui::{prelude::*, *};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::{Rc, Weak},
};

type Build = Rc<dyn Fn() -> AnyElement>;

/// The most frames a second a repeating mark asks for. A window can show
/// many at once, and a turning or pulsing mark reads as smooth at this rate.
pub(crate) const MAX_FPS: f32 = 30.;

/// One reserved slot: where it landed, the clip it landed under, and how to
/// draw the mark there.
struct Slot {
    host: EntityId,
    bounds: Bounds<Pixels>,
    mask: ContentMask<Pixels>,
    fade: f32,
    /// The area of [`hidden_under_pointer`] it sits in, if any.
    hider: Option<HitboxId>,
    build: Build,
}

/// One window's reserved slots and the views allowed to reserve them.
#[derive(Default)]
pub(crate) struct Marks {
    hosts: RefCell<Vec<EntityId>>,
    slots: RefCell<Vec<Slot>>,
    /// What each host draws over its own rows, which no mark shows through.
    covers: RefCell<Vec<(EntityId, Bounds<Pixels>)>>,
    /// Whether the layer is still to prepaint this frame. A slot recorded
    /// after it would show only on the next frame, under whatever covered it.
    open: Cell<bool>,
    /// The [`hidden_under_pointer`] area being prepainted, if any.
    hider: Cell<Option<HitboxId>>,
}

impl Marks {
    /// The marks of `window`, when it has a layer.
    pub(crate) fn of(window: &Window, cx: &App) -> Option<Rc<Self>> {
        cx.try_global::<Layers>()?
            .0
            .get(&window.window_handle().window_id())?
            .upgrade()
    }

    /// Opens a frame for the window's root view `root`, which renders on
    /// every frame and so reserves its slots afresh each time.
    pub(crate) fn begin_frame(&self, root: EntityId) {
        self.open.set(true);
        self.forget(root);
    }

    /// Drops the slots `host` reserved, before it reserves them again or
    /// when it is not drawn at all. A cached view that is not re-rendered
    /// keeps its slots, as GPUI keeps its paint.
    pub(crate) fn forget(&self, host: EntityId) {
        self.slots.borrow_mut().retain(|slot| slot.host != host);
        self.covers.borrow_mut().retain(|(cover, _)| *cover != host);
    }

    /// `mask` less what its host covers: a cover as wide as the mask, such
    /// as a pinned header across the list, moves the edge it overlaps.
    fn uncovered(&self, host: EntityId, mut mask: ContentMask<Pixels>) -> ContentMask<Pixels> {
        for (_, cover) in self
            .covers
            .borrow()
            .iter()
            .filter(|(cover, _)| *cover == host)
        {
            let area = mask.bounds;
            if !cover.intersects(&area)
                || cover.left() > area.left()
                || cover.right() < area.right()
            {
                continue;
            }
            let (mut top, mut bottom) = (area.top(), area.bottom());
            if cover.top() <= top {
                top = cover.bottom().min(bottom);
            } else if cover.bottom() >= bottom {
                bottom = cover.top().max(top);
            }
            mask.bounds =
                Bounds::from_corners(point(area.left(), top), point(area.right(), bottom));
        }
        mask
    }

    fn hosts(&self, view: EntityId) -> bool {
        self.hosts.borrow().contains(&view)
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.slots.borrow().len()
    }
}

/// Every window's marks, so a slot finds its window's layer.
#[derive(Default)]
struct Layers(HashMap<WindowId, Weak<Marks>>);

impl Global for Layers {}

/// The layer drawing one window's moving marks.
pub(crate) struct MarkLayer {
    marks: Rc<Marks>,
    #[cfg(test)]
    pub(crate) renders: usize,
}

impl MarkLayer {
    /// A layer for `window`, drawing the marks `hosts` reserve.
    pub(crate) fn new(window: &Window, hosts: Vec<EntityId>, cx: &mut App) -> Self {
        let marks = Rc::new(Marks {
            hosts: RefCell::new(hosts),
            ..Default::default()
        });
        let layers = &mut cx.default_global::<Layers>().0;
        layers.retain(|_, marks| marks.strong_count() > 0);
        layers.insert(window.window_handle().window_id(), Rc::downgrade(&marks));
        Self {
            marks,
            #[cfg(test)]
            renders: 0,
        }
    }
}

impl Render for MarkLayer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        {
            self.renders += 1;
        }
        LayerElement {
            marks: self.marks.clone(),
            drawn: Vec::new(),
        }
    }
}

struct LayerElement {
    marks: Rc<Marks>,
    drawn: Vec<(ContentMask<Pixels>, Option<HitboxId>, AnyElement)>,
}

impl IntoElement for LayerElement {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for LayerElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        // Takes no room: every mark is placed at its slot's own bounds.
        let style = Style {
            position: Position::Absolute,
            ..Default::default()
        };
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.marks.open.set(false);
        let slots: Vec<_> = self
            .marks
            .slots
            .borrow()
            .iter()
            .map(|slot| {
                let mask = self.marks.uncovered(slot.host, slot.mask);
                (slot.bounds, mask, slot.fade, slot.hider, slot.build.clone())
            })
            .collect();
        self.drawn = slots
            .into_iter()
            .enumerate()
            .map(|(index, (bounds, mask, fade, hider, build))| {
                // Centered in the reserved box, as the slot around it
                // centers a mark drawn smaller than its size.
                let mut mark = div()
                    .w(bounds.size.width)
                    .h(bounds.size.height)
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(fade < 1., |mark| mark.opacity(fade))
                    .child(build())
                    .into_any_element();
                window.with_element_namespace(index, |window| {
                    window.with_content_mask(Some(mask), |window| {
                        mark.prepaint_as_root(
                            bounds.origin,
                            bounds.size.map(AvailableSpace::Definite),
                            window,
                            cx,
                        );
                    });
                });
                (mask, hider, mark)
            })
            .collect();
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        for (index, (mask, hider, mark)) in self.drawn.iter_mut().enumerate() {
            // Hover is known only now: GPUI hit-tests a frame between its
            // prepaint and its paint.
            if hider.is_some_and(|hider| hider.is_hovered(window)) {
                continue;
            }
            window.with_element_namespace(index, |window| {
                window.with_content_mask(Some(*mask), |window| mark.paint(window, cx));
            });
        }
    }
}

/// `element`, drawn over its view's own rows: no layered mark under it
/// shows through.
pub(crate) fn covering(element: impl IntoElement) -> impl IntoElement {
    Covering(element.into_any_element())
}

struct Covering(AnyElement);

impl IntoElement for Covering {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Covering {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        // Laid out as the element itself, so these bounds are its own.
        (self.0.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(marks) = Marks::of(window, cx) {
            let host = window.current_view();
            if marks.hosts(host) {
                marks.covers.borrow_mut().push((host, bounds));
            }
        }
        self.0.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.0.paint(window, cx);
    }
}

/// `element`, whose layered marks are not drawn while the pointer is over
/// it, as a hover style hiding them would do in place: a mark in the layer
/// never sees its host's hover opacity.
pub(crate) fn hidden_under_pointer(element: impl IntoElement) -> impl IntoElement {
    HiddenUnderPointer(element.into_any_element())
}

struct HiddenUnderPointer(AnyElement);

impl IntoElement for HiddenUnderPointer {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for HiddenUnderPointer {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.0.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(marks) = Marks::of(window, cx).filter(|marks| marks.hosts(window.current_view()))
        else {
            self.0.prepaint(window, cx);
            return;
        };
        // A hitbox of its own, inserted before what it holds, so the pointer
        // over anything inside counts and a popup over it does not, exactly
        // as GPUI decides hover styles.
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let outer = marks.hider.replace(Some(hitbox.id));
        self.0.prepaint(window, cx);
        marks.hider.set(outer);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.0.paint(window, cx);
    }
}

/// A moving mark `size` across, drawn by the window's layer when this sits
/// in a view that hosts one, and in place otherwise. `fade` is the opacity
/// of the row around it, which the layer cannot inherit.
pub(crate) fn layered(
    size: f32,
    fade: f32,
    build: impl Fn() -> AnyElement + 'static,
) -> impl IntoElement {
    Layered {
        size,
        fade,
        build: Rc::new(build),
        marks: None,
        drawn: None,
    }
}

struct Layered {
    size: f32,
    fade: f32,
    build: Build,
    marks: Option<Rc<Marks>>,
    drawn: Option<AnyElement>,
}

impl IntoElement for Layered {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Layered {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        self.marks = Marks::of(window, cx).filter(|marks| marks.hosts(window.current_view()));
        if self.marks.is_some() {
            // The mark's own box, so the row keeps its geometry.
            let mut style = Style::default();
            style.size.width = px(self.size).into();
            style.size.height = px(self.size).into();
            style.flex_shrink = 0.;
            return (window.request_layout(style, [], cx), ());
        }
        let mut mark = (self.build)();
        let layout = mark.request_layout(window, cx);
        self.drawn = Some(mark);
        (layout, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(marks) = &self.marks else {
            if let Some(mark) = &mut self.drawn {
                mark.prepaint(window, cx);
            }
            return;
        };
        if marks.open.get() {
            marks.slots.borrow_mut().push(Slot {
                host: window.current_view(),
                bounds,
                mask: window.content_mask(),
                fade: self.fade,
                hider: marks.hider.get(),
                build: self.build.clone(),
            });
            return;
        }
        // Deferred past the layer: drawn here, under this row's own opacity.
        let mut mark = div()
            .size(px(self.size))
            .flex()
            .items_center()
            .justify_center()
            .child((self.build)())
            .into_any_element();
        mark.prepaint_as_root(
            bounds.origin,
            bounds.size.map(AvailableSpace::Definite),
            window,
            cx,
        );
        self.drawn = Some(mark);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(mark) = &mut self.drawn {
            mark.paint(window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::Marks;
    use crate::sidebar::layout_tests::{fixture_window, run_frames};
    use gpui::{Bounds, ContentMask, EntityId, point, px, size};
    use std::time::Duration;

    /// A header pinned across the list hides the marks under it: their clip
    /// starts below it. A narrower cover, or another host's, clips nothing.
    #[test]
    fn a_full_width_cover_moves_the_clip_edge_it_overlaps() {
        let (list, other) = (EntityId::from(1_u64), EntityId::from(2_u64));
        let mask = ContentMask {
            bounds: Bounds::new(point(px(0.), px(40.)), size(px(200.), px(300.))),
        };
        let marks = Marks::default();
        let header = Bounds::new(point(px(0.), px(40.)), size(px(200.), px(24.)));
        marks.covers.borrow_mut().push((list, header));
        let uncovered = marks.uncovered(list, mask);
        assert_eq!(uncovered.bounds.top(), px(64.));
        assert_eq!(uncovered.bounds.bottom(), px(340.));
        assert_eq!(marks.uncovered(other, mask), mask);
        let narrow = Marks::default();
        let chip = Bounds::new(point(px(20.), px(40.)), size(px(60.), px(24.)));
        narrow.covers.borrow_mut().push((list, chip));
        assert_eq!(narrow.uncovered(list, mask), mask);
        marks.forget(list);
        assert_eq!(marks.uncovered(list, mask), mask);
    }

    /// Working agents spin in the layer: time passing redraws the layer
    /// and never the cached sidebar, and the marks still draw.
    #[gpui::test]
    fn moving_marks_redraw_the_layer_and_not_the_sidebar(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(fixture_window);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let (sidebar, layer) = cx.update(|_, cx| {
            let view = view.read(cx);
            (view.sidebar_view.clone(), view.mark_layer.clone())
        });
        let (renders, layer_renders) = cx.update(|window, cx| {
            let marks = Marks::of(window, cx).expect("the window has a layer");
            assert!(
                marks.len() > 0,
                "the fixture's working agents reserve slots"
            );
            (sidebar.read(cx).renders, layer.read(cx).renders)
        });
        assert!(cx.debug_bounds("status-orbit").is_some());
        run_frames(cx, 4, Duration::from_millis(150));
        cx.update(|_, cx| {
            assert_eq!(sidebar.read(cx).renders, renders);
            assert!(layer.read(cx).renders >= layer_renders + 4);
        });
        assert!(cx.debug_bounds("status-orbit").is_some());
    }
}
