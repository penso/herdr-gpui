//! The optional `herdr-projects` integration: detecting the plugin, installing
//! it, and creating a project from a space's folder.
//!
//! Detection, install, and project creation all run off the UI thread; the UI
//! reads only the cached [`State`]. Commands are built by pure helpers so their
//! argv can be tested without executing anything.

use crate::{Error, HerdrWindow, Result, config::Config};
use gpui::{Context, Task};
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

/// The plugin id in the daemon's `plugins.json`.
pub(crate) const PLUGIN_ID: &str = "herdr-projects";
/// The GitHub source the Install button passes to `herdr plugin install`.
pub(crate) const PLUGIN_SOURCE: &str = "eliasstravik/herdr-projects";
/// This crate's own contract with `command -v`: the plugin's binary name, used
/// as a fallback when the install location is not known yet.
pub(crate) const PLUGIN_BINARY: &str = "herdr-projects";

/// The plugin as the daemon's registry reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Installed {
    pub(crate) version: String,
    pub(crate) root: PathBuf,
}

/// Cached detection and in-flight work for the plugin.
#[derive(Default)]
pub(crate) struct State {
    /// The plugin as last detected; `None` when it is missing.
    pub(crate) installed: Option<Installed>,
    /// A detection pass finished, so `installed == None` is a real answer.
    pub(crate) checked: bool,
    /// The last detection, install, or creation failure, for Preferences.
    pub(crate) error: Option<String>,
    pub(crate) detect: Option<Task<()>>,
    pub(crate) install: Option<Task<()>>,
    pub(crate) create: Option<Task<()>>,
}

impl State {
    pub(crate) fn installing(&self) -> bool {
        self.install.is_some()
    }

    /// What Preferences shows for the plugin, most specific first.
    pub(crate) fn plugin_status(&self) -> String {
        if self.installing() {
            return "Installing…".into();
        }
        match (&self.installed, self.checked) {
            (Some(installed), _) if installed.version.is_empty() => "Installed".into(),
            (Some(installed), _) => format!("Installed (v{})", installed.version),
            (None, true) => "Not installed".into(),
            (None, false) => "Checking…".into(),
        }
    }
}

/// The path of the daemon's plugin registry, beside the GUI config.
pub(crate) fn plugins_path() -> Result<PathBuf> {
    let mut path = Config::path()?;
    path.pop();
    path.push("plugins.json");
    Ok(path)
}

/// The plugin's executable inside its installed root.
pub(crate) fn plugin_binary(root: &Path) -> PathBuf {
    let mut path = root.join("target").join("release").join(PLUGIN_BINARY);
    if cfg!(windows) {
        path.set_extension("exe");
    }
    path
}

/// The plugin's documented default root, used when the switch is turned on
/// before a folder was chosen.
pub(crate) fn default_projects_root() -> Result<PathBuf> {
    Ok(crate::config::home()?.join(".herdr-projects"))
}

/// The `herdr` CLI the Install button runs, preferring the daemon's own answer.
pub(crate) fn herdr_binary() -> PathBuf {
    std::env::var_os("HERDR_BIN_PATH")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("herdr"))
}

/// `herdr-projects new <name> --repo <folder> --root <root>` — the same command
/// a user runs by hand; the plugin slugifies the folder name.
pub(crate) fn new_project_command(program: &Path, name: &str, repo: &Path, root: &Path) -> Command {
    let mut command = Command::new(program);
    command
        .arg("new")
        .arg(name)
        .arg("--repo")
        .arg(repo)
        .arg("--root")
        .arg(root);
    command
}

/// `herdr plugin install <source> --yes` — noninteractive, so it never hangs
/// on the install preview.
pub(crate) fn install_command() -> Command {
    let mut command = Command::new(herdr_binary());
    command
        .arg("plugin")
        .arg("install")
        .arg(PLUGIN_SOURCE)
        .arg("--yes");
    command
}

/// Parses a `plugins.json` body for the plugin's entry. Missing is not an
/// error; malformed JSON is.
pub(crate) fn installed_in(text: &str) -> Result<Option<Installed>> {
    let plugins: Vec<serde_json::Value> = serde_json::from_str(text)?;
    for plugin in &plugins {
        if plugin.get("plugin_id").and_then(serde_json::Value::as_str) != Some(PLUGIN_ID) {
            continue;
        }
        if let Some(root) = plugin
            .get("plugin_root")
            .and_then(serde_json::Value::as_str)
        {
            let version = plugin
                .get("version")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            return Ok(Some(Installed {
                version: version.to_owned(),
                root: PathBuf::from(root),
            }));
        }
    }
    Ok(None)
}

/// Reads the daemon's registry. Blocking; runs on a background task.
pub(crate) fn detect() -> Result<Option<Installed>> {
    match std::fs::read_to_string(plugins_path()?) {
        Ok(text) => installed_in(&text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Runs the plugin installer. Blocking; runs on a background task.
pub(crate) fn install_plugin() -> Result<()> {
    let output = install_command().output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(Error::HerdrProjectsInstall {
            detail: output_tail(&output),
        })
    }
}

/// Creates a project from `cwd` using the plugin binary at `binary`, or the
/// one on `PATH`. Blocking; runs on a background task.
pub(crate) fn create_project(binary: Option<&Path>, cwd: &str, root: &Path) -> Result<()> {
    let Some(name) = Path::new(cwd).file_name().and_then(|name| name.to_str()) else {
        return Err(Error::HerdrProjectsCreate {
            detail: format!("{cwd} has no folder name to use as a project name"),
        });
    };
    let program = binary.map_or_else(|| PathBuf::from(PLUGIN_BINARY), Path::to_path_buf);
    let output = new_project_command(&program, name, Path::new(cwd), root).output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(Error::HerdrProjectsCreate {
            detail: output_tail(&output),
        })
    }
}

/// A bounded diagnostic from a failed command: stderr, else stdout, else the
/// exit status. Never more than a few lines.
fn output_tail(output: &Output) -> String {
    const LIMIT: usize = 400;
    let mut text = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if text.is_empty() {
        text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    }
    if text.chars().count() > LIMIT {
        let mut tail: Vec<char> = text.chars().rev().take(LIMIT).collect();
        tail.reverse();
        text = tail.into_iter().collect();
    }
    if text.is_empty() {
        output.status.code().map_or_else(
            || "terminated by signal".to_owned(),
            |code| format!("exit status {code}"),
        )
    } else {
        text
    }
}

impl HerdrWindow {
    /// Re-checks the plugin, off the UI thread.
    pub(crate) fn refresh_herdr_projects(&mut self, cx: &mut Context<Self>) {
        self.refresh_herdr_projects_with(detect, cx);
    }

    pub(crate) fn refresh_herdr_projects_with(
        &mut self,
        detect: impl FnOnce() -> Result<Option<Installed>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.herdr_projects.detect.is_some() {
            return;
        }
        let detection = cx.background_executor().spawn(async move { detect() });
        self.herdr_projects.detect = Some(cx.spawn(async move |this, cx| {
            let result = detection.await;
            let _ = this.update(cx, |this, cx| {
                this.herdr_projects.detect = None;
                this.herdr_projects.checked = true;
                match result {
                    Ok(installed) => {
                        this.herdr_projects.installed = installed;
                        this.herdr_projects.error = None;
                    }
                    Err(error) => {
                        this.herdr_projects.error =
                            Some(format!("Could not check for herdr-projects: {error}"));
                    }
                }
                cx.notify();
            });
        }));
    }

    /// Installs the plugin, off the UI thread, then re-detects it.
    pub(crate) fn install_herdr_projects(&mut self, cx: &mut Context<Self>) {
        self.install_herdr_projects_with(install_plugin, cx);
    }

    pub(crate) fn install_herdr_projects_with(
        &mut self,
        install: impl FnOnce() -> Result<()> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.herdr_projects.install.is_some() {
            return;
        }
        self.herdr_projects.error = None;
        let installing = cx.background_executor().spawn(async move { install() });
        self.herdr_projects.install = Some(cx.spawn(async move |this, cx| {
            let result = installing.await;
            let _ = this.update(cx, |this, cx| {
                this.herdr_projects.install = None;
                match result {
                    Ok(()) => this.refresh_herdr_projects(cx),
                    Err(error) => {
                        this.herdr_projects.error =
                            Some(format!("Could not install herdr-projects: {error}"));
                    }
                }
                cx.notify();
            });
        }));
    }

    /// Creates a project from a space's folder, off the UI thread.
    pub(crate) fn create_project_from_space(&mut self, cwd: String, cx: &mut Context<Self>) {
        let Some(root) = self.config.projects_root.clone() else {
            self.local_error = Some("Choose a projects folder before creating a project.".into());
            cx.notify();
            return;
        };
        let binary = self
            .herdr_projects
            .installed
            .as_ref()
            .map(|installed| plugin_binary(&installed.root));
        self.create_project_from_space_with(
            move |cwd| create_project(binary.as_deref(), cwd, &root),
            cwd,
            cx,
        );
    }

    pub(crate) fn create_project_from_space_with(
        &mut self,
        create: impl FnOnce(&str) -> Result<()> + Send + 'static,
        cwd: String,
        cx: &mut Context<Self>,
    ) {
        if self.herdr_projects.create.is_some() {
            return;
        }
        let creating = cx.background_executor().spawn(async move { create(&cwd) });
        self.herdr_projects.create = Some(cx.spawn(async move |this, cx| {
            let result = creating.await;
            let _ = this.update(cx, |this, cx| {
                this.herdr_projects.create = None;
                if let Err(error) = result {
                    this.local_error = Some(format!("Could not create the project: {error}"));
                }
                cx.notify();
            });
        }));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_plugin_from_a_registry_body() {
        let text = r#"[
            {"plugin_id": "other", "plugin_root": "/x", "version": "1.0"},
            {"plugin_id": "herdr-projects", "plugin_root": "/p/root", "version": "0.2.34"}
        ]"#;
        assert_eq!(
            installed_in(text).unwrap(),
            Some(Installed {
                version: "0.2.34".into(),
                root: PathBuf::from("/p/root"),
            })
        );
        assert_eq!(installed_in("[]").unwrap(), None);
        assert!(installed_in("not json").is_err());
    }

    #[test]
    fn plugin_binary_lives_in_the_release_directory() {
        let binary = plugin_binary(Path::new("/p/root"));
        assert_eq!(
            binary.parent().unwrap(),
            Path::new("/p/root/target/release")
        );
        #[cfg(windows)]
        assert_eq!(binary.file_name().unwrap(), "herdr-projects.exe");
        #[cfg(not(windows))]
        assert_eq!(binary.file_name().unwrap(), "herdr-projects");
    }

    #[test]
    fn new_project_argv_matches_the_documented_cli() {
        let command = new_project_command(
            Path::new("/bin/herdr-projects"),
            "all_gis_services_next",
            Path::new("/mnt/Data/Project"),
            Path::new("/home/me/.herdr-projects"),
        );
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            [
                "new",
                "all_gis_services_next",
                "--repo",
                "/mnt/Data/Project",
                "--root",
                "/home/me/.herdr-projects"
            ]
        );
    }

    #[test]
    fn install_is_noninteractive_and_names_the_source() {
        let command = install_command();
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, ["plugin", "install", PLUGIN_SOURCE, "--yes"]);
    }

    #[test]
    fn output_tail_is_bounded_and_falls_back_to_the_status() {
        assert_eq!(
            output_tail(&Output {
                status: exit_status(3),
                stdout: Vec::new(),
                stderr: b"boom\n".to_vec(),
            }),
            "boom"
        );
        assert_eq!(
            output_tail(&Output {
                status: exit_status(3),
                stdout: Vec::new(),
                stderr: Vec::new(),
            }),
            "exit status 3"
        );
        let long = output_tail(&Output {
            status: exit_status(1),
            stdout: Vec::new(),
            stderr: "x".repeat(1000).into_bytes(),
        });
        assert_eq!(long.chars().count(), 400);
    }

    #[cfg(unix)]
    fn exit_status(code: i32) -> std::process::ExitStatus {
        use std::os::unix::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(code << 8)
    }
    #[cfg(windows)]
    fn exit_status(code: i32) -> std::process::ExitStatus {
        use std::os::windows::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(code)
    }
}
