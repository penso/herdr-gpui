use super::*;
use crate::config::{ChangeCounts, preferences::Preference};

#[test]
fn change_counts_are_named_and_default_to_the_pull_request() -> anyhow::Result<()> {
    for config in [Config::default(), Config::parse(DEFAULT_CONFIG)?] {
        assert_eq!(
            config.sidebar_style.change_counts,
            ChangeCounts::PullRequest
        );
    }
    for (name, counts) in [
        ("pr", ChangeCounts::PullRequest),
        ("local", ChangeCounts::Local),
    ] {
        assert_eq!(counts.as_str(), name);
        assert_eq!(
            Config::parse(&format!("[sidebar]\nchange_counts = \"{name}\""))?
                .sidebar_style
                .change_counts,
            counts
        );
    }
    for bad in ["\"both\"", "\"Local\"", "true"] {
        assert!(Config::parse(&format!("[sidebar]\nchange_counts = {bad}")).is_err());
    }
    Ok(())
}

#[test]
fn change_counts_round_trip_through_the_file() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("local.toml");
    fs::write(&path, "[sidebar]\nchange_counts = \"pr\" # mine\n")?;
    Config::save_preference_path(Preference::SidebarChangeCounts(ChangeCounts::Local), &path)?;
    let text = fs::read_to_string(&path)?;
    assert!(text.contains("change_counts = \"local\" # mine"), "{text}");
    assert_eq!(
        Config::parse(&text)?.sidebar_style.change_counts,
        ChangeCounts::Local
    );
    Ok(())
}
