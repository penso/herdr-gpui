use super::*;
use crate::config::cjk_fonts::{
    LINUX_CJK_FONTS, MACOS_CJK_FONTS, PLATFORM_CJK_FONTS, WINDOWS_CJK_FONTS, cjk_fallbacks,
    preferred_script,
};
use herdr_i18n::{CjkScript, Language, LanguageIdentifier};
use std::collections::BTreeSet;

fn id(tag: &str) -> anyhow::Result<LanguageIdentifier> {
    Ok(tag.parse()?)
}

fn installed(families: &[&str]) -> BTreeSet<String> {
    families.iter().map(|family| (*family).to_owned()).collect()
}

fn every_family(fonts: &[(CjkScript, &str)]) -> BTreeSet<String> {
    fonts
        .iter()
        .map(|(_, family)| (*family).to_owned())
        .collect()
}

#[test]
fn the_preferred_script_leads_and_the_rest_keep_table_order() {
    let all = every_family(MACOS_CJK_FONTS);
    assert_eq!(
        cjk_fallbacks(MACOS_CJK_FONTS, None, &all),
        [
            "PingFang SC",
            "PingFang TC",
            "PingFang HK",
            "Hiragino Sans",
            "Apple SD Gothic Neo"
        ]
    );
    for (script, first) in [
        (CjkScript::Simplified, "PingFang SC"),
        (CjkScript::Traditional, "PingFang TC"),
        (CjkScript::HongKong, "PingFang HK"),
        (CjkScript::Japanese, "Hiragino Sans"),
        (CjkScript::Korean, "Apple SD Gothic Neo"),
    ] {
        let chosen = cjk_fallbacks(MACOS_CJK_FONTS, Some(script), &all);
        assert_eq!(
            chosen.first().map(String::as_str),
            Some(first),
            "{script:?}"
        );
        assert_eq!(chosen.len(), 5, "{script:?}");
    }
}

#[test]
fn windows_names_each_family_once() {
    let chosen = cjk_fallbacks(
        WINDOWS_CJK_FONTS,
        Some(CjkScript::HongKong),
        &every_family(WINDOWS_CJK_FONTS),
    );
    assert_eq!(
        chosen,
        [
            "Microsoft JhengHei UI",
            "Microsoft YaHei UI",
            "Yu Gothic UI",
            "Malgun Gothic"
        ]
    );
}

#[test]
fn linux_takes_the_first_installed_family_of_each_script() {
    // Source Han Sans for Japanese only, and the older Chinese faces when
    // neither Noto nor Source Han is there for Chinese.
    let chosen = cjk_fallbacks(
        LINUX_CJK_FONTS,
        Some(CjkScript::Japanese),
        &installed(&[
            "DejaVu Sans",
            "Source Han Sans",
            "WenQuanYi Zen Hei",
            "Droid Sans Fallback",
        ]),
    );
    assert_eq!(chosen, ["Source Han Sans", "WenQuanYi Zen Hei"]);
    let chosen = cjk_fallbacks(
        LINUX_CJK_FONTS,
        Some(CjkScript::Traditional),
        &every_family(LINUX_CJK_FONTS),
    );
    assert_eq!(
        chosen,
        [
            "Noto Sans CJK TC",
            "Noto Sans CJK SC",
            "Noto Sans CJK HK",
            "Noto Sans CJK JP",
            "Noto Sans CJK KR"
        ]
    );
    assert!(cjk_fallbacks(LINUX_CJK_FONTS, None, &installed(&["DejaVu Sans"])).is_empty());
}

#[test]
fn the_setting_names_the_script_before_the_system() -> anyhow::Result<()> {
    let system = [id("en-US")?, id("zh-TW")?];
    assert_eq!(
        preferred_script(&Language::System, &system),
        Some(CjkScript::Traditional)
    );
    let japanese = Language::Locale("ja".parse()?);
    assert_eq!(
        preferred_script(&japanese, &system),
        Some(CjkScript::Japanese)
    );
    // A pseudo-locale and a non-CJK setting leave the choice to the system.
    let pseudo = Language::Locale(herdr_i18n::Pseudo::Accented.locale());
    assert_eq!(
        preferred_script(&pseudo, &system),
        Some(CjkScript::Traditional)
    );
    let french = Language::Locale("fr".parse()?);
    assert_eq!(preferred_script(&french, &[id("en")?]), None);
    Ok(())
}

#[test]
fn interface_faces_search_icons_then_cjk_and_the_terminal_only_icons() -> anyhow::Result<()> {
    let mut config = Config::parse("language = 'ko'\n[tabs]\nfallback = ['Menlo']")?;
    let families = || {
        let mut families: Vec<String> = every_family(PLATFORM_CJK_FONTS).into_iter().collect();
        families.push("Symbols Nerd Font Mono".to_owned());
        families
    };
    config.resolve_fonts_with(families, |_| false);
    let expected_cjk = cjk_fallbacks(
        PLATFORM_CJK_FONTS,
        Some(CjkScript::Korean),
        &families().into_iter().collect(),
    );
    assert!(!expected_cjk.is_empty());
    let mut expected = vec!["Symbols Nerd Font Mono".to_owned()];
    expected.extend(expected_cjk);
    assert_eq!(config.ui.fallbacks.as_deref(), Some(expected.as_slice()));
    assert_eq!(config.sidebar.fallbacks, config.ui.fallbacks);
    // The terminal keeps the icon cascade alone; a configured list is kept.
    assert_eq!(
        config.terminal.fallbacks.as_deref(),
        Some(["Symbols Nerd Font Mono".to_owned()].as_slice())
    );
    assert_eq!(
        config.tabs.fallbacks.as_deref(),
        Some(["Menlo".to_owned()].as_slice())
    );
    Ok(())
}
