//! The `en-XA` leak check (docs/i18n.md), shared by every window's test:
//! each line a surface shows under the accented pseudo-locale must be a
//! whole translated message, so text the app writes without one, or a label
//! clipped short, fails.
//!
//! GPUI shapes each line once and reuses it while it is unchanged, so a
//! recording text system sees what each step newly draws. A surface is
//! checked by what it adds over the frame before it. Each window keeps its
//! own test in its module's `tests/en_xa.rs`, opening its surfaces through
//! [`under_en_xa`] and checking each with [`assert_translated`].
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::toggle_pseudo;
use gpui::{
    Bounds, DevicePixels, Font, FontId, FontMetrics, FontRun, GlyphId, LineLayout, NoopTextSystem,
    Pixels, PlatformTextSystem, RenderGlyphParams, Size, TestAppContext, TextRenderingMode,
    VisualTestContext,
};
use herdr_i18n::Pseudo;
use std::{
    borrow::Cow,
    sync::{Arc, Mutex},
};

/// Passes every call to GPUI's test text system and keeps each line shaped.
pub(crate) struct Recorder {
    inner: NoopTextSystem,
    lines: Mutex<Vec<String>>,
}

impl Recorder {
    fn new() -> Self {
        Self {
            inner: NoopTextSystem,
            lines: Mutex::default(),
        }
    }

    /// The lines shaped since the last call.
    pub(crate) fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.lines.lock().unwrap())
    }
}

impl PlatformTextSystem for Recorder {
    fn add_fonts(&self, fonts: Vec<Cow<'static, [u8]>>) -> anyhow::Result<()> {
        self.inner.add_fonts(fonts)
    }
    fn all_font_names(&self) -> Vec<String> {
        self.inner.all_font_names()
    }
    fn font_id(&self, descriptor: &Font) -> anyhow::Result<FontId> {
        self.inner.font_id(descriptor)
    }
    fn font_metrics(&self, font_id: FontId) -> FontMetrics {
        self.inner.font_metrics(font_id)
    }
    fn typographic_bounds(
        &self,
        font_id: FontId,
        glyph_id: GlyphId,
    ) -> anyhow::Result<Bounds<f32>> {
        self.inner.typographic_bounds(font_id, glyph_id)
    }
    fn advance(&self, font_id: FontId, glyph_id: GlyphId) -> anyhow::Result<Size<f32>> {
        self.inner.advance(font_id, glyph_id)
    }
    fn glyph_for_char(&self, font_id: FontId, ch: char) -> Option<GlyphId> {
        self.inner.glyph_for_char(font_id, ch)
    }
    fn glyph_raster_bounds(
        &self,
        params: &RenderGlyphParams,
    ) -> anyhow::Result<Bounds<DevicePixels>> {
        self.inner.glyph_raster_bounds(params)
    }
    fn rasterize_glyph(
        &self,
        params: &RenderGlyphParams,
        raster_bounds: Bounds<DevicePixels>,
    ) -> anyhow::Result<(Size<DevicePixels>, Vec<u8>)> {
        self.inner.rasterize_glyph(params, raster_bounds)
    }
    fn layout_line(&self, text: &str, font_size: Pixels, runs: &[FontRun]) -> LineLayout {
        if let Ok(mut lines) = self.lines.lock() {
            lines.push(text.to_owned());
        }
        self.inner.layout_line(text, font_size, runs)
    }
    fn recommended_rendering_mode(&self, font_id: FontId, font_size: Pixels) -> TextRenderingMode {
        self.inner.recommended_rendering_mode(font_id, font_size)
    }
}

/// Runs `test` in a headless app showing `en-XA`, whose text system
/// records every line drawn.
pub(crate) fn under_en_xa(
    name: &'static str,
    test: impl Fn(&mut TestAppContext, &Recorder) + std::panic::RefUnwindSafe,
) {
    let recorder = Arc::new(Recorder::new());
    gpui::run_test(
        1,
        &[],
        0,
        &mut |dispatcher, _| {
            let mut cx = TestAppContext::build_with_text_system(
                dispatcher.clone(),
                Some(name),
                recorder.clone(),
            );
            cx.update(|cx| toggle_pseudo(Pseudo::Accented, cx));
            test(&mut cx, &recorder);
            cx.update(|cx| cx.quit());
            cx.run_until_parked();
            drop(cx);
            dispatcher.drain_tasks();
        },
        None,
    );
}

/// Draws the window's next frame and runs what it scheduled.
pub(crate) fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

/// Text with no letter in it, such as a separator or a count, is nothing
/// to translate.
fn is_text(line: &str) -> bool {
    line.chars().any(char::is_alphabetic)
}

/// Fails unless `shown`, the lines a surface added, holds a translated
/// message and nothing else but `data`: names users chose, and what the
/// daemon, the keymap or the system reports.
pub(crate) fn assert_translated(surface: &str, shown: &[String], data: &[String]) {
    assert!(
        shown.iter().any(|line| herdr_i18n::pseudo::is_marked(line)),
        "{surface} drew no translated text"
    );
    let leaks: Vec<_> = shown
        .iter()
        .map(|line| line.trim())
        .filter(|line| is_text(line))
        .filter(|line| !herdr_i18n::pseudo::is_marked(line))
        .filter(|line| !data.iter().any(|data| data == line))
        .collect();
    assert!(
        leaks.is_empty(),
        "{surface} shows text that is not a message: {leaks:#?}"
    );
}
