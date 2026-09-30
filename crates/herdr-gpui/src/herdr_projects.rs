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

/// A project folder found under the projects root: a directory with a
/// `PROJECT.md`, which is what the plugin itself creates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Project {
    pub(crate) slug: String,
    pub(crate) path: PathBuf,
}

impl Project {
    /// How the row is named, matching the plugin's humanized slug.
    pub(crate) fn label(&self) -> String {
        humanize(&self.slug)
    }
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
    /// Project folders found under the projects root, by slug.
    pub(crate) projects: Vec<Project>,
    /// A folder scan finished; before it, an empty list means "not read yet".
    pub(crate) projects_checked: bool,
    pub(crate) detect: Option<Task<()>>,
    pub(crate) install: Option<Task<()>>,
    pub(crate) create: Option<Task<()>>,
    pub(crate) scan: Option<Task<()>>,
    pub(crate) open: Option<Task<()>>,
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

/// The plugin's slug rule: lower-case; each run of other characters becomes one
/// hyphen; at most 40 characters; no trailing hyphen.
pub(crate) fn slugify(text: &str) -> String {
    let mut slug = String::new();
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let mut slug: String = slug.chars().take(40).collect();
    while slug.ends_with('-') {
        slug.pop();
    }
    slug
}

/// Words split on `-` and `_`, each capitalized, as the plugin names a project
/// that has no explicit `name` in `PROJECT.md`.
pub(crate) fn humanize(slug: &str) -> String {
    slug.split(['-', '_'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The project folders under `root`: real directories containing a
/// `PROJECT.md`. A missing root is an empty list; hidden entries such as
/// `.machines.json`, `.progress` and `.ticker.lock` are ignored.
pub(crate) fn scan_projects(root: &Path) -> Result<Vec<Project>> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(Error::HerdrProjectsFolder {
                detail: error.to_string(),
            });
        }
    };
    let mut projects = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| Error::HerdrProjectsFolder {
            detail: error.to_string(),
        })?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let is_dir = entry
            .file_type()
            .map_err(|error| Error::HerdrProjectsFolder {
                detail: error.to_string(),
            })?
            .is_dir();
        if !is_dir || !entry.path().join("PROJECT.md").is_file() {
            continue;
        }
        projects.push(Project {
            slug: name,
            path: entry.path(),
        });
    }
    projects.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(projects)
}

/// `herdr-projects open <slug> --root <root>` — the plugin's standard way to
/// open an existing project.
pub(crate) fn open_project_command(program: &Path, slug: &str, root: &Path) -> Command {
    let mut command = Command::new(program);
    command.arg("open").arg(slug).arg("--root").arg(root);
    command
}

/// Opens a project by slug with the plugin binary at `binary`, or the one on
/// `PATH`. Blocking; runs on a background task.
pub(crate) fn open_project(binary: Option<&Path>, slug: &str, root: &Path) -> Result<()> {
    let program = binary.map_or_else(|| PathBuf::from(PLUGIN_BINARY), Path::to_path_buf);
    let output = open_project_command(&program, slug, root)
        .output()
        .map_err(|error| Error::HerdrProjectsOpen {
            detail: error.to_string(),
        })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(Error::HerdrProjectsOpen {
            detail: output_tail(&output),
        })
    }
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
    let output = install_command()
        .output()
        .map_err(|error| Error::HerdrProjectsInstall {
            detail: error.to_string(),
        })?;
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
    // A project for this folder already exists: nothing to create, and the
    // refreshed list shows its row instead of surfacing an "already exists"
    // failure.
    if root.join(slugify(name)).is_dir() {
        return Ok(());
    }
    let program = binary.map_or_else(|| PathBuf::from(PLUGIN_BINARY), Path::to_path_buf);
    let output = new_project_command(&program, name, Path::new(cwd), root)
        .output()
        .map_err(|error| Error::HerdrProjectsCreate {
            detail: error.to_string(),
        })?;
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
                    // The error already names the one thing that failed.
                    Err(error) => this.herdr_projects.error = Some(error.to_string()),
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
                match result {
                    // The folder exists now; show it at once, without waiting
                    // for the daemon to open a space for it.
                    Ok(()) => this.refresh_projects(cx),
                    Err(error) => this.local_error = Some(error.to_string()),
                }
                cx.notify();
            });
        }));
    }

    /// Re-reads the projects folder, off the UI thread, so the Projects section
    /// and the per-space buttons reflect what is on disk.
    pub(crate) fn refresh_projects(&mut self, cx: &mut Context<Self>) {
        if self.herdr_projects.scan.is_some() {
            return;
        }
        let Some(root) = self.config.projects_root.clone() else {
            self.herdr_projects.projects.clear();
            self.herdr_projects.projects_checked = true;
            cx.notify();
            return;
        };
        let scanning = cx
            .background_executor()
            .spawn(async move { scan_projects(&root) });
        self.herdr_projects.scan = Some(cx.spawn(async move |this, cx| {
            let result = scanning.await;
            let _ = this.update(cx, |this, cx| {
                this.herdr_projects.scan = None;
                this.herdr_projects.projects_checked = true;
                match result {
                    Ok(projects) => this.herdr_projects.projects = projects,
                    Err(error) => this.local_error = Some(error.to_string()),
                }
                cx.notify();
            });
        }));
    }

    /// Opens a project that has no space yet, off the UI thread.
    pub(crate) fn open_disk_project(&mut self, slug: String, cx: &mut Context<Self>) {
        let Some(root) = self.config.projects_root.clone() else {
            self.local_error = Some("Choose a projects folder before opening a project.".into());
            cx.notify();
            return;
        };
        let binary = self
            .herdr_projects
            .installed
            .as_ref()
            .map(|installed| plugin_binary(&installed.root));
        self.open_disk_project_with(
            move |slug| open_project(binary.as_deref(), slug, &root),
            slug,
            cx,
        );
    }

    pub(crate) fn open_disk_project_with(
        &mut self,
        open: impl FnOnce(&str) -> Result<()> + Send + 'static,
        slug: String,
        cx: &mut Context<Self>,
    ) {
        if self.herdr_projects.open.is_some() {
            return;
        }
        let opening = cx.background_executor().spawn(async move { open(&slug) });
        self.herdr_projects.open = Some(cx.spawn(async move |this, cx| {
            let result = opening.await;
            let _ = this.update(cx, |this, cx| {
                this.herdr_projects.open = None;
                if let Err(error) = result {
                    this.local_error = Some(error.to_string());
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

    fn temp_root(tag: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("herdr-projects-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn scan_projects_keeps_only_directories_with_project_md() {
        let root = temp_root("scan");
        std::fs::create_dir_all(root.join("alpha")).unwrap();
        std::fs::write(root.join("alpha").join("PROJECT.md"), "# alpha").unwrap();
        std::fs::create_dir_all(root.join("empty")).unwrap();
        std::fs::write(root.join("file"), "x").unwrap();
        std::fs::write(root.join(".machines.json"), "{}").unwrap();
        std::fs::create_dir_all(root.join(".progress")).unwrap();
        assert_eq!(
            scan_projects(&root).unwrap(),
            vec![Project {
                slug: "alpha".into(),
                path: root.join("alpha"),
            }]
        );
        let missing = root.join("missing");
        std::fs::remove_dir_all(&root).unwrap();
        // A missing folder is an empty list, not an error.
        assert_eq!(scan_projects(&missing).unwrap(), Vec::<Project>::new());
    }

    #[test]
    fn slug_and_label_match_the_plugins_rules() {
        assert_eq!(slugify("WeatherDashboard"), "weatherdashboard");
        assert_eq!(slugify("all_gis_services_next"), "all-gis-services-next");
        assert_eq!(slugify("  My Project!  "), "my-project");
        assert_eq!(slugify("--"), "");
        assert_eq!(humanize("weatherdashboard"), "Weatherdashboard");
        assert_eq!(humanize("all-gis-services-next"), "All Gis Services Next");
    }

    #[test]
    fn create_project_does_nothing_when_the_folder_already_exists() {
        let root = temp_root("exists");
        std::fs::create_dir_all(root.join("weatherdashboard")).unwrap();
        // Reaching a missing binary would fail, so Ok means it was never run.
        assert!(
            create_project(
                Some(Path::new("/nonexistent/herdr-projects")),
                "/repos/WeatherDashboard",
                &root
            )
            .is_ok()
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn open_project_argv_matches_the_documented_cli() {
        let command = open_project_command(
            Path::new("/bin/herdr-projects"),
            "weatherdashboard",
            Path::new("/home/me/.herdr-projects"),
        );
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            [
                "open",
                "weatherdashboard",
                "--root",
                "/home/me/.herdr-projects"
            ]
        );
    }

    #[test]
    fn a_create_failure_reads_as_one_phrase() {
        let message = Error::HerdrProjectsCreate {
            detail: "boom".into(),
        }
        .to_string();
        assert_eq!(message, "Failed to create the project: boom");
        assert!(!message.contains("Could not"));
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
