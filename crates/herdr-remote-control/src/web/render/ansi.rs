//! A pane's screen as Herdr's `pane.read` gives it, text with SGR color
//! sequences, turned into escaped spans. Terminal output is untrusted: only
//! SGR parameters are read, every other escape sequence and control
//! character is dropped, and text reaches the page only through maud's
//! escaping. Colors from the 16-color palette become classes, so the page's
//! theme decides them; 256-color and RGB values become inline colors.

use maud::{Markup, html};

/// The most screen text rendered; a visible screen is far smaller.
const MAX_TEXT: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Color {
    #[default]
    Default,
    /// One of the 16 palette colors.
    Named(u8),
    Rgb(u8, u8, u8),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Style {
    fg: Color,
    bg: Color,
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    reverse: bool,
    hidden: bool,
    strike: bool,
}

/// A rendered screen and its widest line in columns, which the phone fits
/// to its width.
pub(crate) struct Screen {
    pub(crate) markup: Markup,
    pub(crate) columns: usize,
}

/// Renders `text` (lines of SGR-colored text) as spans, line by line.
pub(crate) fn screen(text: &str) -> Screen {
    let text = match text.char_indices().nth(MAX_TEXT) {
        Some((cut, _)) => &text[..cut],
        None => text,
    };
    let mut style = Style::default();
    let mut columns = 0;
    let mut lines = Vec::new();
    for line in text.split('\n') {
        let (runs, width) = runs(line.trim_end_matches('\r'), &mut style);
        columns = columns.max(width);
        lines.push(runs);
    }
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

/// One line as runs of equally styled text, and its width in characters.
fn runs(line: &str, style: &mut Style) -> (Vec<(Style, String)>, usize) {
    let mut runs: Vec<(Style, String)> = Vec::new();
    let mut width = 0;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            match chars.peek() {
                // An OSC (a title or a link) runs to BEL or ST, unshown.
                Some(']') => {
                    let mut previous = '\0';
                    for c in chars.by_ref() {
                        if c == '\x07' || (previous == '\x1b' && c == '\\') {
                            break;
                        }
                        previous = c;
                    }
                    continue;
                }
                Some('[') => {}
                // A charset choice names its set in one more character.
                Some('(' | ')' | '*' | '+') => {
                    chars.next();
                    chars.next();
                    continue;
                }
                // Any other escape is two characters.
                Some(_) => {
                    chars.next();
                    continue;
                }
                None => continue,
            }
            {
                chars.next();
                let mut parameters = String::new();
                let mut end = None;
                for c in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&c) {
                        end = Some(c);
                        break;
                    }
                    if parameters.len() < 64 {
                        parameters.push(c);
                    }
                }
                if end == Some('m') {
                    apply(style, &parameters);
                }
            }
            // Any other CSI sequence is dropped whole.
            continue;
        }
        let c = if c == '\t' { ' ' } else { c };
        if c.is_control() {
            continue;
        }
        width += 1;
        match runs.last_mut() {
            Some((last, text)) if last == style => text.push(c),
            _ => runs.push((*style, c.to_string())),
        }
    }
    (runs, width)
}

/// Applies one SGR sequence's parameters.
fn apply(style: &mut Style, parameters: &str) {
    let mut codes = parameters
        .split([';', ':'])
        .map(|code| code.parse::<u16>().unwrap_or(0));
    if parameters.is_empty() {
        *style = Style::default();
        return;
    }
    while let Some(code) = codes.next() {
        match code {
            0 => *style = Style::default(),
            1 => style.bold = true,
            2 => style.dim = true,
            3 => style.italic = true,
            4 => style.underline = true,
            7 => style.reverse = true,
            8 => style.hidden = true,
            9 => style.strike = true,
            21 | 22 => (style.bold, style.dim) = (false, false),
            23 => style.italic = false,
            24 => style.underline = false,
            27 => style.reverse = false,
            28 => style.hidden = false,
            29 => style.strike = false,
            30..=37 => style.fg = named(code - 30),
            90..=97 => style.fg = named(code - 90 + 8),
            40..=47 => style.bg = named(code - 40),
            100..=107 => style.bg = named(code - 100 + 8),
            39 => style.fg = Color::Default,
            49 => style.bg = Color::Default,
            38 | 48 => {
                let color = extended(&mut codes);
                if code == 38 {
                    style.fg = color;
                } else {
                    style.bg = color;
                }
            }
            _ => {}
        }
    }
}

fn named(index: u16) -> Color {
    Color::Named(u8::try_from(index).unwrap_or(7))
}

/// `5;N` (256 colors) or `2;R;G;B` after a 38 or 48.
fn extended(codes: &mut impl Iterator<Item = u16>) -> Color {
    fn byte(codes: &mut impl Iterator<Item = u16>) -> u8 {
        codes
            .next()
            .map_or(0, |value| u8::try_from(value).unwrap_or(u8::MAX))
    }
    match codes.next() {
        Some(5) => palette(byte(codes)),
        Some(2) => Color::Rgb(byte(codes), byte(codes), byte(codes)),
        _ => Color::Default,
    }
}

/// The 256-color palette: 16 named colors, a 6×6×6 cube, then 24 grays.
fn palette(index: u8) -> Color {
    match index {
        0..=15 => Color::Named(index),
        16..=231 => {
            let index = index - 16;
            let level = |value: u8| if value == 0 { 0 } else { 55 + value * 40 };
            Color::Rgb(level(index / 36), level(index / 6 % 6), level(index % 6))
        }
        _ => {
            let gray = 8 + (index - 232) * 10;
            Color::Rgb(gray, gray, gray)
        }
    }
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
