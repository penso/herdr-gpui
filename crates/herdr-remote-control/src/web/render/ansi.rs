//! A parsed screen ([`crate::ansi`]) as escaped spans. Text reaches the page
//! only through maud's escaping. Colors from the 16-color palette become
//! classes, so the page's theme decides them; 256-color and RGB values
//! become inline colors.

use crate::ansi::{self, Color, Style};
use maud::{Markup, html};

/// A rendered screen and its widest line in columns, which the phone fits
/// to its width.
pub(crate) struct Screen {
    pub(crate) markup: Markup,
    pub(crate) columns: usize,
}

/// Renders `text` (lines of SGR-colored text) as spans, line by line.
pub(crate) fn screen(text: &str) -> Screen {
    let ansi::Screen { lines, columns } = ansi::parse(text);
    let markup = html! {
        @for (index, runs) in lines.iter().enumerate() {
            @if index > 0 { "\n" }
            @for (style, text) in runs {
                @if *style == Style::default() {
                    (text)
                } @else {
                    span class=[classes(style)] style=[inline(style)] { (text) }
                }
            }
        }
    };
    Screen { markup, columns }
}

fn classes(style: &Style) -> Option<String> {
    let (fg, bg) = if style.reverse {
        (style.bg, style.fg)
    } else {
        (style.fg, style.bg)
    };
    let mut classes = Vec::new();
    match fg {
        Color::Named(index) => classes.push(format!("t-fg{index}")),
        Color::Default if style.reverse => classes.push("t-fg-bg".to_owned()),
        _ => {}
    }
    match bg {
        Color::Named(index) => classes.push(format!("t-bg{index}")),
        Color::Default if style.reverse => classes.push("t-bg-fg".to_owned()),
        _ => {}
    }
    for (on, class) in [
        (style.bold, "t-b"),
        (style.dim, "t-d"),
        (style.italic, "t-i"),
        (style.underline, "t-u"),
        (style.strike, "t-s"),
        (style.hidden, "t-h"),
    ] {
        if on {
            classes.push(class.to_owned());
        }
    }
    (!classes.is_empty()).then(|| classes.join(" "))
}

fn inline(style: &Style) -> Option<String> {
    let (fg, bg) = if style.reverse {
        (style.bg, style.fg)
    } else {
        (style.fg, style.bg)
    };
    let mut rules = Vec::new();
    if let Color::Rgb(r, g, b) = fg {
        rules.push(format!("color:#{r:02x}{g:02x}{b:02x}"));
    }
    if let Color::Rgb(r, g, b) = bg {
        rules.push(format!("background:#{r:02x}{g:02x}{b:02x}"));
    }
    (!rules.is_empty()).then(|| rules.join(";"))
}
