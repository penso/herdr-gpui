use super::*;
use core::prelude::v1::test;

fn snapshot() -> anyhow::Result<ClientShellSnapshot> {
    let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))?;
    for agent in &mut snapshot.agents {
        agent.tokens.clear();
        agent.state_labels.clear();
    }
    for workspace in &mut snapshot.workspaces {
        workspace.tokens.clear();
    }
    Ok(snapshot)
}

fn layout(text: &str) -> anyhow::Result<SidebarLayout> {
    Ok(SidebarLayout::from_daemon_config(&text.parse()?)?)
}

#[test]
fn reports_one_safe_bounded_sample_per_scope_and_name() -> anyhow::Result<()> {
    let mut snapshot = snapshot()?;
    assert_eq!(
        host_report("Local", &snapshot),
        HostReport {
            host: "Local".into(),
            ..HostReport::default()
        }
    );
    let mut agent = snapshot.agents[0].clone();
    agent.tokens = vec![
        ("summary".into(), "fix \u{202e}auth".into()),
        ("model".into(), "   ".into()),
    ];
    let mut second = agent.clone();
    second.tokens = vec![("summary".into(), "other".into())];
    second.state_labels = vec![("working".into(), " deep in the mines ".into())];
    snapshot.agents = vec![agent.clone(), second, agent];
    snapshot.workspaces[0].tokens = vec![("summary".into(), "ci green".into())];
    let report = host_report("Local", &snapshot);
    assert_eq!(
        report.reported,
        [
            Reported {
                scope: SidebarScope::Agents,
                name: "summary".into(),
                sample: "fix auth".into(),
            },
            Reported {
                scope: SidebarScope::Spaces,
                name: "summary".into(),
                sample: "ci green".into(),
            },
        ]
    );
    assert_eq!(report.labelled_agents, 1);
    assert_eq!(report.label_sample.as_deref(), Some("deep in the mines"));

    snapshot.workspaces[0].tokens = (0..MAX_REPORTED + 8)
        .map(|index| (format!("k{index:03}"), "x".repeat(SAMPLE_LIMIT * 2)))
        .collect();
    let report = host_report("Local", &snapshot);
    assert_eq!(report.reported.len(), MAX_REPORTED);
    assert!(
        report
            .reported
            .iter()
            .all(|reported| reported.sample.chars().count() <= SAMPLE_LIMIT)
    );
    Ok(())
}

fn report(host: &str, reported: &[(SidebarScope, &str, &str)]) -> HostReport {
    HostReport {
        host: host.into(),
        reported: reported
            .iter()
            .map(|(scope, name, sample)| Reported {
                scope: *scope,
                name: (*name).into(),
                sample: (*sample).into(),
            })
            .collect(),
        ..HostReport::default()
    }
}

#[test]
fn values_merge_hosts_and_keep_configured_ones_hideable() -> anyhow::Result<()> {
    let reports = [
        report(
            "Local",
            &[
                (SidebarScope::Agents, "summary", "fix auth"),
                (SidebarScope::Spaces, "ci", "green"),
            ],
        ),
        report("devbox", &[(SidebarScope::Agents, "summary", "deploy")]),
    ];
    let layout = layout("[ui.sidebar.agents]\nrows = [[\"agent\", \"$summary\"], [\"$old\"]]\n")?;
    let values = sidebar_values(&reports, &layout);
    let summary: Vec<_> = values
        .iter()
        .map(|value| {
            (
                value.scope,
                value.name.as_str(),
                value.sample.as_deref(),
                value.hosts.join(","),
                value.shown,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (SidebarScope::Agents, "old", None, String::new(), true),
            (
                SidebarScope::Agents,
                "summary",
                Some("fix auth"),
                "Local,devbox".into(),
                true
            ),
            (
                SidebarScope::Spaces,
                "ci",
                Some("green"),
                "Local".into(),
                false
            ),
        ]
    );
    assert_eq!(values[1].token(), "$summary");
    assert!(values[1].matches("fix") && values[1].matches("summ"));
    assert!(!values[0].matches("fix"));
    Ok(())
}

#[test]
fn previews_follow_the_layout_and_drop_rows_without_values() -> anyhow::Result<()> {
    let values = sidebar_values(
        &[report(
            "Local",
            &[(SidebarScope::Agents, "summary", "fix auth")],
        )],
        &SidebarLayout::default(),
    );
    let mut source = PreviewSource {
        values: &values,
        label_sample: None,
        machine: None,
    };
    let text = |rows: Vec<Vec<PreviewPart>>| -> Vec<Vec<String>> {
        rows.into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|part| match part {
                        PreviewPart::StateIcon => "*".to_owned(),
                        PreviewPart::Text(text, _, _) => text,
                    })
                    .collect()
            })
            .collect()
    };
    let defaults = SidebarLayout::default();
    assert_eq!(
        text(preview_agent(&defaults.agents, &source)),
        [vec!["*", "herdr-gpui", "settings"], vec!["Claude Code"]]
    );
    let layout = layout(
        "[ui.sidebar.agents]\nrows = [[\"state_icon\", \"machine\", \"state_text\"], [\"$summary\"], [\"$unreported\"]]\n",
    )?;
    source.label_sample = Some("deep in the mines");
    source.machine = Some("devbox");
    let rows = preview_agent(&layout.agents, &source);
    assert_eq!(
        rows[1],
        [PreviewPart::Text(
            "fix auth".into(),
            PreviewRole::Plugin,
            TokenStyle::default()
        )]
    );
    assert_eq!(
        text(rows),
        [vec!["*", "devbox", "deep in the mines"], vec!["fix auth"]]
    );
    assert_eq!(
        text(preview_space(&defaults.spaces, &source)),
        [
            vec!["*", "herdr-gpui"],
            vec!["feat/plugin-values", "\u{2191}2"]
        ]
    );
    Ok(())
}

#[test]
fn previews_carry_token_styles_and_drop_values_a_rule_hides() -> anyhow::Result<()> {
    let values = sidebar_values(
        &[
            report("Local", &[(SidebarScope::Agents, "summary", "secret plan")]),
            report("devbox", &[(SidebarScope::Agents, "model", "opus")]),
        ],
        &SidebarLayout::default(),
    );
    let source = PreviewSource {
        values: &values,
        label_sample: None,
        machine: None,
    };
    let layout = layout(
        "[ui.sidebar.agents]\nrows = [\n  [{ token = \"$summary\", rules = [{ starts_with = \"secret\", hide = true }] }],\n  [{ token = \"$model\", fg = \"#ff0000\", bold = true, rules = [{ equals = \"opus\", dim = true }] }],\n]\n",
    )?;
    // The hidden summary's row disappears; the model keeps its token style
    // with the matching rule's dim on top.
    assert_eq!(
        preview_agent(&layout.agents, &source),
        [vec![PreviewPart::Text(
            "opus".into(),
            PreviewRole::Plugin,
            TokenStyle {
                fg: Some(0xff0000),
                bold: Some(true),
                dim: Some(true),
            },
        )]]
    );
    Ok(())
}
