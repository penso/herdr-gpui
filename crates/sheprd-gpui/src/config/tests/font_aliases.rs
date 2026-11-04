use super::*;
use gpui::{
    Bounds, DevicePixels, Font, FontId, FontMetrics, FontRun, GlyphId, LineLayout, NoopTextSystem,
    Pixels, PlatformTextSystem, RenderGlyphParams, Size, TextRenderingMode, TextSystem,
};
use std::{borrow::Cow, sync::Arc};

const FAMILY: &str = "Fixture Mono";
const ALIAS: &str = "Fixture Mono Book";
const ICON_ALIAS: &str = "Fixture Mono NFM Book";

#[test]
fn a_resolvable_legacy_family_does_not_report_a_missing_font() -> anyhow::Result<()> {
    let platform = Arc::new(AliasFontPlatform);
    let text_system = TextSystem::new(platform.clone());
    let mut config = Config::parse(&format!("[terminal]\nfamily = '{ALIAS}'"))?;

    // The platform enumerates the typographic family but accepts its legacy alias.
    assert_eq!(text_system.all_font_names(), [FAMILY]);
    let selected = text_system.resolve_font(&config.terminal.font());
    assert_eq!(
        text_system
            .get_font_for_id(selected)
            .context("resolved font descriptor")?
            .family
            .as_ref(),
        ALIAS
    );

    config.resolve_fonts_with(
        || text_system.all_font_names(),
        |family| platform.font_id(&gpui::font(family.to_owned())).is_ok(),
    );

    assert_eq!(config.terminal.family, ALIAS);
    assert_eq!(
        config.diagnostic(),
        None,
        "a family alias the rendering backend resolves must not be reported missing"
    );
    Ok(())
}

#[test]
fn a_missing_family_still_warns_when_gpui_substitutes_a_default() -> anyhow::Result<()> {
    let platform = Arc::new(AliasFontPlatform);
    let text_system = TextSystem::new(platform.clone());
    let mut config = Config::parse("[terminal]\nfamily = 'Missing Fixture'")?;
    let selected = text_system.resolve_font(&config.terminal.font());
    assert_eq!(
        text_system
            .get_font_for_id(selected)
            .context("default font descriptor")?
            .family
            .as_ref(),
        ".ZedMono"
    );

    config.resolve_fonts_with(
        || text_system.all_font_names(),
        |family| platform.font_id(&gpui::font(family.to_owned())).is_ok(),
    );

    assert_eq!(config.terminal.family, "Missing Fixture");
    assert_eq!(
        plain(config.diagnostic()).as_deref(),
        Some("Configured fonts not installed: \"Missing Fixture\"")
    );
    Ok(())
}

#[test]
fn aliases_are_recognized_in_every_face_and_explicit_fallbacks() -> anyhow::Result<()> {
    let platform = AliasFontPlatform;
    let mut config = Config::parse(
        &["sidebar", "tabs", "terminal", "ui"]
            .map(|section| {
                format!(
                    "[{section}]\nfamily = '{ALIAS}'\nfallback = ['{ALIAS}', 'Missing Fixture']"
                )
            })
            .join("\n"),
    )?;

    config.resolve_fonts_with(
        || platform.all_font_names(),
        |family| platform.font_id(&gpui::font(family.to_owned())).is_ok(),
    );

    assert_eq!(config.missing_fonts, ["Missing Fixture"]);
    for face in [&config.sidebar, &config.tabs, &config.terminal, &config.ui] {
        assert_eq!(face.family, ALIAS);
        assert_eq!(
            face.fallbacks.as_deref(),
            Some([ALIAS.to_owned(), "Missing Fixture".to_owned()].as_slice())
        );
    }
    Ok(())
}

#[test]
fn a_resolvable_patched_alias_does_not_raise_the_icon_font_notice() -> anyhow::Result<()> {
    let platform = AliasFontPlatform;
    let mut config = Config::parse(&format!("[terminal]\nfamily = '{ICON_ALIAS}'"))?;

    config.resolve_fonts_with(
        || platform.all_font_names(),
        |family| platform.font_id(&gpui::font(family.to_owned())).is_ok(),
    );

    assert_eq!(config.diagnostic(), None);
    assert!(!config.icon_font_missing);
    assert_eq!(config.terminal.family, ICON_ALIAS);
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "requires an explicitly selected installed font family alias"]
fn a_native_family_alias_is_accepted_without_accepting_missing_names() -> anyhow::Result<()> {
    let alias = env::var("HERDR_TEST_FONT_ALIAS")?;
    let source = font_kit::source::SystemSource::new();
    let installed = source.all_families()?;
    anyhow::ensure!(
        !installed
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&alias)),
        "select an alias that enumeration omits"
    );
    source.select_family_by_name(&alias)?;
    let mut config = Config::default();
    config.terminal.family = alias.clone();

    config.resolve_fonts(|| installed.clone());

    assert_eq!(config.diagnostic(), None);
    assert_eq!(config.terminal.family, alias);

    let missing = format!("Missing Font {}", uuid::Uuid::new_v4());
    anyhow::ensure!(source.select_family_by_name(&missing).is_err());
    config.terminal.family = missing.clone();
    config.resolve_fonts(|| installed);
    assert_eq!(config.missing_fonts, [missing]);
    Ok(())
}

// Only the operating-system font boundary is faked; GPUI's resolution stays real.
struct AliasFontPlatform;

impl PlatformTextSystem for AliasFontPlatform {
    fn all_font_names(&self) -> Vec<String> {
        vec![FAMILY.into()]
    }

    fn font_id(&self, font: &Font) -> gpui::Result<FontId> {
        match font.family.as_ref() {
            FAMILY => Ok(FontId(1)),
            ALIAS => Ok(FontId(2)),
            ".ZedMono" => Ok(FontId(3)),
            ICON_ALIAS => Ok(FontId(4)),
            _ => anyhow::bail!("font family not found"),
        }
    }

    fn add_fonts(&self, fonts: Vec<Cow<'static, [u8]>>) -> gpui::Result<()> {
        NoopTextSystem.add_fonts(fonts)
    }

    fn font_metrics(&self, font_id: FontId) -> FontMetrics {
        NoopTextSystem.font_metrics(font_id)
    }

    fn typographic_bounds(&self, font_id: FontId, glyph_id: GlyphId) -> gpui::Result<Bounds<f32>> {
        NoopTextSystem.typographic_bounds(font_id, glyph_id)
    }

    fn advance(&self, font_id: FontId, glyph_id: GlyphId) -> gpui::Result<Size<f32>> {
        NoopTextSystem.advance(font_id, glyph_id)
    }

    fn glyph_for_char(&self, font_id: FontId, ch: char) -> Option<GlyphId> {
        NoopTextSystem.glyph_for_char(font_id, ch)
    }

    fn glyph_raster_bounds(
        &self,
        params: &RenderGlyphParams,
    ) -> gpui::Result<Bounds<DevicePixels>> {
        NoopTextSystem.glyph_raster_bounds(params)
    }

    fn rasterize_glyph(
        &self,
        params: &RenderGlyphParams,
        bounds: Bounds<DevicePixels>,
    ) -> gpui::Result<(Size<DevicePixels>, Vec<u8>)> {
        NoopTextSystem.rasterize_glyph(params, bounds)
    }

    fn layout_line(&self, text: &str, size: Pixels, runs: &[FontRun]) -> LineLayout {
        NoopTextSystem.layout_line(text, size, runs)
    }

    fn recommended_rendering_mode(&self, font_id: FontId, size: Pixels) -> TextRenderingMode {
        NoopTextSystem.recommended_rendering_mode(font_id, size)
    }
}
