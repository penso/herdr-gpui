//! What approval is pinned to. For most scripts that is the script's own
//! text. A script that calls `run` also runs commands from the repository's
//! `.herdr/worktree.toml`, so those commands are part of what a person
//! reviews and approves: they are shown with the plan, folded into the
//! hash, and the runner runs the copy it read when the run started.

use sha2::{Digest, Sha256};
use sheprd_automation::{Failure, Plan, PlannedStep, Source, Step};
use std::path::Path;

/// The `setup` and `run` scripts of a repository's `.herdr/worktree.toml`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorktreeScripts {
    pub setup: Option<String>,
    pub run: Option<String>,
}

impl WorktreeScripts {
    /// Reads them; a missing or unreadable file has none.
    pub fn read(repo: &Path) -> Self {
        let file = repo.join(".herdr").join("worktree.toml");
        let Ok(text) = std::fs::read_to_string(file) else {
            return Self::default();
        };
        let Ok(table) = text.parse::<toml::Table>() else {
            return Self::default();
        };
        let script = |name: &str| {
            table
                .get("scripts")
                .and_then(|scripts| scripts.get(name))
                .and_then(toml::Value::as_str)
                .filter(|script| !script.trim().is_empty())
                .map(str::to_owned)
        };
        Self {
            setup: script("setup"),
            run: script("run"),
        }
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        match name {
            "setup" => self.setup.as_deref(),
            "run" => self.run.as_deref(),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fingerprint {
    /// What approval is pinned to.
    pub hash: String,
    /// The worktree scripts a `run` step would run, when the plan has one.
    pub worktree: Option<WorktreeScripts>,
}

impl Fingerprint {
    pub fn of(source: &Source, plan: &Plan, repo: Option<&Path>) -> Self {
        let Some(repo) = repo.filter(|_| uses_run(plan)) else {
            return Self {
                hash: source.hash.clone(),
                worktree: None,
            };
        };
        let worktree = WorktreeScripts::read(repo);
        // An unambiguous encoding: each script is tagged present or
        // missing, and a present one carries its length.
        let mut digest = Sha256::new();
        digest.update(source.hash.as_bytes());
        for script in [&worktree.setup, &worktree.run] {
            match script {
                Some(script) => {
                    digest.update([1]);
                    digest.update((script.len() as u64).to_be_bytes());
                    digest.update(script.as_bytes());
                }
                None => digest.update([0]),
            }
        }
        Self {
            hash: digest
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
            worktree: Some(worktree),
        }
    }

    /// The worktree scripts as the plan shows them, for review.
    pub fn describe(&self) -> Option<String> {
        let worktree = self.worktree.as_ref()?;
        let mut text = String::from("  runs these .herdr/worktree.toml scripts:\n");
        for (name, script) in [("setup", &worktree.setup), ("run", &worktree.run)] {
            match script {
                Some(script) => {
                    text.push_str(&format!("    {name}:\n"));
                    for line in script.lines() {
                        text.push_str(&format!("      {}\n", sheprd_automation::visible(line)));
                    }
                }
                None => text.push_str(&format!("    {name}: (none)\n")),
            }
        }
        Some(text)
    }
}

/// Whether any path of the plan has a `run` step.
fn uses_run(plan: &Plan) -> bool {
    fn any(steps: &[PlannedStep]) -> bool {
        steps.iter().any(|step| {
            matches!(step.call.step, Step::Run { .. })
                || matches!(&step.on_failure, Failure::Branch(instead) if any(instead))
        })
    }
    plan.triggers.iter().any(|trigger| any(&trigger.steps))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
