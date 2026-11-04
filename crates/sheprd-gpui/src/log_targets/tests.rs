use super::*;

#[test]
fn every_sheprd_crate_the_app_links_is_logged() {
    let manifest: toml::Table = toml::from_str(include_str!("../../Cargo.toml")).unwrap();
    let dependencies = manifest["dependencies"].as_table().unwrap();
    let linked: Vec<String> = dependencies
        .keys()
        .filter(|name| name.starts_with("sheprd-"))
        .map(|name| name.replace('-', "_"))
        .collect();
    assert!(!linked.is_empty());
    for name in linked {
        assert!(LOG_TARGETS.contains(&name.as_str()), "{name} is not logged");
    }
    assert!(LOG_TARGETS.contains(&"sheprd_gpui"));
}
