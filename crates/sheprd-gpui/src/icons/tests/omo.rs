#![allow(clippy::unwrap_used)]

use super::*;
use gpui::{DevicePixels, Image, ImageFormat, TestAppContext};
use sha2::{Digest, Sha256};

#[test]
fn omo_identity_uses_png_without_changing_pi() {
    let omo = AgentIcon::from_identity(Some("omo"));
    assert_eq!(omo, AgentIcon::Omo);
    assert_eq!(omo.path(), "icons/agent-omo.png");
    assert_eq!(AgentIcon::from_identity(Some("pi")), AgentIcon::Pi);
    assert_eq!(AgentIcon::Pi.path(), "icons/agent-pi.svg");
    let listed = Icons.list("icons/agent-").unwrap();
    assert_eq!(listed.iter().filter(|path| *path == omo.path()).count(), 1);
}

#[test]
fn omo_artwork_preserves_approved_source_colors_and_alpha() {
    let bytes = Icons.load(AgentIcon::Omo.path()).unwrap().unwrap();
    // The approved assets/omo-mark.png, copied byte-for-byte, not re-encoded.
    let digest: String = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(
        digest,
        "4b72f36d0fd223e7966eb13b49d1d18e5463f02652ab9d6bf1ec77ed112ffc85"
    );
    let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
        .unwrap()
        .to_rgba8();
    assert_eq!(image.dimensions(), (256, 256));
    assert!(image.pixels().any(|pixel| pixel.0 == [244, 244, 244, 255]));
    assert!(image.pixels().any(|pixel| pixel.0 == [4, 22, 23, 255]));
    assert!(image.pixels().any(|pixel| pixel.0[3] == 0));
    assert!(image.pixels().any(|pixel| (1..255).contains(&pixel.0[3])));
    assert!(
        image.enumerate_pixels().all(|(x, y, pixel)| {
            !(x == 0 || x == 255 || y == 0 || y == 255) || pixel.0[3] == 0
        })
    );
}

#[gpui::test]
fn omo_embedded_png_decodes_through_gpui_without_color_or_alpha_loss(cx: &mut TestAppContext) {
    let bytes = Icons.load(AgentIcon::Omo.path()).unwrap().unwrap();
    let source = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
        .unwrap()
        .to_rgba8();
    let renderer = cx.update(|cx| cx.svg_renderer());
    let rendered = Image::from_bytes(ImageFormat::Png, bytes.into_owned())
        .to_image_data(renderer)
        .unwrap();
    assert_eq!(rendered.size(0).width, DevicePixels(256));
    assert_eq!(rendered.size(0).height, DevicePixels(256));
    let pixels = rendered.as_bytes(0).unwrap();
    assert_eq!(pixels.len(), source.as_raw().len());
    // GPUI stores BGRA; only the red/blue channel order may change.
    for (actual, expected) in pixels.chunks_exact(4).zip(source.pixels()) {
        assert_eq!(actual, [expected[2], expected[1], expected[0], expected[3]]);
    }
}

#[gpui::test]
fn omo_dispatches_to_img_and_other_agents_to_tinted_svg(cx: &mut TestAppContext) {
    cx.update(|_| {
        for agent in AgentIcon::ALL {
            for color in [0x000000, 0xffffff, 0xff0000] {
                let mut element = agent.element(16., color);
                if *agent == AgentIcon::Omo {
                    assert!(element.downcast_mut::<gpui::Img>().is_some());
                } else {
                    assert!(element.downcast_mut::<gpui::Svg>().is_some());
                }
            }
        }
    });
}
