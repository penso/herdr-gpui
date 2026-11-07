use super::*;
use unic_langid::langid;

#[test]
fn chinese_picks_its_glyphs_by_script_then_region() {
    for (language, script) in [
        (langid!("zh"), CjkScript::Simplified),
        (langid!("zh-CN"), CjkScript::Simplified),
        (langid!("zh-SG"), CjkScript::Simplified),
        (langid!("zh-Hans"), CjkScript::Simplified),
        (langid!("zh-Hans-HK"), CjkScript::Simplified),
        (langid!("zh-TW"), CjkScript::Traditional),
        (langid!("zh-Hant"), CjkScript::Traditional),
        (langid!("zh-Hant-TW"), CjkScript::Traditional),
        (langid!("zh-HK"), CjkScript::HongKong),
        (langid!("zh-MO"), CjkScript::HongKong),
        (langid!("zh-Hant-HK"), CjkScript::HongKong),
        (langid!("yue"), CjkScript::HongKong),
        (langid!("yue-Hans"), CjkScript::Simplified),
    ] {
        assert_eq!(CjkScript::of(&language), Some(script), "{language}");
    }
}

#[test]
fn japanese_and_korean_have_their_own_glyphs_and_others_none() {
    assert_eq!(CjkScript::of(&langid!("ja-JP")), Some(CjkScript::Japanese));
    assert_eq!(CjkScript::of(&langid!("ko-KR")), Some(CjkScript::Korean));
    for language in [
        langid!("en-US"),
        langid!("fr"),
        langid!("he"),
        langid!("en-XA"),
    ] {
        assert_eq!(CjkScript::of(&language), None, "{language}");
    }
}

#[test]
fn the_first_cjk_language_preferred_wins() {
    let languages = [langid!("en-US"), langid!("ja"), langid!("zh-TW")];
    assert_eq!(CjkScript::preferred(&languages), Some(CjkScript::Japanese));
    assert_eq!(CjkScript::preferred(&[langid!("fr")]), None);
}
