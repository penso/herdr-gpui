//! The main window's side of `[backdrop]`: its platform appearance, its
//! picture, and the colors its terminals and chrome paint with. The rules
//! are in `crate::backdrop`.
use super::HerdrWindow;
use crate::{
    backdrop::{self, Fill, Picture},
    terminal_painter::Ground,
};
use gpui::{prelude::*, *};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows_material;

/// What one window has done about its backdrop so far.
#[derive(Default)]
pub(crate) struct State {
    /// The system caption's colors, which follow the theme on Windows.
    caption: crate::caption::Caption,
    picture: Picture,
    /// The running decode; replacing it cancels the one before.
    load: Option<Task<()>>,
    /// Set only when it changes: the platform call is not free.
    #[cfg(not(target_os = "macos"))]
    appearance: WindowBackgroundAppearance,
    /// The same on macOS, with the color an opaque window fills with; none
    /// before the first frame sets it.
    #[cfg(target_os = "macos")]
    surface: Option<macos::Surface>,
}

impl HerdrWindow {
    pub(crate) fn fill(&self) -> Fill {
        Fill::new(&self.theme, &self.config.backdrop)
    }

    pub(crate) fn ground(&self) -> Ground {
        match self.fill().translucent() {
            true => Ground::Backdrop,
            false => Ground::Opaque,
        }
    }

    /// Brings the platform appearance and the picture in line with the
    /// config. Called while rendering, where the window is at hand; each
    /// changes only when the config does, and the decode runs off this thread.
    pub(super) fn sync_backdrop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.backdrop.caption.sync(window, &self.theme);
        let config = &self.config.backdrop;
        // An opaque ground hides a picture, so none is decoded for it.
        let path = config.translucent().then(|| config.image_path()).flatten();
        let state = &mut self.backdrop;
        if let Some(path) = state.picture.want(path) {
            let decode = cx.background_executor().spawn({
                let path = path.clone();
                async move { backdrop::decode(&path) }
            });
            state.load = Some(cx.spawn(async move |this, cx| {
                let result = decode.await;
                let _ = this.update(cx, |this, cx| {
                    this.backdrop.picture.finish(path, result);
                    this.backdrop.load = None;
                    cx.notify();
                });
            }));
        }
        for texture in state.picture.take_released() {
            let _ = window.drop_image(texture);
        }
        let appearance = backdrop::appearance(config, state.picture.covers());
        #[cfg(target_os = "macos")]
        {
            let surface = macos::Surface {
                appearance,
                ground: self.theme.background,
            };
            if state.surface != Some(surface) {
                macos::apply(window, state.surface, surface);
                state.surface = Some(surface);
            }
        }
        #[cfg(not(target_os = "macos"))]
        if appearance != state.appearance {
            #[cfg(windows)]
            windows_material::apply(window, state.appearance, appearance);
            #[cfg(not(windows))]
            window.set_background_appearance(appearance);
            state.appearance = appearance;
        }
    }

    /// Paints what lies behind everything on `root`. An opaque window takes
    /// the theme's background there. A translucent one leaves the root clear,
    /// or shows the picture, and each region paints its own translucent
    /// color instead: a panel over a translucent ground would cover twice and
    /// read as more solid than the opacity chosen. On macOS neither paints
    /// where a page shows from beneath the window's drawing.
    pub(super) fn with_backdrop(&self, root: Div) -> Div {
        #[cfg(target_os = "macos")]
        let pages = self.browser.drawn_pages();
        if !self.fill().translucent() {
            // First, so everything else paints over it.
            #[cfg(target_os = "macos")]
            return root.child(crate::browser::ground_around_pages(
                rgb(self.theme.background).into(),
                pages,
            ));
            #[cfg(not(target_os = "macos"))]
            return root.bg(rgb(self.theme.background));
        }
        let Some(texture) = self.backdrop.picture.texture() else {
            return root;
        };
        // Absolute, so it takes no part in the column's layout, and first, so
        // everything else paints over it.
        #[cfg(target_os = "macos")]
        return root.child(crate::browser::picture_around_pages(texture, pages));
        #[cfg(not(target_os = "macos"))]
        root.child(
            img(texture)
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .object_fit(ObjectFit::Cover),
        )
    }

    /// `content` with `color` painted under it, as `.bg(color)` on it would
    /// paint, except on macOS where a page shows from beneath the window's
    /// drawing: every background under a page leaves it unpainted.
    pub(crate) fn with_ground(
        &self,
        content: impl IntoElement + Styled,
        color: impl Into<Hsla>,
    ) -> AnyElement {
        #[cfg(target_os = "macos")]
        return crate::browser::AroundPages::new(content, color.into(), self.browser.drawn_pages())
            .into_any_element();
        #[cfg(not(target_os = "macos"))]
        content.bg(color.into()).into_any_element()
    }

    /// For a background painted by hand: where it must paint around pages
    /// to leave them showing, or `None` while no page lies under `bounds`,
    /// so it may paint its own shape. Call it while painting.
    pub(crate) fn pages_under(
        &self,
    ) -> impl Fn(Bounds<Pixels>) -> Option<Vec<Bounds<Pixels>>> + 'static {
        #[cfg(target_os = "macos")]
        {
            let pages = self.browser.drawn_pages();
            move |bounds| {
                let pieces = crate::browser::around_pages(bounds, pages.borrow().values().copied());
                (pieces != [bounds]).then_some(pieces)
            }
        }
        #[cfg(not(target_os = "macos"))]
        |_| None
    }
}
