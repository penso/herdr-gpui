use super::*;

/// `mouse_scroll_lines` follows Herdr: three by default, and a count Herdr
/// would refuse keeps the default instead of failing the file.
#[test]
fn mouse_scroll_lines_default_parse_and_fall_back() -> anyhow::Result<()> {
    assert_eq!(parsed("")?.mouse_scroll_lines.get(), 3);
    assert_eq!(DEFAULT_MOUSE_SCROLL_LINES.get(), 3);
    assert_eq!(
        parsed("[ui]\nmouse_scroll_lines = 7")?
            .mouse_scroll_lines
            .get(),
        7
    );
    for invalid in ["0", "-2", "70000", "'five'", "1.5"] {
        let settings = parsed(&format!(
            "[ui]\nmouse_scroll_lines = {invalid}\ncopy_on_select = false"
        ))?;
        assert_eq!(settings.mouse_scroll_lines.get(), 3, "value {invalid}");
        assert!(!settings.copy_on_select, "value {invalid} kept the rest");
    }
    Ok(())
}

/// `right_click_passthrough_modifier` accepts Herdr's grammar and matches
/// only the exact combination; what Herdr rejects, or GPUI cannot report,
/// leaves right-clicks with the menu.
#[test]
fn right_click_passthrough_modifier_parses_herdr_grammar() -> anyhow::Result<()> {
    use gpui::Modifiers;
    let modifier = |value: &str| -> anyhow::Result<RightClickPassthrough> {
        Ok(
            parsed(&format!("[ui]\nright_click_passthrough_modifier = {value}"))?
                .right_click_passthrough,
        )
    };
    assert_eq!(
        parsed("")?.right_click_passthrough,
        RightClickPassthrough::default()
    );
    let control = Modifiers {
        control: true,
        ..Default::default()
    };
    let alt = Modifiers {
        alt: true,
        ..Default::default()
    };
    let platform = Modifiers {
        platform: true,
        ..Default::default()
    };
    for (value, expected) in [
        ("'ctrl'", control),
        ("'Control'", control),
        ("'alt'", alt),
        ("'option'", alt),
        ("'cmd'", platform),
        ("'command'", platform),
        ("'super'", platform),
        (
            "' ctrl + alt '",
            Modifiers {
                control: true,
                alt: true,
                ..Default::default()
            },
        ),
    ] {
        let passthrough = modifier(value)?;
        assert!(passthrough.matches(expected), "value {value}");
        assert!(!passthrough.matches(Modifiers::default()), "value {value}");
        assert!(
            !passthrough.matches(Modifiers {
                shift: true,
                ..expected
            }),
            "value {value} must match exactly"
        );
    }
    for disabled in [
        "''",
        "'off'",
        "'None'",
        "'disabled'",
        "'meta'",
        "'ctrl+hyper'",
        "'shift'",
        "'ctrl+shift'",
        "'ctrl+'",
        "'banana'",
        "3",
    ] {
        let passthrough = modifier(disabled)?;
        assert_eq!(
            passthrough,
            RightClickPassthrough::default(),
            "value {disabled}"
        );
        for held in [Modifiers::default(), control, alt, platform] {
            assert!(!passthrough.matches(held), "value {disabled}");
        }
    }
    Ok(())
}
