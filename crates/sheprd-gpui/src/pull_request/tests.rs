use super::*;

#[test]
fn each_tone_takes_its_own_palette_color() {
    let theme = crate::config::Theme::default();
    for (tone, expected) in [
        (Tone::Attention, theme.palette[1]),
        (Tone::Held, theme.palette[208]),
        (Tone::Merged, theme.palette[5]),
        (Tone::Muted, theme.muted),
        (Tone::Ready, theme.palette[2]),
        (Tone::Waiting, theme.palette[3]),
    ] {
        assert_eq!(tone_color(tone, &theme), theme.ink(expected), "{tone:?}");
    }
}

#[test]
fn a_badge_is_colored_by_its_tone() -> anyhow::Result<()> {
    let theme = crate::config::Theme::default();
    let mut pr = fixture()?;
    pr.state = State::Merged;
    assert_eq!(color(&pr, &theme), theme.ink(theme.palette[5]));
    Ok(())
}
