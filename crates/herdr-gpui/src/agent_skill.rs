//! The agent skill that tells agents about browser tabs and page notes.
//! Agents only use what they know about, so the app offers once to install
//! it where Claude Code and other agents look for skills, and, once the user
//! agrees, keeps it current on every start: the skill names this app's
//! executable, which moves when the app does. Only files the app wrote,
//! recognized by their marker, are ever rewritten or removed. `[agent_skill]
//! install` in the settings limits all of this, whatever the saved answer.
use crate::{config::SkillInstall, state_file};
use gpui::{App, Global};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const SKILL: &str = include_str!("../../../skills/herdr-gpui-browser/SKILL.md");
/// How agents write automations, installed beside the browser skill.
const AUTOMATIONS: &str = include_str!("../../../skills/herdr-automations/SKILL.md");
const AUTOMATIONS_NAME: &str = "herdr-automations";
/// Marks a skill file this app manages.
const MARKER: &str = "<!-- herdr-gpui-managed-skill v1";
const NAME: &str = "herdr-gpui-browser";
/// Agent configuration directories under the home directory, whose
/// `skills/` subdirectory agents read.
const ROOTS: [&str; 2] = [".claude", ".agents"];

/// A path as one shell word.
fn shell_word(path: &str) -> String {
    if !path.is_empty()
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-+".contains(c))
    {
        path.to_owned()
    } else {
        format!("'{}'", path.replace('\'', r"'\''"))
    }
}

/// The executable agents are told to run: this one. On Windows that is the
/// console program `herdr-gpui.exe`, which a shell waits for, never its
/// windowed launcher. Call it off the UI thread.
pub(crate) fn executable() -> Option<PathBuf> {
    std::env::current_exe().ok()
}

/// How agents run `executable`. A macOS app bundle puts nothing on `PATH`,
/// so they are told exactly which file to run.
pub(crate) fn command(executable: Option<&Path>) -> String {
    executable
        .and_then(Path::to_str)
        .map_or_else(|| "herdr-gpui".to_owned(), shell_word)
}

/// The skill, naming `executable` in its commands.
pub(crate) fn text(executable: Option<&Path>) -> String {
    if executable.and_then(Path::to_str).is_none() {
        return SKILL.to_owned();
    }
    text_naming(&command(executable))
}

/// The skill, naming `command` in its commands, as one shell word.
pub(crate) fn text_naming(command: &str) -> String {
    // Every command line in the skill starts with the executable's name.
    SKILL.replace("herdr-gpui ", &format!("{command} "))
}

/// The automations skill, naming this executable and the `herdr-automate`
/// that ships beside it; a bundle puts neither on `PATH`.
pub(crate) fn automations_text(executable: Option<&Path>) -> String {
    let automate = executable
        .and_then(Path::parent)
        .map(|dir| {
            dir.join(if cfg!(windows) {
                "herdr-automate.exe"
            } else {
                "herdr-automate"
            })
        })
        .filter(|path| path.is_file());
    let mut text = AUTOMATIONS.to_owned();
    if let Some(automate) = automate.as_deref().and_then(Path::to_str) {
        text = text.replace("herdr-automate ", &format!("{} ", shell_word(automate)));
    }
    if executable.and_then(Path::to_str).is_some() {
        text = text.replace("herdr-gpui ", &format!("{} ", command(executable)));
    }
    text
}

fn managed(contents: &str) -> bool {
    contents.contains(MARKER)
}

fn file(root: &Path, name: &str) -> PathBuf {
    root.join("skills").join(name).join("SKILL.md")
}

fn install_error(path: &Path) -> impl FnOnce(std::io::Error) -> crate::Error + '_ {
    move |source| crate::Error::SkillInstall {
        path: path.to_owned(),
        source,
    }
}

/// Writes `text` into each agent directory that exists under `home`, with
/// the automations skill beside it. A file the user wrote there under the
/// same name is left alone. Returns where the skill now is. Blocking; run
/// off the UI thread.
pub(crate) fn install(home: &Path, text: &str) -> crate::Result<Vec<PathBuf>> {
    let placed = place(home, NAME, text)?;
    if placed.is_empty() {
        return Err(crate::Error::SkillInstall {
            path: home.join(ROOTS[0]),
            source: std::io::ErrorKind::NotFound.into(),
        });
    }
    place(
        home,
        AUTOMATIONS_NAME,
        &automations_text(executable().as_deref()),
    )?;
    Ok(placed)
}

/// Writes one skill into each agent directory that exists under `home`.
fn place(home: &Path, name: &str, text: &str) -> crate::Result<Vec<PathBuf>> {
    let mut placed = Vec::new();
    for root in ROOTS.map(|root| home.join(root)) {
        if !root.is_dir() {
            continue;
        }
        let path = file(&root, name);
        match std::fs::read_to_string(&path) {
            Ok(current) if current == text => {}
            Ok(current) if !managed(&current) => continue,
            Ok(_) => std::fs::write(&path, text).map_err(install_error(&path))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir).map_err(install_error(&path))?;
                }
                std::fs::write(&path, text).map_err(install_error(&path))?;
            }
            Err(error) => return Err(install_error(&path)(error)),
        }
        placed.push(path);
    }
    Ok(placed)
}

/// Removes the skill files this app wrote under `home`, and their folders
/// when nothing else is in them. Blocking; run off the UI thread.
pub(crate) fn remove(home: &Path) -> crate::Result<Vec<PathBuf>> {
    let mut removed = Vec::new();
    for root in ROOTS.map(|root| home.join(root)) {
        for name in [NAME, AUTOMATIONS_NAME] {
            let path = file(&root, name);
            let Ok(current) = std::fs::read_to_string(&path) else {
                continue;
            };
            if !managed(&current) {
                continue;
            }
            std::fs::remove_file(&path).map_err(install_error(&path))?;
            if let Some(dir) = path.parent() {
                // Only succeeds when the folder is empty, which is the intent.
                let _ = std::fs::remove_dir(dir);
            }
            removed.push(path);
        }
    }
    Ok(removed)
}

/// Whether any agent directory exists to install into.
fn agents_present(home: &Path) -> bool {
    ROOTS.iter().any(|root| home.join(root).is_dir())
}

/// What the user chose when asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Choice {
    Installed,
    Declined,
}

#[derive(Serialize, Deserialize)]
struct Saved {
    choice: Choice,
}

/// The app's answer about the skill, and whether to ask for one.
#[derive(Default)]
pub(crate) struct AgentSkill {
    choice: Option<Choice>,
    /// Asked at most once per run, by the first window ready to.
    ask: bool,
    /// What the settings allow; unknown until they load, so nothing is
    /// offered, refreshed, or sent to a host against a setting not yet read.
    install: Option<SkillInstall>,
    path: Option<PathBuf>,
    /// Rewrite the installed skill once the settings allow it.
    refresh: bool,
}

impl Global for AgentSkill {}

impl AgentSkill {
    fn path() -> Option<PathBuf> {
        crate::preferences::state_dir().map(|dir| dir.join("agent-skill.json"))
    }

    /// Called before starting GPUI. Only a release build asks or keeps the
    /// skill current: a development build would point agents at itself.
    pub(crate) fn load() -> Self {
        let path = Self::path();
        let choice = path
            .as_deref()
            .and_then(|path| match state_file::read(path, 4096) {
                Ok(bytes) => bytes,
                Err(error) => {
                    tracing::warn!(%error, "Cannot read the agent skill choice");
                    None
                }
            })
            .and_then(|bytes| serde_json::from_slice::<Saved>(&bytes).ok())
            .map(|saved| saved.choice);
        let home = crate::config::home().ok();
        let ask =
            crate::RELEASE_BUILD && choice.is_none() && home.as_deref().is_some_and(agents_present);
        let refresh = crate::RELEASE_BUILD && choice == Some(Choice::Installed);
        Self {
            choice,
            ask,
            install: None,
            path,
            refresh,
        }
    }

    /// An app that has not been asked yet, saving nowhere.
    #[cfg(test)]
    pub(crate) fn unasked() -> Self {
        Self {
            choice: None,
            ask: true,
            install: Some(SkillInstall::Ask),
            path: None,
            refresh: false,
        }
    }

    pub(crate) fn install_global(self, cx: &mut App) {
        // Fixtures never read the user's settings, so they never ask either.
        let follow = self.path.is_some();
        cx.set_global(self);
        if follow {
            crate::config::watch::follow(
                cx,
                "agent skill",
                |config| config.agent_skill.install,
                Self::allow,
            )
            .detach();
        }
        // The notes prompt names the executable, which `executable` finds on
        // disk; resolve it now, off the UI thread, rather than on first use.
        #[cfg(any(target_os = "macos", windows))]
        cx.background_executor()
            .spawn(async {
                crate::control::reload_command();
            })
            .detach();
    }

    /// Applies the settings' `[agent_skill] install`, as loaded or edited.
    fn allow(cx: &mut App, install: SkillInstall) {
        let skill = cx.global_mut::<Self>();
        skill.install = Some(install);
        if install != SkillInstall::Off && std::mem::take(&mut skill.refresh) {
            refresh_installed(cx);
        }
    }

    pub(crate) fn choice(cx: &App) -> Option<Choice> {
        cx.try_global::<Self>().and_then(|skill| skill.choice)
    }

    /// The choice to apply on saved SSH hosts, when the settings let the app
    /// touch them.
    pub(crate) fn remote_choice(cx: &App) -> Option<Choice> {
        cx.try_global::<Self>()
            .filter(|skill| skill.install == Some(SkillInstall::Ask))
            .and_then(|skill| skill.choice)
    }

    /// Whether this window should ask now; claims the question so no other
    /// window asks too.
    pub(crate) fn take_ask(cx: &mut App) -> bool {
        let ask = cx.try_global::<Self>().is_some_and(|skill| {
            skill.ask
                && skill
                    .install
                    .is_some_and(|install| install != SkillInstall::Off)
        });
        if ask {
            cx.global_mut::<Self>().ask = false;
        }
        ask
    }

    /// Records the user's answer, saved off the UI thread.
    pub(crate) fn choose(choice: Choice, cx: &mut App) {
        if !cx.has_global::<Self>() {
            cx.set_global(Self::default());
        }
        let skill = cx.global_mut::<Self>();
        skill.choice = Some(choice);
        skill.ask = false;
        // An answer now outdates the start's refresh: installing writes the
        // skill itself, and a removed one must not come back.
        skill.refresh = false;
        let Some(path) = skill.path.clone() else {
            return;
        };
        cx.background_executor()
            .spawn(async move {
                if let Err(error) = state_file::write(&path, &Saved { choice }) {
                    tracing::warn!(%error, "Cannot save the agent skill choice");
                }
            })
            .detach();
    }
}

/// Records `choice` as the app records the user's answer, for a process
/// without the app's global, such as the command line. Blocking.
pub(crate) fn save_choice(choice: Choice) -> crate::Result<()> {
    let path = AgentSkill::path().ok_or(crate::Error::MissingStateRoot)?;
    state_file::write(&path, &Saved { choice })
}

/// Rewrites the installed skill if this app's text or location changed.
fn refresh_installed(cx: &mut App) {
    let Ok(home) = crate::config::home() else {
        return;
    };
    cx.background_executor()
        .spawn(async move {
            let text = text(executable().as_deref());
            if let Err(error) = install(&home, &text) {
                tracing::warn!(%error, "Cannot refresh the agent skill");
            }
        })
        .detach();
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn both_skills_install_and_go_together() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join(".claude")).unwrap();
        let placed = install(home.path(), SKILL).unwrap();
        assert_eq!(placed.len(), 1);
        let automations = file(&home.path().join(".claude"), AUTOMATIONS_NAME);
        assert!(managed(&std::fs::read_to_string(&automations).unwrap()));
        let removed = remove(home.path()).unwrap();
        assert_eq!(removed.len(), 2);
        assert!(!automations.exists());
    }

    /// The Agent Skills format caps a description at 1,024 characters, and
    /// agents such as Pi refuse a skill over it.
    #[test]
    fn the_skill_description_fits_the_agent_skills_limit() {
        for skill in [SKILL, AUTOMATIONS] {
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
}
