use super::*;
use crate::terminal_painter::symbols::needs_symbol_font;

fn frame(cells: Vec<CellData>) -> FrameData {
    FrameData {
        width: u16::try_from(cells.len()).unwrap_or(u16::MAX),
        height: 1,
        cells,
        cursor: None,
        hyperlinks: Vec::new(),
        graphics: Vec::new(),
    }
}

#[test]
fn private_use_icons_need_a_symbol_font() {
    // A Powerlevel10k prompt: OS logo, folder, thin separators, hourglass,
    // and a Material Design icon from the supplementary plane.
    for symbol in [
        "\u{f179}",
        "\u{f07c}",
        "\u{e0b1}",
        "\u{e0b3}",
        "\u{f252}",
        "\u{e0a0}",
        "\u{f0001}",
        "\u{e000}",
        "\u{f8fe}",
        "\u{ffffd}",
    ] {
        assert!(needs_symbol_font(symbol), "{symbol:?}");
    }
}

#[test]
fn text_graphics_and_painted_separators_need_none() {
    for symbol in [
        "",
        " ",
        "a",
        "é",
        "✔",
        "❯",
        "╭",
        "─",
        "█",
        "漢",
        "🙂",
        // Painted as paths, whatever fonts are installed.
        "\u{e0b0}",
        "\u{e0b2}",
        "\u{e0b4}",
        "\u{e0b6}",
        // Apple's logo, carried by the system faces.
        "\u{f8ff}",
        // The neighbors of both ranges.
        "\u{d7ff}",
        "\u{f900}",
        "\u{effff}",
        "\u{100000}",
    ] {
        assert!(!needs_symbol_font(symbol), "{symbol:?}");
    }
}

#[test]
fn a_frame_needs_one_only_for_a_drawn_icon_cell() {
    assert!(!frame_needs_symbol_font(&frame(Vec::new())));
    assert!(!frame_needs_symbol_font(&frame(vec![
        cell("~"),
        cell("\u{e0b0}"),
        cell(" "),
    ])));
    assert!(frame_needs_symbol_font(&frame(vec![
        cell("~"),
        cell("\u{f07c}"),
    ])));
    // A continuation cell is never drawn.
    let mut skipped = cell("\u{f07c}");
    skipped.skip = true;
    assert!(!frame_needs_symbol_font(&frame(vec![skipped])));
}
