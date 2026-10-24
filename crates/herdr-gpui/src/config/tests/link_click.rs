use super::*;
use crate::config::link_click::LinkClickModifierError;
use gpui::Modifiers;

fn modifier(value: &str) -> anyhow::Result<LinkClickModifier> {
    Ok(Config::parse(&format!("link_click_modifier = {value:?}"))?.link_click_modifier)
}

fn held(control: bool, alt: bool, shift: bool, platform: bool) -> Modifiers {
    Modifiers {
        control,
        alt,
        shift,
        platform,
        function: false,
    }
}

#[test]
fn links_open_with_the_platform_modifier_unless_configured() -> anyhow::Result<()> {
    let default = LinkClickModifier::default();
    assert_eq!(Config::parse("")?.link_click_modifier, default);
    assert_eq!(Config::parse(DEFAULT_CONFIG)?.link_click_modifier, default);
    assert!(default.matches(Modifiers::secondary_key()));
    assert!(!default.matches(Modifiers::none()));
    let (cmd, ctrl) = (
        held(false, false, false, true),
        held(true, false, false, false),
    );
    assert_eq!(default.matches(cmd), cfg!(target_os = "macos"));
    assert_eq!(default.matches(ctrl), !cfg!(target_os = "macos"));
    // Unset, the match stays as loose as before the setting existed: Shift
    // or another key held as well still opens a link.
    let loose = Modifiers {
        shift: true,
        ..Modifiers::secondary_key()
    };
    assert!(default.matches(loose));
    assert!(default.matches(held(true, false, false, true)));
    assert!(!default.matches(held(false, false, true, false)));
    // Setting the same key explicitly matches it exactly.
    let explicit = modifier(if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    })?;
    assert_ne!(explicit, default);
    assert!(explicit.matches(Modifiers::secondary_key()));
    assert!(!explicit.matches(loose));
    Ok(())
}

#[test]
fn link_click_modifiers_parse_herdr_names_and_combinations() -> anyhow::Result<()> {
    let cmd = held(false, false, false, true);
    let ctrl = held(true, false, false, false);
    let shift = held(false, false, true, false);
    for (value, expected) in [
        ("cmd", cmd),
        ("Command", cmd),
        ("super", cmd),
        ("ctrl", ctrl),
        ("CONTROL", ctrl),
        ("shift", shift),
        (" ctrl + shift ", held(true, false, true, false)),
        ("cmd+ctrl", held(true, false, false, true)),
        ("none", Modifiers::none()),
        ("None", Modifiers::none()),
    ] {
        assert!(modifier(value)?.matches(expected), "{value}");
    }
    Ok(())
}

#[test]
fn link_click_modifiers_naming_alt_or_unknown_keys_are_refused() -> anyhow::Result<()> {
    for value in ["alt", "option", "ctrl+alt", "shift+Option"] {
        assert_eq!(
            LinkClickModifier::parse(value),
            Err(LinkClickModifierError::Alt),
            "{value}"
        );
        let error = Config::parse(&format!("link_click_modifier = {value:?}"))
            .err()
            .context("alt accepted")?;
        assert!(error.to_string().contains("cannot include alt"), "{error}");
    }
    for value in ["", "  "] {
        assert_eq!(
            LinkClickModifier::parse(value),
            Err(LinkClickModifierError::Empty)
        );
    }
    for (value, token) in [
        ("meta", "meta"),
        ("ctrl+", ""),
        ("none+shift", "none"),
        ("cmd-shift", "cmd-shift"),
    ] {
        assert_eq!(
            LinkClickModifier::parse(value),
            Err(LinkClickModifierError::Unknown(token.into())),
            "{value}"
        );
    }
    assert!(Config::parse("link_click_modifier = true").is_err());
    Ok(())
}

#[test]
fn link_click_modifiers_match_exactly_apart_from_alt() -> anyhow::Result<()> {
    let ctrl = modifier("ctrl")?;
    assert!(ctrl.matches(held(true, false, false, false)));
    // Alt chooses where the link opens, on top of any modifier.
    assert!(ctrl.matches(held(true, true, false, false)));
    assert!(!ctrl.matches(held(true, false, true, false)));
    assert!(!ctrl.matches(held(true, false, false, true)));
    assert!(!ctrl.matches(Modifiers::none()));
    assert!(ctrl.matches(Modifiers {
        function: true,
        ..held(true, false, false, false)
    }));

    let shift = modifier("shift")?;
    assert!(shift.matches(held(false, false, true, false)));
    assert!(shift.matches(held(false, true, true, false)));
    assert!(!shift.matches(held(true, false, true, false)));

    let none = modifier("none")?;
    assert!(none.matches(Modifiers::none()));
    assert!(none.matches(held(false, true, false, false)));
    assert!(!none.matches(held(true, false, false, false)));
    assert!(!none.matches(held(false, false, true, false)));

    let combo = modifier("cmd+shift")?;
    assert!(combo.matches(held(false, false, true, true)));
    assert!(!combo.matches(held(false, false, false, true)));
    assert!(!combo.matches(held(false, false, true, false)));
    Ok(())
}

#[test]
fn link_click_modifiers_take_a_list_of_combinations() -> anyhow::Result<()> {
    let list = |value: &str| -> anyhow::Result<LinkClickModifier> {
        Ok(Config::parse(&format!("link_click_modifier = {value}"))?.link_click_modifier)
    };
    let either = list(r#"["none", "cmd"]"#)?;
    assert_eq!(either, LinkClickModifier::parse_any(["none", "cmd"])?);
    assert!(either.matches(Modifiers::none()));
    assert!(either.matches(held(false, false, false, true)));
    // Alt still only picks where the link opens.
    assert!(either.matches(held(false, true, false, true)));
    assert!(!either.matches(held(false, false, true, false)));
    assert!(!either.matches(held(false, false, true, true)));
    assert!(!either.matches(held(true, false, false, false)));
    // One entry is the same as the single string.
    assert_eq!(list(r#"["ctrl+shift"]"#)?, modifier("ctrl+shift")?);
    assert_eq!(list(r#"["cmd", "Command"]"#)?, modifier("cmd")?);

    for value in [r#"["none", "alt"]"#, r#"["ctrl+option"]"#] {
        let error = list(value).err().context("alt accepted")?;
        assert!(error.to_string().contains("cannot include alt"), "{error}");
    }
    assert_eq!(
        LinkClickModifier::parse_any(["cmd", "shift+alt"]),
        Err(LinkClickModifierError::Alt)
    );
    let error = list("[]").err().context("empty list accepted")?;
    assert!(error.to_string().contains("is empty"), "{error}");
    assert_eq!(
        LinkClickModifier::parse_any([]),
        Err(LinkClickModifierError::Empty)
    );
    assert_eq!(
        LinkClickModifier::parse_any(["cmd", " "]),
        Err(LinkClickModifierError::Empty)
    );
    assert!(list(r#"["cmd", true]"#).is_err());
    Ok(())
}

#[test]
fn links_highlight_on_hover_only_when_configured() -> anyhow::Result<()> {
    assert!(!Config::parse("")?.highlight_links_on_hover);
    assert!(!Config::parse(DEFAULT_CONFIG)?.highlight_links_on_hover);
    assert!(Config::parse("highlight_links_on_hover = true")?.highlight_links_on_hover);
    assert!(Config::parse("highlight_links_on_hover = \"yes\"").is_err());
    Ok(())
}
