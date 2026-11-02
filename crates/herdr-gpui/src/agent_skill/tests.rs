#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn the_skill_is_marked_and_names_this_executable() {
    assert!(SKILL.starts_with("---\nname: herdr-gpui-browser\n"));
    assert!(managed(SKILL));
    assert!(SKILL.contains("herdr-gpui browser open"));
    assert_eq!(text(None), SKILL);
    let bundled = text(Some(Path::new(
        "/Applications/Herdr.app/Contents/MacOS/Herdr",
    )));
    assert!(bundled.contains("/Applications/Herdr.app/Contents/MacOS/Herdr browser open"));
    assert!(!bundled.contains("herdr-gpui browser"));
    // Every command, not only the browser's, names the executable.
    assert!(!bundled.contains("herdr-gpui "));
    assert!(bundled.contains("/Applications/Herdr.app/Contents/MacOS/Herdr code open"));
    let spaced = text(Some(Path::new("/Users/a b/it's/Herdr")));
    assert!(spaced.contains(r"'/Users/a b/it'\''s/Herdr' browser open"));
}

/// The description lists commands after a colon (`daemon: show a file`),
/// so as a plain YAML scalar it would parse as a nested mapping and fail
/// every agent's frontmatter parser, hiding the skill from agents.
#[test]
fn the_skill_description_is_a_quoted_scalar() {
    assert!(!description(SKILL).contains('"'));
    assert!(!description(AUTOMATIONS).contains('"'));
    assert!(!description(MODS).contains('"'));
}

#[test]
fn the_automations_skill_is_marked_and_names_its_commands() {
    assert!(AUTOMATIONS.starts_with("---\nname: herdr-automations\n"));
    assert!(managed(AUTOMATIONS));
    assert_eq!(automations_text(None), AUTOMATIONS);
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("Herdr");
    std::fs::write(&app, "").unwrap();
    let automate = dir.path().join(if cfg!(windows) {
        "herdr-automate.exe"
    } else {
        "herdr-automate"
    });
    std::fs::write(&automate, "").unwrap();
    let text = automations_text(Some(&app));
    assert!(text.contains(&format!(
        "{} check FILE",
        shell_word(automate.to_str().unwrap())
    )));
    assert!(text.contains(&format!(
        "{} automation approve FILE",
        shell_word(app.to_str().unwrap())
    )));
    assert!(!text.contains("herdr-gpui automation"));
}

/// A Windows install path lands in the description, and YAML reads a
/// backslash in a double-quoted scalar as an escape (#868).
#[test]
fn a_backslashed_install_path_stays_valid_yaml() {
    let dir = tempfile::tempdir().unwrap();
    // One directory named with a backslash on Unix, two on Windows.
    let install = dir.path().join(r"Programs\Herdr UI");
    std::fs::create_dir_all(&install).unwrap();
    let app = install.join("Herdr");
    std::fs::write(&app, "").unwrap();
    let automate = install.join(if cfg!(windows) {
        "herdr-automate.exe"
    } else {
        "herdr-automate"
    });
    std::fs::write(&automate, "").unwrap();
    let text = automations_text(Some(&app));
    let raw = description(&text);
    assert!(!raw.replace(r"\\", "").contains('\\'), "{raw}");
    let word = shell_word(automate.to_str().unwrap());
    assert!(raw.replace(r"\\", r"\").contains(&format!("{word} check")));
    // The body is Markdown, so its commands keep the path as it is.
    assert!(text.contains(&format!("{word} check FILE")));
}

#[test]
fn both_skills_install_and_go_together() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join(".claude")).unwrap();
    let placed = install(home.path(), SKILL).unwrap();
    assert_eq!(placed.len(), 1);
    let automations = file(&home.path().join(".claude"), AUTOMATIONS_NAME);
    assert!(managed(&std::fs::read_to_string(&automations).unwrap()));
    let mods = file(&home.path().join(".claude"), MODS_NAME);
    assert!(managed(&std::fs::read_to_string(&mods).unwrap()));
    let removed = remove(home.path()).unwrap();
    assert!(removed.contains(&automations) && removed.contains(&mods));
    assert!(!automations.exists() && !mods.exists());
}

/// The Agent Skills format caps a description at 1,024 characters, and
/// agents such as Pi refuse a skill over it.
#[test]
fn the_skill_description_fits_the_agent_skills_limit() {
    for skill in [SKILL, AUTOMATIONS, MODS] {
        let length = description(skill).chars().count();
        assert!((1..=1024).contains(&length), "{length} characters");
    }
}

/// The frontmatter's description, without its quotes.
fn description(skill: &str) -> &str {
    skill
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .and_then(|(frontmatter, _)| {
            frontmatter
                .lines()
                .find_map(|line| line.strip_prefix("description: "))
        })
        .and_then(|value| value.strip_prefix('"'))
        .and_then(|value| value.strip_suffix('"'))
        .unwrap()
}

#[test]
fn the_skill_teaches_the_split_the_kit_and_page_results() {
    for taught in [
        "A local file opens in a split next to your terminal",
        "--tab",
        "/.herdr/kit.css",
        "HerdrKit.scenario(",
        "HerdrKit.variants(",
        "herdr.send(",
        "herdr-gpui browser example",
    ] {
        assert!(SKILL.contains(taught), "{taught}");
    }
    let bundled = text(Some(Path::new(
        "/Applications/Herdr.app/Contents/MacOS/Herdr",
    )));
    assert!(bundled.contains("/Applications/Herdr.app/Contents/MacOS/Herdr browser example"));
}

#[test]
fn installing_keeps_to_existing_agent_directories_and_the_users_own_files() {
    let home = tempfile::tempdir().unwrap();
    assert!(matches!(
        install(home.path(), SKILL),
        Err(crate::Error::SkillInstall { .. })
    ));
    assert!(!agents_present(home.path()));
    std::fs::create_dir(home.path().join(".agents")).unwrap();
    std::fs::create_dir(home.path().join(".claude")).unwrap();
    // A skill of the same name the user wrote is theirs.
    let own = file(&home.path().join(".claude"), NAME);
    std::fs::create_dir_all(own.parent().unwrap()).unwrap();
    std::fs::write(&own, "my own skill").unwrap();
    let placed = install(home.path(), SKILL).unwrap();
    assert_eq!(placed, [file(&home.path().join(".agents"), NAME)]);
    assert_eq!(std::fs::read_to_string(&placed[0]).unwrap(), SKILL);
    assert_eq!(std::fs::read_to_string(&own).unwrap(), "my own skill");

    // A newer text replaces the managed copy; the same text is a no-op.
    let newer = format!("{SKILL}\nNew section\n");
    install(home.path(), &newer).unwrap();
    assert_eq!(std::fs::read_to_string(&placed[0]).unwrap(), newer);

    // Removing takes only the managed copies, the automations skill's
    // too, and their now-empty folders.
    let removed = remove(home.path()).unwrap();
    let browser = removed
        .iter()
        .filter(|path| path.parent().is_some_and(|dir| dir.ends_with(NAME)))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(browser, placed);
    assert!(removed.iter().all(|path| path != &own));
    assert!(!placed[0].parent().unwrap().exists());
    assert_eq!(std::fs::read_to_string(&own).unwrap(), "my own skill");
    assert!(remove(home.path()).unwrap().is_empty());
}

#[test]
fn choices_are_saved_as_json() {
    let text = serde_json::to_string(&Saved {
        choice: Choice::Declined,
    })
    .unwrap();
    assert_eq!(text, r#"{"choice":"declined"}"#);
    let saved: Saved = serde_json::from_str(r#"{"choice":"installed"}"#).unwrap();
    assert_eq!(saved.choice, Choice::Installed);
}

mod install_setting;

#[gpui::test]
fn the_offer_is_made_once_and_closing_it_means_not_now(cx: &mut gpui::TestAppContext) {
    use crate::sidebar::layout_tests::fixture_window;
    cx.update(|cx| cx.set_global(AgentSkill::unasked()));
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.active = true;
            view.offer_browser_skill(window, cx);
            assert_eq!(view.menu.page, Some(crate::menu::Page::AgentSkill));
            // Another window, or a later tick, does not ask again.
            view.dismiss_menu(window, cx);
            view.offer_browser_skill(window, cx);
            assert_eq!(view.menu.page, None);
        });
        assert_eq!(AgentSkill::choice(cx), Some(Choice::Declined));
    });
}
