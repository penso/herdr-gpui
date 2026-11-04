use super::*;

fn first(bytes: &[u8]) -> CellData {
    let frame = fed(4, 1, bytes).frame();
    cell(&frame, 0, 0).clone()
}

#[test]
fn unstyled_cells_use_the_default_colors() {
    let plain = first(b"x");
    assert_eq!((plain.fg, plain.bg, plain.modifier), (0, 0, 0));
}

#[test]
fn named_colors_are_palette_indices_plus_one() {
    assert_eq!(first(b"\x1b[30mx").fg, 1);
    assert_eq!(first(b"\x1b[31mx").fg, 2);
    assert_eq!(first(b"\x1b[37mx").fg, 8);
    assert_eq!(first(b"\x1b[90mx").fg, 9);
    assert_eq!(first(b"\x1b[97mx").fg, 16);
    assert_eq!(first(b"\x1b[44mx").bg, 5);
    assert_eq!(first(b"\x1b[31;39mx").fg, 0);
}

#[test]
fn indexed_colors_keep_their_index() {
    let cell = first(b"\x1b[38;5;200;48;5;3mx");
    assert_eq!(cell.fg, 0x0100_00c8);
    assert_eq!(cell.bg, 0x0100_0003);
}

#[test]
fn rgb_colors_are_tagged_rgb() {
    let cell = first(b"\x1b[38;2;1;2;3;48;2;255;128;0mx");
    assert_eq!(cell.fg, 0x0201_0203);
    assert_eq!(cell.bg, 0x02ff_8000);
}

#[test]
fn each_attribute_sets_its_modifier_bit() {
    for (sgr, bit) in [
        ("1", BOLD),
        ("2", DIM),
        ("3", ITALIC),
        ("4", UNDERLINE),
        ("4:3", UNDERLINE),
        ("4:2", UNDERLINE),
        ("7", REVERSED),
        ("8", HIDDEN),
        ("9", STRIKETHROUGH),
    ] {
        let cell = first(format!("\x1b[{sgr}mx").as_bytes());
        assert_eq!(cell.modifier, bit, "SGR {sgr}");
    }
}

#[test]
fn attributes_combine_and_reset() {
    let cell = first(b"\x1b[1;3;9mx");
    assert_eq!(cell.modifier, BOLD | ITALIC | STRIKETHROUGH);
    let frame = fed(4, 1, b"\x1b[1mx\x1b[0my").frame();
    assert_eq!(super::cell(&frame, 1, 0).modifier, 0);
}

#[test]
fn dim_named_colors_become_the_normal_color_drawn_dim() {
    use alacritty_terminal::vte::ansi::{Color, NamedColor};
    assert_eq!(cells::color(Color::Named(NamedColor::DimRed)), (2, true));
    assert_eq!(
        cells::color(Color::Named(NamedColor::DimForeground)),
        (0, true)
    );
    assert_eq!(
        cells::color(Color::Named(NamedColor::Background)),
        (0, false)
    );
    assert_eq!(
        cells::color(Color::Named(NamedColor::BrightWhite)),
        (16, false)
    );
}
