//! The agent skill that tells agents about browser tabs and page notes.
//! Agents only use what they know about, so the app offers once to install
//! it where Claude Code and other agents look for skills, and, once the user
//! agrees, keeps it current on every start: the skill names this app's
//! executable, which moves when the app does. Only files the app wrote,
//! recognized by their marker, are ever rewritten or removed. `[agent_skill]
//! install` in the settings limits all of this, whatever the saved answer.
// i18n: migrated
use crate::{config::SkillInstall, state_file};
use gpui::{App, Global};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const SKILL: &str = include_str!("../../../skills/herdr-gpui-browser/SKILL.md");
/// How agents write automations, installed beside the browser skill.
const AUTOMATIONS: &str = include_str!("../../../skills/herdr-automations/SKILL.md");
const AUTOMATIONS_NAME: &str = "herdr-automations";
/// How agents write mods for this app, installed beside the browser skill.
const MODS: &str = include_str!("../../../skills/herdr-mods/SKILL.md");
const MODS_NAME: &str = "herdr-mods";
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
    escape_description(text)
}

/// The mods skill, naming this executable in its commands.
pub(crate) fn mods_text(executable: Option<&Path>) -> String {
    if executable.and_then(Path::to_str).is_none() {
        return MODS.to_owned();
    }
    escape_description(MODS.replace("herdr-gpui ", &format!("{} ", command(executable))))
}

/// The frontmatter's description is a double-quoted YAML scalar, where a
/// Windows path's backslashes would read as escapes such as `\U`.
fn escape_description(text: String) -> String {
    let Some(line) = text
        .lines()
        .find(|line| line.starts_with("description: ") && line.contains('\\'))
    else {
        return text;
    };
    text.replacen(line, &line.replace('\\', r"\\"), 1)
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
    let executable = executable();
    place(
        home,
        AUTOMATIONS_NAME,
        &automations_text(executable.as_deref()),
    )?;
    place(home, MODS_NAME, &mods_text(executable.as_deref()))?;
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
        for name in [NAME, AUTOMATIONS_NAME, MODS_NAME] {
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
                // i18n: data
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
mod tests;
