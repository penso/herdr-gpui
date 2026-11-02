#![allow(clippy::unwrap_used)]
use super::*;

fn layout(text: &str) -> Result<SidebarLayout, toml::de::Error> {
    SidebarLayout::from_daemon_config(&text.parse().unwrap())
}

#[test]
fn rejects_what_upstream_rejects() {
    for text in [
        "rows = [[\"bogus\"]]",
        "rows = [[\"$\"]]",
        "rows = [[\"$has space\"]]",
        &format!("rows = [[\"${}\"]]", "a".repeat(33)),
        &format!("rows = [{}]", "[\"agent\"],".repeat(17)),
        &format!("rows = [[{}]]", "\"agent\",".repeat(17)),
        "rows = [[{ token = \"agent\", fg = \"red\" }]]",
        "rows = [[{ token = \"agent\", size = 2 }]]",
        "rows = [[{ token = \"state_icon\", rules = [{ equals = \"x\" }] }]]",
        "rows = [[{ token = \"agent\", rules = [{ equals = \"x\", contains = \"y\" }] }]]",
        "rows = [[{ token = \"agent\", rules = [{ gt = 1, ignore_case = true }] }]]",
        "rows = [[{ token = \"agent\", rules = [{ gt = inf }] }]]",
        &format!(
            "rows = [[{{ token = \"agent\", rules = [{}] }}]]",
            "{ equals = \"x\" },".repeat(17)
        ),
    ] {
        assert!(
            layout(&format!("[ui.sidebar.agents]\n{text}")).is_err(),
            "accepted: {text}"
        );
    }
    assert!(layout("[ui.sidebar.spaces]\nrows = [[\"agent\"]]").is_err());
    assert!(
        layout("[ui.sidebar.spaces]\nrows = [[{ token = \"git_status\", rules = [{ equals = \"x\" }] }]]")
            .is_err()
    );
}

#[test]
fn rules_match_like_upstream() {
    let rule = |text: &str| -> TokenRule { toml::from_str(text).unwrap() };
    for (condition, yes, no) in [
        ("equals = 'Local'", "Local", "Localhost"),
        ("contains = 'Local'", "myLocalbox", "remote"),
        ("starts_with = 'Local'", "Localhost", "myLocal"),
    ] {
        assert!(rule(condition).matches(yes, &mut None));
        assert!(!rule(condition).matches(no, &mut None));
        assert!(!rule(condition).matches(&yes.to_ascii_lowercase(), &mut None));
        let folded = rule(&format!("{condition}\nignore_case = true"));
        assert!(folded.matches(&yes.to_ascii_lowercase(), &mut None));
    }
    for condition in ["equals", "contains", "starts_with"] {
        let folded = rule(&format!("{condition} = 'ÉA'\nignore_case = true"));
        assert!(folded.matches("Éa", &mut None));
        assert!(!folded.matches("éa", &mut None));
    }
    assert!(rule("contains = ''\nignore_case = true").matches("", &mut None));
    for (condition, yes, no) in [
        ("gt = 80", ["90", "8.1e1", "+90"], "70"),
        ("lt = 80", ["70", "7.9e1", "-90"], "90"),
    ] {
        for value in yes {
            assert!(
                rule(condition).matches(value, &mut None),
                "{condition}: {value}"
            );
        }
        for value in [no, "80", "90%", " 90", "", "NaN", "inf", "1e999"] {
            assert!(
                !rule(condition).matches(value, &mut None),
                "{condition}: {value}"
            );
        }
    }
}

#[test]
fn first_matching_rule_wins_and_fills_from_the_token_style() {
    let token: ConfiguredToken<AgentToken> = toml::from_str(
        r##"token = "machine"
fg = "#111111"
dim = true
rules = [{ equals = "Local", bold = true }, { starts_with = "L", fg = "#222222" }, { contains = "x", hide = true }]"##,
    )
    .unwrap();
    assert_eq!(
        token.style_for("Local"),
        Some(TokenStyle {
            fg: Some(0x111111),
            bold: Some(true),
            dim: Some(true)
        })
    );
    assert_eq!(token.style_for("Lab").unwrap().fg, Some(0x222222));
    assert_eq!(token.style_for("box"), None);
    assert_eq!(token.style_for("remote"), Some(token.style));
}

#[test]
fn colors_take_both_hex_forms() {
    assert_eq!(parse_color("#abc"), Some(0xaabbcc));
    assert_eq!(parse_color("#A1b2C3"), Some(0xa1b2c3));
    for bad in ["abc", "#ab", "#abcd", "#ggg", "#1234567", "#"] {
        assert_eq!(parse_color(bad), None, "{bad}");
    }
}
