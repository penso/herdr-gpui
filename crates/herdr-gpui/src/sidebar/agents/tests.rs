use super::{Indicators, sorted_agents, state_label};
use crate::config::StatusIcons;
use herdr_client::protocol::AgentStatus;

/// Herdr's settings supply the status colors; its indicator style is the
/// terminal client's, so the chosen set is drawn whatever it says.
#[test]
fn shared_palette_props_keep_the_set_and_follow_chrome_contrast() -> anyhow::Result<()> {
    use crate::{config::Theme, contrast::Contrast, herdr_settings::Settings};
    use anyhow::Context as _;
    let settings = Settings::parse_text(
        "[ui]\nstatus_indicators = 'symbols'\n[theme.custom]\nyellow = '#123456'\n",
    )?;
    for light in [false, true] {
        for name in ["Default", "Catppuccin Latte"] {
            let theme = Theme::builtin(name)
                .context("builtin theme")?
                .with_contrast(Contrast::High);
            let indicators =
                Indicators::new(Some(&settings), StatusIcons::Wave, true, light, &theme);
            assert_eq!(
                indicators,
                Indicators::new(Some(&settings), StatusIcons::Wave, true, light, &theme)
            );
            assert_eq!(indicators.icons, StatusIcons::Wave);
            for status in [
                AgentStatus::Unknown,
                AgentStatus::Idle,
                AgentStatus::Working,
                AgentStatus::Done,
                AgentStatus::Blocked,
            ] {
                assert_eq!(
                    indicators.color(status),
                    theme.ink(settings.status_color(status, light))
                );
                for background in [theme.background, theme.surface, theme.active] {
                    assert!(
                        crate::contrast::ratio(indicators.color(status), background)
                            >= Contrast::High.mark_ratio()
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn prepared_custom_palette_keeps_each_authoritative_status_color() {
    let indicators = Indicators {
        icons: StatusIcons::Dots,
        edge_rail: false,
        colors: [0x112233, 0x223344, 0x334455, 0x445566, 0x556677],
        fade: 1f32.to_bits(),
    };
    for (status, expected) in [
        (AgentStatus::Unknown, 0x112233),
        (AgentStatus::Idle, 0x223344),
        (AgentStatus::Working, 0x334455),
        (AgentStatus::Done, 0x445566),
        (AgentStatus::Blocked, 0x556677),
    ] {
        assert_eq!(indicators.color(status), expected);
        assert_eq!(
            indicators.with_icons(StatusIcons::Orca).color(status),
            expected
        );
    }
}

/// An integration's label names the agent's current status; a label for
/// another status, a blank one, or none leaves the fallback. Labels are
/// untrusted: controls and bidi overrides go, and length is bounded.
#[test]
fn state_labels_name_the_current_status_as_safe_bounded_text() {
    let agent = |status, labels: &[(&str, &str)]| herdr_client::protocol::ClientShellAgent {
        pane_id: "p".into(),
        workspace_id: "w".into(),
        tab_id: "t".into(),
        name: None,
        display_agent: None,
        agent: None,
        title: None,
        terminal_title: None,
        terminal_title_stripped: None,
        agent_status: status,
        state_change_seq: 0,
        state_labels: labels
            .iter()
            .map(|(state, label)| ((*state).into(), (*label).into()))
            .collect(),
        tokens: Vec::new(),
        focused: false,
    };
    let labels = [("working", "deep in the mines"), ("blocked", "  ")];
    for (status, fallback, expected) in [
        (AgentStatus::Working, "working", "deep in the mines"),
        (AgentStatus::Blocked, "blocked", "blocked"),
        (AgentStatus::Done, "done", "done"),
        (AgentStatus::Unknown, "", ""),
    ] {
        assert_eq!(state_label(&agent(status, &labels), fallback), expected);
    }
    let hostile = agent(
        AgentStatus::Working,
        &[("working", "\u{1b}[31mred\u{202e}dlrow\n")],
    );
    assert_eq!(state_label(&hostile, "working"), "[31mreddlrow");
    let long = "x".repeat(10_000);
    let long = agent(AgentStatus::Idle, &[("idle", &long)]);
    assert_eq!(state_label(&long, "idle").chars().count(), 80);
}

fn ordered(
    snapshot: &herdr_client::protocol::ClientShellSnapshot,
    sort: crate::preferences::AgentSort,
) -> Vec<&str> {
    sorted_agents(snapshot, sort)
        .into_iter()
        .map(|agent| agent.pane_id.as_str())
        .collect()
}

#[test]
fn a_plugin_view_order_replaces_the_local_sort_and_filters() {
    use crate::preferences::AgentSort;
    let mut snapshot = crate::sidebar::layout_tests::snapshot(2);
    snapshot.agents[1].agent_status = AgentStatus::Blocked;
    // Without a view the order is local, even when the daemon sent one.
    snapshot.agent_order = vec!["p1".into()];
    assert_eq!(ordered(&snapshot, AgentSort::Grouped), ["p0", "p1"]);
    assert_eq!(ordered(&snapshot, AgentSort::Priority), ["p1", "p0"]);

    snapshot.agent_view_label = Some("review".into());
    snapshot.agent_order = vec!["p1".into(), "gone".into(), "p0".into()];
    for sort in [AgentSort::Grouped, AgentSort::Priority] {
        // A pane the snapshot no longer lists as an agent is skipped.
        assert_eq!(ordered(&snapshot, sort), ["p1", "p0"]);
    }
    snapshot.agents[1].agent_status = AgentStatus::Idle;
    snapshot.agent_order = vec!["p0".into()];
    assert_eq!(ordered(&snapshot, AgentSort::Priority), ["p0"]);
    snapshot.agent_order.clear();
    assert!(ordered(&snapshot, AgentSort::Grouped).is_empty());
}
