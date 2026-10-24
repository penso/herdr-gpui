//! Terminal output becomes escaped, styled spans and nothing else.

use super::*;

fn html(text: &str) -> String {
    screen(text).markup.into_string()
}

#[test]
fn palette_colors_become_classes_and_reset_ends_them() {
    let out = html("\x1b[1;32mok\x1b[0m done");
    assert_eq!(out, r#"<span class="t-fg2 t-b">ok</span> done"#);
    assert_eq!(
        html("\x1b[91mbright\x1b[39m"),
        r#"<span class="t-fg9">bright</span>"#
    );
    assert_eq!(
        html("\x1b[44mbg\x1b[49m"),
        r#"<span class="t-bg4">bg</span>"#
    );
}

#[test]
fn extended_colors_become_inline_colors() {
    assert_eq!(
        html("\x1b[38;5;196mred\x1b[m"),
        r#"<span style="color:#ff0000">red</span>"#
    );
    assert_eq!(
        html("\x1b[38;5;2mgreen"),
        r#"<span class="t-fg2">green</span>"#,
        "the first 16 of 256 colors are the palette"
    );
    assert_eq!(
        html("\x1b[48;2;1;2;3mrgb"),
        r#"<span style="background:#010203">rgb</span>"#
    );
    assert_eq!(
        html("\x1b[38;5;244mgray"),
        r#"<span style="color:#808080">gray</span>"#
    );
}

#[test]
fn reverse_swaps_colors_even_the_defaults() {
    assert_eq!(
        html("\x1b[7msel"),
        r#"<span class="t-fg-bg t-bg-fg">sel</span>"#
    );
    assert_eq!(
        html("\x1b[31;7mx"),
        r#"<span class="t-fg-bg t-bg1">x</span>"#
    );
}

#[test]
fn terminal_text_is_escaped_and_other_sequences_dropped() {
    let out = html("<script>alert(1)</script> & \x1b[2J\x1b[10;5H\x1b]0;title\x07tail\x08");
    assert!(
        out.contains("&lt;script&gt;alert(1)&lt;/script&gt; &amp; "),
        "{out}"
    );
    assert!(
        !out.contains('\x1b') && !out.contains('\x07') && !out.contains('\x08'),
        "{out:?}"
    );
    // A title (OSC) is not shown, and a charset escape leaves no trace.
    assert!(out.ends_with(" tail"), "{out:?}");
    assert!(!out.contains("title"), "{out:?}");
    assert_eq!(html("a\x1b(Bb\x1b]8;;https://x\x1b\\c"), "abc");
}

#[test]
fn lines_and_width_are_kept() {
    let rendered = screen("ab\n\x1b[1mlonger line\x1b[0m\r\nc");
    assert_eq!(rendered.columns, 11);
    assert_eq!(
        rendered.markup.into_string(),
        "ab\n<span class=\"t-b\">longer line</span>\nc"
    );
    // A style carries over a line break, as in a terminal.
    assert_eq!(
        html("\x1b[33mone\ntwo"),
        "<span class=\"t-fg3\">one</span>\n<span class=\"t-fg3\">two</span>"
    );
}

#[test]
fn hostile_parameters_are_bounded() {
    let long = format!("\x1b[{}mx", "9;".repeat(10_000));
    assert!(html(&long).ends_with(">x</span>") || html(&long).ends_with('x'));
    let huge = "y".repeat(400 * 1024);
    assert_eq!(screen(&huge).columns, 256 * 1024);
}
