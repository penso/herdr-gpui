//! RaTeX's engine, with resource checks before rasterization. Keeping the
//! engine's height/depth lets the view align inline formulas with prose.
use super::{
    super::visual::{Drawn, Error, Result},
    MathMode,
};
use ratex_layout::{LayoutOptions, layout, to_display_list, validate_layout};
use ratex_render::{RenderOptions, render_to_png};
use ratex_types::{Color, MathStyle};
use std::sync::Arc;

pub(in crate::chat) fn draw(source: &str, mode: MathMode, size: f32, color: u32) -> Result<Drawn> {
    if !within_limits(source) {
        return Err(Error::MathLimit);
    }
    let nodes = ratex_parser::parse(source).map_err(Error::MathParse)?;
    let options = LayoutOptions {
        style: match mode {
            MathMode::Inline => MathStyle::Text,
            MathMode::Display => MathStyle::Display,
        },
        color: Color::rgb(
            ((color >> 16) & 255) as f32 / 255.,
            ((color >> 8) & 255) as f32 / 255.,
            (color & 255) as f32 / 255.,
        ),
        ..Default::default()
    };
    let tree = layout(&nodes, &options);
    validate_layout(&tree).map_err(Error::Math)?;
    let list = to_display_list(&tree);
    // KaTeX's lowercase height is about .44em versus .54em in our sans
    // prose. Match their optical sizes, including baselines and raster bounds.
    let size = (size * 1.22).clamp(10., 64.);
    let width = list.width as f32 * size + 4.;
    let height = (list.height + list.depth) as f32 * size + 4.;
    // Includes the 2x raster: at most 4 MiB per formula and atlas-safe sides.
    if !width.is_finite()
        || !height.is_finite()
        || width <= 0.
        || height <= 0.
        || width > 2_048.
        || height > 1_024.
        || width * height > 256. * 1_024.
    {
        return Err(Error::MathLimit);
    }
    let png = render_to_png(
        &list,
        &RenderOptions {
            font_size: size,
            padding: 2.,
            device_pixel_ratio: 2.,
            background_color: Color::new(0., 0., 0., 0.),
            ..Default::default()
        },
    )
    .map_err(Error::Math)?;
    let pixels = image::load_from_memory(&png)
        .map_err(Error::Decode)?
        .into_rgba8();
    let width = pixels.width() as f32 / 2.;
    let height = pixels.height() as f32 / 2.;
    let texture = crate::terminal_painter::texture(crate::terminal_painter::fit_bgra(pixels));
    Ok(Drawn {
        texture: Arc::new(texture),
        width,
        height,
        baseline: Some(list.height as f32 * size + 2.),
    })
}

/// Bound recursion before entering the third-party parser. Its own budgets
/// remain useful, but a stack overflow cannot be caught by catch_unwind.
fn within_limits(source: &str) -> bool {
    if source.len() > super::MAX_MATH_BYTES {
        return false;
    }
    let (mut depth, mut commands, mut scripts) = (0usize, 0usize, 0usize);
    let mut chars = source.chars();
    while let Some(ch) = chars.next() {
        if crate::notifications::unsafe_char(ch) && !matches!(ch, '\n' | '\r' | '\t') {
            return false;
        }
        match ch {
            '\\' => {
                commands += 1;
                if commands > 64 {
                    return false;
                }
                // Escaped braces are delimiters, not recursive groups. Skip
                // the next character also for control words and line breaks.
                chars.next();
            }
            '{' => {
                depth += 1;
                if depth > 64 {
                    return false;
                }
            }
            '}' => depth = depth.saturating_sub(1),
            '^' | '_' => {
                scripts += 1;
                if scripts > 128 {
                    return false;
                }
            }
            _ => {}
        }
    }
    true
}
