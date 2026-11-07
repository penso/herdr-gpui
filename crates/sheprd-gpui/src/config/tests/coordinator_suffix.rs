use super::*;

#[test]
fn coordinator_groups_stay_off_until_a_suffix_is_set() -> anyhow::Result<()> {
    let suffix = |text: &str| -> anyhow::Result<Option<String>> {
        Ok(Config::parse(text)?.sidebar_style.coordinator_suffix)
    };
    assert_eq!(Config::default().sidebar_style.coordinator_suffix, None);
    assert_eq!(suffix("")?, None);
    assert_eq!(suffix(DEFAULT_CONFIG)?, None);
    assert_eq!(
        suffix("[sidebar]\ncoordinator_suffix = ' -coord '")?.as_deref(),
        Some("-coord")
    );
    // Blank would make every name a coordinator's, so it means off.
    assert_eq!(suffix("[sidebar]\ncoordinator_suffix = '  '")?, None);
    assert!(Config::parse("[sidebar]\ncoordinator_suffix = 1").is_err());
    Ok(())
}
