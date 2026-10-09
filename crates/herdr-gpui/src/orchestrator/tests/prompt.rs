use super::*;
use crate::orchestrator::prompt::{
    Profile, branch, built_in, compose, load_profiles, render, valid_branch,
};

#[test]
fn templates_fill_agent_launchers_variables_and_keep_the_rest() {
    let mut issue = item(&github(), "377", "Prompt icons render as tofu");
    issue.description = Some("Boxes instead of icons.".into());
    let text = render(
        "{{ issue_title }} ({{issue_identifier}}) in {{ issue_repository }} on {{ issue_provider }}\n{{ issue_text }}\n{{ issue_link }}\n{{ unknown }} {% if x %}",
        &issue,
    );
    assert_eq!(
        text,
        "Prompt icons render as tofu (#377) in penso/herdr-gpui on github\nBoxes instead of icons.\nhttps://github.com/penso/herdr-gpui/issues/377\n{{ unknown }} {% if x %}"
    );
    issue.description = None;
    issue.url = None;
    assert_eq!(
        render("{{ issue_text }} / {{ issue_link }} / {{ open", &issue),
        "(no issue description provided) / (no issue link available) / {{ open"
    );
}

#[test]
fn a_profile_replaces_the_built_in_prompt_and_extra_text_is_appended() {
    let issue = item(&github(), "377", "Icons");
    assert!(built_in(&issue).starts_with("Implement this issue.\n\nProvider: github\nRepository: penso/herdr-gpui\nIdentifier: #377\nTitle: Icons\n"));
    let profile = Profile {
        name: "implementer".into(),
        template: "\nFix {{ issue_title }}.\n".into(),
    };
    assert_eq!(compose(&issue, Some(&profile), ""), "Fix Icons.");
    assert_eq!(
        compose(&issue, Some(&profile), "Use the fallback font."),
        "Fix Icons.\n\nUse the fallback font."
    );
    assert_eq!(compose(&issue, None, "  "), built_in(&issue));
}

#[test]
fn profiles_load_from_agent_launchers_folder_sorted_and_bounded() {
    let home = tempfile::tempdir().unwrap();
    let agents = home.path().join(".config/agent-launcher/agents");
    for (name, body) in [
        ("reviewer", "Review {{ issue_title }}"),
        ("implementer", "Implement"),
        ("empty", "  "),
    ] {
        std::fs::create_dir_all(agents.join(name)).unwrap();
        std::fs::write(agents.join(name).join("prompt.md"), body).unwrap();
    }
    std::fs::create_dir_all(agents.join("huge")).unwrap();
    std::fs::write(agents.join("huge/prompt.md"), "x".repeat(70 * 1024)).unwrap();
    let names: Vec<_> = load_profiles(home.path())
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(names, ["implementer", "reviewer"]);
    assert!(load_profiles(&home.path().join("missing")).is_empty());
}

#[test]
fn branches_follow_herdr_gpui_naming() {
    assert_eq!(
        branch(&item(&github(), "377", "Prompt icons: render as tofu!")),
        "377-prompt-icons-render-as-tofu"
    );
    let mut bead = item(&beads(), "hg-a3f2.1", "Shared SQLite store");
    bead.identifier = "hg-a3f2.1".into();
    assert_eq!(branch(&bead), "hg-a3f2-1-shared-sqlite-store");
    assert_eq!(branch(&item(&github(), "9", "\u{1f600}")), "9");
    for good in ["377-icons", "feat/x", "a.b_c"] {
        assert!(valid_branch(good), "{good}");
    }
    for bad in [
        "", "-x", "x/", "a..b", "a//b", "x.lock", "a b", "a@{1}", ".x", "x;rm",
    ] {
        assert!(!valid_branch(bad), "{bad}");
    }
}
