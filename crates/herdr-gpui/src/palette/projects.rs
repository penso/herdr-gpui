//! Local project discovery and path validation run only on background workers.

use super::Error;
use crate::Result;
use herdr_client::protocol::ClientShellSnapshot;
use std::{
    collections::HashSet,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

const MAX_VISITS: usize = 8192;
const MAX_PROJECTS: usize = 2048;

#[derive(Clone, Debug)]
pub(super) struct Project {
    pub(super) path: PathBuf,
    pub(super) label: String,
}

#[derive(Default)]
pub(super) struct Collection {
    pub(super) projects: Vec<Project>,
    pub(super) errors: Vec<crate::Error>,
}

pub(super) struct Cancellation(pub(super) Arc<AtomicBool>);

impl Default for Cancellation {
    fn default() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }
}

impl Drop for Cancellation {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

fn valid_path(path: &Path) -> Result<&str> {
    path.to_str()
        .filter(|text| {
            path.is_absolute() && text.len() <= 8192 && !text.chars().any(char::is_control)
        })
        .ok_or(Error::ProjectPath.into())
}

/// Expand variables as path data, never as shell syntax. An unset variable is
/// an error rather than silently scanning a different directory.
fn expand(
    root: &str,
    get: impl Fn(&str) -> Option<OsString>,
    home: Option<&Path>,
) -> Result<PathBuf> {
    let mut expanded = OsString::new();
    let mut rest = root;
    if rest == "~" || rest.starts_with("~/") {
        expanded.push(home.ok_or(crate::Error::MissingHome)?);
        rest = &rest[1..];
    }
    while let Some(index) = rest.find('$') {
        expanded.push(&rest[..index]);
        rest = &rest[index + 1..];
        let (name, consumed) = if let Some(braced) = rest.strip_prefix('{') {
            let end = braced.find('}').ok_or(Error::ProjectPath)?;
            (&braced[..end], end + 2)
        } else {
            let end = rest
                .find(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
                .unwrap_or(rest.len());
            (&rest[..end], end)
        };
        if name.is_empty()
            || !name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            return Err(Error::ProjectPath.into());
        }
        expanded.push(
            get(name)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| Error::ProjectVariable(name.into()))?,
        );
        rest = &rest[consumed..];
    }
    expanded.push(rest);
    let path = PathBuf::from(expanded);
    valid_path(&path)?;
    Ok(path)
}

/// Expand one configured root to a path, **without touching the filesystem**.
///
/// A palette row is rebuilt on every keystroke, so this must stay pure: the
/// root is validated when the project is actually created, off the UI thread.
/// Discovery canonicalizes because it has to read the directory anyway.
pub(super) fn expand_root(root: &str) -> Result<PathBuf> {
    let home = crate::config::home().ok();
    expand(root, |name| std::env::var_os(name), home.as_deref())
}

pub(super) fn collect(roots: &[String], cancelled: &AtomicBool) -> Collection {
    let home = crate::config::home().ok();
    let mut collection = Collection::default();
    let mut seen = HashSet::new();
    let mut visits = 0;
    for root in roots {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        let result = (|| -> Result<()> {
            let root = expand(root, |name| std::env::var_os(name), home.as_deref())?;
            // Children are listed under the verbatim root and each brought
            // to plain form on its own: a short root can hold a child too
            // long for the plain form, which `validate` keeps verbatim.
            let root = fs::canonicalize(&root)
                .map_err(|error| crate::Error::from(error).at_path(&root))?;
            for entry in
                fs::read_dir(&root).map_err(|error| crate::Error::from(error).at_path(&root))?
            {
                if cancelled.load(Ordering::Relaxed) {
                    break;
                }
                visits += 1;
                if visits > MAX_VISITS || collection.projects.len() >= MAX_PROJECTS {
                    return Err(Error::ProjectLimit.into());
                }
                let entry = entry.map_err(|error| crate::Error::from(error).at_path(&root))?;
                let name = entry.file_name();
                let Some(label) = name.to_str().filter(|name| !name.starts_with('.')) else {
                    continue;
                };
                // Symlinks are not followed: configured roots may be symlinks,
                // but discovery never escapes them through a child link.
                if !entry
                    .file_type()
                    .map_err(|error| crate::Error::from(error).at_path(&entry.path()))?
                    .is_dir()
                {
                    continue;
                }
                let path = super::canonical::plain_form(entry.path());
                valid_path(&path)?;
                if seen.insert(path.clone()) {
                    collection.projects.push(Project {
                        path,
                        label: label.into(),
                    });
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            let limited = matches!(error, crate::Error::Palette(Error::ProjectLimit));
            collection.errors.push(error);
            if limited {
                break;
            }
        }
    }
    collection
        .projects
        .sort_by_cached_key(|project| (project.label.to_lowercase(), project.path.clone()));
    collection
}

pub(super) fn validate(project: &Project) -> Result<()> {
    valid_path(&project.path)?;
    let path = super::canonical::canonicalize(&project.path)
        .map_err(|error| crate::Error::from(error).at_path(&project.path))?;
    if path != project.path || !path.is_dir() {
        return Err(Error::ProjectRemoved.into());
    }
    Ok(())
}

/// The first surviving pane in the first tab supplies the launch directory.
/// Snapshots have no original-root identity; never infer one from opaque IDs
/// or substitute a foreground process directory.
pub(super) fn workspace_for_path(snapshot: &ClientShellSnapshot, path: &Path) -> Option<String> {
    workspace_roots(snapshot)
        .into_iter()
        .find_map(|(workspace, cwd)| {
            let cwd = Path::new(&cwd);
            (cwd.is_absolute() && super::canonical::canonicalize(cwd).is_ok_and(|cwd| cwd == path))
                .then_some(workspace)
        })
}

pub(super) fn workspace_roots(snapshot: &ClientShellSnapshot) -> Vec<(String, String)> {
    snapshot
        .workspaces
        .iter()
        .filter_map(|workspace| {
            Some((
                workspace.workspace_id.clone(),
                launch_root(snapshot, &workspace.workspace_id)?.to_owned(),
            ))
        })
        .collect()
}

pub(crate) fn launch_root<'a>(
    snapshot: &'a ClientShellSnapshot,
    workspace: &str,
) -> Option<&'a str> {
    let tab = snapshot
        .tabs
        .iter()
        .filter(|tab| tab.workspace_id == workspace)
        .min_by_key(|tab| tab.number)?;
    snapshot
        .panes
        .iter()
        .find(|pane| pane.workspace_id == workspace && pane.tab_id == tab.tab_id)?
        .cwd
        .as_deref()
}

#[cfg(test)]
mod tests;
