use super::*;

#[core::prelude::v1::test]
fn widget_metric_and_accent_text_contrast_on_dark_and_light_surfaces() {
    for name in ["Default", "Catppuccin Latte"] {
        let Some(theme) = Theme::builtin(name) else {
            panic!("missing {name}");
        };
        let color = super::super::text_ink(theme.primary(), theme.surface);
        assert!(
            crate::contrast::ratio(color, theme.surface) >= 4.5,
            "{name}"
        );
    }
}

#[core::prelude::v1::test]
fn widget_button_text_contrasts_with_its_actual_fill_in_both_themes() {
    for name in ["Default", "Catppuccin Latte"] {
        let Some(theme) = Theme::builtin(name) else {
            panic!("missing {name}");
        };
        for selected in [false, true] {
            let mut button = button(&theme, "contrast-test".into(), "Action".into(), selected);
            let Some(color) = button.text_style().color else {
                panic!("button needs an explicit text color");
            };
            let color = color.to_rgb();
            let packed = [color.r, color.g, color.b]
                .into_iter()
                .fold(0, |rgb, channel| {
                    (rgb << 8) | (channel * 255.).round() as u32
                });
            let fill = if selected {
                theme.primary()
            } else {
                theme.surface
            };
            assert!(
                crate::contrast::ratio(packed, fill) >= 4.5,
                "{name} selected={selected}"
            );
        }
    }
}
