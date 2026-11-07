//! A page in the window: the native view WebKit or WebView2 draws a page
//! into, and the element that lays that view out where its group draws.
//!
//! On macOS the view lies beneath the window's own drawing, so the window's
//! menus, toasts, and tooltips draw over it ([`crate::browser::stacking`]).
//! On Windows it is a child window above the window's drawing, and the window
//! hides a page that one of its overlays reaches (`view::present`).
use {
    crate::browser::{TabId, Zoom},
    gpui::{
        App, Bounds, DispatchPhase, Element, ElementId, GlobalElementId, Hitbox, HitboxBehavior,
        InspectorElementId, IntoElement, LayoutId, MouseDownEvent, Pixels, Size, Style, Window,
        point, px,
    },
    std::{
        cell::{Cell, RefCell},
        collections::HashMap,
        rc::Rc,
    },
};

/// Where each page drew in the frame being drawn, in the window's points.
/// Pages fill it as they lay out, and the next frame starts it afresh.
pub(crate) type Drawn = Rc<RefCell<HashMap<TabId, Bounds<Pixels>>>>;

/// What draws a page: a web view, or on macOS, a virtual Mac's display.
enum Surface {
    Web(wry::WebView),
    #[cfg(target_os = "macos")]
    Machine(objc2::rc::Retained<objc2_virtualization::VZVirtualMachineView>),
}

/// One tab's native page.
pub(crate) struct Page {
    view: Surface,
    /// Read as the page lays out, after the window shows or hides it for
    /// the frame: a hidden page stays where it was moved out of the way.
    shown: Cell<bool>,
    /// The zoom the view last took, which starts at the page's own size.
    zoom: Cell<Zoom>,
    /// Hands mouse events between the page and the window's view above it.
    #[cfg(target_os = "macos")]
    router: objc2::rc::Retained<crate::browser::stacking::Router>,
}

impl Page {
    /// Takes over `view`, which wry built as a child of `window`. On macOS it
    /// moves beneath the window's drawing.
    pub(crate) fn new(view: wry::WebView, window: &Window) -> crate::Result<Self> {
        #[cfg(target_os = "macos")]
        let router = {
            use wry::WebViewExtMacOS as _;
            let web =
                objc2::rc::Retained::into_super(objc2::rc::Retained::into_super(view.webview()));
            crate::browser::stacking::stack(web, window)?
        };
        // Nothing until the window lays it out.
        #[cfg(not(target_os = "macos"))]
        {
            let _ = window;
            let _ = view.set_bounds(wry::Rect::default());
        }
        // Shown only once the window presents it.
        Ok(Self {
            view: Surface::Web(view),
            shown: Cell::new(false),
            zoom: Cell::new(Zoom::default()),
            #[cfg(target_os = "macos")]
            router,
        })
    }

    /// Takes over a machine's display, as [`Self::new`] does a web view.
    #[cfg(target_os = "macos")]
    pub(crate) fn machine(
        view: objc2::rc::Retained<objc2_virtualization::VZVirtualMachineView>,
        window: &Window,
    ) -> crate::Result<Self> {
        let native = objc2::rc::Retained::into_super(view.clone());
        native.setHidden(true);
        let router = crate::browser::stacking::stack(native, window)?;
        Ok(Self {
            view: Surface::Machine(view),
            shown: Cell::new(false),
            zoom: Cell::new(Zoom::default()),
            router,
        })
    }

    /// The web view that draws the page, unless a machine does.
    pub(crate) fn web(&self) -> Option<&wry::WebView> {
        match &self.view {
            Surface::Web(view) => Some(view),
            #[cfg(target_os = "macos")]
            Surface::Machine(_) => None,
        }
    }

    /// The view that draws the page.
    #[cfg(target_os = "macos")]
    pub(crate) fn native(&self) -> objc2::rc::Retained<objc2_app_kit::NSView> {
        use objc2::rc::Retained;
        use wry::WebViewExtMacOS as _;
        match &self.view {
            Surface::Web(view) => Retained::into_super(Retained::into_super(view.webview())),
            Surface::Machine(view) => Retained::into_super(view.clone()),
        }
    }

    fn set_visible(&self, visible: bool) {
        match &self.view {
            Surface::Web(view) => {
                let _ = view.set_visible(visible);
            }
            #[cfg(target_os = "macos")]
            Surface::Machine(view) => view.setHidden(!visible),
        }
    }

    pub(crate) fn show(&self) {
        self.shown.set(true);
        self.set_visible(true);
        #[cfg(target_os = "macos")]
        self.router.set_shown(true);
    }

    /// Hides the page, first taking the keyboard back from it: a hidden view
    /// would go on receiving keys.
    pub(crate) fn hide(&self) {
        self.shown.set(false);
        #[cfg(target_os = "macos")]
        {
            self.router.release_keyboard();
            self.router.set_shown(false);
        }
        #[cfg(not(target_os = "macos"))]
        if let Some(view) = self.web() {
            let _ = view.focus_parent();
        }
        self.set_visible(false);
    }

    /// Has the page report light, or dark, as its `prefers-color-scheme`,
    /// as the app's theme is, whatever the system's appearance: so a page
    /// that follows it, such as VS Code's, matches the window it sits in.
    pub(crate) fn set_light(&self, light: bool) {
        #[cfg(target_os = "macos")]
        {
            use objc2_app_kit::{NSAppearance, NSAppearanceCustomization};
            // The values of NSAppearanceNameAqua and NSAppearanceNameDarkAqua.
            let name = if light {
                objc2_foundation::ns_string!("NSAppearanceNameAqua")
            } else {
                objc2_foundation::ns_string!("NSAppearanceNameDarkAqua")
            };
            let appearance = NSAppearance::appearanceNamed(name);
            // A machine's guest keeps its own appearance.
            if let Some(view) = self.web() {
                wry::WebViewExtMacOS::webview(view).setAppearance(appearance.as_deref());
            }
        }
        #[cfg(windows)]
        {
            let theme = if light {
                wry::Theme::Light
            } else {
                wry::Theme::Dark
            };
            if let Some(Err(error)) = self
                .web()
                .map(|view| wry::WebViewExtWindows::set_theme(view, theme))
            {
                tracing::debug!(%error, "Could not set a page's color scheme");
            }
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        let _ = light;
    }

    /// Draws the page at `zoom`. The view keeps it as it navigates, so only a
    /// change reaches the platform. A zoom the platform refused (WebView2 can)
    /// is not taken, so the next presented frame asks again.
    pub(crate) fn zoom(&self, zoom: Zoom) {
        if self.zoom.get() == zoom {
            return;
        }
        let Some(view) = self.web() else {
            return;
        };
        match view.zoom(zoom.factor()) {
            Ok(()) => self.zoom.set(zoom),
            Err(error) => tracing::debug!(%error, "Browser zoom failed"),
        }
    }

    /// The zoom the page is drawn at, which turns the CSS pixels its scripts
    /// measure into the view's points.
    #[cfg(target_os = "macos")]
    pub(crate) fn drawn_zoom(&self) -> Zoom {
        self.zoom.get()
    }

    /// Moves keyboard input into the page.
    pub(crate) fn focus(&self) {
        #[cfg(target_os = "macos")]
        self.router.focus();
        #[cfg(not(target_os = "macos"))]
        if let Some(view) = self.web() {
            let _ = view.focus();
        }
    }

    /// Whether the page holds the window's keyboard. Only macOS tells.
    pub(crate) fn holds_keyboard(&self) -> bool {
        #[cfg(target_os = "macos")]
        return self.router.holds_keyboard();
        #[cfg(not(target_os = "macos"))]
        false
    }

    /// Hands the keyboard back from the page to the window.
    pub(crate) fn blur(&self) {
        #[cfg(target_os = "macos")]
        self.router.blur();
        #[cfg(not(target_os = "macos"))]
        if let Some(view) = self.web() {
            let _ = view.focus_parent();
        }
    }

    /// Refuses drops on the page, or takes them again.
    #[cfg(target_os = "macos")]
    pub(crate) fn refuse_drops(&self, refuse: bool) {
        self.router.refuse_drops(refuse);
    }

    /// Puts the page at `bounds`, in the window's points.
    fn place(&self, bounds: Bounds<Pixels>) {
        #[cfg(target_os = "macos")]
        self.router.place(bounds);
        #[cfg(not(target_os = "macos"))]
        if let Some(view) = self.web() {
            let _ = view.set_bounds(wry::Rect {
                size: wry::dpi::Size::Logical(wry::dpi::LogicalSize::new(
                    bounds.size.width.into(),
                    bounds.size.height.into(),
                )),
                position: wry::dpi::Position::Logical(wry::dpi::LogicalPosition::new(
                    bounds.origin.x.into(),
                    bounds.origin.y.into(),
                )),
            });
        }
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        self.hide();
        // wry takes the page's own view out of the window as it drops; a
        // machine's display has no such owner.
        #[cfg(target_os = "macos")]
        {
            self.router.remove();
            if let Surface::Machine(view) = &self.view {
                crate::machine::release(view);
                view.removeFromSuperview();
            }
        }
    }
}

/// The element a page lays out in: it fills its space, and the native view
/// follows wherever that is.
pub(crate) struct PageElement {
    id: TabId,
    page: Rc<Page>,
    drawn: Drawn,
}

impl PageElement {
    pub(crate) fn new(id: TabId, page: Rc<Page>, drawn: Drawn) -> Self {
        Self { id, page, drawn }
    }
}

impl IntoElement for PageElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for PageElement {
    type RequestLayoutState = ();
    type PrepaintState = Option<Hitbox>;

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
    ) -> (LayoutId, Self::RequestLayoutState) {
        let style = Style {
            size: Size::full(),
            flex_shrink: 1.,
            ..Default::default()
        };
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        _: &mut App,
    ) -> Self::PrepaintState {
        // Whole device pixels, so the page's edges are sharp and meet the
        // window's drawing around them exactly.
        let scale = window.scale_factor();
        let bounds = snap(bounds, scale);
        // Only what shows of it: an opening group slides in under its
        // frame's clip, with the page at its full width. Recorded while
        // hidden too: on Windows an overlay hides the pages it reaches, which
        // go on laying out beneath it.
        let visible = snap(bounds.intersect(&window.content_mask().bounds), scale);
        self.drawn.borrow_mut().insert(self.id, visible);
        if !self.page.shown.get() {
            return None;
        }
        self.page.place(bounds);
        Some(window.insert_hitbox(bounds, HitboxBehavior::Normal))
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        _: &mut App,
    ) {
        let Some(hitbox) = hitbox.take() else {
            return;
        };
        // A press elsewhere in the window takes the keyboard back.
        let page = self.page.clone();
        let area = hitbox.bounds;
        window.on_mouse_event(move |event: &MouseDownEvent, phase, _, _| {
            if phase == DispatchPhase::Bubble && !area.contains(&event.position) {
                page.blur();
            }
        });
        #[cfg(target_os = "macos")]
        crate::browser::stacking::Router::follow(&self.page.router, hitbox, window);
    }
}

/// `bounds` with each edge on the nearest whole device pixel at `scale`.
pub(crate) fn snap(bounds: Bounds<Pixels>, scale: f32) -> Bounds<Pixels> {
    if !scale.is_finite() || scale <= 0. {
        return bounds;
    }
    let edge = |value: Pixels| px((f32::from(value) * scale).round() / scale);
    Bounds::from_corners(
        point(edge(bounds.left()), edge(bounds.top())),
        point(edge(bounds.right()), edge(bounds.bottom())),
    )
}

#[cfg(test)]
mod tests;
