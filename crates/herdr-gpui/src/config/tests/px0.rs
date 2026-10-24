use super::*;

#[test]
fn px0_settings_validate_the_local_workbench() -> anyhow::Result<()> {
    let config = Config::parse("[px0]\nurl = 'http://localhost:7777/'")?;
    assert_eq!(
        config.px0.url.as_ref().map(crate::browser::WebUrl::as_str),
        Some("http://localhost:7777/")
    );
    assert!(config.unknown_keys.is_empty());
    assert!(matches!(
        Config::parse("[px0]\nurl = 'http://example.com/'"),
        Err(crate::Error::Px0(crate::px0::Error::Address))
    ));
    assert_eq!(
        Config::parse(DEFAULT_CONFIG)?.px0,
        crate::px0::Config::default()
    );
    Ok(())
}
