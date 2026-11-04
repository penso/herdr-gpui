#![allow(clippy::unwrap_used)]
use super::*;

fn template(name: &str) -> Template {
    Template {
        id: format!("id-{name}"),
        name: name.into(),
        display_name: name.to_uppercase(),
        organization_name: String::new(),
        active_version_id: "v1".into(),
        deprecated: false,
    }
}

fn preset(name: &str, default: bool) -> Preset {
    Preset {
        id: format!("p-{name}"),
        name: name.into(),
        default,
    }
}

#[test]
fn a_template_is_named_or_the_only_one() {
    let only = super::template(vec![template("dev")], None).unwrap();
    assert_eq!(only.name, "dev");
    let both = || vec![template("dev"), template("gpu")];
    for name in ["gpu", "GPU", "id-gpu"] {
        assert_eq!(super::template(both(), Some(name)).unwrap().name, "gpu");
    }
    let Err(Error::Template { name, available }) = super::template(both(), None) else {
        panic!("several templates need a name");
    };
    assert_eq!((name, available.as_str()), (None, "dev, gpu"));
    let missing = super::template(both(), Some("cpu")).unwrap_err();
    assert_eq!(
        missing.to_string(),
        "Coder has no template named cpu; it has dev, gpu."
    );
    assert!(super::template(Vec::new(), None).is_err());
}

#[test]
fn an_exact_name_wins_over_a_display_name_and_a_shared_one_is_refused() {
    let mut shown_dev = template("gpu");
    shown_dev.display_name = "Dev".into();
    let exact = super::template(vec![shown_dev.clone(), template("dev")], Some("dev")).unwrap();
    assert_eq!(exact.name, "dev");
    let mut other = template("cpu");
    other.display_name = "dev".into();
    let Err(Error::AmbiguousTemplate { name, matches }) =
        super::template(vec![shown_dev, other], Some("DEV"))
    else {
        panic!("a display name two templates share must be refused");
    };
    assert_eq!((name.as_str(), matches.as_str()), ("DEV", "gpu, cpu"));
}

#[test]
fn a_preset_is_named_or_the_template_s_default() {
    let dev = template("dev");
    let presets = || vec![preset("small", false), preset("large", true)];
    assert_eq!(
        super::preset(&dev, presets(), None).unwrap().unwrap().name,
        "large"
    );
    assert_eq!(
        super::preset(&dev, presets(), Some("SMALL"))
            .unwrap()
            .unwrap()
            .name,
        "small"
    );
    assert_eq!(super::preset(&dev, Vec::new(), None).unwrap(), None);
    let missing = super::preset(&dev, presets(), Some("huge")).unwrap_err();
    assert_eq!(
        missing.to_string(),
        "The Coder template dev has no preset named huge; it has small, large."
    );
}

#[test]
fn long_lists_are_cut() {
    let names: Vec<String> = (0..30).map(|index| format!("t{index}")).collect();
    let text = listed(names.iter().map(String::as_str));
    assert!(text.ends_with("t19, …"), "{text}");
    assert_eq!(listed(std::iter::empty()), "none");
}
