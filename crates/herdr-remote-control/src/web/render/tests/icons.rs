use super::*;

/// Symbols iOS draws as color emoji, or that read as nothing at a glance.
const SYMBOLS: [&str; 5] = ["⚙", "⊕", "⌄", "◉", "▭"];

#[test]
fn home_draws_its_controls_as_line_icons() {
    let layout = Layout {
        connected: true,
        boot: "b".into(),
        workspaces: vec![workspace(vec![row("p1")])],
    };
    let summaries = HashMap::new();
    let lists = Lists {
        computer: "MacBook Pro",
        layout: &layout,
        summaries: &summaries,
        now_ms: 0,
        indicators: Indicators::Dots,
    };
    for page in [home(&lists).into_string(), board(&lists).into_string()] {
        for symbol in SYMBOLS {
            assert!(!page.contains(symbol), "{symbol} in {page}");
        }
        assert!(page.contains(r#"<svg class="icon""#), "{page}");
        assert!(page.contains(r#"aria-label="Settings""#), "{page}");
    }
}

#[test]
fn agents_show_their_logos_not_symbols() {
    for agent in [
        "claude", "codex", "gemini", "copilot", "opencode", "pi", "amp",
    ] {
        let mark = agent_icon(Some(agent), "w-8 h-8").into_string();
        assert!(mark.contains("<svg"), "{agent}: {mark}");
        for symbol in ["✳", "◎", "✦", "⌘", "◇", "π"] {
            assert!(!mark.contains(symbol), "{agent}: {mark}");
        }
    }
    // "copilot" holds "pi"; it keeps its own logo.
    assert_ne!(
        agent_icon(Some("copilot"), "").into_string(),
        agent_icon(Some("pi"), "").into_string()
    );
}
