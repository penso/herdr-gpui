//! The interface faces' cascade for Chinese, Japanese, and Korean text.
//!
//! Han characters share codepoints across regions, so a platform's own
//! cascade draws them in whichever region's shapes it finds first, often
//! Japanese for a Chinese reader. Naming the right regional family first, by
//! the languages the user prefers, draws them as that reader expects. The
//! terminal keeps its own cascade: its cells need monospace widths these
//! proportional faces do not have.

use herdr_i18n::{CjkScript, Language, LanguageIdentifier};
use std::collections::BTreeSet;

/// A platform's CJK families by the script they draw, in the order searched
/// when no preferred language names a script. A script may list several
/// families; the first installed one serves it.
pub(super) type CjkFonts = &'static [(CjkScript, &'static str)];

/// Every macOS release since 10.11 ships PingFang, Hiragino Sans, and Apple
/// SD Gothic Neo.
pub(super) const MACOS_CJK_FONTS: CjkFonts = &[
    (CjkScript::Simplified, "PingFang SC"),
    (CjkScript::Traditional, "PingFang TC"),
    (CjkScript::HongKong, "PingFang HK"),
    (CjkScript::Japanese, "Hiragino Sans"),
    (CjkScript::Korean, "Apple SD Gothic Neo"),
];

/// The `UI` cuts are the interface variants Windows draws its own chrome
/// with. Microsoft JhengHei serves Hong Kong as well as Taiwan.
pub(super) const WINDOWS_CJK_FONTS: CjkFonts = &[
    (CjkScript::Simplified, "Microsoft YaHei UI"),
    (CjkScript::Traditional, "Microsoft JhengHei UI"),
    (CjkScript::HongKong, "Microsoft JhengHei UI"),
    (CjkScript::Japanese, "Yu Gothic UI"),
    (CjkScript::Korean, "Malgun Gothic"),
];

/// Distributions package Noto Sans CJK, its Source Han Sans twin, or only
/// the older WenQuanYi and Droid faces, which draw Chinese shapes.
pub(super) const LINUX_CJK_FONTS: CjkFonts = &[
    (CjkScript::Simplified, "Noto Sans CJK SC"),
    (CjkScript::Traditional, "Noto Sans CJK TC"),
    (CjkScript::HongKong, "Noto Sans CJK HK"),
    (CjkScript::Japanese, "Noto Sans CJK JP"),
    (CjkScript::Korean, "Noto Sans CJK KR"),
    (CjkScript::Simplified, "Source Han Sans SC"),
    (CjkScript::Traditional, "Source Han Sans TC"),
    (CjkScript::HongKong, "Source Han Sans HC"),
    (CjkScript::Japanese, "Source Han Sans"),
    (CjkScript::Korean, "Source Han Sans K"),
    (CjkScript::Simplified, "WenQuanYi Micro Hei"),
    (CjkScript::Simplified, "WenQuanYi Zen Hei"),
    (CjkScript::Simplified, "Droid Sans Fallback"),
];

pub(super) const PLATFORM_CJK_FONTS: CjkFonts = if cfg!(target_os = "linux") {
    LINUX_CJK_FONTS
} else if cfg!(windows) {
    WINDOWS_CJK_FONTS
} else {
    MACOS_CJK_FONTS
};

/// The script whose glyphs the user reads Han characters in: from the
/// `language` setting when it names a CJK language, else the first such
/// language the system prefers. The setting counts even before messages in
/// that language exist, since the glyphs are wanted either way.
pub(super) fn preferred_script(
    language: &Language,
    system: &[LanguageIdentifier],
) -> Option<CjkScript> {
    let configured = match language {
        Language::Locale(locale) if locale.pseudo().is_none() => Some(locale.id()),
        Language::Locale(_) | Language::System => None,
    };
    CjkScript::preferred(configured.into_iter().chain(system))
}

/// The installed families of `fonts` to search for CJK glyphs: the first
/// installed family of each script, the `preferred` script's first and the
/// rest in table order, each family once.
pub(super) fn cjk_fallbacks(
    fonts: CjkFonts,
    preferred: Option<CjkScript>,
    installed: &BTreeSet<String>,
) -> Vec<String> {
    let mut scripts: Vec<CjkScript> = preferred.into_iter().collect();
    for (script, _) in fonts {
        if !scripts.contains(script) {
            scripts.push(*script);
        }
    }
    let mut families: Vec<String> = Vec::with_capacity(scripts.len());
    for script in scripts {
        let family = fonts
            .iter()
            .filter(|(candidate, _)| *candidate == script)
            .map(|(_, family)| *family)
            .find(|family| installed.contains(*family));
        if let Some(family) = family
            && !families.iter().any(|chosen| chosen == family)
        {
            families.push(family.to_owned());
        }
    }
    families
}
