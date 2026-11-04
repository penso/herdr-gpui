use super::*;
use crate::config::{
    preferences::Preference,
    status_bar::{Detail, Edit},
};

#[test]
fn the_gpu_is_off_by_default_and_parses() -> anyhow::Result<()> {
    for config in [Config::parse("")?, Config::parse(DEFAULT_CONFIG)?] {
        assert!(!config.show_gpu_load);
        assert!(!config.show_sidebar_gpu_load);
        assert!(!config.gpu_sampled());
        assert_eq!(config.status_bar.gpu_detail, Detail::Detailed);
    }
    let config = Config::parse("show_gpu_load = true\n[status_bar]\ngpu_detail = 'compact'\n")?;
    assert!(config.show_gpu_load);
    assert!(!config.show_sidebar_gpu_load);
    assert!(config.gpu_sampled());
    // Either switch alone reads the GPU.
    let sidebar = Config::parse("show_sidebar_gpu_load = true\n")?;
    assert!(sidebar.show_sidebar_gpu_load && !sidebar.show_gpu_load);
    assert!(sidebar.gpu_sampled());
    assert_eq!(config.status_bar.gpu_detail, Detail::Compact);
    assert!(config.unknown_keys.is_empty(), "{:?}", config.unknown_keys);
    assert!(Config::parse("show_gpu_load = 'yes'\n").is_err());
    assert!(Config::parse("[status_bar]\ngpu_detail = 'tiny'\n").is_err());
    Ok(())
}

#[test]
fn gpu_edits_round_trip_through_the_overrides_file() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("local.toml");
    for preference in [
        Preference::ShowGpuLoad(true),
        Preference::ShowSidebarGpuLoad(true),
        Preference::StatusBar(Edit::GpuDetail(Detail::Compact)),
    ] {
        Config::save_preference_path(preference, &path)?;
    }
    let config = Config::parse(&fs::read_to_string(&path)?)?;
    assert!(config.show_gpu_load && config.show_sidebar_gpu_load);
    assert_eq!(config.status_bar.gpu_detail, Detail::Compact);
    Config::save_preference_path(Preference::ShowGpuLoad(false), &path)?;
    assert!(!Config::parse(&fs::read_to_string(&path)?)?.show_gpu_load);
    Ok(())
}
