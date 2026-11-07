//! Finding scripts and recording what the daemon knows about each: its
//! hash, its plan as text, its triggers and caps, or why it does not load.
//! A script is only planned again when its text changes.

use crate::{
    Result,
    fingerprint::Fingerprint,
    paths::{Paths, repository_scripts},
    store::{ScriptRow, Store},
};
use sheprd_automation::{
    Plan, Schedule, Source,
    report::{ScriptCaps, TriggerStatus},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// The most scripts one host keeps; more are ignored, in path order.
pub const MAX_SCRIPTS: usize = 200;

/// A script file and the repository it runs against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
    pub repo: Option<PathBuf>,
}

/// Every script in the personal folder and in each registered repository.
pub fn find(paths: &Paths, store: &Store) -> Result<Vec<Found>> {
    let mut found = Vec::new();
    for path in luau_files(&paths.personal()) {
        found.push(Found { path, repo: None });
    }
    for repo in store.repos()? {
        let repo = PathBuf::from(repo);
        for path in luau_files(&repository_scripts(&repo)) {
            found.push(Found {
                path,
                repo: Some(repo.clone()),
            });
        }
    }
    found.truncate(MAX_SCRIPTS);
    Ok(found)
}

fn luau_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "luau")
                && path.is_file()
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}

/// Where a repository's scripts go, for messages.
pub fn repository_scripts_hint(repo: &Path) -> String {
    repository_scripts(repo).to_string_lossy().into_owned()
}

/// Where a script file belongs: the personal folder (no repository) or a
/// registered repository's automations folder.
pub fn locate(paths: &Paths, store: &Store, file: &Path) -> Result<Found> {
    let file = fs::canonicalize(file).map_err(crate::Error::io(file))?;
    let parent = file.parent().map(Path::to_path_buf).unwrap_or_default();
    let canonical = |dir: PathBuf| fs::canonicalize(&dir).unwrap_or(dir);
    if parent == canonical(paths.personal()) {
        return Ok(Found {
            path: file,
            repo: None,
        });
    }
    for repo in store.repos()? {
        let repo = PathBuf::from(repo);
        if parent == canonical(repository_scripts(&repo)) {
            return Ok(Found {
                path: file,
                repo: Some(repo),
            });
        }
    }
    Err(crate::Error::NotAutomation(file))
}

/// What a script's plan says, or why it has none.
#[cfg(all(test, unix))]
pub fn plan_row(source: &Source, path: String, repo: Option<&Path>, now: i64) -> ScriptRow {
    let plan = Plan::build(source, sheprd_automation::Limits::default());
    let fingerprint = plan
        .as_ref()
        .ok()
        .map(|plan| Fingerprint::of(source, plan, repo));
    row(source, plan.as_ref(), fingerprint.as_ref(), path, repo, now)
}

/// A script's row from its plan, or the reason it has none, pinned to
/// `fingerprint` when it has one.
pub fn row(
    source: &Source,
    plan: std::result::Result<&Plan, &sheprd_automation::Error>,
    fingerprint: Option<&Fingerprint>,
    path: String,
    repo: Option<&Path>,
    now: i64,
) -> ScriptRow {
    let base = ScriptRow {
        path,
        name: source.name.clone(),
        repo: repo.map(|repo| repo.to_string_lossy().into_owned()),
        hash: fingerprint.map_or_else(|| source.hash.clone(), |found| found.hash.clone()),
        seen_at: now,
        ..ScriptRow::default()
    };
    match plan {
        Ok(plan) => ScriptRow {
            plan: Some(format!(
                "{plan}{}",
                fingerprint
                    .and_then(Fingerprint::describe)
                    .unwrap_or_default()
            )),
            triggers: triggers(plan),
            caps: Some(ScriptCaps {
                agents_per_run: plan.caps.agents_per_run,
                tokens_per_day: plan.caps.tokens_per_day,
                wall_time_seconds: plan.caps.wall_time.as_secs(),
            }),
            ..base
        },
        Err(error) => ScriptRow {
            error: Some(error.to_string()),
            ..base
        },
    }
}

/// The plan's triggers as reports describe them, with no next firing yet.
pub fn triggers(plan: &Plan) -> Vec<TriggerStatus> {
    plan.triggers
        .iter()
        .enumerate()
        .map(|(index, trigger)| TriggerStatus {
            index,
            name: trigger.name.clone(),
            schedule: trigger.schedule.to_string(),
            zone: match trigger.schedule {
                Schedule::At { .. } | Schedule::Cron { .. } => trigger.zone.to_string(),
                Schedule::Every { .. } | Schedule::Manual => String::new(),
            },
            next: None,
        })
        .collect()
}
