use super::*;
use crate::config::preferences::Preference;
use std::collections::HashSet;

#[test]
fn every_listed_feature_reads_its_own_key() -> anyhow::Result<()> {
    let keys: HashSet<_> = Feature::ALL.iter().map(|feature| feature.key()).collect();
    assert_eq!(keys.len(), Feature::ALL.len());
    for &feature in Feature::ALL {
        let none: &[&str] = &[];
        assert_eq!(crate::i18n::takes(feature.message()), Some(none));
        assert_eq!(
            crate::i18n::takes(feature.description_message()),
            Some(none)
        );
        assert!(!Features::default().enabled(feature));
        let config = Config::parse(&format!("[features]\n{} = true", feature.key()))?;
        for &other in Feature::ALL {
            assert_eq!(
                config.features.enabled(other),
                other == feature,
                "{other:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn feature_preferences_write_under_the_features_table() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("local.toml");
    for &feature in Feature::ALL {
        for value in [true, false] {
            Config::save_preference_path(Preference::Feature(feature, value), &path)?;
            let config = Config::parse(&fs::read_to_string(&path)?)?;
            assert_eq!(config.features.enabled(feature), value, "{feature:?}");
        }
    }
    Ok(())
}
